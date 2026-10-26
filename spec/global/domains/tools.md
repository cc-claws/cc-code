# 工具系统 领域

## 领域综述

工具三层架构（Core/Meta/Deferred）、工具输出截断持久化、RTK 输出压缩代理、工具参数 Schema 预校验与 UI 展示规范。

## 核心流程

1. **三层工具过滤与加载**：
   - **Core 工具（12 个）**：`Read`、`Write`、`Edit`、`Glob`、`Grep`、`Bash`、`WebFetch`、`WebSearch`、`Agent`、`AskUserQuestion`、`TodoWrite`、`Skill` 始终在模型可见工具数组中。
   - **Meta 工具（2 个）**：`SearchExtraTools`、`ExecuteExtraTool` 负责动态检索和分发 Deferred 工具。
   - **Deferred 工具**：`Cron*`、`LspTool`、`mcp__*` 按需加载，减少 Prompt token 开销。
   - 非 MCP deferred 能力目录按工具名排序，仅包含描述的首个非空行（最多 160 个 Unicode 字符，超出用 `…`）；MCP 工具不进入该缓存前缀目录。
   - 完整描述和参数 schema 由 `SearchExtraTools` 按需返回。发现工具不代表授权，也不要求立即调用；需要且已授权时才通过 `ExecuteExtraTool` 执行。
2. **输出截断持久化**：
   - 超过大小限制的输出通过 `persist_truncated_output` 写入本地临时文件，并在返回中附带文件路径提示供 LLM 通过 `Read` 查看。
3. **展示层渲染与截断**：
   - 工具调用 Header 采用严格单行展示，终端宽度受限时通过 `truncate_to_display_width`（基于 `unicode-width`）动态单行截断并以 `…` 闭合，避免换行挤占视口。
4. **RTK 输出压缩代理（双轨制）**：
   - 探测外部 `rtk` 二进制（`where`/`which`）；命中且命令属潜在可重写类别时执行 `rtk rewrite "<command>"` 重写命令以降低输出 Token，失败则回退执行原命令。
   - 覆盖 git/cargo/npm/docker/kubectl/python/php/go/grep/find/ls/cat/diff/curl/wget 等 27 类命令；过滤 RTK 宿主注入的 stderr 噪音。
5. **工具参数 Schema 预校验**：
   - 对齐参考实现的 `formatZodValidationError`，结构化汇总输出「缺失参数 / 意外参数 / 类型错误」；`Unexpected parameter` 时列出允许的参数清单。
   - `suggest_tool_mismatch` 启发式诊断：依据 `TOOL_SIGNATURE_HINTS` 特征表在工具错配时建议正确工具（WebFetch/Bash/Grep/Read/Agent）。
   - `SchemaFailureTracker` 熔断：同一工具 Schema 校验连续失败 ≥2 次时注入强提示，阻断盲目重试循环。

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 核心文件系统工具 | 5 个（`Read`, `Write`, `Edit`, `Glob`, `Grep`），已移除 `FolderOperation` |
| 工具分层实现 | `CORE_TOOLS` 白名单 + `ToolSearchMiddleware` 代理 |
| 输出持久化 | `cc-middlewares/src/tools/output_persist.rs` 统一截断写入磁盘 |
| Header 截断算法 | `truncate_to_display_width`（按 CJK 2 列宽与 ASCII 1 列宽动态匹配） |
| RTK 输出压缩 | 外部 `rtk` 二进制探测 + `rtk rewrite` 命令重写，失败回退原命令（`cc-middlewares/src/process/mod.rs`） |
| Schema 预校验 | `validate_against_schema` 结构化错误汇总 + `suggest_tool_mismatch` 启发式 + `SchemaFailureTracker` 连续 2 次熔断（`cc-agent/src/agent/executor/tool_dispatch.rs`） |

## 模型可见描述

- 内置描述和参数说明统一英文，以实际实现为准；不改变工具执行、schema 字段或审批链。
- `Read` 的 offset 是从 0 开始的跳过行数，默认读取 2000 行；32 MiB 文件上限不会被范围参数绕过，PDF 页码读取尚未实现。
- `Write` 默认完整替换，append=true 追加；`Edit` 做匹配替换。提示要求先检查已有内容，不宣称工具会检查此前是否调用过 Read。
- `Agent` 区分独立消息上下文与继承会话快照，不承诺完整实时历史或文件隔离；isolation 参数仍是保留项。
- `Agent` 的 `action` 参数（`spawn` 默认 / `stop`）决定行为：`action=stop` 按 `task_id` 精确取消正在运行的后台子 Agent，**只接受精确 `task_id`**（不做任务名/描述模糊匹配，避免停错兄弟任务）。`prompt` **不在 schema 的 required 中**——`action=stop` 无需 prompt，硬编码必填会让模型凭空编造 prompt；必填性在 `invoke` 内按 `action` 分支校验。取消复用 `BackgroundTaskRegistry::cancel()`（触发 `CancellationToken` 让执行器正常终态收尾，不 abort）。详见 `docs/prd/subagent-drawer-and-virtual-routing.md` §4.4.1。
- **对齐参考实现措辞（v0.6.138）:** 参照参考实现的工具描述，仅搬运无模板函数、不依赖我方缺失能力的静态句子——① Bash / WebFetch 改为 **NOT 反向枚举**（读→`Read` NOT `cat`/`head`/`tail`；改→`Edit` NOT `sed`/`awk`；写→`Write` NOT `echo>`/`<<EOF`；内容检索→`Grep` NOT `grep`/`rg`；文件名检索→`Glob` NOT `find`/`ls`；回复用户→直接输出文本 NOT `echo`/`printf`），WebFetch 补「仅用于远程 http(s)，无点主机名不支持」边界；② `Grep` 首行禁令「ALWAYS use Grep for content search. NEVER invoke `grep`/`rg` as a Bash command」，`Read` 补「只读文件不能列目录」，`TodoWrite` 补「同一时刻只保留一个 `in_progress`」，`AskUserQuestion` 补「仅用于答案会改变后续动作的决策」；③ **`Write` 改为参考实现的 NEVER 语义**——默认不再主动创建 `.md`/README，仅用户明确要求时创建（行为语义变更，仅描述文本，无逻辑变更）
- 工具调用必须遵守既有授权与运行时审批，Meta 包装器不授予新权限。
- `Skill` 加载匹配技能及可选参数，返回 camelCase 元数据与完整指令；`context: fork` 等待隔离执行完成。主代理和子代理均注册，详情见 [skill-trigger.md](./skill-trigger.md)。

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
**涉及文件:** cc-middlewares/src/tools/output_persist.rs, terminal.rs, grep.rs, glob.rs, folder.rs, web_fetch.rs, tool_bridge.rs, resource_tool.rs
**CLAUDE.md 链接:** false

### issue_2026-05-23-migrate-web-tools-to-tavily-backend

**摘要:** WebSearch/WebFetch 后端迁移至 Tavily 兼容接口
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** Tavily, WebSearch, WebFetch, Bing, 后端迁移
**问题本质:** Bing HTML 解析不稳定且维护成本高，需迁移到统一 API 后端
**通用模式:** 外部依赖（搜索引擎）应通过统一 API 封装，避免直接解析 HTML；API 迁移时需完整移除旧实现防止代码残留
**涉及文件:** cc-middlewares/src/middleware/web_search.rs, cc-middlewares/src/middleware/web_fetch.rs, cc-middlewares/src/middleware/web_common.rs
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
**技术决策:** 对齐参考实现：默认采用 files_with_matches 模式，并在 content 模式下支持 head_limit / offset 分页
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

最后更新：2026-10-26
