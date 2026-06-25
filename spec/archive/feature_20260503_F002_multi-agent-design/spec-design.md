# Feature: 20260503_F002 - multi-agent-design

## 需求背景

当前 peri 的多 agent 系统（SubAgentMiddleware + SubAgentTool）存在两个不足：

1. **无 Fork 路径**：所有子 agent 都是独立上下文（Normal 路径），无法复用父 agent 的消息历史。对于需要继承完整上下文的任务（如继续多文件重构、延续父 agent 的分析），Normal 路径需要将所有背景信息塞入 prompt，导致 token 浪费和 prompt 质量下降。
2. **System Prompt 指导不足**：当前 `11_subagent.md` 仅 21 行，缺少"何时不用 Agent""如何写好 prompt""可用 agent 列表"等关键指导，LLM 的委派决策和 prompt 质量不稳定。

参考 Claude Code 的多 agent 设计（Fork + Normal 双路径 + 丰富的 prompt 指导），对齐用户体验。

## 目标

- **Fork 路径**：子 agent 可继承父 agent 的完整消息历史 + system prompt + 工具集，实现 Anthropic Prompt Cache 命中
- **Prompt 指导优化**：扩写 System Prompt 中的 Agent 工具指导段落，覆盖使用时机、prompt 写作技巧、示例
- **双路径并存**：Normal 路径（subagent_type 指定）保持不变，Fork 路径（fork: true）为新增能力

## 方案设计

### 1. Fork 路径设计

#### 1.1 触发条件

| 参数 | Normal 路径 | Fork 路径 |
|------|------------|-----------|
| `fork` | `false` / 缺省 | `true` |
| `subagent_type` | 必填（指定 agent 类型） | 忽略（不读 agent 定义文件） |
| `prompt` | 必填 | 必填 |

Fork 路径通过显式 `fork: true` 参数触发，不改变现有 `subagent_type` 缺省报错的行为。

#### 1.2 消息构建

```
Fork 子 agent 的 AgentState.messages = [
    父 state.messages 的完整拷贝（Human/Ai/System/Tool 全部）,
    新增一条 Human 消息（包含 fork 指令 + prompt）
]
```

具体流程：

1. `SubAgentTool.invoke()` 检测 `fork == true`
2. 从父 agent 的 `AgentState` 获取消息历史（需新增接口传递 `Arc<Vec<BaseMessage>>`）
3. 深拷贝消息列表（避免修改父状态）
4. 追加一条 Human 消息：`<fork_directive>\n{rules}\n</fork_directive>\n\n{prompt}`
5. 创建子 `AgentState`，用拷贝+追加后的消息列表初始化
6. 子 agent 使用与父相同的 system prompt 和工具集

#### 1.3 Fork 指令模板

参考 Claude Code 的 `buildChildMessage()`，但简化为：

```xml
<fork_directive>
You are a forked agent continuing from the parent conversation.
You have full access to the conversation history above.

RULES:
1. Do NOT spawn sub-agents — execute directly using your tools
2. Do NOT ask questions — act on the directive below
3. Stay strictly within your assigned scope
4. Report structured facts, then stop
5. Keep your response under 500 words unless specified otherwise

Output format:
  Scope: <your assigned scope in one sentence>
  Result: <the answer or key findings>
  Key files: <relevant file paths>
  Files changed: <list if you modified files>
</fork_directive>

{prompt}
```

#### 1.4 工具集继承

Fork 子 agent 继承父的**全量工具集**（包含 Agent 工具本身）。防递归通过 fork 指令中的规则约束（规则 1），而非硬编码排除。

> 设计决策：Fork 子 agent 保留 Agent 工具是为了工具列表与父完全一致，最大化 Anthropic tools-block 的 Prompt Cache 命中。如果硬编码排除 Agent，工具列表与父不同，tools-block cache 会 miss。

#### 1.5 System Prompt 继承

