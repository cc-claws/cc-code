# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Rust Agent 框架，包含 **6 个 Workspace Crate**：

- **`rust-create-agent`**：核心框架——ReAct 循环执行器、Middleware trait、LLM 适配器、工具系统、线程持久化（SQLite）、遥测（OTel）
- **`rust-agent-middlewares`**：中间件实现（文件系统、终端、HITL、SubAgent、Skills、Todo、Cron、MCP 等）
- **`perihelion-widgets`**：独立 widget crate（BorderedPanel/ScrollableArea/SelectableList 等 11 组件），零内部依赖，仅依赖 ratatui + pulldown-cmark
- **`rust-agent-tui`**：交互式 TUI 应用，基于 ratatui
- **`langfuse-client`**：Langfuse 遥测客户端
- **`acpx-g`**：DAG workflow engine——YAML 定义工作流、Web API、SQLite 持久化

核心价值：高兼容（复用 `.claude/` 配置零迁移）、可插拔（中间件模式按需组合）、生产可用（异步+OTel 追踪）。

`rmcp` crate（v1.6.0）通过 `[patch.crates-io]` 指向本地 `rust-mcp-patch/`，修复部分 MCP 服务器对 `notifications/initialized` 返回 HTTP 200 + 空 body 导致的 `UnexpectedContentType(None)` 错误。上游发布修复后删除补丁目录即可。

## 开发命令

```bash
cargo build                          # 构建所有 crate
cargo build -p rust-create-agent     # 构建指定 crate
cargo run -p rust-agent-tui          # 运行 TUI
cargo run -p rust-agent-tui -- -a    # 启用 HITL 审批模式
cargo test                           # 全量测试
cargo test -p rust-create-agent --lib -- test_name  # 运行单个测试
lefthook install                     # 安装 git hooks
lefthook run pre-commit              # 手动运行 pre-commit（fmt/check/clippy）
```

## Workspace 依赖关系

```
rust-create-agent (核心框架，零内部依赖)
    ↑
rust-agent-middlewares (中间件实现)
    ↑
perihelion-widgets (零内部依赖，仅依赖 ratatui + pulldown-cmark)
    ↑
rust-agent-tui (TUI 应用，依赖 widgets + middlewares)

langfuse-client (遥测客户端，独立)
acpx-g (DAG workflow engine，独立)
```

## 数据流

**ReAct 循环**（`rust-create-agent`）：AgentInput → chain.collect_tools → chain.run_before_agent → loop(max_iterations=50) { LLM generate_reasoning → [有工具调用] before_tool → 并发执行 → after_tool → emit events | [最终回答] → emit TextChunk + StateSnapshot → after_agent }。

**TUI 异步通信**（`rust-agent-tui`）：submit_message() 通过 mpsc(32) AgentEvent channel 驱动 agent task，poll_agent() 每帧 try_recv 更新 UI。审批事件通过 mpsc(4) ApprovalEvent channel 转发，弹窗确认通过 oneshot 解除。渲染管道独立线程：RenderEvent → RenderCache(RwLock) → terminal.draw()。

**系统提示词**：`build_system_prompt(overrides, cwd, features)` 合成，段落文件位于 `rust-agent-tui/prompts/sections/`（静态 01-08 + Feature-gated 10-13 + 动态覆盖块）。`PromptFeatures` 控制条件段落注入（hitl/subagent/cron/skills）。

**消息类型**：`BaseMessage` 四变体（Human/Ai/System/Tool），`ContentBlock` 七变体（Text/Image/Document/ToolUse/ToolResult/Reasoning/Unknown）。

**LLM 适配层**：`BaseModel` trait（OpenAI/Anthropic 实现）→ `BaseModelReactLLM`（适配为 `ReactLLM`）。`RetryableLLM<L>` 装饰器提供指数退避重试。

**Thinking/推理模式**：`ThinkingConfig`（`rust-agent-tui/src/config/types.rs`）控制推理参数，Anthropic 用 `thinking + output_config.effort`，OpenAI 用 `reasoning_effort`。`budget_tokens` 最小 1024，`max_tokens` 必须 > `budget_tokens`。

**事件系统**：核心层 `AgentEvent` 11 种变体（AiReasoning/TextChunk/ToolStart/ToolEnd/StepDone/StateSnapshot/MessageAdded/LlmCallStart/LlmCallEnd/LlmRetrying/BackgroundTaskCompleted）。TUI 层扩展 Done/Error/ApprovalNeeded/AskUserBatch 等。

