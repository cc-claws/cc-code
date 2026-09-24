# Changelog

Perihelion Agent 版本变更记录。

---

## v0.6.75 — 2026-09-24

### Features

- **工具参数 Schema 校验可读性优化与启发式诊断（#234, #235）**：重构 `validate_against_schema`，对齐 Claude Code `formatZodValidationError` 风格，结构化分项输出缺失参数、意外未定义参数与类型不符；基于参数特征指纹对 `WebFetch`（误传 `url`+`prompt`）、`Bash`（误传 `command`）、`Agent`（缺失子 agent 类型且未设 `fork`）提供启发式纠偏建议；新增同一工具连续 2 次参数校验失败熔断拦截（Circuit Breaker），防止模型陷入盲目重试死循环。

### Fixes

- **引入独立输入泵以安全启用鼠标悬停并修复消息区滚动条交互（#232, #236）**：通过独立后台输入泵（`InputPump`）隔离 Windows 控制台下的重入与阻塞风险，安全消费 `MouseEventKind::Moved` 鼠标悬停事件；精确计算消息区滚动条滑块交互范围与相对拖拽位移，消除滚动条点击漂移与量化跳跃。

---

## v0.6.74 — 2026-09-23

### Fixes

- **RTK git status 噪音过滤与移除毒性通用折叠（#207, #214, #233）**：针对 RTK 格式的 `clean — nothing to commit` 增加专门匹配清洗；移除通用多行重复块折叠（避免代码上下文被意外吞并），恢复兜底分支原始输出。

---

## v0.6.73 — 2026-09-23

### Fixes

- **Windows 下执行外部子进程触发全屏黑白闪屏（#229, #231）**：解决在 Windows 平台下执行 PHP 等子进程时未隔离控制台，导致外部运行时修改控制台代码页（936 与 65001 切换）触发宿主终端自愈物理清屏（`terminal.clear()`）的问题；在 Windows shell 创建时增加 `CREATE_NO_WINDOW (0x08000000)` 标志实现控制台完全隔离，子进程代码页变动不再干扰父 TUI 会话。

### Performance

- **长内容下高频鼠标滚轮防抖批处理与滑块平滑拖拽（#230, #231）**：引入有界事件批处理机制（`EventReader`），合并高频连续滚轮事件，大幅减少排队重绘导致的掉帧与迟滞；从真实帧缓冲区读取滚动条滑块区域并以初始按下位置为锚点做相对计算，消除滑块点击漂移与量化跳跃。

---

## v0.6.72 — 2026-09-23

### Features

- **消息区 Markdown 超链接点击打开默认浏览器（#225, #228）**：解决此前渲染层丢弃 `dest_url` 导致 Markdown 超链接 `[text](url)` 无法点击、终端无法探测的问题；在 `peri-widgets` 和 `peri-tui` 链路保留链接目标与字符级命中区，消息区单次点击链接即可跨平台唤起系统默认浏览器（Windows / macOS / Linux），并与文本选区、多行折行、引用块、列表项完整对齐。

---

## v0.6.71 — 2026-09-23

### Changes

- **禁用 sticky header 顶部固定消息条（#218, #219）**：消息区顶部最近用户消息固定条信息重复、占用布局高度且过长时截断，整体禁用渲染，布局高度归还消息区；关联 headless 测试统一标记 `#[ignore]`，恢复功能时删除标记即可。

### Fixes

- **附件栏标题与 Del 提示接入 i18n 多语言（#222）**：修复英文环境下「待发送附件」栏标题与 Del 删除提示仍显示硬编码中文的问题，文案接入 `lc.tr()` i18n 链路并补齐中英 locales，附件栏文案跟随语言设置切换。

---

## v0.6.70 — 2026-09-23

### Fixes

- **TUI 状态栏模型已切换但网关请求仍用旧模型（#169, #215）**：解除 ACP Client 在无会话时对配置同步的静默丢弃，`session/new` 与 `session/load` 消费 `model` 参数，修复发送首条消息前切换模型 100% 不生效的缺陷；`/history` 恢复历史会话传参规范化并加服务端全名反查兜底，防止恢复后回退默认模型。
- **快捷键调整与 Tips/文档收口（#169, #215）**：废弃 `Ctrl+T`/`Alt+M` 模型循环快捷键（模型切换统一走命令面板），新增 `Alt+P` 等价打开命令面板（`Ctrl+P`/`Alt+P`）；纠正 tips 中"长按 Ctrl+V"、"Ctrl+N/P 切换 Session"等与代码不符的描述，补齐 PageUp/Down 等快捷键的触发条件；同步清理 CLAUDE.md、README、注释中的 `Ctrl+T` 残留并修正"禁止 PageUp/PageDown"的过时规范。

