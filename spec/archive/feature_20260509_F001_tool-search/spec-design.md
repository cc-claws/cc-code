# Feature: 20260509_F001 - Tool Search 延迟加载

## 需求背景

当前 Peri 将所有工具（12 个内置 + 3 个 Cron + 1 个 MCP Resource + N 个 MCP 工具）的完整 schema 全部序列化为 `ToolDefinition` 发送给 LLM。每个工具定义（name + description + inputSchema）平均消耗数百 token。

随着 MCP 服务器连接数增长，工具数量可能达到数十甚至上百个，产生三个问题：

1. **Token 浪费** — 大量低频工具的 schema 占用上下文窗口，留给实际推理的 token 减少
2. **模型注意力稀释** — 过多的工具定义干扰模型对核心工具的选择准确性
3. **Prompt Cache 不稳定** — MCP 服务器连接/断开时工具列表变化，导致缓存失效（特别是 Anthropic prompt cache）

## 目标

- 将工具分为 Core（始终加载）和 Deferred（按需发现），减少发送给 LLM 的工具 schema 数量
- 通过两个元工具 `SearchExtraTools` + `ExecuteExtraTool` 实现延迟工具的发现和代理执行
- 工具数组在会话中保持稳定，不因发现新工具而变化（保护 prompt cache）
- 使用 TF-IDF + 关键词混合搜索，零外部依赖

## 方案设计

### 1. 工具分类

```
┌──────────────────────────────────────────────────────────────────┐
│                      All Tools                                   │
├────────────────────────────┬─────────────────────────────────────┤
│   Core Tools (~12 个)      │   Deferred Tools (其余全部)          │
│   始终加载，直接调用        │   仅注册名称列表，按需发现            │
│   CORE_TOOLS 常量定义       │   is_deferred_tool() 判定            │
└────────────────────────────┴─────────────────────────────────────┘
```

**Core Tools**（`CORE_TOOLS` 常量，`HashSet<&str>`）：

| 类别 | 工具名 |
|------|--------|
| 文件操作 | Read, Write, Edit, Glob, Grep, folder_operations |
| 执行 | Bash |
| Web | WebFetch, WebSearch |
| 交互 | Agent, AskUserQuestion |
| 管理 | TodoWrite |

**Deferred Tools**（不进入 API tools 数组）：

| 类别 | 工具 |
|------|------|
| Cron | CronRegister, CronList, CronRemove |
| MCP 资源 | mcp__read_resource |
| MCP 工具 | 所有 `mcp__*` 工具 |
| 未来扩展 | 新增的低频工具 |

**判定逻辑**（`is_deferred_tool`）：

```rust
fn is_deferred_tool(tool_name: &str) -> bool {
    !CORE_TOOLS.contains(tool_name)
        && !META_TOOLS.contains(tool_name)  // SearchExtraTools, ExecuteExtraTool
}
```

- 不需要 `alwaysLoad` 字段：Core 白名单 + 元工具白名单覆盖所有需要始终加载的工具
- 新增内置工具默认为 Deferred，需要时手动加入 `CORE_TOOLS`

### 2. 架构集成

#### 2.1 集成方式：修改 ReActAgent 工具收集逻辑

不新增中间件，而是在 `ReActAgent.execute()` 的工具收集阶段增加过滤逻辑。原因：

- ToolSearch 是核心引擎行为，不属于横切关注点（中间件模式）
- 需要在 `all_tools` 构建后才能过滤，而中间件的 `collect_tools` 是构建 `all_tools` 的输入
- 需要将 deferred tools 存入共享索引供 `ExecuteExtraTool` 查找，这个索引需要在执行器层面持有

#### 2.2 ToolSearchMiddleware（在 `peri-middlewares` 中）

虽然核心逻辑在执行器，但搜索和执行两个元工具的实现放在 middlewares crate：

```
peri-middlewares/src/tool_search/
├── mod.rs              # 模块入口，导出公共接口
├── core_tools.rs       # CORE_TOOLS 定义 + is_deferred_tool()
├── search_tool.rs      # SearchExtraTools 实现
├── execute_tool.rs     # ExecuteExtraTool 实现
├── tool_index.rs       # TF-IDF 索引构建和搜索
└── keyword_search.rs   # 关键词搜索
```

#### 2.3 数据流