**消息管线**：`MessagePipeline` 统一管理消息状态，`PipelineAction` 枚举描述 UI 变更，`reconcile_tail()` 在 Done/Interrupted 时触发尾部重建。

## Tool Search 延迟加载

非核心工具（MCP 工具、Cron 工具等）从 LLM 工具列表中移除，通过 `SearchExtraTools` 按需发现、`ExecuteExtraTool` 代理执行。

**核心工具（12 个，始终发送给 LLM）**：Read/Write/Edit/Glob/Grep/folder_operations/Bash/WebFetch/WebSearch/Agent/AskUserQuestion/TodoWrite

**架构**：`ReActAgent.with_tool_filter(is_deferred_tool)` 过滤 `tool_refs`（LLM 可见），`with_shared_tools(Arc<RwLock<HashMap>>)` 将所有工具写入共享注册表供 `ExecuteExtraTool` 代理执行。`ToolSearchMiddleware` 注册两个元工具，在 `before_agent` 时从 `shared_tools` 读取 deferred 工具自动构建 `ToolSearchIndex`，然后注入延迟工具列表到 system prompt。

**[TRAP]** `Box<dyn BaseTool>` 不能直接转为 `Arc<dyn BaseTool>`（Rust 不支持 unsized trait object 的所有权转移）。executor 使用 `box_to_arc()` 通过中间 `ToolWrapper` struct（持有 `ManuallyDrop<Box<dyn BaseTool>>`，实现 `BaseTool` trait 透传）安全地完成转换。**绝不能使用 `Box::into_raw` + `Arc::from_raw`**——`Box` 指向 `T`，`Arc` 指向 `ArcInner<T>`，布局不同会导致 UB。

**[TRAP]** `std::sync::RwLockReadGuard` 不是 `Send`，在 async 函数中不能跨 `.await` 持有。使用 `parking_lot::RwLock`（guard 是 `Send`）或在 await 前 clone Arc。

## HITL 审批

默认需审批工具：`Bash`、`folder_operations`、`Agent`、`Write`、`Edit`、`delete_*`、`rm_*`、`mcp__*`、`WebFetch`、`WebSearch`。

## Skills

搜索顺序：`~/.claude/skills/` → `skillsDir`（`~/.peri/settings.json`） → `./.claude/skills/` → 插件 skills，同名先到先得。

**插件 skills 集成**：`SkillsMiddleware.with_extra_dirs()` 是插件 skills 路径注入入口，修改 SkillsMiddleware 搜索逻辑时必须保留此扩展点。

每个 skill 是子目录，内含 `SKILL.md`（YAML frontmatter: `name`, `description`）。输入 `/` 前缀触发 Skills 浮层，Tab 导航，Enter 补全为 `/skill-name`。

**Frontmatter 解析**：skill 和插件命令的 Markdown 文件使用 `gray_matter` crate（YAML engine）解析 frontmatter。新增命令解析时必须复用 `Matter::<YAML>::new()` 模式，不手动解析 `---` 分隔符。

## Fork 模式

子 agent 继承父 agent 的完整消息历史 + system prompt + 工具集，通过 fork directive 规则约束防递归。

## Background Agent 模式

后台 agent 通过独立事件通道 + 通知通道完成（不共享父 event_handler），最大 3 个并发。父 agent Done 后若有后台任务，自动保持通道存活并在最后一个完成时触发 continuation。

## 中间件链执行顺序

```
1. AgentDefineMiddleware      ← 解析 agent 定义，设置 model/maxTurns 等覆盖
2. AgentsMdMiddleware         ← 读 CLAUDE.md/AGENTS.md 注入 system
3. SkillsMiddleware           ← Skills 摘要注入 system
4. SkillPreloadMiddleware     ← #skill-name 全文注入（fake tool 序列）
5. FilesystemMiddleware       ← 6 个文件系统工具（Read/Write/Edit/Glob/Grep/folder_operations）
6. TerminalMiddleware         ← Bash 工具
7. WebMiddleware             ← WebFetch/WebSearch 工具
8. TodoMiddleware             ← after_tool 解析 TodoWrite
9. CronMiddleware             ← Cron 调度工具
10. HumanInTheLoopMiddleware   ← before_tool 拦截敏感工具
11. SubAgentMiddleware        ← Agent 工具
12. McpMiddleware             ← MCP 工具和资源注入（仅 pool 初始化成功时注册）
[ReActAgent.with_system_prompt()] ← system prompt prepend
```