---

## v0.6.69 — 2026-09-23

### Fixes

- **工具调用可靠性：边界提示词、字段级参数校验与动作循环检测（#200, #211）**：系统提示词新增工具边界互斥与参数错误处置规则；工具参数调用前按 JSON Schema 预校验，报错包含缺失字段名、期望类型与实际收到的 keys；新增动作签名循环检测，连续 3 轮相同工具动作时注入纠正消息，防止无效重复动作。
- **多行命令 stdout 截断修复（#212, #211）**：Windows 上含字面换行符的多行命令改走 Git Bash 执行，不再被 `cmd /C` 截断为首行输出，`node -e` 等多行脚本可完整返回全部 stdout。
- **命令输出通用重复行/块折叠（#214, #211）**：`filter_command_output` 兜底分支新增通用折叠——首行相同的多行块或连续相同行出现 ≥3 次时保留前 2 个样例并附折叠摘要，高重复度构建警告不再原样刷入上下文。

---

## v0.6.68 — 2026-09-21

### Fixes

- **移除 Git Bash fallback 重试标记混入 LLM 上下文（#209, #210）**：在 Windows 下 Bash 工具 cmd /C 执行失败后使用 Git Bash 重试时，不再将 `[Retried with Git Bash]` 机制性元信息追加到输出末尾，避免干扰 LLM 上下文。
- **Read 工具图片魔数校验与伪图片防御（#207, #208）**：在读取图片前先验证文件头部魔数字节（Magic Bytes），防止文本或损坏文件被误传为图片导致模型报错。
- **消息气泡长段落续行悬挂缩进对齐修复（#205, #206）**：修复 TUI 渲染长消息气泡时段落续行的悬挂缩进对齐问题。
- **修复 Windows 8.3 短文件名波浪号拦截与测试时序（#203, #204）**：修复剪贴板在 Windows 8.3 格式短文件名中包含 `~` 时的错误拦截，并增强 headless 测试时序稳定性。

---

## v0.6.66 — 2026-09-21

### Features & Improvements

- **执行中消息队列与按轮次 steering**：Agent 执行期间输入的消息进入待发队列，按轮次以增量 StateSnapshot 注入执行循环，不再打断当前执行或丢失历史。
- **粘贴本机图片路径自动转为附件**：粘贴 `file://` URL 或引号包裹的绝对路径时直接识别为图片附件，避免把路径文字当图片发送。

### Fixes

- **修正 headless 多轮消息快照回归测试（#201, #202）**：按增量 steering 协议补齐 `begin_round` 并发送第二轮增量消息，仅修正测试建模，不改变生产逻辑。

---

## v0.6.64 — 2026-09-21

### Features & Improvements

- **清洗 RTK 外部宿主 stderr 提示噪音（#184, #185）**：过滤 RTK 外部宿主写入 stderr 的提示噪音，避免误导 Agent 判断。

---

## v0.6.63 — 2026-09-20

### Features & Improvements

- **对齐本地命令块渲染样式（#182, #183）**：
  - 将用户 `!` 本机命令展示重构为与 Claude Code 一致的命令块布局；
  - 标题行使用整行背景与粉色 `!` 前缀，输出内容以 `  └ ` 引导的树状缩进对齐呈现；
  - 隐藏命令块标题中冗余的 exit code 与 cwd 信息，无输出时以 `(No output)` 占位提示；
  - 保留完整底层执行、ANSI 解析、超长折叠截断与后台化控制逻辑。

---

## v0.6.62 — 2026-09-20

### Features & Improvements

- **状态栏新增 Prompt Cache 命中率指标**：以常驻百分比替换瞬时 CPU 指标，并根据命中率分级显示颜色，继续保留 MEM 指标。
- **移除消息区冗余的低命中率警告气泡**：低于 80% 时仅保留 tracing 告警，缓存状态统一由状态栏展示，避免打断正常阅读。

---

## v0.6.60 — 2026-09-20

### Features & Improvements