```
                    ReActAgent.execute()
                           │
            ┌──────────────┼──────────────┐
            ▼              ▼              ▼
     provider_tools   middleware_tools   manual_tools
            │              │              │
            └──────────────┼──────────────┘
                           ▼
                  all_tools: HashMap<String, &dyn BaseTool>
                           │
                  ┌────────┴────────┐
                  ▼                 ▼
           core_tools          deferred_tools
           (直接传给 LLM)      (存入 ToolSearchIndex)
                  │                 │
                  ▼                 ▼
          [Core Tools]      ┌───────┴───────┐
          + SearchExtra     │  ToolSearchIndex │
          + ExecuteExtra    │  (名称+描述+    │
                           │   schema 索引)  │
                           └───────┬───────┘
                                   │
                    ┌──────────────┼──────────────┐
                    ▼              ▼              ▼
            模型调用            模型调用        模型调用
         SearchExtraTools   ExecuteExtraTool   直接调用
                    │              │           Core Tool
                    ▼              ▼
              搜索返回         查找 deferred
              工具详情         tool → invoke
```

### 3. ReActAgent 修改

#### 3.1 新增字段

```rust
pub struct ReActAgent {
    // ... 现有字段 ...
    /// 延迟工具索引（由 ToolSearchIndex 持有）
    tool_search_index: Arc<ToolSearchIndex>,
    /// 是否启用 ToolSearch（始终 true，预留扩展点）
    tool_search_enabled: bool,
}
```

#### 3.2 工具收集修改（executor.rs）

在 `all_tools` 构建完成后，增加过滤和索引构建：

```rust
// 现有逻辑：收集所有工具
let mut all_tools: HashMap<String, &dyn BaseTool> = ...;

// 新增：分离 core 和 deferred
let (core_refs, deferred_refs): (Vec<_>, Vec<_>) = all_tools.values()
    .copied()
    .partition(|t| !is_deferred_tool(t.name()));

// 新增：注册元工具（需要引用 deferred 索引）
let search_tool = SearchExtraTools::new(Arc::clone(&self.tool_search_index));
let execute_tool = ExecuteExtraTool::new(Arc::clone(&self.tool_search_index));
// 将元工具也加入 core_refs

// 构建延迟工具索引
self.tool_search_index.build(deferred_refs);

// 传给 LLM 的工具列表（会话期间不变）
let tool_refs: Vec<&dyn BaseTool> = core_refs;
```

#### 3.3 工具执行修改

`ExecuteExtraTool` 的 invoke 需要能查找并调用 deferred tools。关键：`all_tools` HashMap 中仍然包含所有工具（包括 deferred），只是传给 LLM 的 `tool_refs` 被过滤了。因此 `ExecuteExtraTool` 需要持有 deferred tools 的引用。

方案：`ToolSearchIndex` 内部持有 `HashMap<String, &dyn BaseTool>` 的 deferred 子集，`ExecuteExtraTool` 通过索引查找并调用。

### 4. 搜索引擎设计

#### 4.1 ToolSearchIndex

```rust
pub struct ToolSearchIndex {
    /// deferred tools 的可执行引用
    tools: RwLock<HashMap<String, Arc<dyn BaseTool>>>,
    /// TF-IDF 索引
    tfidf_index: RwLock<TfIdfIndex>,
}
```

- `build()`: 从 deferred tools 构建 TF-IDF 索引
- `search(query, limit)`: 混合搜索，返回排序后的工具信息
- `get_tool(name)`: 按名称获取工具引用（供 ExecuteExtraTool 使用）
- `list_names()`: 返回所有 deferred tool 名称（供 system prompt 注入）

#### 4.2 混合搜索算法

```
最终分数 = 关键词分数 × 0.4 + TF-IDF 分数 × 0.6
```

**TF-IDF 搜索**（`tool_index.rs`）：

- 对三个字段加权：name (3.0)、description (2.5)、searchHint (1.0)
- CJK 分词：按字符级别分割（简单实现，不需要 NLP 库）
- ASCII 分词：按空格、下划线、连字符分割
- 计算词频 (TF) × 逆文档频率 (IDF)，余弦相似度排序

**关键词搜索**（`keyword_search.rs`）：

- 工具名解析：CamelCase 分词（`CronCreate` → `["cron", "create"]`）、MCP 前缀拆解（`mcp__slack__send_message` → `["slack", "send", "message"]`）
- 查询词与工具名/描述的精确匹配加权
- `+` 前缀表示必选词

#### 4.3 SearchExtraTools 工具