手动注册工具（`register_tool`）优先级最高，覆盖同名中间件工具。

## 上下文压缩

Token 累积达到上下文窗口阈值（默认 85%）时自动触发：

1. **Micro-compact**：零 API 调用，清除可压缩工具结果/图片/文档
2. 如仍超限 → **Full Compact**：LLM 生成 9 段结构化摘要替换历史
3. **Re-inject**：重新注入最近文件 + Skills

## MCP 中间件

通过 `McpMiddleware` 将外部 MCP 服务器提供的工具和资源注入 ReAct 循环。基于 `rmcp` crate 实现。

**配置加载**：`McpConfig::load_merged_config(cwd)` 合并两层配置：

| 来源 | 路径 | 说明 |
|------|------|------|
| 全局 | `~/.peri/settings.json` 的 `config.mcpServers` 或 `mcpServers` | 所有项目共享 |
| 项目级 | `{cwd}/.mcp.json` 的 `mcpServers` | 项目特定，同名覆盖全局 |

**服务器配置**（`McpServerConfig`）：

| 字段 | 说明 |
|------|------|
| `command` + `args` + `env` | stdio 传输：启动子进程 |
| `url` + `headers` | Streamable HTTP 传输：连接远程服务器 |
| `oauth` | OAuth 2.0 认证配置（`authorizationUrl`/`tokenUrl`/`clientId`/`clientSecret`/`scopes`） |
| `disabled` | 设为 `true` 禁用该服务器（TUI 面板可切换） |
| `${VAR}` 占位符 | 所有字符串字段自动展开环境变量 |

**工具命名**：`mcp__{server_name}__{tool_name}`，HITL 对 `mcp__` 前缀的工具默认需审批。

**插件 MCP 命名空间**：插件定义的 MCP 服务器使用 `{plugin_name}__{server_name}` 前缀命名空间，`ConfigSource::Plugin` 标记来源，合并后工具名为 `mcp__{plugin_name}__{server_name}__{tool_name}`。新增 MCP 配置合并逻辑时必须遵循此前缀规则。

**插件 MCP Env 展开**：必须在合并之前执行（per-plugin 独立上下文），避免不同插件的同名 env var 交叉污染。`expand_server_config_with_context` 在 Step 2 立即调用，Plugin 来源的 server config 在合并到 `merged.mcp_servers` 之前已完成 env 变量替换。

**ClaudeSettings 反序列化陷阱 [TRAP]**：`extraKnownMarketplaces` 字段在 Claude Code 中可能是对象格式 `{"name": {source, ...}}`。`deserialize_known_marketplaces` 自定义反序列化器需同时支持对象和数组两种格式，否则整个 `ClaudeSettings` 解析失败会导致 `load_enabled_plugins` 静默失败、插件 MCP 服务器全部丢失。同样，`enabledPlugins` 也有对象/数组两种格式（已有 `deserialize_enabled_plugins` 处理）。**`enabledPlugins` 写入必须用对象格式** `{"id": true}`，数组格式会导致 Claude Code 报 `Expected record, but received array`。

**Plugin Sources 旁路表**：`load_merged_config_full` 返回 `(McpConfigFile, HashMap<String, String>)`，其中 `HashMap` 的 key 格式为 `"plugin:{name}:{server}"`（与合并后 config 中的 server name 一致），value 为 `"name@marketplace"`。`LoadedPlugin` 当前没有 `marketplace` 字段，marketplace 信息只能从 `InstalledPlugin`（`installed_plugins.json`）中获取。`load_installed_plugins` 读取路径需从 `claude_home` 参数推导（`claude_home/plugins/installed_plugins.json`），不能使用 `None`（会读 `~/.claude/plugins/installed_plugins.json`）。

**资源读取**：`mcp__read_resource` 工具，参数 `server_name` + `uri`，120 秒超时。

**连接池**（`McpClientPool`）：

- 首次 agent 启动时惰性初始化（`agent_ops.rs`），后续复用
- stdio 连接超时 10 秒，HTTP 连接超时 30 秒
- 连接失败的 server 记录为 `Failed` 状态，不影响其他 server
- App 退出时调用 `pool.shutdown()` 优雅关闭所有连接

**代码结构**（`rust-agent-middlewares/src/mcp/`）：