- **对齐 OpenAI Codex 规范的动态终端标题（Status Surface）体系（#162, #177）**：
  - 参考 OpenAI Codex CLI (`codex-rs`) 官方规范，将终端标题提升为标准的状态表面抽象；
  - **双轨制会话主题提炼**：首轮 Prompt 发送后，轨 1 本地确定性提取器 0ms 即时渲染标题；轨 2 在后台异步派发超轻量 LLM 总结任务精准提炼主题并更新（支持 `PERI_DISABLE_TITLE_GENERATION` 环境变量关闭），具备会话属主校验与防覆盖手动 `/rename` 机制；
  - **动态生命周期状态感知**：任务执行期间点阵/菊花帧帧动态旋转；回复完成后展示标志性橙色菊花 `✴`；HITL 审批与提问交互等待时提升为 `[ ! ] Action Required` 呼吸闪烁；
  - **底层安全清洗与写出去重**：过滤控制字符与 Trojan Source / Bidi 隐形字符，字形簇截断保护，OSC 0 内容变动才写出，消除 Idle 态无谓 I/O 抖动；
  - **宿主终端保护**：进入/退出 AlternateScreen 时通过 XTerm title stack（`\x1b[22;0t` push / `\x1b[23;0t` pop）保护并 100% 还原宿主原有标题。

---

## v0.6.59 — 2026-09-20

### Features & Improvements

- **支持 Alt+V 快捷键粘贴图片附件并对齐分支规范（#165, #166）**：
  - 针对 Windows Terminal / PowerShell / VS Code 等现代终端在宿主层拦截 `Ctrl+V` 用于纯文本粘贴、导致系统剪贴板中的图片无法传入 TUI 的问题，新增 `Alt+V` 专用快捷键（兼容大写 `V` 及 macOS `Option+V` 字符 `√`），实现稳定穿透终端宿主触发剪贴板图片提取并挂载为待发送附件；
  - 文本粘贴回退统一走 `paste_text_into_textarea` 管道，保障文件路径归一化与多行文本折叠占位符行为一致；
  - `CLAUDE.md` 明确本仓库分支规范为最高优先级，禁用 `#` 命名以保护 GitHub Actions PR CI 链路，并补充现代终端粘贴快捷键实战避坑指南。

---

## v0.6.58 — 2026-09-20

### Bug Fixes