Fork 子 agent 直接使用父的已渲染 system prompt（通过 `system_builder` 获取），不重新构建。保证 system prompt 字节级一致，最大化 cache 命中。

#### 1.6 与 Normal 路径的代码分支

```rust
// SubAgentTool.invoke() 伪代码
async fn invoke(&self, input: Value) -> Result<String, ...> {
    let prompt = input["prompt"].as_str()...;
    let is_fork = input["fork"].as_bool().unwrap_or(false);

    if is_fork {
        self.invoke_fork(prompt, parent_messages).await
    } else {
        // 现有 Normal 路径不变
        self.invoke_normal(input).await
    }
}
```

### 2. 父消息历史传递

#### 2.1 当前问题

当前 `SubAgentTool` 无法获取父 agent 的消息历史——它在 `invoke()` 时只收到 `input: Value`。

#### 2.2 解决方案

在 `SubAgentTool` 新增字段：

```rust
pub struct SubAgentTool {
    // ... 现有字段 ...
    /// 父 agent 消息历史的共享引用（Fork 路径使用）
    parent_messages: Option<Arc<parking_lot::RwLock<Vec<BaseMessage>>>>,
}
```

通过 `with_parent_messages()` builder 设置。在 `peri-tui/src/app/agent.rs` 组装 SubAgentTool 时，将 `AgentState.messages` 的共享引用传入。

> 备选方案：在 `invoke()` 签名中增加 `context: &ToolContext` 参数（包含 messages、cwd 等），但这需要修改 `BaseTool` trait，影响面过大。共享引用方案侵入性最小。

### 3. System Prompt Agent 工具指导优化

#### 3.1 当前状态

`11_subagent.md` 共 21 行，包含：

- When to use（3 条）
- Delegation guidelines（4 条）
- Context isolation（1 段）

#### 3.2 优化方案

重写 `11_subagent.md`，参考 Claude Code 的 `prompt.ts` 结构，分为以下段落：

```markdown
# SubAgent Delegation

## 概述
（保留现有内容，简要说明 Agent 工具用途）

## Available agent types
（动态注入：扫描 .claude/agents/ 目录，列出 agent 名称 + description）
- {agent_type}: {whenToUse} (Tools: {tools_description})

## When to use
（扩展：增加更多场景）

## When NOT to use
（新增：参考 CC 的 "When NOT to use" 段落）
- 读取特定文件 → 用 Read 工具
- 搜索类定义 → 用 Grep 工具
- 在 2-3 个文件中搜索 → 用 Read 工具
- 不相关的任务

## Writing the prompt
（新增：参考 CC 的 "Writing the prompt" 段落）
- 像 briefing 一个刚进来的聪明同事
- 解释目标和原因
- 已排除的可能性
- 需要简短回复时明确说明
- "Never delegate understanding" 原则

## Fork mode (fork: true)
（新增）
- 继承完整上下文，prompt 是 directive 而非 briefing
- 不重新解释背景
- 明确 scope 边界

## Usage notes
（新增：参考 CC 的 usage notes）
- 包含简短 description
- 结果不直接对用户可见，需向用户转述
- 清楚告知 agent 是写代码还是只做研究
- 可并发启动多个 agent（单消息多 tool_use）

## Examples
（新增：2-3 个示例，参考 CC 的 examples）
```

#### 3.3 动态 Agent 列表注入

当前 agent 列表硬编码在 system prompt 中（如 CLAUDE.md 的 "你可以使用 Agent 工具委派子任务给以下专门 Agent" 段落）。改为：

1. `11_subagent.md` 中放置占位符 `{available_agents}`
2. 系统提示词构建函数在运行时扫描 `.claude/agents/` 目录
3. 解析每个 agent 定义的 frontmatter（name + description + tools）
4. 格式化为 `- {agent_type}: {description} (Tools: {tools})` 列表
5. 替换占位符

