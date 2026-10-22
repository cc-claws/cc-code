# cc-middlewares

中间件实现 crate，为 Agent 提供文件系统、终端、MCP、Hooks 等能力。

## 概述

`cc-middlewares` 实现了 `cc-agent` 的 `Middleware` trait，提供 19 个中间件按固定顺序组成链：

## 中间件链

```
1.  AgentsMdMiddleware       ← 指引文件注入（AGENTS.md/CLAUDE.md，同目录合并去重 + 跨目录拼接 + 限额）
2.  MemoryMiddleware        ← 自动记忆索引注入
3.  AgentDefineMiddleware    ← agent 定义，model/maxTurns 覆盖
4.  SkillsMiddleware         ← Skills 摘要注入（含插件 extra_dirs）
5.  SkillPreloadMiddleware   ← #skill-name 全文注入
6.  AtMentionMiddleware      ← @path 解析，注入 Read 工具调用
7.  FilesystemMiddleware     ← 5 个文件系统工具
8.  GitAttributionMiddleware ← before_tool/after_tool 追踪 Write/Edit 贡献字符数
9.  TerminalMiddleware       ← Bash
10. WebMiddleware            ← WebFetch/WebSearch
11. TodoMiddleware           ← after_tool 解析 TodoWrite
12. CronMiddleware           ← Cron 调度
13. HookMiddleware           ← hooks 事件拦截（多组实例）
14. HumanInTheLoopMiddleware ← before_tool 拦截
15. SubAgentMiddleware       ← Agent 工具
16. McpMiddleware            ← MCP 工具和资源（条件注册，pool 成功时）
17. ToolSearchMiddleware     ← SearchExtraTools/ExecuteExtraTool 代理
18. LspMiddleware            ← LSP 工具 + after_tool 文件变更同步（条件注册）
19. CompactMiddleware        ← before_model 钩子触发上下文压缩（条件注册）
[ReActAgent.with_system_prompt()] ← prepend
```

注：McpMiddleware、LspMiddleware、CompactMiddleware 为条件注册，仅在配置可用时生效。

## 核心中间件

### FilesystemMiddleware

提供 5 个核心文件系统工具：

| 工具 | 说明 |
|------|------|
| `Read` | 读取文件内容 |
| `Write` | 写入文件 |
| `Edit` | 编辑文件（字符串替换） |
| `Glob` | 文件名匹配搜索 |
| `Grep` | 文件内容搜索 |

### TerminalMiddleware

Bash 工具，支持：

- 命令执行
- 超时控制
- Windows Git Bash fallback
- 后台 Shell（Ctrl+B）

### McpMiddleware

基于 `rmcp` crate 的 MCP 集成：

```toml
# .mcp.json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
    }
  }
}
```

工具命名：`mcp__{server_name}__{tool_name}`

### HookMiddleware

4 种执行类型，14 种事件：

| 执行类型 | 说明 |
|----------|------|
| Command | 外部命令 |
| Prompt | LLM 提示 |
| Http | HTTP 请求 |
| Agent | 子 Agent |

Exit code 控制：0=Allow，1=Warn，2=Block

### SubAgentMiddleware

子 Agent 支持：

- `.claude/agents/` 下定义
- 扁平 `{agent_id}.md` 或嵌套 `{agent_id}/agent.md`
- Background 模式（后台运行）
- Fork 模式（独立会话）

### LspMiddleware

LSP 集成，10 种操作 + 自动文件同步：

```rust
// after_tool 自动同步
Write/Edit 工具 → didChange → didSave → LSP Server
```

## 工具输出截断

统一的输出截断机制（`output_persist`）：

```rust
// 超长输出自动截断（tools/output_persist.rs）
// 阈值：MAX_OUTPUT_CHARS = 100_000 字符、MAX_OUTPUT_LINES = 2_000 行
let output = truncate_tool_output(&output);
// 完整内容需要落盘时：persist_truncated_output(&full_output)
// 将全文写入临时文件，并返回指向该文件的提示字符串
```

## 插件系统

兼容参考实现的插件生态：

```json
// ~/.cc-code/settings.json
{
  "enabledPlugins": {
    "plugin-id": true
  }
}
```

插件通过以下方式注入：

- `plugin_skill_dirs` → `SkillsMiddleware.with_extra_dirs()`
- `plugin_hooks` → `HookMiddleware`
- `plugin_mcp` → `McpMiddleware`

## 使用示例

### 自定义中间件

```rust
use cc_agent::prelude::*;
use async_trait::async_trait;

struct MyMiddleware;

#[async_trait]
impl Middleware<AgentState> for MyMiddleware {
    fn name(&self) -> &str { "my-middleware" }

    async fn before_tool(&self, state: &mut AgentState, call: &ToolCall) -> AgentResult<ToolCall> {
        // 工具调用前的逻辑
        Ok(call.clone())
    }

    async fn after_tool(&self, state: &mut AgentState, call: &ToolCall, result: &ToolResult) -> AgentResult<()> {
        // 工具调用后的逻辑
        Ok(())
    }
}
```

### 注册中间件

```rust
let agent = ReActAgent::new(llm)
    .add_middleware(Box::new(MyMiddleware))
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    .add_middleware(Box::new(TerminalMiddleware::new()));
```

## 依赖关系

```
cc-middlewares
  ├── cc-agent       # 核心 Agent 框架
  ├── cc-lsp         # LSP 客户端
  ├── rmcp             # MCP 客户端
  └── gray_matter      # Frontmatter 解析
```

## 测试

```bash
cargo test -p cc-middlewares
```

## 详细文档

- [CLAUDE.md](./CLAUDE.md) — 开发指南和陷阱记录
- [Hooks 规范](../spec/global/domains/hooks.md)
- [MCP 集成](../spec/global/domains/mcp.md)