- **修复 Windows 传统控制台歧义字符残影与 Markdown 表格视口溢出缺陷（#158, #159, #163）**：
  - 新增 `WidthSafeBackend` 终端适配层与 `ConsoleWidthProbe` 物理列宽探针，在 Windows 绘制输出边界动态探测物理光标偏移，针对不匹配的东亚歧义字符（`∴`、`●`）以及特殊符号（制表框 `+`、对勾 `v`、叉号 `x`、进度条 `#`/`-` 等）提供等宽平替，彻底解决 Windows CMD/conhost（新宋体 CP936）下光标偏移与行尾幽灵残影问题；
  - 终端生命周期集成 `TuiBackend`，在 `draw_app` 中感知字体、字号与代码页指纹变化自适应重绘与清理缓存；
  - Markdown 表格渲染引入视口硬约束和多列溢出防御削减，彻底杜绝表格宽度超出视口触发操作系统级硬折行；优化长路径断词，优先在路径分隔符（`/`、`\`、`-`、`_`）处断行，避免扩展名孤立；
  - 消息气泡渲染时预先扣除前缀缩进（2 字符），避免内容满宽后加前缀触发折行；
  - 修复 CI 在 Ubuntu 下的类型复杂度警告与 macOS 平台异步钩子测试超时。

---

## v0.6.57 — 2026-09-20

### Bug Fixes

- **修复工具包装器丢失多模态图片内容导致 Read 识图失败的缺陷（#157）**：
  - `ToolWrapper`、`ArcToolWrapper` 及 `peri-middlewares` 中的 Arc 包装器三个 `BaseTool` 包装器补齐 `invoke_content` 透传委托，此前包装器未转发该方法，结构化多模态内容（图片 Base64）在包装层被丢弃，图片无法进入模型上下文；
  - 新增 `read_multimodal_test.rs` 回归测试（8 项），覆盖图片内容经包装器完整传递的链路。

---

## v0.6.56 — 2026-09-20

### Features & Improvements

- **Read 工具支持多模态图片读取（#153, #154）**：
  - 在 `BaseTool` trait 中新增 `invoke_content` 方法和 `ToolContent` 结构体，支持工具返回结构化多模态内容（如图片 Base64），同时保持 `invoke` 纯文本接口 100% 向后兼容，所有现有工具无需任何修改；
  - `ToolResult` 增加 `content: Option<MessageContent>` 字段，调度链路优先使用结构化内容写入 state，打通工具→LLM 的多模态通道；
  - `ReadFileTool` 从二进制扩展名列表中移除 PNG/JPG/JPEG/GIF/WebP/BMP，新增图片识别→字节读取→Base64 编码→`ContentBlock::Image` 返回的完整链路，含 20MB 大小保护；
  - 搭配多模态模型（如 Claude 3.5/3.7 Sonnet、GPT-4o 等）时，Agent 可通过 `Read("image.png")` 直接读取本地图片进行视觉分析。

---

## v0.6.55 — 2026-09-18

### Bug Fixes

- **修复常驻子进程导致后台 shell 任务无限停留 running 的缺陷（#149, #150）**：
  - 为 `run_streaming_child` 和 `execute_shell_command_with_stdin` 引入管道超时保护机制（`PIPE_DRAIN_TIMEOUT = 2s`），在主进程退出后若常驻子进程（如 `Start-Process` 或脚本脱离启动的常驻服务）继承了管道写端句柄，超时后自动中止读取等待，彻底解决 reader task 永远等不到 EOF 导致后台任务永久停留在 `running` 状态且无法派发完成通知的问题；
  - 累积输出移入线程安全的共享缓冲区，即使触发管道超时，主进程退出前产生的所有标准输出和错误日志仍 100% 完整保留，杜绝输出丢失；
  - 使用 `tokio::join!` 并发等待 stdout 与 stderr 读取任务，保证最差等待耗时严格控制在 2 秒以内。

---

## v0.6.54 — 2026-09-18

### Bug Fixes

- **消除 agent Bash 运行提示与生命周期切换时的终端清屏闪烁（#145, #146）**：
  - 移除 `register_agent_shell`、`background_agent_foreground`、`poll_agent_shells` 对 `request_terminal_clear_redraw()` 的调用，依赖 Ratatui 原生单元格 Diff 驱动平滑重绘，消除物理清屏 `terminal.clear()` 带来的瞬间黑屏/白屏闪烁；
  - 重构 `render_thread.rs` 中的秒数刷新逻辑，移除粗暴的 `message_hashes.clear()`；首次跨越 2s 阈值时仅增量失效当前消息 hash，超过 2s 持续运行时仅增量更新秒数行与动画帧，100% 复用历史消息缓存，避免高频全量重绘抖动。

---

## v0.6.53 — 2026-09-18

### Features & Improvements

- **工具行 Header 超长改为单行省略号截断（#141）**：
  - 移除此前工具调用 Header 的多行折行与 8 列缩进，恢复并保证严格单行展示，不再挤占视口垂直高度；
  - 新增 `truncate_to_display_width`，结合 `unicode-width` 按显示列宽动态计算与截断，并在末尾优雅追加 `…` 闭合括号；
  - 支持中英文长参数与长路径自适应截断，聚合工具组同步对齐，彻底杜绝换行。

### Bug Fixes

- **对齐 Spinner 完成态总结行与生命周期修复（#139）**：
  - 对齐 Claude Code 风格的过去式总结行动词与时刻展示（`✻ {verb} for {elapsed} · done {HH:MM}`）；
  - 移除 loading 态多余前导空格对齐 column 0，消除任务完成瞬间的水平抖动；
  - 重构中断（Ctrl+C）与报错生命周期清理，防止异常状态误展示成功动词。

---

## v0.6.52 — 2026-09-17

### Bug Fixes

- **状态栏运行耗时秒数补零与 Emoji 宽度对齐**：修复状态栏耗时未补零导致的字符残影和抖动，以及变体选择器导致的 Emoji 列宽错位
  - 秒数与分钟统一增加 `{:02}` 补零（如 `31m06s`），固定字符串渲染宽度，消除由 59 秒进入个位数秒时少 1 个字符造成的残影
  - 替换含 `U+FE0F` 的 `⏱️` 为标准单字符 `⏱`，消除不同终端下光标列宽计算偏移造成的字符错位叠加

---

## v0.6.51 — 2026-09-17

### Bug Fixes

- **Anthropic 适配器支持自适应解析 SSE 流式响应**：修复非流式请求（如后台 SubAgent 任务）收到反向代理网关强制返回的 SSE 流时，因直接调用 `serde_json::from_str` 导致反序列化崩溃的问题
  - 在非流式请求体构建中显式声明 `"stream": false`
  - 在 `handle_anthropic_response` 中自适应捕获 SSE 文本，复用 `SseParser` 还原为标准的 Anthropic 消息 JSON 结构，无损向下兼容各种代理/网关环境
  - 增加针对文本、工具调用（tool_use）、错误事件的 3 组完整回归测试用例

---

## v0.6.50 — 2026-09-17

### Bug Fixes

- **Markdown 终端宽度变化重复渲染**：修复终端宽度变化时，因未清空旧行缓存导致增量追加模式错误触发、内容翻倍重复渲染的问题
  - 宽度变化时显式清空 `rendered.lines`，彻底重置渲染流状态
  - 增加宽度变更防重复渲染的单元测试锁定正确行为

---

## v0.6.49 — 2026-09-17

### Refactor

- **移除 FolderOperation 工具**：
  - 彻底移除 `FolderOperationsTool` 及其全部单元测试，将文件系统工具集收敛为标准的 5 个工具（`Read`, `Write`, `Edit`, `Glob`, `Grep`）
  - 核心工具白名单（Core Tools）由 12 个收敛为 11 个
  - 清理 HITL 审批白名单、ACP 协议映射（`infer_tool_kind`）与 TUI 展示层中的冗余映射逻辑
  - 同步更新系统提示词和相关架构规范文档

---

## v0.6.48 — 2026-09-17

### Bug Fixes

- **Markdown 表格渲染挤压与空列**：宽终端下 AI 消息中的表格按固定 80 列渲染导致列被压成每行 2-3 个中文字、右侧留白，且表格尾部多出空白列
  - 表格渲染宽度改为跟随真实终端宽度，resize 时自动重解析
  - 修复 TableBuilder 行尾重复 push_cell 导致的多余空列
  - 列宽分配改为累积比例，空列不再吞掉剩余宽度
- **Clippy 兼容 Rust 1.98**：修复 `manual_slice_fill`、`drain_collect`、`chunks_exact_to_as_chunks` 三类新 lint
- **Windows CI 测试稳定性**：长前台命令测试改用 ping 模拟 sleep，消除 PowerShell 冷启动超时

---

## v0.6.47 — 2026-07-15

### Bug Fixes

- **provider_type 大小写不敏感**：settings.json 中 `type` 字段写成 `Anthropic`/`ANTHROPIC` 等大小写形式不再错误 fallback 到 OpenAI
- **Clippy 兼容 Rust 1.97**：修复 `input_field.rs` 的 `useless_borrows_in_formatting` 和 `markdown/mod.rs` 的 `manual_clear`

---

## v0.6.46 — 2026-07-09

### Bug Fixes

- **npm install 配置迁移字段格式修复**：`migrateFromClaudeCode` 输出改为 Rust 配置期望的 camelCase 字段（`type`、`apiKey`、`baseUrl`），修复新用户从 Claude Code 迁移后配置无法识别的问题
- **AgentShellSlot 计时器冻结**：任务结束后 `elapsed()` 不再持续增长，行为对齐 `BackgroundShell`

### Docs

- **README 默认英文**：`README.md` 切换为英文，`README_ZH.md` 保留中文版

---

## v0.6.45 — 2026-07-09

### Refactor

- **状态栏重构为 codebuddy-hud 风格**：动态 2-3 行布局，支持多工具并发跟踪、Git dirty 状态标记、上下文进度条优化

### Bug Fixes

- **clippy items-after-test-module**：将 `status_bar.rs` 测试模块移到文件末尾
- **tip-6 快捷键描述修正**：Ctrl+U/D → PageUp/Down（Ctrl+U 是删除到行首，Ctrl+D 是关闭 shell stdin）

---

## v0.6.44 — 2026-07-03

### Features

- **agent Bash 超时自动后台化**：BashTool 超时时不再无条件杀进程，支持通过 `auto_background_tx` 通知 TUI 将仍在运行的前台任务自动转后台继续运行
- **Windows cmd /C 引号修复**：移除 `has_cmd_special_chars` + `/S /C` 包裹，改用 `raw_arg` 传递命令文本，修复 `python "D:/x.py"` 引号泄漏到 argv 的问题
- **Ctrl+B 交互优化**：后台化后先聚焦底部 shell 入口，Enter 才打开面板，避免遮挡主视图

### Bug Fixes

- **clippy unused import + needless_borrow**：修复 `terminal.rs` 中 `timeout` 导入和 `&command` 引用的 clippy 错误

---

## v0.6.43 — 2026-07-02

### Refactor

- **状态栏工具名映射统一**：删除 `status_bar.rs` 中重复的 `format_tool_display_name` 函数，复用 `tool_display::format_tool_name`，修复 `FolderOperations` 在状态栏未简写为 "Folder" 的问题

---

## v0.6.42 — 2026-07-02

### Bug Fixes

- **后台面板输出自适应终端高度**（#110）：后台任务面板 output 区域根据终端高度动态调整，避免内容溢出
- **Ctrl+B 显示已运行时间**（#110）：后台 shell 任务在面板中展示已运行时长
- **Bash Ctrl+B 提示计时修正**：`Ctrl+B` 提示计时起始点修正，message pipeline 新增 transform/reconcile 阶段支持后台状态注入

---

## v0.6.41 — 2026-07-02

### Bug Fixes

- **MultiplexBroker 竞速跳过全 Reject**：ChannelBroker 无授权时不再抢先返回 Reject，MultiplexBroker 继续等待 TUI broker 的 Approve，消除「no authorized channels」误报
- **Ctrl+B 竞态**：`background_agent_foreground` 入口处 drain pending agent shell 注册，避免延迟注册未到达时 Ctrl+B 找不到前台 shell
- **/model 命令动态 alias**：改为从当前 provider 动态收集非空 alias 匹配，替代硬编码 opus/sonnet/haiku
- **Command 模式光标偏移**：draw_bar_cursor 改用 display_textarea 作为光标源，修正 ! 前缀移除后的位置偏移
- **tip 文案同步**：tip-2 改为「当前 Provider 的可用模型间切换」，tip-6 改为 Ctrl+U/D 滚动

---

## v0.6.40 — 2026-07-02

### Features

- **状态栏权限模式 cycle 提示**：权限模式标签后追加 `(Shift+Tab to cycle)` 灰色提示，方便用户发现快捷键

---

## v0.6.39 — 2026-07-02

### Features

- **跨 provider 模型切换**（#107）：model selection 值使用 `provider_id::alias` 格式，支持同名 alias 跨 provider 正确切换；模型列表展示所有 provider 的可用模型

### Bug Fixes

- **状态栏工具历史行对齐**（#105）：修复第二行在无内容时缺少前导空格导致与其他行左对齐不一致
- **Bash 输出预览窗口缩小**（#109）：Bash 输出截断阈值从 2000 行/100KB 降为 50 行/20KB，完整内容落盘供 Read 按需查看，避免大输出撑爆 context window

---

## v0.6.38 — 2026-07-01

### Bug Fixes

- **模型名上下文窗口标记过滤**：配置中 `mimo-v2.5-pro[1M]` 等含 `[...]` 后缀的模型名，传 API 时自动过滤为 `mimo-v2.5-pro`

---

## v0.6.37 — 2026-07-01

### Bug Fixes

- **状态栏第二行占位恢复**：修复工具执行前默认占位丢失的问题

---

## v0.6.36 — 2026-07-01

### Bug Fixes

- **状态栏第二行快捷键调整**：默认 hints 从 `Tab ::切换模式` 改为 `Ctrl+O ::详情`；详细模式新增 `● Verbose` 标识 + `Ctrl+O ::退出详细`；format_hints 支持空描述跳过

---

## v0.6.35 — 2026-07-01

### Features

- **PermissionMode 循环跳过 DontAsk**：Shift+Tab 循环切换权限模式时跳过 DontAsk，顺序变为 Default → AcceptEdit → AutoMode → Bypass
- **shell_command_with_shell 新增**：支持显式指定 shell（powershell/pwsh/bash），Hook executor 现在正确传递 shell 参数

### Bug Fixes

- **status_bar DontAsk 渲染修复**：补回 DontAsk match 分支防止 non-exhaustive 编译错误，删除残留 hint 引用

---

## v0.6.34 — 2026-07-01

### Features

- **滚动条和状态栏优化**（#104）：优化 TUI 滚动条和状态栏的渲染效果

### Bug Fixes

- **Windows CI 流式输出测试超时**：修复 Windows CI 环境下 Python 流式输出测试超时的问题
- **滚动条测试修复**：修复滚动条相关测试用例

### Documentation

- 完善项目所有 crate 的文档

---

## v0.6.33 — 2026-06-30

### Bug Fixes

- **控制字符渲染异常修复**（#102）：修复控制字符和 ANSI 转义序列导致 TUI 渲染异常的问题
- **Clippy 警告修复**：`map_or(true, ...)` 改为 `is_none_or(...)`，适配 Rust 1.95 新增的 `unnecessary_map_or` lint

---

## v0.6.32 — 2026-06-29

### Bug Fixes

- **后台 shell 通知显示优化**（#100）：后台 shell 完成/等待输入的 XML 通知在 TUI 聊天区显示为可读的中文提示（SystemNote），而非原始 XML 标签。前台小命令快速结束不再打断对话流，仅后台化（Ctrl+B）命令注入通知
- **`/review` skill fallback 测试修复**：mock skill 名称从 `review` 改为 `deploy`，避免与内置 `/review` 命令冲突
- **`Stdio` import 条件编译**：`peri-middlewares/terminal.rs` 的 `std::process::Stdio` 加 `#[cfg(windows)]` 修复非 Windows 平台 Clippy unused-import 错误