| 文件 | 职责 |
|------|------|
| `config.rs` | 配置加载、合并、`${VAR}` 展开 |
| `transport.rs` | 传输层工厂（stdio / StreamableHTTP） |
| `client.rs` | 连接池管理、HTTP headers 注入 |
| `tool_bridge.rs` | MCP 工具 → `BaseTool` 桥接 |
| `resource_tool.rs` | MCP 资源读取工具 |
| `middleware.rs` | `Middleware` trait 实现，`collect_tools` 注入 |

## SubAgents（子 Agent 委派）

`Agent` 工具允许 LLM 将子任务委派给 `.claude/agents/{agent_id}/agent.md` 定义的专门 agent 执行。插件 agent 通过 `scan_agents_with_extra_dirs` 追加搜索路径，同名 agent_id 去重（项目级优先），系统提示词中的 agent 列表由 `prompt.rs` 的 `format_available_agents` 生成。

**工具过滤规则**：

- `tools` 字段为空 → 子 agent 继承所有父工具（排除 `Agent` 自身，防递归）
- `tools` 字段有值 → 仅保留允许列表中的工具
- `disallowedTools` 字段 → 额外排除指定工具

**返回值格式**：

```
[子 agent 执行了 N 个工具调用: tool1, tool2, tool3]

Final response text here
```

## TUI 命令

输入 `/` 前缀触发统一浮层，Tab 导航，Enter 补全，支持前缀唯一匹配。

| 命令 | 说明 |
|------|------|
| `/login` | 管理 Provider 配置 |
| `/model` | 模型选择面板 |
| `/model <alias>` | 直接切换（opus/sonnet/haiku） |
| `/history` | 历史对话浏览 |
| `/agents` | SubAgent 定义管理 |
| `/compact` | 上下文压缩 |
| `/clear` | 清空消息列表 |
| `/config` | 查看/编辑运行时配置 |
| `/cost` | 查看 token 用量和成本 |
| `/context` | 查看上下文窗口使用情况 |
| `/memory` | 管理持久化记忆 |
| `/help` | 命令列表 |

## TUI Headless 测试模式

`rust-agent-tui` 支持无真实终端的 headless 集成测试。

```rust
#[tokio::test]
async fn test_example() {
    let (mut app, mut handle) = App::new_headless(120, 30);

    // 必须在发送事件前注册监听
    let notified = handle.render_notify.notified();

    app.push_agent_event(AgentEvent::AssistantChunk("Hello".into()));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    notified.await;  // 等待渲染线程处理完成

    handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
    assert!(handle.contains("Hello"));
}
```

**注意事项：**

- `notified()` 必须在 `process_pending_events()` **之前**调用
- `AssistantChunk` 事件会发送 2 个 `RenderEvent`
- CJK 字符在 `TestBackend` 中有宽字符填充，断言应使用 ASCII 内容
- 测试位于 `rust-agent-tui/src/ui/headless.rs`

## 关键模式

```rust
// 组装 agent（系统提示词通过 with_system_prompt() 注入）
ReActAgent::new(BaseModelReactLLM::new(model))
    .max_iterations(50)
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    .register_tool(Box::new(AskUserTool::new(invoker)))
    .with_event_handler(Arc::new(FnEventHandler(move |ev| { tx.try_send(ev); })))
    .execute(AgentInput::text(input), &mut AgentState::new(cwd))
```

**SubAgent 委派：**

```rust
let parent_tools: Arc<Vec<Arc<dyn BaseTool>>> = Arc::new(
    FilesystemMiddleware::new().tools(cwd)
        .into_iter()
        .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn BaseTool>)
        .collect()
);
let llm_factory = Arc::new(move || {
    Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
});
let system_builder = Arc::new(|overrides: Option<&AgentOverrides>, cwd: &str| {
    build_system_prompt(overrides, cwd, PromptFeatures::detect())
});
ReActAgent::new(llm)
    .add_middleware(Box::new(
        SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory)
            .with_system_builder(system_builder)
    ))
```

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 模型名称 |
| `YOLO_MODE=true` | 默认行为，跳过 HITL 审批（不影响 AskUserQuestion） |
| `YOLO_MODE=false` | 启用 HITL 审批 |
| `RUST_LOG` | 日志级别（默认 `info`） |
| `RUST_LOG_FILE` | 日志文件路径 |
| `RUST_LOG_FORMAT=json` | 使用 JSON 格式输出日志 |
| `LANGFUSE_*` | Langfuse 追踪配置 |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry OTLP 导出端点 |