> 这与 Claude Code 的 `shouldInjectAgentListInMessages()` 机制类似。CC 为了避免动态列表 bust tools-block cache，将 agent 列表放在 attachment message 而非 tool description 中。我们暂不做此优化，先放在 system prompt 中。

### 4. 工具描述（AGENT_DESCRIPTION）优化

同步更新 `SubAgentTool` 的 `AGENT_DESCRIPTION` 常量，增加 fork 参数说明和更完整的使用指导。

## 实现要点

### 4.1 Fork 路径实现

**核心变更文件**：

- `peri-middlewares/src/subagent/tool.rs`：新增 `invoke_fork()` 方法
- `peri-middlewares/src/subagent/mod.rs`：新增 `with_parent_messages()` builder
- `peri-tui/src/app/agent.rs`：组装时传入 `parent_messages`

**关键技术决策**：

- 消息深拷贝：`BaseMessage` 已实现 `Clone`，使用 `clone_from()` 或手动遍历拷贝
- Fork 子 agent 的 `AgentState` 初始化时需要设置已包含的消息列表（当前 `AgentState::new()` 创建空状态，需新增 `with_messages()` 方法或 `new_with_messages()` 构造器）
- 事件透传：Fork 子 agent 与 Normal 子 agent 一样共享父事件处理器

### 4.2 Prompt 指导优化

**核心变更文件**：

- `peri-tui/prompts/sections/11_subagent.md`：重写
- `peri-tui/src/prompt.rs`：系统提示词构建函数增加 agent 列表扫描和占位符替换逻辑
- `peri-middlewares/src/subagent/tool.rs`：更新 `AGENT_DESCRIPTION`

### 4.3 防递归

Fork 子 agent 的防递归策略：通过 fork 指令规则约束（"Do NOT spawn sub-agents"），而非硬编码排除 Agent 工具。理由：

1. 保持工具列表与父完全一致，最大化 Prompt Cache 命中
2. LLM 在 fork 场景下有足够的上下文理解指令约束
3. 即使 LLM 违反指令调用了 Agent，子 agent 的 Normal 路径仍有防递归（排除 Agent 工具）

### 4.4 依赖

- `AgentState` 需支持从已有消息列表初始化（新增构造器）
- `SubAgentTool` 需新增 `parent_messages` 字段
- 无新增外部 crate 依赖

## 约束一致性

- **Middleware Chain 模式**：Fork 路径跳过 agent 定义文件解析，子 agent 的 middleware 链由代码硬编码（AgentsMd → Skills → SkillPreload → Todo），与 Normal 路径一致
- **工具系统**：Fork 子 agent 使用 `register_tool` 注册全量父工具（无过滤），与现有 `register_tool` 优先级最高的约束一致
- **消息不可变历史**：Fork 路径拷贝父消息后追加，不修改原始历史，保持不可变约束
- **事件驱动 TUI 通信**：Fork 子 agent 与 Normal 子 agent 共享事件处理器，无额外 channel
- **编码规范**：遵循 Rust 2021 edition + async-trait + tracing 日志

## 验收标准

- [ ] `fork: true` 参数可触发 Fork 路径，子 agent 继承完整父消息历史
- [ ] Fork 子 agent 的 system prompt 与父字节级一致（验证 cache 命中）
- [ ] Fork 子 agent 的工具集与父完全一致（包含 Agent 工具）
- [ ] Fork 子 agent 遵循指令模板（scope/result/key files 格式）
- [ ] `11_subagent.md` 重写，包含 When NOT to use / Writing the prompt / Fork mode / Usage notes / Examples 段落
- [ ] 动态 agent 列表注入到 system prompt
- [ ] `AGENT_DESCRIPTION` 更新，包含 fork 参数说明
- [ ] Normal 路径（subagent_type 指定）行为不变，所有现有测试通过
- [ ] 新增 Fork 路径单元测试（消息继承、工具集一致性、防递归）
- [ ] 新增集成测试：headless 模式下验证 Fork 子 agent 事件透传