---

## v0.6.29 — 2026-06-28

### Features

- **Ctrl+B 后台 Shell**（#99）：Shell 命令支持 Ctrl+B 转为后台运行，输出写入磁盘，支持后台任务面板查看
- **`/commit` 命令**（#93）：一键 git commit，自动生成 commit message
- **`/review` 命令**（#95）：PR 代码审查
- **`/export` 命令**（#95）：对话导出
- **Read 工具行范围显示**（#91）：Read tool header 显示 offset/limit 行范围
- **全局屏幕选区（ScreenSelection）**（#94）：新增基于渲染 Buffer 的全局选区，覆盖面板、状态栏、sticky header、bg agent bar、空白区域。与消息区域现有 TextSelection 跨区域衔接，松开鼠标自动复制到剪贴板，蓝色高亮显示。详见 [spec/features/screen-selection-prd.md](spec/features/screen-selection-prd.md)
- **消息区域 TextSelection 内容锚定**：选区以消息内容为锚（而非屏幕坐标），滚动后选区跟随内容，复制纯文本不受 buffer 渲染影响
- **双击选整行**：消息区双击用 TextSelection 选整行（纯文本），其他非 textarea 区域双击用 ScreenSelection 选整屏行
- **spinner / 总结行可选可复制**：`✻ Brewed for...` + 进度条等位于 messages_area 底部但不在 wrap_map 内的行，现可通过 ScreenSelection 选中复制
- **选区 auto-scroll 改进**：auto-scroll 仅在鼠标移出消息区域外时触发，区域内首末行可正常选中（修复"最后 1 行难选中"）
- **复制 toast UX**：复制成功后状态栏显示 "已复制 N 个字符" toast

