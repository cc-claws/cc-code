# 工具系统 领域

## 领域综述

工具三层架构（Core/Meta/Deferred）、工具输出截断持久化与 UI 展示规范。

## 核心流程

1. **三层工具过滤与加载**：
   - **Core 工具（11 个）**：`Read`、`Write`、`Edit`、`Glob`、`Grep`、`Bash`、`WebFetch`、`WebSearch`、`Agent`、`AskUserQuestion`、`TodoWrite` 常驻 System Prompt。
   - **Meta 工具（2 个）**：`SearchExtraTools`、`ExecuteExtraTool` 负责动态检索和分发 Deferred 工具。
   - **Deferred 工具**：`Cron*`、`LspTool`、`mcp__*` 按需加载，减少 Prompt token 开销。
2. **输出截断持久化**：
   - 超过大小限制的输出通过 `persist_truncated_output` 写入本地临时文件，并在返回中附带文件路径提示供 LLM 通过 `Read` 查看。
3. **展示层渲染与截断**：
   - 工具调用 Header 采用严格单行展示，终端宽度受限时通过 `truncate_to_display_width`（基于 `unicode-width`）动态单行截断并以 `…` 闭合，避免换行挤占视口。

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 核心文件系统工具 | 5 个（`Read`, `Write`, `Edit`, `Glob`, `Grep`），已移除 `FolderOperation` |
| 工具分层实现 | `CORE_TOOLS` 白名单 + `ToolSearchMiddleware` 代理 |
| 输出持久化 | `peri-middlewares/src/tools/output_persist.rs` 统一截断写入磁盘 |
| Header 截断算法 | `truncate_to_display_width`（按 CJK 2 列宽与 ASCII 1 列宽动态匹配） |

---

## Issue 经验附录

### issue_2026-05-15-tool-output-truncation-with-disk-persist

**摘要:** 工具输出超长时截断 + 持久化磁盘 + 提示 Read 读取剩余内容
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 输出截断, 磁盘持久化, 工具输出, output_persist
**问题本质:** 截断后的工具输出直接丢弃，LLM 需要重新执行整个工具（浪费 token），无法获取完整数据
**通用模式:** 截断时完整数据写入临时文件，截断结果中附文件路径提示。LLM 可按需 Read 完整内容，避免重复工具调用
**技术决策:** 共享函数 `persist_truncated_output` 统一处理 7 个工具的截断持久化（Bash/Grep/Glob/FolderOperations/WebFetch/MCP ToolBridge/MCP ResourceTool），Read/WebSearch 排除
**涉及文件:** peri-middlewares/src/tools/output_persist.rs, terminal.rs, grep.rs, glob.rs, folder.rs, web_fetch.rs, tool_bridge.rs, resource_tool.rs
**CLAUDE.md 链接:** false

### issue_2026-05-23-migrate-web-tools-to-tavily-backend

**摘要:** WebSearch/WebFetch 后端迁移至 Tavily 兼容接口
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** Tavily, WebSearch, WebFetch, Bing, 后端迁移
**问题本质:** Bing HTML 解析不稳定且维护成本高，需迁移到统一 API 后端
**通用模式:** 外部依赖（搜索引擎）应通过统一 API 封装，避免直接解析 HTML；API 迁移时需完整移除旧实现防止代码残留
**涉及文件:** peri-middlewares/src/middleware/web_search.rs, peri-middlewares/src/middleware/web_fetch.rs, peri-middlewares/src/middleware/web_common.rs
**CLAUDE.md 链接:** false

### issue_2026-06-27-ctrl-b-background-shell

**摘要:** Ctrl+B Background Shell 交互机制
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** 长耗时前台命令会阻塞整个 Agent 回合与 TUI 交互
**通用模式:** 长时任务应具备随时解耦前台阻塞的能力，通过异步状态机与文件落盘保证进程生命周期可观测
**技术决策:** 实现后台进程管理与 Ctrl+B 交互，将输出重定向至磁盘并释放前台交互
**涉及文件:** spec/archive-issues/2026-06-27-ctrl-b-background-shell.md
**CLAUDE.md 链接:** false

### issue_2026-06-25-grep-default-mode-and-pagination

**摘要:** Grep 工具优化与分页机制
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** 默认输出整段匹配内容易迅速打满上下文，且缺少按条数分页机制
**通用模式:** 搜索类工具必须采用「先概括后深入」的层级设计，默认输出高 token 效率的路径列表，按需分页深入
**技术决策:** 对齐 Claude Code：默认采用 files_with_matches 模式，并在 content 模式下支持 head_limit / offset 分页
**涉及文件:** spec/archive-issues/2026-06-25-grep-default-mode-and-pagination.md
**CLAUDE.md 链接:** false

### issue_2026-06-03-edit-tool-tab-indent-mismatch

**摘要:** Edit 工具无法编辑 Tab 缩进文件
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** 字符串精确匹配算法未对 Tab 和空格做等价归一化或在提取时强行展开了 Tab
**通用模式:** 文本替换工具在保留原格式的同时，必须在诊断阶段对看不见的空白字符（Tab vs 空格、CRLF vs LF）给出高精度提示
**技术决策:** 保留文件原有缩进格式并在匹配失败时提供更清晰的诊断提示
**涉及文件:** spec/archive-issues/2026-06-03-edit-tool-tab-indent-mismatch.md
**CLAUDE.md 链接:** false

### issue_2026-06-01-tool-output-truncation-bypass

**摘要:** 工具输出截断机制被绕过
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** 部分新加工具或管道直接构造 ToolResult 消息未经过公共输出截断过滤器
**通用模式:** 安全与限制类机制（如输出截断）必须收敛至统一的中间件或公共管道出口，禁止各工具自行实现甚至跳过
**技术决策:** 统一所有工具输出通过 output_persist 公共实现执行截断与落盘
**涉及文件:** spec/archive-issues/2026-06-01-tool-output-truncation-bypass.md
**CLAUDE.md 链接:** false

### issue_2026-05-25-at-mention-directory-read-semantics

**摘要:** @ 目录引用误调用 Read 工具
**状态:** Fixed
**归档日期:** 2026-09-20
**问题本质:** 解析 @ 引用时未区分文件与目录，导致将目录路径当作普通文本文件喂给 Read 工具报错
**通用模式:** 用户输入引用的实体在进入工具调用或提示词前必须进行语义类型判定，文件与目录应路由至不同的处理逻辑
**技术决策:** 检查文件属性，若为目录则转换为目录文件列表或提示信息以 System 消息注入
**涉及文件:** spec/archive-issues/2026-05-25-at-mention-directory-read-semantics.md
**CLAUDE.md 链接:** false
