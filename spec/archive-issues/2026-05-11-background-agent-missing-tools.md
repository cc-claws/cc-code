> 归档于 2026-05-16，原路径 spec/issues/2026-05-11-background-agent-missing-tools.md

# Background Agent 工具继承缺失——子 agent 仅能使用 TodoWrite

**状态**：Fixed + Verify
**优先级**：高
**创建日期**：2026-05-11
**修复 commit**：`56fb890` fix(subagent): add diagnostic logging for background agent tool inheritance

## 问题描述

后台 Background Agent（`run_in_background: true`）启动后，子 agent 报告仅有 `TodoWrite` 可用，缺少 `Read`、`Write`、`Edit`、`Bash`、`Grep`、`Glob` 等核心工具，导致子 agent 陷入死循环无法完成任务。使用 `general-purpose` 内置 agent（`tools: "*"`）时复现。

## 症状详情

- **触发场景**：LLM 调用 Agent 工具并设置 `run_in_background: true`，`subagent_type` 为 `general-purpose`
- **实际行为**：子 agent 的 LLM 报告 "TodoWrite 是我目前唯一可用的工具"，无法读取文件、执行命令等
- **期望行为**：子 agent 应继承父 agent 的所有核心工具（Read/Write/Edit/Bash/Grep/Glob/folder_operations）+ MCP 工具

## 复现条件

- **复现频率**：用户报告至少发生一次，具体复现条件待确认
- **触发步骤**：
  1. 启动 agent 会话
  2. LLM 自动调用 Agent 工具，设置 `run_in_background: true`，`subagent_type: "general-purpose"`
  3. 观察 Background agent 的 LLM 工具列表
- **环境**：general-purpose 内置 agent（`tools: "*"`）

## 根因分析

### 代码路径

`parent_tools` 构造（`peri-tui/src/app/agent.rs:198-213`）：

```rust
let mut parent_tools: Vec<Box<dyn BaseTool>> = FilesystemMiddleware::build_tools(&cwd);
parent_tools.extend(TerminalMiddleware::build_tools(&cwd));
// + MCP tools if pool exists
```

`SubAgentMiddleware::new(parent_tools, ...)` 接收这些工具，Background agent 路径（`peri-middlewares/src/subagent/tool.rs:472-475`）通过 `filter_tools()` 过滤：

```rust
let filtered_tools = self.filter_tools(
    &agent_def.frontmatter.tools,       // ToolsValue::List(vec!["*"])
    &agent_def.frontmatter.disallowed_tools,
);
```

`filter_tools()` 对 `tools: "*"` 的处理（`tool.rs:278-314`）：`is_wildcard = true`，跳过 allowed 过滤，所有 `parent_tools` 应通过。

### 架构隐患

Background agent 构建子 agent 时（`tool.rs:494-523`）只添加了 4 个 middleware：

| Middleware | 提供的工具 |
|-----------|-----------|
| `AgentsMdMiddleware` | 无 |
| `SkillsMiddleware` | 无 |
| `SkillPreloadMiddleware` | 无（fake tool 序列） |
| `TodoMiddleware` | `TodoWrite` |

核心工具完全依赖 `filtered_tools`（从 `parent_tools` 通过 `register_tool` 注册）。

**与 Normal 路径对比**：Normal 路径（`tool.rs:745-783`）的 middleware 配置与 Background 路径完全一致，同样依赖 `register_tool`。如果 Normal 路径正常而 Background 路径异常，差异点可能在于：

1. **`tokio::spawn` 闭包捕获**：Background agent 在 `tokio::spawn(async move { ... })` 中执行（`tool.rs:546`），`agent_builder` 被移入闭包。如果工具引用在 move 后失效，可能导致工具丢失
2. **无 event_handler**：Background agent 不设置 `with_event_handler`（注释说明避免事件混入父 agent 流），但这不应影响工具可用性
3. **`parent_tools` 的 `Arc` 共享**：`parent_tools` 通过 `Arc<Vec<Arc<dyn BaseTool>>>` 共享，`filter_tools` 创建 `ArcToolWrapper(Arc::clone(tool))`。如果 `Arc` 引用计数有问题，可能导致工具在 spawn 后被 drop

### 可能的根因假设

**假设 A**：`parent_tools` 在 `SubAgentMiddleware` 构造时正确包含工具，但 Background agent spawn 时工具引用因所有权转移而失效。需要检查 `ArcToolWrapper` 是否正确实现了 `BaseTool` trait 的所有方法（特别是 `name()` 和 `schema()`）。

**假设 B**：存在时序问题——`parent_tools` 在 MCP pool 初始化之前构建，某些场景下 MCP 工具缺失导致工具列表不完整。但核心工具（Filesystem + Terminal）不受此影响。

**假设 C**：`register_tool` 的工具在 `ReActAgent::execute` 的 `collect_tools` + `self.tools` 合并过程中被覆盖或丢失。

## 修复方案

### 方案 A：添加防御性日志（诊断优先）

在 `filter_tools` 返回后、`register_tool` 循环中添加日志，记录实际注册的工具名称：

```rust
// tool.rs invoke_background, line 521
for tool in &filtered_tools {
    tracing::debug!(tool_name = %tool.name(), "background agent: registering tool");
}
```

在 `ReActAgent::execute` 中记录 `all_tools` 的最终内容：

```rust
// executor/mod.rs, after line 212
tracing::debug!(tool_count = all_tools.len(), tool_names = ?all_tools.keys().collect::<Vec<_>>(), "agent: final tool set");
```

### 方案 B：架构改进（长期）

让 Background agent 的 middleware 配置与父 agent 对齐，不再依赖 `parent_tools` + `register_tool` 的间接传递：

```rust
// 替代 register_tool，直接添加 middleware
agent_builder = agent_builder
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    .add_middleware(Box::new(TerminalMiddleware::new()));
// MCP tools 也通过 McpMiddleware 注入
```

## 相关代码

- `peri-tui/src/app/agent.rs:198-213` —— `parent_tools` 构造
- `peri-tui/src/app/agent.rs:271-283` —— `SubAgentMiddleware::new(parent_tools, ...)`
- `peri-middlewares/src/subagent/tool.rs:278-314` —— `filter_tools()` 实现
- `peri-middlewares/src/subagent/tool.rs:438-598` —— `invoke_background()` 完整路径
- `peri-middlewares/src/subagent/tool.rs:494-523` —— Background agent middleware 配置
- `peri-middlewares/src/subagent/built-in/general-purpose.md` —— general-purpose agent 定义（`tools: "*"`）
- `peri-agent/src/agent/executor/mod.rs:188-222` —— 工具收集和过滤逻辑