### Bug Fixes

- **拖选溢出 panic**：`visual_row + scroll_offset` 计算改用 `saturating_add`，修复 `scroll_offset=usize::MAX`（初始/提交/scroll_to_bottom 状态）时拖选导致 exit code 101 的崩溃
- **安装脚本 tag 前缀**：`install.sh` / `install.ps1` 匹配 `npm-v*` release tag 前缀
- **Clippy warnings**：`ShellCommandPool` Default derive + `map_or` → `is_none_or`

---

## v0.6.22 — 2026-06-27

### Security

- **MCP OAuth CSRF 防御**（#82）：回调服务器注入 rmcp 生成的 state 参数，纵深防御 CSRF 攻击
- **Session UUID 校验**（#74）：`session/load` + `session/resume` 增加 UUID 格式校验和存在性校验，防止路径穿越
- **文件权限加固**（#81）：history_persistence 文件权限 0600，grandparent 目录权限校验
- **At-mention 目录注入防护**（#77）：防止 `@path` 引用越权访问
- **工具输出截断加固**（#80）：防止超长输出绕过截断机制

### Bug Fixes

- **输入框鼠标乱码**（#88）：移除 `?1003h`（any-event tracking），防止 ConPTY 缓冲区溢出导致 SGR 鼠标转义序列泄漏为文本
- **Windows Ctrl+C 双击退出**（#86）：100ms debounce 防止 ConPTY 重复事件
- **AskUser 弹窗高度**（#76）：修复弹窗高度计算错误
- **Tab 缩进编辑**（#76）：修复 tab 缩进文件的 Edit 工具匹配问题
- **Grep offset 测试**（#86）：兼容 `persist_truncated_output` 附加行