**输入参数**：

```json
{
  "query": "string, 搜索关键词或自然语言描述"
}
```

**输出**：按相关性排序的工具信息列表，每个包含 name、description、parameters（完整 JSON Schema）。

```json
{
  "results": [
    {
      "name": "mcp__slack__send_message",
      "description": "Send a message to a Slack channel",
      "parameters": { "...full schema..." }
    }
  ],
  "total_available": 15
}
```

#### 4.4 ExecuteExtraTools 工具

**输入参数**：

```json
{
  "tool_name": "string, deferred tool 的名称",
  "params": { "... 工具参数 ..." }
}
```

**执行流程**：

1. 从 `ToolSearchIndex` 查找目标工具
2. 调用目标工具的 `invoke(params)` 方法
3. 返回结果（与直接调用完全等价）
4. 权限检查由 HITL middleware 的 `before_tool` 链自动处理（因为 `ExecuteExtraTool` 本身在 `all_tools` 中，middleware 链会拦截它；但实际权限需要透传给目标工具）

**权限透传设计**：`ExecuteExtraTool` 的 `before_tool` 返回时，HITL middleware 检查到的是 `ExecuteExtraTool` 本身，而非目标 deferred tool。两种方案：

- **方案 A**（推荐）：`ExecuteExtraTool` 的参数中包含目标工具名和完整参数，HITL 弹窗展示时显示目标工具信息（名称 + 参数），用户审批的是实际操作而非元操作
- **方案 B**：`ExecuteExtraTool` 绕过 HITL，权限检查在 invoke 内部手动调用目标工具的权限逻辑

选择方案 A，在 `before_tool` 阶段通过修改 `ToolCall` 展示信息实现，不修改 middleware 链。

### 5. System Prompt 注入

在 system prompt 末尾注入一段轻量提示，列出所有 deferred tools 的名称和简短描述（不含完整 schema）：

```
## Available Deferred Tools

The following tools are available but not loaded by default. Use SearchExtraTools to discover their full schema, then use ExecuteExtraTool to invoke them.

- CronRegister: Register a scheduled task
- CronList: List all scheduled tasks
- CronRemove: Remove a scheduled task
- mcp__read_resource: Read MCP resources
- mcp__slack__send_message: Send a message to Slack
- mcp__github__create_issue: Create a GitHub issue
...

To use a deferred tool:
1. Call SearchExtraTools with a relevant query to get the tool's full schema
2. Call ExecuteExtraTool with the tool name and parameters
```

注入方式：通过 `ToolSearchMiddleware` 的 `before_agent` 钩子 prepend system message。此中间件注册在 MCP 中间件之后，确保能获取完整的 MCP 工具列表。

### 6. ToolSearchMiddleware

虽然核心过滤逻辑在 `ReActAgent`，但元工具注册和 system prompt 注入通过中间件完成：

```rust
pub struct ToolSearchMiddleware {
    tool_search_index: Arc<ToolSearchIndex>,
}

impl ToolSearchMiddleware {
    pub fn new(tool_search_index: Arc<ToolSearchIndex>) -> Self;
}

#[async_trait]
impl<S: State> Middleware<S> for ToolSearchMiddleware {
    fn name(&self) -> &str { "ToolSearch" }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(SearchExtraTools::new(Arc::clone(&self.tool_search_index))),
            Box::new(ExecuteExtraTool::new(Arc::clone(&self.tool_search_index))),
        ]
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 注入 deferred tools 列表到 system prompt
        let deferred_list = self.tool_search_index.format_deferred_list();
        let prompt = format_deferred_tools_prompt(&deferred_list);
        state.prepend_message(BaseMessage::system(prompt));
        Ok(())
    }
}
```

### 7. 中间件链执行顺序更新

```
 1. AgentDefineMiddleware
 2. AgentsMdMiddleware
 3. SkillsMiddleware
 4. SkillPreloadMiddleware
 5. FilesystemMiddleware
 6. TerminalMiddleware
 7. WebMiddleware
 8. TodoMiddleware
 9. CronMiddleware
10. HumanInTheLoopMiddleware
11. SubAgentMiddleware
12. McpMiddleware
13. ToolSearchMiddleware        ← 新增
```

`ToolSearchMiddleware` 必须在 `McpMiddleware` 之后，因为需要 MCP 工具已经注册到 `all_tools` 中才能正确分类。

### 8. HITL 集成