配置通过 `~/.peri/settings.json` 的 `env` 字段注入环境变量（已替代 .env 文件）。

## CLI 参数

| 参数 | 说明 |
|------|------|
| `-a, --approve` | 启用 HITL 审批（设置 `YOLO_MODE=false`） |

运行时 `Shift+Tab` 循环切换 5 级权限模式，`Alt+M` 循环切换模型（opus→sonnet→haiku）。

**多 session 分屏**：支持多个 agent session 并列分屏显示，外层彩色边框指示当前聚焦 session。

**Ctrl+C 中断恢复**：中断 agent 执行时，已输入的用户文本自动恢复到输入框。

## 编码规范

- Rust 2021 edition，tokio async/await + async-trait
- 库 crate 用 `thiserror`，应用层用 `anyhow::Result`
- 日志用 `tracing` 宏，禁止 `println!`/`eprintln!`
- 单元测试 `#[cfg(test)] mod tests`，bin crate 集成测试在 `src/` 内（不支持 `tests/` 目录）
- 文件组织：每模块一目录，`mod.rs` 入口
- Workspace resolver = "2"，禁止下层 crate 依赖上层
- 禁止使用 `ℹ`（U+2139）符号和 `[i]` 前缀，系统消息无需额外前缀标记
- **字符串截断必须用字符级操作**：`&s[..N]` 按字节切片，CJK 字符占 3 字节，N 值落在多字节字符内部会 panic。应使用 `s.chars().take(N).collect::<String>()` 或 `s.char_indices().nth(N)` 做字符边界安全的截断。`s.len()` 返回字节数，`s.chars().count()` 返回字符数，截断长度判断也必须用字符数。

## 开发注意事项

- **新增弹窗面板**：`Event::Paste` 独立于 key event 链，必须在该分支单独拦截；`Ctrl+V` 需在 `handle_xxx_panel` 内单独处理。
- **EditField 导航**：`next()/prev()` 链必须与表单实际渲染字段一致。
- **快捷键设计**：禁止使用 `Shift + 字母`（A-Z）组合。编辑状态下 `Shift+字母` 等同于输入大写字母，二者不可区分。全局操作用 `Ctrl + 字母` 或功能键，面板操作用 `↑/↓`、`Space`、`Enter`、`Esc`。
- **字符串显示宽度**：终端列宽计算使用 `unicode-width` crate（`UnicodeWidthStr::width()` / `UnicodeWidthChar::width()`），CJK 等全角字符占 2 列。面板列表项截断需基于显示宽度而非 `char` 数量。
- **测试隔离——禁止写入全局配置**：`config::save()` 默认写入 `~/.peri/settings.json`。Headless 测试（`new_headless`）通过 `App.config_path_override` 将保存路径重定向到临时目录。新增面板操作方法若需持久化配置，必须调用 `App::save_config(cfg, self.config_path_override.as_deref())` 而非直接调用 `crate::config::save(cfg)`，否则测试会覆盖用户的真实 Provider/API Key 配置。
- **`CommandRegistry::dispatch` 借用限制 [TRAP]**：`dispatch(&self, app: &mut App)` 同时需要 `&self`（registry）和 `&mut App`，由于 registry 通过 session 嵌套在 App 内，Rust 借用检查器无法通过字段投影拆分。当前通过 `std::mem::take` + put-back workaround 解决。若要消除此 workaround，需重构 `dispatch` 签名（如改为传入 `&CommandRegistry` + 独立的 `&mut` 参数而非整个 `&mut App`）。

## 面板系统

`PanelManager` + `PanelComponent` trait 组件化架构（`app/panel_manager.rs` / `app/panel_component.rs`）。新增面板只需定义 `PanelState` 变体 + 实现 trait，无需修改 `event.rs` / `status_bar.rs` / `main_ui.rs`。

- **双作用域**：`session_panels`（Session-scoped）和 `global_panels`（Global-scoped），`App::open_panel()` 自动处理跨作用域互斥
- **统一入口**：`open_panel(PanelKind)` / `close_all_panels()`，业务逻辑在 `panel_ops.rs`
- **特殊面板**（不纳入 PanelManager）：Setup Wizard、OAuth Prompt、Interaction Prompts——全屏覆盖或 agent/MCP 触发，在 `event.rs` 中优先处理
- **快捷键提示**：面板内部禁止渲染提示行，统一由状态栏通过 `status_bar_hints()` 自描述，新增面板实现该方法即可