### Chore

- npm 版本 bump 到 0.6.22

---

## v0.6.21 — 2026-06-27

### Features

- **Windows Git Bash fallback**：`cmd /C` 执行 Linux 命令（`grep`/`ls`/`find` 等）失败时自动 fallback 到 Git Bash，Agent 无需自行重试
  - 多语言 stderr 匹配（English/中文/法语/德语）+ 兜底模式
  - `GIT_BASH_PATH` 环境变量支持，`bash --version` 验证
  - `MSYS_NO_PATHCONV=1` 防止 MSYS 路径转换
  - 剩余超时继承（总超时 - cmd 耗时，至少 10s）
  - `git commit -m` 重写与 fallback 的 temp 文件清理时序修复

---

## v0.6.19 — 2026-06-26

### Bug Fixes

- **Windows GBK 编码修复**：`shell_exec.rs`（TUI `!command`）和 `executor.rs`（hook）的 subprocess stdout/stderr 在 Windows 中文环境下正确解码 GBK→UTF-8，不再显示乱码
- 共享 `decode_output_bytes()` 提取到 `peri-agent/encoding.rs`，消除重复代码
- `shell_exec.rs` 中文 anyhow context 消息改为英文

---

## v0.6.18 — 2026-06-26

### i18n

- **peri-lsp 硬编码中文改英文**：error.rs 12 处、transport.rs 8 处、client.rs 4 处、pool.rs 8 处，共 32 处 `#[error()]` / tracing / 错误字符串
- **peri-agent 硬编码中文改英文**：sqlite_store.rs 2 处、filesystem.rs 4 处，共 6 处 anyhow context 消息