`ExecuteExtraTool` 的 HITL 审批展示：

- 工具名称显示：`ExecuteExtraTool → mcp__slack__send_message`
- 参数展示：直接展示目标工具的参数（`params` 字段内容）
- 审批逻辑：`ExecuteExtraTool` 本身在 HITL 默认拦截列表中（如果加入），或者将目标工具名传递给 HITL 判断

推荐方案：`ExecuteExtraTool` 的 HITL 审批判断基于目标工具名。在 `ExecuteExtraTool` 的参数中包含 `tool_name`，HITL middleware 可以检查这个字段来决定是否需要审批。如果目标工具是 `mcp__slack__send_message`（`mcp__*` 前缀），则默认需要审批。

实现方式：`ExecuteExtraTool` 实现一个 `target_tool_name()` 方法或通过参数解析，HITL middleware 在 `before_tool` 中特殊处理 `ExecuteExtraTool`。

## 实现要点

### 关键技术决策

1. **过滤发生在执行器而非中间件**：ToolSearch 是核心引擎行为，直接修改 `ReActAgent.execute()` 的工具收集逻辑。中间件仅负责元工具注册和 prompt 注入。

2. **`all_tools` 保留完整工具集**：`HashMap<String, &dyn BaseTool>` 包含所有工具（core + deferred + meta），保证 `ExecuteExtraTool` 可以通过索引查找并执行任何 deferred tool。传给 LLM 的 `tool_refs: Vec<&dyn BaseTool>` 才是过滤后的列表。

3. **Arc 包装 deferred tools**：由于 `all_tools` 中的引用生命周期与 `execute()` 绑定，`ToolSearchIndex` 需要持有 `'static` 引用。方案：在工具收集阶段将 deferred tools clone 为 `Arc<dyn BaseTool>` 存入索引。

4. **TF-IDF 自实现**：不引入外部 NLP 依赖，使用简单的 CJK 字符级分词 + ASCII 空格分词，足以满足工具搜索场景。

### 难点

- **HITL 权限透传**：`ExecuteExtraTool` 需要让 HITL 正确展示目标工具信息。需要在 HITL middleware 中增加对 `ExecuteExtraTool` 的特殊处理逻辑。
- **工具生命周期**：`ToolSearchIndex` 在 `execute()` 期间持有 deferred tools 的 `Arc<dyn BaseTool>` 引用，需要确保这些引用在整个 ReAct 循环中有效。
- **SearchExtraTools 的 searchHint**：MCP 工具没有 `searchHint` 字段，仅依赖 name 和 description。未来可通过 MCP 配置扩展。

### 依赖

- 无新增外部 crate 依赖
- TF-IDF 实现为纯 Rust 代码，使用 `std::collections` 和基础数学运算

## 约束一致性

本方案与 `spec/global/constraints.md` 完全一致：

- **Middleware Chain 模式**：通过 `ToolSearchMiddleware` 提供元工具和 prompt 注入，符合横切关注点解耦原则
- **BaseTool trait**：`SearchExtraTools` 和 `ExecuteExtraTool` 均实现 `BaseTool` trait，工具接口统一
- **异步优先**：搜索和执行均为 async 实现
- **消息不可变历史**：deferred tools 列表通过 `prepend_message` 注入 system prompt，不修改历史
- **HITL 安全约束**：`ExecuteExtraTool` 的权限透传确保 MCP 工具仍受 HITL 保护

无架构偏离。

## 验收标准

- [ ] Core Tools 白名单定义完整（12 个内置工具），其余工具均为 Deferred
- [ ] `SearchExtraTools` 元工具：输入 query，返回匹配的 deferred tool 列表（含完整 schema）
- [ ] `ExecuteExtraTools` 元工具：输入 tool_name + params，代理执行 deferred tool 并返回结果
- [ ] API tools 数组仅包含 core tools + 2 个元工具，会话期间保持稳定
- [ ] System prompt 末尾注入 deferred tools 名称+描述列表（不含完整 schema）
- [ ] TF-IDF + 关键词混合搜索正常工作，CJK 分词支持
- [ ] HITL 正确拦截 `ExecuteExtraTool`，展示目标工具名称和参数
- [ ] 单元测试：搜索索引构建、混合搜索排序、ExecuteExtraTool 代理执行
- [ ] MCP 工具（`mcp__*`）全部为 Deferred，通过搜索发现和代理执行
- [ ] 无新增外部 crate 依赖