---

## v0.6.17 — 2026-06-26

### i18n

- **spinner 和 thinking 块跟随语言设置**：peri-widgets 新增 `set_mode_with_label()` / `pick_verb_from()` 接口，TUI 调用方通过 `lc.tr()` 传入翻译后的 label。用户 `/lang en` 后 spinner 显示 "Thinking…"，`/lang zh-CN` 显示 "思考中…"
- 新增 `spinner-thinking` / `spinner-tool-use` / `spinner-responding` / `spinner-thinking-header` 翻译 key（en + zh-CN）

---

## v0.6.15 — 2026-06-25

### Bug Fixes

- **TUI 模型切换快捷键收敛**：删 Ctrl+Shift+T / Alt+Shift+M（与 Ctrl+P 命令面板重叠），统一 Ctrl+P 作为 Provider/Model/Effort 完整选择入口
- **Ctrl+T 硬编码 alias bug**：原硬编码 `[opus, sonnet, haiku]` 三选一，未按当前 Provider 实际配置过滤——切到只配 1 个 alias 的 Provider 时无法切换。改为从激活 Provider 的 `ProviderModels` 动态收集非空 alias

### Documentation

- CLAUDE.md 新增「PR / Issue 流程」+「分支命名规则」段落
- 分支名禁用 `#` 字符（会让 GitHub Actions `pull_request` trigger 静默失效）
- spec/issues/ 补模型切换快捷键收敛 issue 详细分析文档

### Chore

- 清理误传的 `.claude/CLAUDE.md`（adim 钉钉/PowerShell/PHP 那套无关内容），加入 `.gitignore`

---

## v0.6.13 — 2026-06-25

### npm 包

- npm 包二进制命名统一为 `cc-code-*`（原 `peri-*`），与 CI workflow 对齐
- `install.js` 下载文件名、解压目标、Windows wrapper 全部改为 `cc-code`
- `bin/cc-code` wrapper 查找 `cc-code-bin` / `cc-code.exe`
- `npm/README.md` 命令示例更新为 `cc-code`
- 删除 `mimo-code-vs-peri-analysis.md`

---

## v0.99.14 — 2026-06-02

### Performance

- 全局分配器从 mimalloc 切换到 jemalloc，碎片管理更优
- tokio worker_threads 限制为 4，18 核机器节省约 56 MB 栈空间
- list_threads 排除 cached_context 大字段，每线程内存从约 1 MB 降至约 1 KB
- LlmCallStart.messages 改为 Arc\<Vec\>，消除每次 LLM 调用的全量 clone
- history_for_cancel 用 Option\<MessageId\> 替代完整消息 clone

### Features

- **Rewind 对话回滚**：双击 ESC 弹窗选择回滚点，支持 /rewind 命令
- **/gc 命令**：手动内存回收 + RSS/jemalloc breakdown 诊断

### Bug Fixes

- PermissionRequest hook 在 Bypass/AutoMode 下不应触发
- 从 ~/.claude/settings.json 加载全局 hooks + TUI 退出时 fire SessionEnd
- /clear 时关闭旧 session 防内存泄漏
- 过滤 ACP 下发命令中与本地注册表重复的条目
- AgentResult invoke 消息优化，防止 LLM 轮询循环

### Refactoring

- CLAUDE.md 拆分为子模块文件
- 提取 ACP 共享逻辑，消除 TUI/Stdio 重复代码
- 移除 /split 命令
