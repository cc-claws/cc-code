# Changelog

Perihelion Agent 版本变更记录。

---

## Unreleased

## v0.6.186 — 2026-10-23

### Fixes

- **Bash 从等待进入执行时状态行不再整段消失（#706）**：工具块有三个互斥渲染分支（后台运行 / 排队与等待审批 / 执行中），执行中分支原先要求 `elapsed >= 2s`，导致 `Queued → Running` 切换后、2 秒到达前上一分支已退出而本分支尚未进入，状态行整段消失约 2 秒，界面只剩工具头。现改为执行一开始即渲染裸 `Running…` 占住状态行；耗时秒数与 `ctrl+b` 提示仍保持 2 秒后才出现，短命令不因计时刷屏。配套修正渲染线程的跨阈值重建判定——原逻辑只检查缓存里「有没有 Running 行」，裸 `Running…` 出现后该判定恒为真，跨过 2 秒将不再重建、秒数与提示永不出现，改为比对期望的完整状态文本。
- **排队与审批阶段的指示器改回恒亮（#706）**：`Queued` / `AwaitingApproval` 的 `●` 恢复常亮，只有 `Running` 闪烁（800ms 周期不变）。对齐参考实现的 `isQueued` 独立分支设计：排队不是「活跃」，等待语义由同屏 `Waiting…` / `Waiting for permission…` 文案承载；两者都闪会让「在等审批」与「正在执行」无法区分。

## v0.6.185 — 2026-10-23

### Fixes

- **统一未完成工具指示器闪烁频率（#698）**：修复处于 `⎿ Waiting…` 或 `⎿ Waiting for permission…` 等待阶段时 `● Bash` 指示器不闪烁、以及在不同状态间闪烁节奏一快一慢严重视觉错位的问题。`ToolCallStatus::Pending` 与 `Running` 共享 `(tick / 4).is_multiple_of(2)` 呼吸算法（800ms 亮 / 800ms 灭），只要指示器呈灰色未完成状态即全局同频同相位闪烁；打通渲染线程心跳与增量刷新管线，避免无 Running 工具时动画定时器休眠。
- **状态栏无就绪 Agent 时不再显示占位（#696）**：当前工作目录下活跃根会话数为 0 时，状态栏第三行不再显示 `· ← 0 agents`，消除无意义占位并释放状态栏宽度。
- **只读工具纯动作行独立显示时首字母大写（#694）**：无前置思考块的独立只读工具组（如 `Read 1 file`）在消息区独立成行折叠展示时，渲染侧自动将首字母转为大写，消除裸小写开头带来的视觉突兀，同时保持续写场景（`Thought for 3s, read 1 file`）的小写格式不变。
- **中断结果行移除重复 ✗ 符号并修正文案大小写（#692）**：工具被用户中断后，结果行不再同时展示红 `●` 与 `✗` 重复失败符号，避免相邻折角视觉拥挤；文案统一为首字母大写的 `Interrupted by user`。

## v0.6.184 — 2026-10-23

### Features

- **Bash 实时输出 L3（#675）**：前台命令首次输出立即采样，后续每秒最多一次，并补发静默前被节流的尾部。stdout/stderr 共用 64 KiB 展示缓存，运行两秒后显示最后五个视觉行、剩余行数、耗时、显式 timeout 与字节数，详细模式展开保留尾部；复用完成态 ANSI、URL、JSON 与折行规则。新增 `config.tool_output_streaming`（默认 `true`，支持项目配置覆盖），关闭后保留排队、审批与运行阶段。完成、取消和后台移交清理展示缓存及定时器，主/子 Agent 按调用来源隔离；增量输出不写模型历史，完整最终结果与磁盘输出保持原有语义，print/stdio 忽略展示事件。

### Fixes

- **Bash 排队与审批 L2（#675）**：工具审批前先显示 `Waiting…`，人工审批按调用 ID 精确显示 `Waiting for permission…`。取消或拒绝直接闭合工具块，并发结果保持原始调用顺序；同批排队与开始事件只发布最终阶段。增量状态不写入模型历史，print 与 stdio 忽略排队事件，Write 快照仍在批准后的 ToolStart 采集。

## v0.6.182 — 2026-10-22

### Fixes

- **OSC 8 注入改到输出层，修掉「链接后面整行文字不落笔」（#671 真根因）**：`ratatui` 的 `Buffer::diff` 用 `symbol.width()` 判断一个 cell 占几列，并把后续 `width - 1` 个 cell 从差分里跳过不画。v0.6.178 把 `ESC]8;;URL BEL 文字 ESC]8;;BEL` 整段塞进 `Cell` symbol，而序列载荷（URL）是可打印字符——`width()` 因此等于 URL 长度，链接后面几十个 cell 被整段跳过。线上表现：URL 只剩首字母（`origin h`），紧随其后的文字错位或残留上一帧内容。现改为 buffer 只存纯文本，超链接在 `WidthSafeBackend` 写字节时按坐标查表包裹（`terminal_backend/hyperlink.rs`，线程局部、每帧重置）；附回归测试锁定「buffer 内不得出现转义序列」与「链接后面的 cell 必须照常落笔」。
- **工具结果与 `!` 命令输出里的 URL 变成真超链接（#671 补全）**：v0.6.178 只给消息区的 Markdown 链接（`[文字](url)`）注入了 OSC 8，工具结果、`!` 命令输出、以及没有显示文字而退化成纯 URL 的链接仍是纯文本——终端不知道它是链接，悬停无下划线提示、Ctrl/Cmd+点击 无反应，鼠标单击也不会打开浏览器。同时这批 URL 被硬塞了 ANSI Cyan + 下划线，色号与修饰都与参考实现的 `createHyperlink`（`chalk.blue` + OSC 8、不加下划线）对不上。现在只在工具结果与 `!` 命令输出这两种消息里扫 `http(s)://` 并登记链接命中区（AI 正文、代码块与用户输入一律不参与，与参考实现的 `OutputLine` 口径一致），OSC 8 与点击命中共用同一份数据；`style_urls()` 随之删除，URL 改用亮蓝（`38;5;12`，不加人工下划线；参考实现源码为 `chalk.blue`=SGR 34，实测该终端下 34 偏深，12 号槽才与参考实现观感一致），悬停样式交给终端。识别口径不变（`find_url_ranges`，与参考实现 `OutputLine.tsx::URL_IN_JSON` 同源）。
- **OSC 8 启用条件不再借道 ConPTY 悬停能力**：原门控用 `conpty::hover_available()`——它衡量的是「宿主是否上报 ConPTY 鼠标悬停事件」，与「终端是否支持 OSC 8」并不等价，VS Code 集成终端、tmux、SSH 远端等会被判 false，于是连正文里的 Markdown 链接也点不动。改为默认开启，仅 `DISABLE_HYPERLINKS` 显式关闭；不认 OSC 8 的终端会丢弃该序列。

## v0.6.181 — 2026-10-22

### Fixes

- **运行中 Bash 的计时与指示器不再被事件流饿死（#683）**：长时间运行且期间无输出的命令，`Running… (Xs)` 秒数会停在某个值不再增长，块首 `●` 动画同时冻结，只有按 `Ctrl+O` 这类改变渲染键的操作才让它跳一次。根因是渲染线程每轮循环都重新创建 200ms 的 `sleep`：只要事件间隔短于 200ms（流式输出、每帧视口上报），sleep 就在到期前被 `rx.recv()` 抢先并丢弃，tick 一次都执行不到；而 `rebuild` 补不上这个洞——渲染键 = `content_hash ⊕ detail/diff 标志`，不含流逝时间，内容不变时直接复用旧行。现改为跨事件单调推进的 `tick_deadline` + `sleep_until`，`select!` 加 `biased` 保证 deadline 到期时 tick 分支优先被选中；`live` 转 false 时重置 deadline，避免积压后连发。新增回归用例 `test_worker_tick_not_starved_by_viewport_flood`，在 50ms 间隔的视口事件下断言秒数持续刷新。
- **发版重跑时 tag 存在性改走远程判断（#682）**：重跑 release 流程时不再用本地状态判断 `npm-v*` tag 是否已存在，改查远程，避免本地残缺状态误判导致重复打 tag 或漏发。

## v0.6.180 — 2026-10-22

### Fixes

- **Bash 执行状态 L1（#675）**：工具块使用显式执行阶段，空输出完成后显示 `Done`（详细模式为 `(No output)`），不再保留运行指示。显式 timeout 以 `Running… (Ns · timeout Nm)` 展示，并在完成结果底部保留尾注；后台化提示使用悬挂缩进，子 Agent 不显示该快捷键。历史恢复一律为完成态。
- **npm 发布校验等待 registry 异步处理（#680）**：v0.6.179 的发布在校验环节因 registry 尚未同步完成而中断；现发布校验改为等待 registry 异步处理完成后再确认结果。

## v0.6.179 — 2026-10-22

### Changed

- **npm 二进制改由平台可选依赖分发，去掉安装脚本（#678）**：主包 `@cc-claw/code` 不再在安装期执行脚本下载二进制。二进制拆成五个平台包（`@cc-claw/code-win32-x64`、`@cc-claw/code-darwin-x64`、`@cc-claw/code-darwin-arm64`、`@cc-claw/code-linux-x64`、`@cc-claw/code-linux-arm64`），经主包 `optionalDependencies` 引用，npm 按宿主 `os`/`cpu` 只装对应一个；`bin/cc-code` 启动器从平台包内定位并拉起二进制。安装过程除 npm registry 外不访问网络，也不执行任何安装脚本。

## v0.6.178 — 2026-10-22

### Features

- **消息区 Markdown 链接输出 OSC 8 终端超链接（#671）**：此前消息里的链接只能看到纯文本 URL（或带下划线的链接文字），在支持 OSC 8 的终端（Windows Terminal、iTerm2、WezTerm、kitty 等）里**点不动**——悬停无链接提示、Ctrl/Cmd+点击 无反应、右键菜单没有「打开链接」，只能手工复制 URL。根因是终端本身不知道哪段文字是链接：缺少 OSC 8 转义序列（Markdown 层早已产出 `LinkHit` 命中区，鼠标点击本就能打开，但终端侧无从得知）。现按 Codex CLI 的 `terminal_hyperlinks` 思路落地：转义序列**只注入 `Cell` symbol**，不参与任何布局或折行计算，注入点在 `Paragraph` 渲染进 Frame buffer 之后。新增 `cc-tui/src/ui/osc8.rs` 负责 grapheme 命中区→显示列换算（全角字符占两列）、逐 cell 包裹，以及 URL 控制字符剥离（否则含 `\x07` 的 URL 能提前终止序列并注入任意终端指令）；`message_area` 的 `ViewportClip` 增加 `link_rows`，在 cache 读锁内一次算好视口内的「行偏移 + 列区间」，输出层只做坐标平移。仅在终端能吃 VT 序列时启用，否则保留纯文本 URL，不把裸转义序列画到屏幕上。

### Fixes

- **宽度适配后端不再把终端转义序列当作字形测量（#671）**：本项目输出层的 `WidthSafeBackend` 会把非 ASCII 字形拿去探针**实测列宽**并在不符时降级替换——这是 Codex CLI 没有的环节。OSC 8 序列若流到这里会被当成宽字符测量，导致链接失效且整行错位。此前只靠各探针自觉（Windows 探针见控制字符返回 `None`），而探针替身与未来新增的探针并不保证如此，实测确实会去测量。现改为**适配器层强制守卫**：含控制字符的 symbol 一律原样输出，不再依赖下游探针。附带回归测试 `test_width_safe_backend_passes_osc8_symbols_untouched`，同时断言转义序列不会进入列宽探测。

---

## v0.6.177 — 2026-10-22

### Fixes

- **`.md` 文件在 Write/Edit 预览中不再染色（#669）**：此前 `.md` 内容会按 Markdown 语法着色，标题与粗体出现明显颜色；而 参考实现 对同类文件没有任何可见着色——它虽然把 `md` 识别为 markdown，但 highlight.js 对 Markdown 只产出 `section` / `strong` scope，参考实现的三套主题 scope 表均未映射这两个键，颜色回退到默认前景色，`.md` 与 `.txt` 的输出逐字节相同。现从 `EXTENSION_LANGS` 移除 `md` / `markdown` / `mdown` / `mkdn`，`.md` 按未识别语言回退纯文本，同时作用于 Write 新建预览与 Edit diff。新增回归测试 `test_render_new_file_code_md_falls_back_to_plain_text`，并把 `test_detect_language_by_extension` 的 `.md` 断言改为 `None`；同步更正 `code-highlight.md` 的语言识别规则与 2026-10-20 审计报告中的旧结论。

---

## v0.6.176 — 2026-10-22

### UI / Style

- **Markdown 渲染全面对齐参考实现（#666）**：此前 Markdown 的标题配色、列表编号、引用条、分隔线、行内代码与表格框线均与 参考实现 有肉眼可见的差异，同一条回复在两边长得不像。现逐项对齐：① 标题 H1 为 `BOLD|ITALIC|UNDERLINED`、H2 及以下仅 `BOLD`，且**不再带前景色**；② 无序列表统一 `- `，有序列表按嵌套深度在「数字 / 字母 / 罗马数字」间切换；③ 引用块 `▎` 用 DIM、正文用 ITALIC，行首空格不带样式；④ 分隔线渲染为 `---` 且无颜色，删除线关闭；⑤ 行内代码改用 `Theme::permission()`（dark 下 `#B1B9F9`）作前景色；⑥ 链接在没有 OSC 8 支持时显示纯 URL 且不带样式，`mailto:` 显示为邮箱地址；图片输出其 URL。

- **表格对齐参考实现 `MarkdownTable` 框线表（#666）**：新增 `MIN_COLUMN_WIDTH=3`、`SAFETY_MARGIN=4`、`MAX_ROW_LINES=4` 三项约束，列宽由「最长单词宽（下限）/ 整格内容宽（理想）」计算后再分配，表头居中并在**每行之间**补齐 `├─┼─┤` 分隔线；单元格内折行按 grapheme 逐个保留行内样式，断点语义对齐 `wrap-ansi` 的 hard 折行。内容超出行数上限或可用宽度不足时回退为纵向键值渲染，避免窄终端下把表格挤成乱码。

### Tests

- **新增 Markdown 渲染对拍基线与 126 例金标准（#666）**：新增 `cc-widgets/tests/cc_markdown_parity.rs` 与 `fixtures/cc_markdown_golden.json`，逐 grapheme 比对纯文本与归一化样式标签（`b/i/u/d/s` 与 `#rrggbb`），用同一套用例验证本项目与 参考实现 的表现一致；14 项已知偏离单独登记，并额外校验「实义字符一致」与「行宽不超过渲染宽度」两条不变式。同时新增 `examples/md_dump.rs` 与 `docs/markdown-verify.md` 便于人工比对。

### Refactor

- **Markdown 解析入口与主题接口调整（#666）**：`parse_markdown_with_options` 增加 `hyperlinks` 开关，解析选项收窄为 `ENABLE_TABLES | ENABLE_TASKLISTS`，避免开启 `Options::all()` 后 math、footnote 内容被静默丢弃；`Theme` 新增 `permission()`（含默认实现），`CustomTheme` 相应新增 `permission` 字段。同时修复嵌套行内样式的 `inline_style_stack` 还原问题。

---

## v0.6.175 — 2026-10-22

### UI / Style

- **统一 Bash 及工具输出折叠截断提示为 `… +N lines (ctrl+o to expand)`（#661）**：此前 TUI 中工具结果截断存在两套不一致风格——diff 与新建文件预览使用 `… +N lines (ctrl+o to expand)`，而 Bash 工具结果、用户 `!` 本机命令输出以及错误摘要行显示为 `... (N more lines) (ctrl+o to expand)`。现抽象统一的 `format_expand_hint` 辅助函数，将所有截断提示统一为 `… +N lines (ctrl+o to expand)` 风格，包含单复数行支持（单数 `… +1 line`）与单行折行超额截断格式（`… (ctrl+o to expand)`）。

### Fixes

- **彻底移除浮层对视口高度的干扰，消除留白与全屏误切（#659）**：PR #658 试图通过 `total`/`persistent` 拆分缓解输入 `/` 后删除导致的视口留白，但 `frame_rows`（含浮层）仍会击穿终端高度（`frame_rows >= size.height`），在标准 24 行终端下将视口永久锁死全屏并造成大面积空白；且终端原生滚屏不可逆，视口收缩后屏底留白。现彻底重构：临时浮层（命令提示与 @ 文件提及）天然属于覆盖在输入框上方已有空间之上的 Overlay，且内部具备 `available` 空间自适应逻辑，不计入持久视口 `required_rows`，恢复单调递增视口判定，杜绝了抖动、留白和意外全屏锁死。

---

## v0.6.174 — 2026-10-22

### Fixes

- **输入 `/` 触发命令提示后删除，视口底部留白（#658）**：紧凑启动（growing）模式下，输入 `/` 会把命令候选浮层高度并入 `required_rows`，`ViewportState::prepare()` 的 `allocated_rows` 单调递增记住了这个高度；删除 `/` 后浮层消失、内容高度回落，但 `allocated_rows` 不收缩，多出的行全部分给消息区，底部成片空白。现把 `required_rows()` 拆成 `total`（含浮层，决定当帧视口面积）与 `persistent`（不含浮层，决定 `allocated_rows` 的持久记忆），新增 `ViewportState::prepare_persistent()`——浮层贡献只影响当帧 `area`，不再被持久记忆，浮层消失后视口高度正确回落。`prepare()` 保留原签名并委托给新方法，既有调用方与测试不变。附带回归测试 `test_hint_popup_expand_then_dismiss_leaves_no_blank_gap`。

- **Write 文件内容预览恢复（#654）**：修复文件新建、覆盖成功后只有结果行而无内容预览，以及追加预览重复内容的问题。旧内容改为在执行侧审批通过后、发出 `ToolStart` 前采集，经 ACP 元数据传给 TUI，主、子 Agent 与 StateSnapshot 重建共同使用，避免异步队列导致界面读到写入后的文件。工具入参、模型历史和真实写入行为保持原样，同内容仍不展示空 diff；不超过 500,000 字节的普通 UTF-8 旧文件参与真实 diff，其余沿用内容预览回退。快照仍是进程内有界缓存，不跨进程持久化。

---

## v0.6.173 — 2026-10-21

### Fixes

- **URL 链接背景与状态栏模型名对齐（PR #656）**：工具结果和 `!` 输出的 URL 显式使用终端默认背景，避免继承消息区域背景后，同为 ANSI Cyan 的链接与状态栏模型名出现可见色差；下划线和链接识别保持原样。

---

## v0.6.172 — 2026-10-21

### Features

- **欢迎卡显示模型档位与版本更新提示（#653）**：空会话首屏此前只有「CC Code  v<版本>」一行标题、当前目录一行和开始提示，用户看不出当前跑的是哪个模型、哪一档推理强度，也不知道手上这份二进制是不是最新。现补齐两处：① 标题拆成品牌名 ACCENT + BOLD、版本号 MUTED 两段，版本号按剩余宽度收口而品牌名永不截断；② 新增 `model · (effort effort)` 副标题，模型名取 `services.model_name`（`/model` 切换后同步更新）、档位取配置 `thinking.effort`，任一项缺失或未配 Provider 时整行省略——不编造默认档位。③ 探测到新版本时，用 WARNING `Update available! <当前> → <最新>` + MUTED `To update, run: cc-code update` 两行**替换**原来的开始提示。新增 `cc-tui/src/app/update_check.rs`：启动后在后台查一次 GitHub release 列表（只认 `npm-v*` tag，跳过 prerelease，按语义版本取最大值而非取列表第一条），整个请求压在 3s 超时内，结果经 oneshot 回传、事件循环 `poll_update_check()` 收取，渲染路径只读缓存、永不等待网络；任何失败（无网、限流、JSON 变体）都只记 `debug` 日志并保持正常欢迎页。每个进程只查一次，`CC_CODE_DISABLE_UPDATE_CHECK=1`（或 `CCCODE_DISABLE_UPDATE_CHECK`）可关闭；`cargo test` 下自动禁用，不产生真实请求。

### Fixes

- **空会话首屏不再显示无意义的 Cache/MEM 指标（#653）**：状态栏第三行在还没有任何 LLM 响应时一直挂着 `Cache - · MEM 19MB`——此时 Cache 必然是命中不了的占位符，MEM 也只是空转进程的基线值，两个数字既不提供信息，又把权限模式与快捷键提示挤到一边。现给这块内容加上「已有消息」判定：复用消息区欢迎卡的既有口径（`current_view_messages()` 为空即视为未开始），而不用 `session_start_time`——后者只在首条消息发出时置位，会把恢复历史会话和聚焦子 Agent 视口误判成「未开始」。隐藏仅限这两项，同一行的权限模式、后台任务、重试、MCP、LSP 与快捷键提示不受影响。

### UI

- **欢迎卡边框在中英文下严格闭合，高度恒为 7 行**：更新提示是**替换**而非追加开始提示——卡片内高由吉祥物的 5 行决定，开始提示占 1 行、更新提示占 2 行，正好补满那 1 行空位，因此启动占用不随「是否有更新」变化；无 Provider 时省略副标题、同时补上 `/login` 引导，两项抵消。同步更新 `TUI-STYLE.md` 的欢迎卡规格（新增「模型与档位」「更新提示」两行说明与高度不变量）。

---

## v0.6.171 — 2026-10-21

### Features

- **紧凑启动与随内容增长的终端视口（#652）**：终端此前一启动就占满整屏，空会话下方大片留白。现改为从 shell 输出下方紧凑启动，随输入、回复与面板按实际视觉行数增长，长满宿主高度后才锁定全屏：新增 `terminal_backend/viewport.rs` 与 `main.rs` 的增长起点、必要滚屏、单向全屏锁定与退出区域清理（同步输出切屏与首帧，避免切屏闪烁与退出残留）；`ui/main_ui/viewport_layout.rs` 统一内容与底部高度测量，`ui/render_thread.rs` 增加宽度重排完成通知；`ui/welcome.rs` 与 `message_area.rs` 渲染紧凑吉祥物欢迎卡作为纯 UI 前缀参与滚动，并在首条用户消息前保留一行空白；`app/text_selection.rs`、`event/mod.rs`、`main.rs` 修正非零视口起点及欢迎前缀下的复制、链接与高亮坐标；`app/panel_memory.rs` 恢复外部编辑器挂起前的终端模式，`terminal_backend/compatible.rs` 把清理范围限制为应用自己拥有的行。视口阶段只属于本次终端运行——折叠、新建会话和放大窗口都保留已占用高度与全屏锁定。

---

## v0.6.170 — 2026-10-21

### Fixes

- **工具结果与 `!` 命令输出里的 URL 链接色定稿为 ANSI Cyan（#649）**：此前裸 URL 用 `theme::cur().ide`（深色 `#4782C8`），随 `/theme` 切换漂移，且与正文 `text_soft` 明度接近，扫读时难以快速定位。现 `message_render/mod.rs::style_urls()` 固定用 `Color::Cyan` + 下划线，不随主题切换，与状态栏模型名 / 分支名同一口径。覆盖两个调用点：`format_prefixed_text()`（Bash / Read / Grep / Glob 等所有走 `⎿` 前缀的工具结果行）与 `format_shell_output_lines()`（用户 `!命令` 输出）。识别口径对齐参考实现 `OutputLine.tsx::URL_IN_JSON`，额外切掉句末 `)` `]` `}` `,` `;` `.` `:`；仍非 OSC 8 真超链接（ratatui cell 模型塞不了转义序列，`sanitize_display_text()` 会剥 `\x1b`），点击由终端自身 URL 识别提供。注意与 Markdown 链接区分：AI 回复正文里的 `[text](url)` 走 `theme.link()` = SAGE `#4EBA65`，不受本改动影响。

---

## v0.6.169 — 2026-10-21

### Features

- **长会话详情按视口渲染（#629）**：新增 `config.detail_render_strategy` 与 `/config`「详情渲染」选项，支持 auto/cache/virtual，保留 cache 兼容路径。auto 在估算详情达到 30 万行或成品达到 64 MiB 时升档；virtual 使用全局高度索引和逻辑内容锚点，只发布当前消息窗口，并将两种模式的离屏成品共用 16 MiB LRU 预算。跨窗口选区按源消息/行/字素导出，复制不阻塞输入；宽度、主题、历史切换立即使旧选区失效。修复 Resize 合并误吞后继控制事件及旧窗口回写滚动位置的竞态。该预算不覆盖源 VM、单条可见巨型消息、剪贴板或进程 RSS；单条消息内切片仍属 #627 边界。

---

## v0.6.168 — 2026-10-20

### Features

- **Bash 输出 JSON 逐行美化（OutputLine 装饰器，对齐参考实现 `OutputLine`）**：新增 `cc-tui/src/ui/message_render/decorators/output_line.rs`，Bash / MCP 工具结果统一过这一层：逐行尝试 JSON 美化——单行 JSON 展开为 2 空格缩进；带精度守卫（紧凑重序列化与原文去空白后不一致即放弃美化，避免大整数被降级、`1.10` 被改写等伪造输出）；顶层非 `{` / `[` 的行走快速路径跳过解析；整段超过 10,000 字符跳过美化。折行与截断口径同步统一：按显示宽度预折行、每行 `trimEnd`、折叠态只显示前 3 行 + `… +N lines (ctrl+o to expand)`。

### Fixes

- **diff 删除行三档配色、工具路径按参考实现显示、状态栏 Edit 命名**：diff 删除行改为三档深浅配色区分删除强度；工具调用中的文件路径按参考实现口径显示；状态栏把 Edit 操作显示为 `Edit`，不再误写为 `Update`。

---

## v0.6.167 — 2026-10-20

### UI

- **工具结果与 `!` 命令输出中的 URL 以链接色 + 下划线呈现**：在纯文本里识别 `http://` / `https://` 起头的 URL（手写前缀扫描，不引正则；句末的 `)` / `]` / `}` 等标点自动切除），着色在 span 阶段完成，与 JSON 美化、截断共享同一份文本口径。`!` shell 命令输出先按 ANSI 切段保留命令自带着色，再逐段挑出 URL 上链接色（`cc-tui/src/ui/message_render/mod.rs` 的 `find_url_ranges` / `style_urls`）。这不是 OSC 8 真超链接（ratatui cell 模型不支持），点击仍由终端自身的 URL 识别提供。

---

## v0.6.166 — 2026-10-20

### Performance

- **Ctrl+O 反复切换不再全量重渲染（#626，PR #631）**：此前 ToggleDetail 会清空全部消息的渲染哈希并对整段历史重渲染、重折行，270 条消息的会话单次切换约 6.9 秒、10,000 条消息约 239 秒（debug 实测）。现渲染线程引入三件套：①**渲染键**——内容哈希只与真正影响该消息渲染的维度（detail / diff 标志且仅当该消息类型对该维度敏感时）合成键，未变的维度切换直接复用已渲染行；②**双变体缓存**——每条消息同时保留展开 / 折叠两份渲染结果，切换时整表交换；③**分段 wrap 缓存**——折行结果按消息分段缓存（段键含逐行 epoch，指示器秒数、spinner 动画等行内容变更会使对应段失效），流式输出时只有尾段重折。实测：270 条消息热切换约 33ms、10,000 条消息热切换 1.38 秒；执行期（流式尾巴 + 运行中工具）反复切换与从零渲染逐行一致，有专门回归测试锁定（含合并前的两处回归修复：分段越界 panic、指示器行旧 wrap 复用）。

### Features

- **diff 与 markdown 渲染对齐参考实现（PR #638）**：diff 渲染新增逐行语法高亮（syntect；语法集换用 bat 的 two_face 集合，语言覆盖 75 → 220；新增 Monokai Extended dark / light 主题对齐参考实现 diff 配色），删除行配三档底色与词级高亮；markdown 块间空行改为只来自源文本（块级元素不再无条件补空行），收敛全量重建入口。配套领域文档与审计台账见 `docs/2026-10-20-diff-highlight-alignment-audit.md`。
- **候选浮层运行中交互对齐参考实现 useTypeahead（Agent 运行时浮层可操作）**：此前 Agent 运行中（loading）时斜杠命令/Skills 候选浮层只渲染不响应——`↑`/`↓`/`Tab` 被 `!loading` 守卫挡掉，「看得到列表却选不动」。现对齐参考实现 `useTypeahead.tsx` 语义：`↑`/`↓`/`Tab` 在候选存在时即可导航（不再按 loading 分流，`Tab` 同时避免运行中落入通用分支插入字面 Tab 字符）；`Enter` 仅在用户已通过 Tab/↑/↓ 明确选中候选时执行补全（只改输入框文本不提交），未选中时保持既有语义走 `handle_busy_input` 入队；`Esc` 分层关闭——先关候选浮层（不中断 Agent，`@` 提及弹窗关闭优先级同步前移与渲染一致），再按一次才中断运行，关闭时记录 `hint_dismissed_for` 抑制位，输入文本不变不再弹、编辑后自动恢复；`↓` 优先级调整为候选浮层导航 > 后台 shell 入口聚焦 > @ 提及导航 > 滚动消息区，解决「有后台 shell + 输入 `/xxx` 时浮层选不动」。涉及 `cc-tui/src/event/keyboard/normal_keys.rs`、`cc-tui/src/app/hint_ops.rs`、`ui_state.rs`（新增 `hint_dismissed_for`）。
- **「立即补充」（steer）遇前台命令 2 秒宽限期后自动转后台**：此前前台 Bash 命令阻塞时 steering 注入永远轮不到（只在本轮 LLM 调用前 drain）。现点「立即补充」时若恰有一条前台命令在跑，先登记 2 秒宽限期（`STEER_FOREGROUND_GRACE`，短命令如 `ls`/`git status` 自行结束即收口）；宽限期到仍在前台则自动移交后台（等价替用户按 `Ctrl+B`）并提示「前台命令已转入后台，补充信息将立即生效」（locale `queue-steer-backgrounded`）；多条前台并行不自动选择，与 `Ctrl+B` 行为一致；重复点不重置计时器。涉及 `cc-tui/src/app/shell_command.rs`、`queued_messages.rs`、`ui_state.rs`（新增 `SteerForegroundGrace`）。

### UI

- **状态栏权限模式标签增加 `⏵⏵` 前缀符号**：对齐参考实现 `PermissionMode.ts` 的 symbol 字段，`auto`/`bypass` 标签前显示 `⏵⏵`（`cc-tui/src/ui/main_ui/status_bar.rs`）。
- **状态栏行首缩进与工具段截断收敛**，抽屉未选中行配色对齐提示行。

### Fixes

- **Glob 遍历禁用符号链接跟随并加安全上限（PR #636）**：文件遍历此前 `follow_links(true)`，在 `/proc`、`/sys` 等虚拟文件系统上会因符号链接指数级膨胀；现禁止跟随符号链接、跳过虚拟文件系统根目录，并加遍历条目数（500,000）与时长（10 秒）上限，超限返回已收集结果并标记截断。
- **主题切换后全量重建并推送提示**：切换主题后消息区按新调色板全量重建（不再残留旧主题颜色），并弹出切换提示。
- **抽屉导航不再改写输入框**：主会话抽屉导航时输入框压暗为定稿行为，子视口内例外；子 Agent 视图内忽略 BackTab 切换；**后台任务面板只列后台 shell**，不再重复展示子 Agent 与前台命令。

---

## v0.6.165 — 2026-10-20

### Fixes

- **`cc-code update` 恒定报安装脚本 SHA256 mismatch，升级通道被永久堵死（#632）**：update 流程从浮动的 `main` 分支下载 `scripts/install.sh`，但期望的 SHA256 是编译期硬编码常量（`cc-tui/src/update.rs`）。`scripts/install.sh` 在 `main` 上任何一次改动都会让所有已发布版本的校验失败，且提示文案误报为「脚本可能被篡改」。现令下载地址与校验值同源：`cc-tui/build.rs` 从 HEAD 提交的 blob 原文计算 SHA256（避开 CRLF checkout 差异），并把同一 commit sha 注入为 `CC_INSTALL_SCRIPT_REF`，`update.rs` 据此拼出 `raw.githubusercontent.com/cc-claws/cc-code/<commit>/scripts/install.sh`，`install.ps1` 同步钉到同一 commit。已发布版本各自拉取自己构建时那个提交的脚本副本，脚本后续演进不再影响升级通道；`UPDATE THIS when scripts/install.sh changes` 的人工约束随之移除。无 git 环境（源码包）回退到 `main` 并输出构建警告。注意：本次修复之前发布的旧版本内置旧哈希，需手动执行一次安装脚本才能升到本版本。

---

## v0.6.164 — 2026-10-19

### Fixes

- **tmux 内复制改走 OSC 52 直写 client tty（#624，PR #625）**：tmux 环境下原复制路径依赖外部剪贴板命令 / tmux 缓冲区中转，复制经常失效。现检测到 tmux 时直接向客户端 tty 写 OSC 52 转义序列，由终端模拟器本体设置系统剪贴板，绕开 tmux 的中转限制；附真实 tmux 会话的端到端传输测试锁定该路径。

---

## v0.6.163 — 2026-10-19

### Fixes

- **jemalloc 启动配置从未生效（#622，PR #623）**：`alloc_config.rs` 通过环境变量设置的是 `MALLOC_CONF`，但 tikv-jemalloc 根本不读这个名字——它只认 `_RJEM_MALLOC_CONF` 环境变量或静态符号 `_rjem_malloc_conf`，此前所有分配器调优（decay、后台线程等）一直在空转。现改为在二进制内导出静态 `_rjem_malloc_conf` 配置符号；发布版二进制已实测验证调优真正生效（以 `stats_print` 运行可见配置被读取）。
- **ripgrep 包内二进制永不落地（#620，PR #621）**：安装脚本解压 ripgrep 的 tar.gz 时失败，导致发布包内的 `bin/rg` 缺失、依赖 rg 的功能不可用；现修正解压路径处理，rg 随安装正确落地。

---

## v0.6.162 — 2026-10-19

### Fixes

- **普通模式下 Bash 单行超长 JSON 截断失效刷屏，及 WebSearch 详细模式长参数展开修复（#618）**：
  1. **按折行视觉行截断工具输出**：此前普通模式（非详细模式）下工具输出的 3 行截断判定（`max_lines = 3`）在折行前按 `\n` 换行符计算逻辑行。当 Bash 输出单行无换行的巨型 JSON 时，因逻辑行数仅为 1 行而绕过截断，并在推入渲染区时被终端宽度硬折为上百行视觉行导致刷屏。现重构输出流渲染为 `push_visual_capped_output`，统一在折行后按累计视觉行数配额精确截断（严格限制 ≤3 视觉行），并自适应显示 `... (ctrl+o to expand)` 或 `... (N more lines) (ctrl+o to expand)`。本地 `!` shell 命令同步采用该视觉行截断机制。
  2. **解除 WebSearch / WebFetch 参数的过早硬截断**：此前 `WebSearch`（`query`）和 `WebFetch`（`url`）在消息入队摘要时被硬编码 `truncate(s, 60)` 切成带 `...` 的残串，导致用户在按 `Ctrl+O` 开启详细模式时下游无法还原完整参数折行展示。现将入队字符上限放宽到与 `Bash` 一致的防御上限 `TOOL_ARGS_MAX_CHARS = 4000`，由下游渲染层自适应终端列宽做单行截断（普通模式）或完整折行展开（详细模式）。

---

## v0.6.161 — 2026-10-19

### Refactor

- **面板/弹窗光标指示符与配色统一（#615）**：`TUI-STYLE.md:355` 早已约定「面板/弹窗选中行统一 `❯` + THINKING 色」，但代码里 16 处各自手写 `"❯"` / `"❯ "` / `"\u{276f}"` 三套字面量，配色也混用 `selected_fg` / `claude` / `thinking`，改一次光标样式只能逐文件搜索替换，且容易写成新的转义形式。现 `cc-tui/src/ui/theme/mod.rs` 提供唯一入口 `SYMBOL_CURSOR` / `SYMBOL_CURSOR_SPACED` / `cursor_style()`（THINKING 色）并附契约测试，agent、cron、hooks、login、mcp、memory、model、theme、command_palette、thread_browser 面板与 ask_user、hitl_content、hints、setup_wizard、rewind 弹窗及 at_mention 下拉全部改用该入口。真实变色仅两处：会话浏览器光标 `selected_fg` → THINKING、回滚弹窗光标 `claude` → THINKING，其余为等价替换（符号仍为 `❯`、宽度仍 2 列，布局与键位不变）。输入框提示符、UserBubble 消息前缀、sticky header 装饰前缀语义不同，刻意保留原样。

---

## v0.6.160 — 2026-10-19

### Fixes

- **后台子 Agent 运行中完成时，同一份结果被重复投递给模型（#613）**：后台任务在主 Agent 本轮尚未结束时完成，完整 output 会经两条通道各进一次上下文——`drain_notifications` 以 `BaseMessage::human(result.to_notification())` 写入 history（`cc-agent/src/agent/executor/final_answer.rs`），自动续接再由 `inject_bg_result_messages` 注入合成的 `AgentResult` tool_use + tool_result（`cc-acp/src/session/executor.rs`）。两份文本同源逐字相同，模型在同一段对话里看到两遍，续接轮退化为复述上一条回答（实测会话 messages 109 → 113，续接轮回复即「已在上一条消息中完整给出」）。现 `inject_bg_result_messages` 在注入前按 `[Background task {short_id} ` 前缀（与 `to_notification()` 同源的 8 位 id，`completed` / `failed` 共用）在 history 中判重，命中即跳过该条合成注入；全部命中时不再追加空的 AI 工具调用消息。三种时序均正确：工具步骤后 drain（history 已有 → 跳过）、final answer 后 drain（history 已有 → 跳过，续接轮仍能在 history 中看到）、`Done` 之后完成（history 无 → 正常注入且仅一次）。新增 3 个单测覆盖全量跳过、部分跳过与失败通知判重。

---

## v0.6.159 — 2026-10-19

### Fixes

- **取消 Spinner 动词的强制中文多语言分流，统一使用英文动词（#611）**：此前 Spinner 动词按界面语言强制切到机翻中文列表 `ZH_VERBS`，导致出现如“漂移中”、“光合中”、“编舞中”等生硬突兀的直译词。现将 `DEFAULT_VERBS` 与 `verbs_for_lang` 统一指向原版英文趣味动词列表 `EN_VERBS`（如 `Thinking…`、`Drifting…` 等），不再按语言分流；单测同步对齐。

---

## v0.6.158 — 2026-10-18

### Fixes

- **macOS bash 用户装完命令找不到（#585，PR #606）**：`scripts/install.sh` 把 PATH 一律写进 `.bashrc`，但 macOS 的登录 shell 读的是 `.bash_profile`，bash 用户安装后新开终端仍然没有 `cc-code`。现按平台与 shell 类型选择正确的 rc 文件写入。
- **从外部编辑器返回后 TUI 残缺（#587，PR #605）**：在外部编辑器（如 `$EDITOR`）里编辑输入返回后，终端画面残缺错位；现返回时强制清屏并全量重绘一帧。

---

## v0.6.157 — 2026-10-18

### Features

- **`/theme` 新增 `auto`，按终端颜色能力与背景明暗自动选主题（#604）**：此前 `/theme` 只能显式选一套配色——Apple Terminal 这类只有 16/256 色的终端一旦选中 `light` / `dark`（24-bit 真彩色调色板）就会退化，而切系统深浅色或换终端跑同一份配置时还得手改 `settings.json`。现 `theme = "auto"` 由新模块 `cc-tui/src/terminal_theme/` 两级判定：① **颜色能力**——`TERM_PROGRAM=Apple_Terminal` 记为 `Ansi`，其余记为 `TrueColor`；② **背景明暗**——先发 OSC 11 查询（`\x1b]11;?\x07`），按回复的 sRGB 相对亮度判明暗（阈值 0.5，接受 8/16 bit 分量与 BEL / ST 两种终止符），终端不应答时 macOS 退回系统外观（`defaults read -g AppleInterfaceStyle`，键不存在即浅色），其余平台按深色兜底。两维组合映射到既有的 `light-ansi` / `dark-ansi` / `light` / `dark` 四套主题，`default` 与 daltonized 偏好不参与自动选择。判定结果缓存进 `OnceLock`（`theme::cur()` 在渲染热路径上，子进程探测最多发生一次）；OSC 往返在 `enable_raw_mode()` 之后、`EventReader::start()` 之前由 `terminal_theme::preload()` 完成（回复从 stdin 原始字节读入，事件读取器一旦启动就会抢先消费），且仅在持久化主题为 `auto` 时预探测，显式主题的用户不付这笔启动开销，stdin / stdout 任一侧不是 tty 时直接跳过以免吃掉管道输入。`/theme` 面板该行显示 `Auto (follow terminal: <解析结果>)`，面板高度随 `ThemeName::ALL` 伸缩。**刻意不从 Terminal.app 的 `Default Window Settings` profile 推断背景色**：那只是下一个新窗口用哪个 profile，窗口可在 `Shell > Show Inspector` 里单独指定（本机默认 profile 是黑底的 Pro，实际窗口却是白底的 Basic），据此判定必错，该链路已删除。

## v0.6.156 — 2026-10-18

### Fixes

- **11 个 RSS 内存测试在 macOS 上读数恒为 0，测试等于空转（#589）**：这批测试统一用 `std::fs::read_to_string("/proc/self/status")` 解析 `VmRSS` 取进程 RSS，而 `/proc` 只存在于 Linux——macOS 上读取失败被 `unwrap_or(0)` 吞成 0，`saturating_sub` 再把「0 减 0」洗成「无增长」，断言永远是绿的。实测 `syntect_first_load_cost` 在 macOS 上打印 `RSS = 0 KB (+0 KB)` 且 `test result: ok`，11 个测试全部空转（同一 pid 用 `ps -o rss=` 读到 964 KB，证明确有读数可用，只是取错了通道）。现新增 `cc-tui/tests/common/mod.rs` 与 `cc-widgets/tests/common/mod.rs`（内容一致），改用 `sysinfo` 的 `process.memory()` 取 RSS / 虚拟内存，口径与 `cc-tui/src/alloc_config.rs` 一致；**采集失败直接 panic、绝不返回 0**（0 会被饱和减法洗成「无增长」，等于把测量失败伪装成通过）；删除 11 个测试文件里各自复制的 `/proc` 实现（含 `jemalloc_precise_attribution.rs` 的 `current_vsize_kb()` 与 `headless_app_real_rss.rs` 手写的 `windows_sys` 版），`headless_first_round_stages.rs` 的 `print_rss_delta` 去掉 `#[cfg(unix)]`，`cc-widgets` 的 dev-dependencies 补 `sysinfo`。另新增守护测试 `rss_measurement_sanity.rs`（`cc-tui` / `cc-widgets` 各一份，单独成测试二进制避免扰动并行测试）：触碰 64 MiB 后 RSS 必须涨 ≥32 MiB，反向验证时把读数改回 `return 0` 会立即失败（`RSS 基线读数必须 > 0，实得 0 KB`），测试基础设施自身空转能被当场发现。验证：macOS 实测基线 2244 KB → 50 条 markdown 后 8412 KB（+6168 KB），与 `ps -o rss=` 同 pid 交叉校验误差 32 KB（<2%），13 个受影响测试目标与 `clippy --all-targets -D warnings` 全绿。

## v0.6.155 — 2026-10-18

> 本条为事后补记：0.6.155 发版时漏更新 CHANGELOG，PR #596 / #598 均在该版中。

### Fixes

- **状态栏按主题取色，修复 Light 主题下白字贴浅底（#597）**：`/theme` 切到 Light 系列后，消息区变成浅底深字，状态栏却仍沿用深色主题的字面量（主文字 `Rgb(255,255,255)`、次要信息 `Rgb(153,153,153)`），白字压浅底几乎不可读；同时第一行走 ANSI 名色、第三行写死 RGB，换终端配色方案时同一根状态栏会劈成两种风格。`status_bar.rs` 新增 `SbColors`，按当前主题解析一帧渲染所需的全部颜色：`default` 主题逐值保留原有字面量（ANSI 名色 + 写死 RGB，观感零变化，守住 #522 的结论）；其余 6 套主题取 `/theme` palette 的 `cyan / yellow / magenta / success / warning / error / inactive / text / selection_bg / cursor_bg`；上下文仪表原本是 ANSI `Green/Yellow/Red`，与 `SAGE` 的 `Rgb(78,186,101)` 并非同值，故单列 `context_ok / context_warn / context_high`，避免默认主题被顶替；`ansi_style` 改名 `fg_style`（取色已不限于 ANSI 名色）。
- **markdown 缓存补字节预算，条数上限降为兜底（#594）**：`cc-widgets/src/markdown/cache.rs` 只有 `CACHE_CAPACITY = 1024` 的**条数**上限、没有**字节**上限，1024 条 × 平均 53 KB ≈ 52.8 MB 常驻（实测满容量 139258 行 / 952218 Span，最大单条 85 KB）；而 `make_key` 里含 `max_width`，每次 resize 都产生一批新宽度变体，把缓存灌满。现加 8 MB 字节预算、条数上限降为兜底，超预算按 LRU 淘汰；`diff/byte_cache.rs` 上提为 `cc-widgets/src/byte_cache.rs`，供 diff 与 markdown 共用；顺带修掉条数上限触发 `LruCache::put` 静默淘汰时不扣减字节、导致 `bytes` 单调虚高的既有缺陷（改插入前自行 `peek` + `pop_lru` 并扣减）；`/gc` 的 markdown-cache 行补字节预算读数。

## v0.6.154 — 2026-10-18

### Fixes

- **`/gc` 的差值不再被钳成 0（#593、#595）**：`OS RSS - jemalloc resident` 与「已知估算合计 - allocated」两个差值此前用 `saturating_sub` 计算，而被减的一方并不保证更大——实测 jemalloc resident（93.6 MB）大于内核 RSS（84.9 MB）时真差是 `-8.7 MB`，却被钳成 `= 0 B`，渲染出一个算术不成立的等式；而「差值为负」与「两边完全相等」在诊断上含义相反，钳位让两者不可区分。现统一改为有符号差，负值时分别补一条提示（resident 仍计入内核已不再统计的 madvise 页，两个口径不可直接比较／估算项互相重叠且按 `capacity` 估算含冗余）。同批修正三处同源噪音：删掉与 `RSS:` 行完全重复的 `OS RSS:` 行（同一个 sysinfo `process.memory()` 读数，只是整数 MB 截断后再相减，白丢 0.8 MB）；jemalloc stats 读取失败时不再用 RSS 冒充 `allocated`（否则 `allocated-RSS` 恒为 0，把测量失败伪装成「完全一致」）；macOS 上 `stats.retained` 恒为 0（`opt.retain` 默认 false），改为输出「不适用」而不是一个永远为零的读数。
- **`Ctrl+Enter` 在支持键盘增强协议的终端上真正生效（#592）**：`keyboard_modes.rs` 从未被 `lib.rs` 声明，`enable_keyboard_enhancement()` 全仓零调用点，应用因此从未向终端推送 kitty keyboard protocol——`Ctrl+Enter`、`Ctrl+I`、`Esc` 等组合在 iTerm2 ≥3.5 / WezTerm / Ghostty / kitty / Alacritty 上都拿不到区分信号，排队消息的「立即补充」快捷键随之失效。现接通该模块（只推送 `DISAMBIGUATE_ESCAPE_CODES`，不碰会改变按键语义的 `REPORT_EVENT_TYPES` / `REPORT_ALTERNATE_KEYS`），在 `enable_raw_mode()` 之后启用、正常退出与 SIGTERM/SIGINT 恢复路径各恢复一次；不支持的终端（如 Terminal.app）直接忽略该序列、行为与之前一致，另可用 `PERI_TUI_DISABLE_KEYBOARD_ENHANCEMENT=1` 关闭。
- **排队提示按平台显示可用的「立即补充」键（#591）**：`Ctrl+Enter` 只有在终端能把「Ctrl+Enter」与普通 `Enter` 区分开时才可用——Windows 的 crossterm 走 `ReadConsoleInput` 能读到修饰位，而 macOS `Terminal.app` 按下时发出的就是 `\r`，Linux 常见终端默认也未开启 kitty 协议，提示行却一律写死 `Ctrl+Enter`，这些平台上用户按键毫无反应。现按 `target_os` 选择文案：Windows 仍提示 `Ctrl+Enter`，macOS 改提示 `⌥S`，Linux 改提示 `Alt+S`；删行键三平台都可区分，统一保留 `Ctrl+X`。
- **macOS 启动时清屏一次，修复 Terminal.app 画面残留（#582）**：Terminal.app 的 alternate screen buffer 跨程序保留，上一次 TUI 崩溃/被 kill 时的最后一帧会留在屏上，而 ratatui 首帧是 diff 模式（只写与空 buffer 不同的 cell），未覆盖处就透出旧画面，表现为启动后黑条/旧画面残留。现仅在 macOS 启动时清屏一次并重置内部 diff buffer，Windows/Linux 不参与编译，启动行为保持不变。

### Style

- **状态栏三行行首缩进统一为 2 空格（#584）**：三行行首缩进原本各为 1 个空格，视觉上与内容贴得过紧；现统一改为 2 空格，三行左边缘对齐更清晰，测试断言同步更新。

## v0.6.153 — 2026-10-16

### Fixes

- **TUI 用户可见文案接线到 i18n，FTL 零死 key（#578）**：i18n 框架本身完整（en/zh-CN 各 550+ key 一一对应、占位符零错配、代码引用的 key 全部有定义），但大量用户可见文案没走 `lc.tr()` 而是硬编码在 Rust 里，导致中文界面残留英文、英文界面出现中文；同时 FTL 中有 93 个 key 从未被任何代码引用。现将命令反馈（`/doctor`、`/rename`、`/effort`、`/model`、`/export`、`/channel` 等）、面板与弹层标题、状态栏、消息渲染等处的文案统一接线到 i18n（新增 70 个 key、补齐 8 处未翻译条目），删除 72 个死 key，FTL 中已不存在未被引用的 key。布局用空格（面板标题 padding、缩进）从 FTL 值移回代码，避免被 FTL 吞掉；`is_compact` 不再依赖中文动词文案，改读 `compact_manual` 显式标志。影响范围限于 TUI 文案，无行为逻辑变更。
- **配置「内联 Diff」打开后不生效，Write/Edit 内容不显示**：`diff_enabled` 此前只写进配置文件并初始化会话的 ui 字段，但真正消费开关的渲染线程出生时 `diff_visible` 写死为 false、Rebuild 事件又不携带标志，全仓唯一会发 `ToggleDiff` 事件的只有运行时切换动作——走配置这条门（配置面板或改 settings.json，含重启）开关永远到不了渲染线程，普通页面上 Write/Edit 的 diff 从不渲染，Write 的正文内容因此完全看不见（Ctrl+O 详细模式不受影响）。现补齐两处接线：`ChatSession::new` 在会话创建时把配置的初始标志以 Toggle 事件同步给渲染线程；配置面板保存后同步当前会话的 ui 状态并补发 `ToggleDiff`，改完立即生效。渲染线程新增契约测试：Toggle 事件先于首个 Rebuild 到达时 Write diff 正常渲染、无事件时保持隐藏（#580）。

## v0.6.152 — 2026-10-16

### Fixes

- **等待后台子 Agent 期间终端标题保持动画（#575）**：主 agent 一轮结束时 `ui.loading` 会被清掉，即使后台子 Agent 仍在跑（那种情况只设 `agent_done_pending_bg` 并保持通道存活），而 `current_title_status()` 只看 `ui.loading`，于是交棒瞬间标题就落到 `Done`（固定 ✴）或 `Idle`（无图标）、动画停住——「还在等子 Agent」与「已经结束」在标题上无法区分。现补一档：不 loading 但仍有活跃后台子 Agent 时判为 `Working`，与主视口 `✻ Waiting for N background agents to finish` 同判据（`has_running_subagents`）；`ActionRequired` 的最高优先级不变。

## v0.6.151 — 2026-10-16

### Features

- **后台 shell 完成通知改为运行中插入，不再等轮次结束（#571）**：agent 运行中后台化（`Ctrl+B` / 前台超时自动转后台）的 Bash 命令跑完后，完成通知此前一律积压在内存队列、只有轮次结束（`Done`）才被 drain 注入——延迟上限等于该任务剩余时长（实测被积压十几分钟），而 `Done` 出口本身也不可靠：轮末优先 flush 用户排队消息会整段跳过通知的 drain，`handle_interrupted` / `handle_error` 两条分支根本不消费该队列。现复用既有的 steering 插入通道，把注入时机前移到「通知产生时」：运行中异步发起 steer 并每帧回收回执，空闲时仍合并为一条消息，跨会话通知仍留给对应会话；消费回执的确认时机同时钉在「真正写入 state 之前」（`cc-agent` 侧把 steering 消费点从轮首移到 `before_model` 之后、compact 之后），因此 `-32001`（未入队）与 `-32002`（已入队未消费）都代表未送达、可安全回队重试。连接中断等无法判定的错误只保留原文并记 `tracing::warn!`、不自动重投（宁可不送也不重复）；中断/报错后不自动唤醒新一轮，滞留通知随用户下一次主动提交一并送出。

### Fixes

- **系统提示写入后立即重绘，未知命令不再等到下次输入才显示（#573）**：空闲态输入不存在的 slash 命令（如 `/dream`）回车后界面毫无反应——提示其实已经写进消息状态，但 TUI 消息区是事件驱动渲染，`App::push_system_note` 只写 `view_messages` 而不发渲染事件，调用方必须自己补一次 `request_rebuild()`，而未知命令提示与「运行中不可执行」提示都漏了这一步，于是要等用户再敲一次键才显示出来。现把重绘内置于 `App::push_system_note`（该方法的语义本就是「添加一条系统通知」，可见性该由它保证），并去掉因此重复的显式调用；同类路径（`/rewind` 无历史、排队消息 steer 失败、切换会话失败等提示）一并修复。

## v0.6.150 — 2026-10-16

### Features

- **`/history` 改为输入历史面板，会话浏览迁移到 `/resume`（#566）**：`history` 在 CLI 语境里默认指「你输入过的内容」（shell 的 `history` 就是列历史命令），但 peri 的 `/history` 打开的是历史会话列表——想查自己输入过什么只能靠 `Ctrl+↑/↓` 一条条回溯，看不到全貌；数据本身早在 `ui.input_history` 里（且跨会话持久化），只是没有接出去。新增 `InputHistoryPanel`：一次列出多条输入历史，数据源与 `Ctrl+↑/↓` 完全同一份，`↑`/`↓`/`j`/`k`/`PageUp`/`PageDown` 导航、`Enter` 只把选中项**回填输入框**（不代发，与回溯语义一致）、`Esc` 关闭、空态有提示；多行条目折叠为单行并按显示宽度截断，光标行带标记与强调色、滚动跟随光标。**`/history` 语义变更（破坏性）**：现在打开输入历史列表，原「历史会话浏览」整体迁到 `/resume`（它原本就是 `/history` 的别名）；i18n、`TUI-STYLE.md`、ACP `AvailableCommand` 同步更新。

### Fixes

- **状态栏系统提示至少停留 5 秒（#564）**：Jev 语义门判定服务不可用（429/超时等）时，HITL 会通过 `SystemNotice` 播报「安全判定服务不可用，已降级到兜底分类器」，但它的清除时机比显示时机还积极——提示在 `before_tool` 阶段发出，紧接着就是本次工具调用的 `ToolStart`，而 `ToolStart` / `AssistantChunk` / `Done` / `Error` 四条路径都会把它置空，用户只看到闪一下、读不到降级原因。安全能力降级是用户应当知情的事实，现按仓库已有的「最短可见时长」范式（`TOOL_MIN_VISIBLE_MS`）给提示也垫一档：`SystemNotice` 记录 `visible_until = 创建时刻 + 5s`，清除点从「直接置空」改为「到期才清」，真正的抹除由事件循环每轮空转时的到期检查完成并触发重绘；语义是「至少 5 秒」而非固定 5 秒（期间无新事件则一直显示到下个事件），切换 thread 仍无条件清除、不跨上下文残留。
- **多轮对话后工具资源不再累积，中间件工具与会话池的引用环拆开（#568）**：中间件工具在每轮执行时转换为共享对象，但包装器未析构内部 `Box`，工具持有的资源持续累积；延迟工具代理与子 Agent 模型工厂还反向持有注册表及会话池，阻止资源在所有者释放后回收。现为工具包装器补上唯一析构（最后一个 `Arc` 释放时才析构内部 `Box`，此前每轮收集的工具及其资源都会泄漏），`execute_tool` 改用弱引用访问共享注册表（仅在查找目标时临时升级，执行期间只持有目标工具），`builder` / `agent_pool` 将子 Agent 模型工厂对会话池的引用改为弱引用并保留存活会话的模型缓存复用。最后一个持有者释放后正常析构，已开始的工具调用可在注册表释放后继续完成，会话结束后后台工厂仍能直接创建模型。

## v0.6.149 — 2026-10-16

### Features

- **Agent Center 详情面板重排与预览 Markdown 化（#554）**：详情面板换行位置改为「短字段同行、长字段标签独占一行」——`Model:` / `Reasoning:` 标签与值同行，`Project` / `Branch` / `Last message` / `Prompt` 标签独占一行、值另起一行，块与块之间留空行分隔；标签统一 muted，值沿用状态栏配色（模型青、项目黄、分支青）。`Last message` / `Prompt` 预览改走消息区同款 markdown 渲染（`**粗体**`、行内代码、列表、链接正常呈现）并按显示列宽折行，长链接不再被 `…` 截断；高度分配改为除 `Prompt` 预留 3 行外全部给 `Last message`（原先两段对半分配且各封顶 3 行）。列表默认分页 `PAGE_SIZE` 10 → 20。

### Fixes

- **状态栏会话计数改 Ready 口径（#554）**：状态栏 `← {n} agents` 原先统计当前目录累计历史会话，长期使用的目录下可达数百条（实测 194 条），与用户在 `/agents` 里实际在用的会话数严重不符。改为与 Agent Center `Ready` 过滤同口径——只统计当前目录 24h 内有更新、且未进入 Needs you / Working 的根会话（实测 194 → 10）。pill 文案保持不变。
- **「内联 Diff」开关不再顺带展开整页（#558）**：该开关按设计只决定**普通页面**是否内联显示 Write/Edit 的 diff（不喜欢看的用户可关掉，需要时按 `Ctrl+O` 去详细页面看）。此前它与 `Ctrl+O` 详细模式状态被合并成同一个渲染标志，打开后会连带放大 shell 输出行数上限、强制展开所有工具（Read 除外）、取消工具参数折行、展开 reasoning 与粘贴内容。现改为两个标志分开传递：普通页面只多出 diff 块，其余展开行为保持折叠档；详细模式语义不变。
- **删除确认弹层底色（#554）**：原先误用 `theme::cur().background`——该字段在所有主题下都是品牌青绿色（暗色主题 `rgb(0,188,212)`、ansi 主题 `Cyan`），铺满整块弹层极为刺眼；改用弹层专用的 `popup_bg`（暗色主题黑、亮色主题白）。
- **Agent Center 过滤器提示文案（#554）**：`tab/⇧tab filter` → `tab/shift+tab filter`，与仓库其它位置（`welcome.rs` 的 `Shift+Tab`）及该页脚的小写全写风格统一。

## v0.6.148 — 2026-10-16

### Features

- **Bash 失败退出码置顶与正文整齐缩进（#551）**：在 TUI 终端渲染层优化命令失败体验。当 Bash 返回非零退出码时，将其提升为首行置顶的 `  ⎿ Error: Exit code {code}`（红色醒目展示），自动剥离底层追加在输出末尾的 `[Exit code: {code}]` 标签及多余空行，输出正文整体以 `    ` 4 列悬挂缩进对齐，彻底解决长输出时错误原因被淹没在屏外的问题。

### Refactor

- **`message_render` 目录级修饰器解耦（#551）**：重构 `cc-tui/src/ui/message_render.rs` 单文件（90KB+），升格为目录模块；提取独立的 `decorators/` 子模块（`bash`、`glob`、`read`、`ask_user`），将工具输出展示修饰与富文本换行、链接命中等纯排版算法解耦，并为各修饰器配齐纯逻辑单测。
- **全仓历史遗留测试命名全英文规范化（#549）**：清扫全仓早期遗留的 66 个中文及中英混排测试用例命名，统一重构为标准全英文 `snake_case`（`test_<被测对象>_<场景>`），代码标识符纯英文达标率 100%。

## v0.6.147 — 2026-10-16

### Features

- **跨项目 Agent Command Center（#542）**：`/agents` 打开全页根会话中心，按 Project / Status / Model 分组，提供 All / Needs you / Working / Ready / Inactive 五档过滤、10 条真分页与 Show more、标题搜索、重命名、永久删除二次确认、150ms 防抖详情（Model / Reasoning / Project / Branch / Last message / Prompt）。空草稿且无弹窗时按 `←` 打开，状态栏提示当前项目会话数；窄屏隐藏右栏，长标题和中文路径按终端列宽截断。
- **会话目录跟随恢复目标**：跨项目 Enter 与 `n` 使用目标会话目录，系统提示词按 ACP session 冻结，文件补全、相对路径、hooks、MCP、技能与分支缓存随目录切换；不改变进程 cwd。运行中任务、Shell、排队消息或审批未结束时拒绝切换，避免中断和结果串台；Enter 当前会话直接返回。`n` 创建空白草稿，首次发送前不落库；`o` 复用当前项目 `/history`。
- **会话状态与快照持久化**：根会话轮次持久化 active / done / cancelled / error，新增 model / reasoning_effort / branch 独立列并幂等迁移旧库。SQLite 先分页线程再 JOIN 当页消息统计，计数索引不读取消息；文件系统后端读取轻量索引、仅统计当页文件。过滤与搜索作用于已加载页，计数覆盖全部根会话。仅本进程实时显示 Working / Needs you，其它进程按保存状态和 24 小时阈值降级；历史 done / cancelled / error 为 Inactive。

### Fixes

- **会话中心并发分页**：外部更新或新增改变目录排序后，已加载列表重新对齐最新目录的连续前缀，防止旧 offset 与去重跳过未加载会话；页请求期间发生的更新也会重新对齐，保持选中会话。
- **Windows 旧库迁移与临时存储权限**：保护已有数据库及 WAL/SHM 的权限后再收紧父目录，避免旧库因继承权限重算而无法打开；Headless 测试和 TUI/Stdio 临时降级数据库改放专用子目录，避免收紧整个系统 Temp 目录并影响其它文件。

### Changed

- 原 agent 定义选择入口从 `/agents` 改为 **`/agent-design`**；`/tasks` 入口由会话中心替代，旧命令、双标签面板及接线已在独立清理 PR 中删除（#547），定时任务仍使用 `/cron`。`/history` 与 `--continue` 保持当前项目语义。

## v0.6.146 — 2026-10-15

### Chores

- **Windows CI 上 cc-tui 的 shell / 面板测试不再间歇失败（#538）**：`windows-latest` job 的 `cargo test --workspace` 长期偶发失败，而同一 commit 在 ubuntu / macos 全绿，失败形态统一是「等不到异步结果」（`Elapsed(())`、面板显示 `0 lines of 0.0 KB`）而非断言值不符。根因是这些用例用**固定绝对墙钟超时**（5s / 10s，另有一处 500ms 执行期限）等待真实子进程（`cmd` / git-bash / 嵌套 powershell）的输出或退出，libtest 默认多线程并发执行时多个用例同时抢进程与 IO，4 核 runner 上启动与调度延迟被放大到超出等待窗口；500ms 那处更是断言命令必须已打印 `started`，期限短于进程启动开销本身即不可靠。现新增 `#[cfg(test)] mod test_lock`：一把全局异步锁 `PROCESS_TEST_LOCK`（`tokio::sync::Mutex::const_new`，不引入新依赖）串行化真起进程的用例，消除「并发抢进程」这个放大器；5s / 10s 窗口统一为共享预算 `PROCESS_TEST_TIMEOUT`（30s）；`deadline_reports_timeout` 的执行期限 500ms → 3s（脚本 `sleep 3` → 15，保证超时先于命令自然结束触发），`preserves_root_exit` 的期限 1s → 1.5s、观察窗 1250ms → 1750ms。只做串行化与放宽窗口，不改变任何用例语义。**纯测试基础设施改动，无用户可见行为变化。**

## v0.6.145 — 2026-10-15

### Features

- **输入框内 `/command` 高亮对齐参考实现（#529）**：输入框由 tui-textarea 整块渲染且只设单一前景色，命令文本与正文同色，于是出现「从提示浮层补全命令后强调色丢失」的观感。现按 参考实现 `findSlashCommandPositions` 的同一规则扫描 `/command` token（`/` 前为行首或空白、其后首字符为 ASCII 字母、token 尾字符集 `[a-zA-Z0-9:-_]`），以 `custom_highlight` 上 suggestion 蓝（优先级低于光标与选区）；仅主输入框显示态生效，`!` shell 命令态跳过以免把 `/etc` 之类路径着色。规则与参考实现一致、不区分「已知命令」与路径，故行首的 `/usr/bin` 会把 `/usr` 着色。

### Fixes

- **Rewind 入口改为 `/rewind` 命令，移除双击 `Esc` 触发（#531）**：Rewind（对话回滚）此前只在 Agent 空闲时靠「2 秒内双击 `Esc`」触发，而 `Esc` 的通用语义是取消/中断——中断 Agent 后下意识连按 `Esc` 就会弹出回滚选择器；反过来想主动唤起时却无命令入口，TUI 命令表里没有 `rewind`，无参 `/rewind` 会被透传给 ACP 并以空参数报错。现 TUI registry 注册 `rewind`（别名 `undo`）承接无参入口并打开回滚选择器，用户在面板确认后仍向 ACP 提交带 JSON 参数的 `/rewind` 执行截断 history + 逆向恢复文件（ACP 侧行为不变）；双击 `Esc` 分支与仅服务它的 `rewind_pending_since` 字段一并移除，`Esc` 回归取消/中断语义。顺带订正了 `TUI-STYLE.md` 中「`Esc` 退出程序」的错误描述。TUI 命令表 30 → 31 个。
- **运行中敲命令不再被静默当成普通消息发给模型（#533）**：Agent 运行中按 `Enter` 时，输入在**任何命令判定之前**就进了排队队列，文本的命令身份随之丢失；出队时只有 ACP 层那 7 个命令会被重新识别，其余（`/theme`、`/model`、`/gc`、`/exit`、`/help` 等约 24 个 TUI 独有命令）连同未知命令、`!` shell 输入一起被当成普通 prompt 喂给模型——表现为 `/exit` 不退出、`/gc` 不回收内存、敲错命令反被模型一本正经地回答。现将「是命令还是消息、归谁执行」收敛到唯一判定入口 `CommandSystem::resolve`（`cc-tui/src/app/command_system.rs`），空闲派发（`App::route_slash_input`）与运行中分流（`App::handle_busy_input`）共用：普通消息 / Skill / ACP 命令 / 转发型本地命令入队延后执行，需要空闲 UI 的本地命令、`!` shell 与未知命令**拒绝并保留输入框内容**。`CommandRegistry` 拆出 `find` 供判定、`dispatch` 复用同一份匹配规则，消除判定与执行两套逻辑漂移的可能。
- **顺带修掉三处同源问题**：① `Command::blocked_while_busy` 成为「运行中能否延后」的显式声明（默认需空闲 UI，仅 `commit`/`review`/`recap`/`init`/`streaming` 为可延后），并加白名单回归测试锁死分类；② `/history` 里逐命令自写的「运行中拒绝」自查统一上收到入口判定，命令内不再各写一套；③ `/streaming` 此前只是 `submit_message` 内部的前缀特例，因此只能靠「排队后消费」触达、空闲时键入反报「未知命令」，现提升为正式命令（`blocked_while_busy = false`），`submit_message` 特例移除。
- **`Ctrl+Enter` 不能把命令插入正在执行的轮次**：`steer` 会把内容插入当前轮次，命令经此路径会退化成普通文本，现对命令拒绝并提示本轮结束后执行；普通消息与 Skill 仍可照常插入。
- **移除死代码 `rewind_busy_hint_until`** 及其状态栏渲染分支：该字段自引入起从未被赋值，「运行中提示」统一由 `push_system_note`（锚点机制，可跨轮次重建存活）承载。

## v0.6.144 — 2026-10-15

### Fixes

- **后台子 Agent 抽屉「正在做什么」列稳定显示助手文本（#526）**：该列此前在工具名（`Bash` / `Grep`）与助手文本之间来回跳，观感上一直在闪动。根因是**两个来源在争同一个字段**：服务端把 `ToolStart` 的工具名写进任务快照的 `current_activity`，经 ACP 的 `currentActivity` 载荷下发，TUI 的 `progress` 分支又用它覆盖本地从助手文本采集的同名字段。现服务端不再写入工具名（`cc-middlewares/src/subagent/background.rs`），TUI 也不再接受该载荷（`cc-tui/src/app/agent_ops/subagent.rs`），该列只由助手文本与状态文案驱动。
- **抽屉中间列增加 200ms 时间节流**：子 Agent 连续输出时，该列此前以事件到达速率刷新——`poll_agent` 只要有子 Agent 在跑就恒判定「需要渲染」（`has_running_subagents()`），而帧率限制只对主 Agent 的 loading 生效（非 loading 路径始终立即渲染），于是子 Agent 单独运行时反而完全没有帧率上限。现新增渲染快照 `activity_shown` 与 `sync_activity_display()`（`DRAWER_ACTIVITY_THROTTLE` = 200ms），最多每 200ms 刷新一次；**首次填充**与**子 Agent 结束**时立即同步，最后一句不会被节流窗口吞掉。节流只作用于渲染快照，`current_activity` 仍保持高频（子视口与日志可用）——**丢的是渲染帧，不是状态**。

## v0.6.143 — 2026-10-15

### Fixes

- **状态栏配色固定，不再跟随 `/theme` 调色板（#523）**：`/theme` 上线（#509）时顺带做了一次「颜色统一到调色板」扫描，把状态栏原本写死的颜色改成 `theme::cur()` 调色板字段——模型名 / cwd / git 分支从 ANSI 具名色（`Cyan` / `Yellow` / `Magenta`）变成固定 RGB，`✓` 从 `Green` 变成固定 RGB，上下文仪表从 `Red` / `Yellow` / `Green` 变成 `error` / `warning` / `success` 字段，结果状态栏观感随主题漂移，且并无工单要求改它。状态栏配色是打磨定稿的固定值，现新增固定配色模块 `sb`（沿用 #509 之前 `theme.rs` 的常量口径），`status_bar.rs` 中 52 处 `theme::cur().xxx` 改回原固定色（原本 ANSI 具名色的仍用具名色、原本写死 RGB 的仍用 RGB），并移除不再引用的 `ui::theme` 导入；状态栏不参与主题化，渲染结果与 #509 之前逐字节一致。

## v0.6.142 — 2026-10-15

### Fixes

- **主题面板补入 session 面板键盘路由（#521）**：`/theme` 打开的主题面板 scope 为 `Session`，但键盘与粘贴路由的白名单未包含 `PanelKind::Theme`，导致按键被穿透到后续 Stage 而非分发给面板——表现为 `↑`/`↓` 不移动光标、`Esc` 关不掉面板。现补齐 `panels.rs`（键盘）与 `event/mod.rs`（粘贴）两处路由，并新增 `test_theme_panel_consumes_arrow_and_esc_keys` 回归测试；主题面板恢复方向键选择与 `Esc` 关闭，其余面板路由行为不变。

## v0.6.141 — 2026-10-15

### Features

- **Spinner tips 调度对齐参考实现（#511）**：原 tip 是按渲染 tick 每约 3 秒无差别轮换 18 条文案，无 id、无冷却、无历史。现整体替换为 参考实现的调度模型：每条 tip 有稳定 id 与会话冷却（内置 18 条沿用既有中英双语文案，冷却 10 个会话），每轮开始挑选一条并**扣留**、轮结束时才发布展示（每轮最多一条、轮内不换），挑选在冷却已满足者中取「距上次展示间隔会话数最长」者；展示历史按会话计数持久化到 `~/.cc-code/spinner-tips.json`。本轮运行超过 30 分钟时，调度的 tip 被 /clear 时间覆盖 tip 顶替（文案与参考实现一致）。新增配置 `spinner_tips_enabled`（缺省启用）与 `spinner_tips_override`（自定义条目：字符串或 `{id, text, cooldownSessions, priority}` 对象 + `tipsFile` 外部文件 + 自定义 `label` 前缀，schema 与参考实现的 spinnerTipsOverride 对齐，同 id 只保留首条）。注：cc-code 无 `/btw` 命令，参考实现的 30 秒 btw 覆盖 tip 不移植。

### Fixes

- **MCP 协议库 rmcp 升级至 3.5.1（#519）**：适配 rmcp 3.x 的数据模型与接口变更，MCP 客户端连接、工具调用与消息处理迁移到新版 API。

## v0.6.140 — 2026-10-15

### Features

- **Spinner 变色对齐参考实现（#510）**：删除自研的「时间热度四档」配色（思考越久动词越黄）——经 REA 对 参考实现 2.1.300 二进制实证，参考实现并无热度分档。动词改走参考实现的成对流光机制：底色 `claude` + 流光色 `claudeShimmer`，流光为 ±1 列窄带、50ms 步进、周期 = 文本宽 + 20、自左向右扫过；compact 态底色沿用 thinking 紫、流光保持默认对。新增**停滞转红**：无新 token 超过 10s 后流光熄火，动词与字形整体向 error 色平滑插值（再 10s 渐满、每 50ms 向目标靠拢 10%、新 token 到达立即清零、工具执行期间不触发）。状态词文案阶梯保留，配色改为 参考实现的 thinkingIntensity（思考段超 10s 后自 inactive 向 warning 插值）。调色板同步删除 `spinner_heat_lv1..lv4` 扩展字段。

- **`/theme` 主题命令：7 套可切换配色，6 套与参考源码逐值一致**：此前 TUI 配色是编译期常量，无法切换。现将全部 UI 颜色抽成统一调色板（69 个与 参考实现 相同的语义字段 + cc-code 扩展字段），渲染层 900 余处引用改走当前调色板：`default` 即现行配色（升级后默认外观不变）；`dark` / `light` / `light-daltonized` / `dark-daltonized` / `light-ansi` / `dark-ansi` 六套的名称与配色与参考源码的 `src/utils/theme.ts` **逐值一致**，由一致性测试逐字段断言锁定防漂移。`/theme` 打开选择面板——光标移动实时预览、Enter 确认、Esc 还原，与 参考实现 的 ThemePicker 行为一致；`/theme <name>` 可直接切换；选择持久化到配置 `theme` 字段，重启自动恢复。cc-widgets 的 markdown / diff 渲染同步注入当前调色板，切主题时代码块与 diff 一并变色。补漏对齐：markdown 正文此前走 widgets 的静态默认主题、代码块语法高亮固定暗色 syntect 主题、输入框边框与状态栏图标（模型名/分支/上下文用量）、工具状态圆点等处仍是写死颜色——现已全部纳入调色板（语法高亮随主题明暗族在 base16-ocean 暗/亮之间切换，markdown 渲染缓存 key 含主题指纹、切换即失效），生产代码全量审计无写死颜色残留。

## v0.6.139 — 2026-10-14

### Features

- **自动记忆系统（Auto-Memory）对齐参考实现 规范（#507）**：模型获得跨会话持久记忆，**纯文件架构、零专有工具**——系统提示词向模型呈现项目记忆目录，模型用通用的 `Write`/`Edit` 直接维护带四类型 YAML frontmatter 的 Markdown 记忆文件，并在入口索引 `MEMORY.md` 登记单行指针。索引有界（≤200 行且 ≤25KB，超限安全截断并追加 warning），与既有 `~/.claude/projects/`、`~/.cc-code/projects/` 记忆资产**双向兼容**。安全门为自动记忆路径设 carve-out 豁免，模型可自主读写记忆目录而不被受保护目录规则拦截；会话创建时冻结索引快照并置于动态边界之后，保障 Prompt Cache 前缀稳定；记忆路径在 TUI 中脱敏渲染。
- **系统提示词静态段落对齐参考实现 的行为约束（#504）**：`01_intro` 新增「信息足够即行动」与 Corrections（只在会改变用户决策时纠正）；`03_doing_tasks` 新增 **Scope discipline**（不超出请求范围加功能/抽象、优先编辑现有文件、不为不可能场景加错误处理、删除无用代码而非留兼容垫片）；`04_actions` 新增授权与影响判定框架（破坏性/难逆/影响共享状态/上传第三方四类需确认，一次授权不跨上下文）；`06_tone_style` 明确「用户只看文本输出」与结果诚实性（声称完成须基于本次会话观察到的结果）。
- **主 Agent 可按 `task_id` 取消后台子 Agent（`Agent(action="stop")`）**：此前用户下达「取消厦门那个子agent」时，主 Agent 只能回复「我没有停止子 Agent 的能力，请你自己按 x」——把本该由 Agent 完成的协调动作推给了用户。现扩展 `Agent` 工具（**不新增独立工具**）新增 `action`（`spawn` 默认 / `stop`）与 `task_id`：`action=stop` 时按 `task_id` 精确停止，底层复用 `BackgroundTaskRegistry::cancel()`（触发执行器 `CancellationToken`，让执行器正常发终态、落库、清理，不 abort 任务）。**只接受精确 `task_id`，不做任务名/描述模糊匹配**——兄弟任务描述常高度相似（「查询广州天气」/「查询厦门天气」），模糊匹配会停错对象。停止结果回给模型可读确认（`Stopped background task bg-xxx (任务摘要).`），使其能复述「已取消负责查询厦门天气的子 Agent」。
- **`Agent` 工具 `prompt` 不再硬编码 required**：`action=stop` 时无需 `prompt`，若 schema 强制必填，模型会为通过校验而凭空编造一个 prompt。必填性下沉到 `invoke` 内按 `action` 分支校验（`spawn` 需 `prompt`，`stop` 需 `task_id`）。
- **停止终态独立成行并区分发起方**：时间线不再把「被停止」混入 `finished`/`failed`，改为 `● Agent "x" was stopped by main Agent`（用户按键停止则 `stopped by user`）。抽屉行同步改为**红色实心 `●`**（`theme::ERROR`）+ 红色 `stopped by user|main Agent`，取代原橙色 `✗ stopped`——红实心比橙 `✗` 更醒目，用户一眼确认已停。
- **等待态计数行 `✻ Waiting for N background agents to finish`**：主 Agent 一轮结束后若仍有后台任务在跑，主视口打印该行（单复数随数量变化），让用户明确「还差几个」；活跃数归零时自动移除，避免残留误导。

### Fixes

- **子 Agent 完成后的独立视口补齐完成总结行（#485）**：切入刚跑完的后台子 Agent 独立视口时，底部此前完全空白，缺少主视口那条常驻总结行（`✻ Brewed for 32s · done 16:30`）。根因在渲染层：第二段取值要求 `viewport_spinner().last_summary_elapsed_ms() > 0`，而该值只在「活跃态 → `Idle`」切换时被捕获，子 Agent 的完成路径既没在 `peri/subagent_lifecycle` 的 `"stopped"` 分支调用 `set_mode(Idle)`，`BackgroundAgentSession::handle_event` 的 `AgentEvent::Done` 又落进 `_ => {}`，摘要耗时恒为 0 → 退化为「不渲染任何行」。现于 `"stopped"` 分支补齐 spinner 结算（对齐主会话 `set_loading(false)`），并在 `handle_event` 增加 `AgentEvent::Done` 兜底（`set_mode` 幂等）；停止/失败等异常结束再 `clear_summary()`，避免失败任务显示成功文案。新增两个回归用例，其中一个在移除修复后必然失败，用于守住该缺陷。
- **统一界面指示符与行首圆点为公共常量 `SYMBOL_DOT`（#487）**：终端里同一类「行首状态圆点」此前一大一小——Bash 工具头与 AI 回复首行用 `●`（U+25CF），子 Agent 启动/完成/停止行却硬编码 `•`（U+2022），终端渲染后直径与位置肉眼可辨。现于 `cc-tui/src/ui/theme.rs` 提炼全局契约常量 `SYMBOL_DOT`（`●`）与 `SYMBOL_DOT_SPACED`（`● `），并明确红线「严禁使用 U+2022 等异形圆点」；`agent_events_bg.rs`、`agent_ops/subagent.rs`、`message_render.rs` 的硬编码圆点改为引用公共常量，渲染层解析改为**向后兼容历史数据中的 `•`、输出一律规范为 `SYMBOL_DOT_SPACED`**。
- **Windows 下默认静音 mimalloc 的底层 `stderr` 裸打（#483）**：Windows 长时间多轮会话或高负载子 Agent 期间，全局分配器 `mimalloc` 遇到虚拟内存提交耗尽（OS error 1455 / `ERROR_COMMITMENT_LIMIT`）时，会经 `_mi_warning_message` 直接向操作系统底层 `stderr` 打印 `mimalloc: warning: cannot commit OS memory ...`，**完全绕过 `tracing` 并击穿 Crossterm/Ratatui 的备用屏幕**，把当前渲染行（状态条、Todo 列表）打花撕裂。现于 `alloc_config/windows_tuning.rs` 的 `init()` 阶段用 `set_option` 将 `mi_option_show_errors` 与 `mi_option_verbose` 默认置 0（查证 mimalloc C 源码：两者为 0 时 `_mi_warning_message` 首行即 `return`），并注册 no-op 顶层钩子兜底。
- **修复 auto-compact 被误判为手动 compact、导致中途提前关闭 loading（#481）**：长会话触发 auto-compact 后底部 Tip 提示行与 spinner 行消失、Bash 卡片仍在运行。真因是 `ExecutorEvent::CompactStarted` 未携带触发来源，手动/自动共用同一事件，TUI 侧 `handle_compact_started` 无条件把 `compact_manual` 置 `true`，压缩结束即 `set_loading(false)`；而 auto-compact 后 agent 在服务端继续 resubmit 且不再 `set_loading(true)`，本回合剩余全程处于 `loading=false`，级联导致：Tip/spinner 行消失、Ctrl+C 与 Esc 无法中断、回车由「排队」反转为「直接并发提交」。现 `CompactStarted` 增加 `#[serde(default)] manual: bool` 从源头带上来源，TUI 按 `is_manual` 门控 `set_loading(false)`，auto-compact 保持 loading 直至 agent 最终结束。

- **主视口按 `Ctrl+O` 展开子 Agent 的 Prompt 卡片**：后台子任务启动摘要默认仍是极简树形（`• N background agents launched` + `├──`/`└──` 任务行），按 `Ctrl+O` 进入详细模式后切换为经典工具卡片——`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:`（绿色加粗）+ 指令**全文**（6 列悬挂缩进折行），再次 `Ctrl+O` 收回，便于在主会话直接核验下发给各子 Agent 的指令。为此 `AgentEvent::SubagentStarted` 新增 `prompt` 字段携带指令原文（此前只有 `task_preview` 40 字摘要，全文在事件流中被丢弃），`BackgroundAgentSession.prompt` 持久该会话指令，`MessageViewModel::SystemNote` 新增 `detail_content`（参与 `PartialEq`/`Hash`，由 `detail_mode` 单选渲染）。卡片圆点统一为系统大实心圆点 `●`（`U+25CF`，与 `● Bash(...)` 同字形同大小），修复了此前误用小圆点 `•`（`U+2022`）导致「圆点比其他工具小一号」的观感差异；同时修掉逐行解析用字节下标切片（`&line[4..]`）在 `└` 多字节字形上 panic 的缺陷。

- **`AgentResult` 不再被推荐为可调用工具（#470）**：工具发现索引排除仅用于历史兼容的 `AgentResult`，延迟工具提示、关键词搜索与 `select:` 精确查询均不再返回它；描述首行在 160 字摘要内完整保留直接/间接调用禁令与自动通知指引。保留旧会话误调用的明确错误和后台结果的 `tool_use` / `tool_result` 自动回流，不改变通用摘要长度或跨轮提示词缓存机制。

## v0.6.138 — 2026-10-14

### Features

- **核心工具描述对齐参考实现官方措辞（边界句 / NOT 反向枚举 / Write NEVER 语义）**：参照参考实现的工具描述（v2.1.299 参考源码），仅搬运其中**无模板函数、不依赖我方缺失能力**的静态句子。① **Bash / WebFetch 改为 NOT 反向枚举**：Bash 原抽象句「优先用专用工具」展开为 6 条映射（读→`Read` NOT `cat`/`head`/`tail`；改→`Edit` NOT `sed`/`awk`；写→`Write` NOT `echo>`/`<<EOF`；内容检索→`Grep` NOT `grep`/`rg`；文件名检索→`Glob` NOT `find`/`ls`；回复用户→直接输出文本 NOT `echo`/`printf`）；WebFetch 新增「仅用于远程 http(s)，读本地用 `Read`、检索本地用 `Grep`/`Glob`、跑命令用 `Bash`，无点主机名（如 localhost）不支持」边界。② **补齐 `Grep`/`Read`/`TodoWrite`/`AskUserQuestion` 边界**：Grep 首行禁令「ALWAYS use Grep for content search. NEVER invoke `grep`/`rg` as a Bash command」（与 Bash 描述双向引导）；Read 补「只读文件不能列目录，列目录用 Glob、搜内容用 Grep」；TodoWrite 补「同一时刻只保留一个 `in_progress`，完成即标记而非批量更新」；AskUserQuestion 补「仅用于答案会改变后续动作的决策；存在约定默认值或可从代码库验证时，选显然项直接推进」。③ **`Write` 改为 参考实现的 NEVER 语义**：原「任务或项目约定需要时创建文档」与参考实现方向相反，改为「NEVER create documentation files (\*.md) or README files unless explicitly requested by the user」——**行为语义变更：默认不再主动创建 `.md`/README，仅用户明确要求时创建**（副作用：agent 不再主动落库 spec 文档，属对齐参考实现 的预期行为）。仅改描述文本，无代码逻辑变更。

### Fixes

- **ReAct 纠正提示并入 ToolResult 正文，修复熔断提示对模型不可见且导出丢失（#495）**：三类纠正提示（连续失败、Schema 校验熔断、动作循环）此前以独立 `BaseMessage::System` 注入，存在两处致命缺陷——① **模型侧不可见**：Anthropic/OpenAI 适配器把 System 消息抽取到请求顶层 `system` 字段，提示因此脱离对话流末端，自回归解码时看不到，熔断形同虚设（这正解释了实测中模型连续 5 次复读同一错误参数而未被拦截）；② **导出/快照丢失**：`StateSnapshot` 与 `/export` 均以 `!is_system()` 过滤，提示既不进 `origin_messages` 也不出现在导出中。现改为**并入其所属 `ToolResult` 的消息正文**：位置精确（贴着触发它的错误结果）、模型可见（留在对话流末端）、导出可见（`BaseMessage::Tool` 不受 system 过滤）、零 UI 污染（不新增消息/伪用户气泡）、多模态安全（结构化内容仅尾插文本 block）；动作循环提示为整步级信号，附于本轮末条结果，无工具结果时退化为追加人类消息（与 `STUCK_HINT` 既有约定一致）。新增 4 个回归测试（正文并入 / 无提示零侵入 / 多模态保留 / `is_system` 过滤后仍可见）。
- **`-p --output-format stream-json` 补齐 `tool_use` 真实参数**：该模式下输出工具调用事件时 `input` 字段被硬编码为 `null`，消费方（脚本、eval、CI 断言）只能拿到工具名、拿不到任何参数，无法校验参数正确性。根因是 `ToolStart` 事件本身已携带完整 `input: serde_json::Value`，但 `PrintCollector::handle_event` 的 StreamJson 分支在 match 时用 `..` 丢弃、序列化写死 `null`。现解构并原样序列化 `input`（其余字段与事件形态不变）。仅影响 stream-json 输出；`text` / `json` 格式与 TUI 路径不受影响，且此前 `input` 恒为 `null`、消费方本就无法依赖其值，向后兼容。新增 3 项测试（参数透传 / 空对象参数保持 `{}` / `tool_result` 结构回归）。

## v0.6.137 — 2026-10-14

### Fixes

- **后台 shell 完成通知按批次合并注入，消除多轮级联唤醒（#491）**：`Bash(run_in_background=true)` 任务完成后会强制唤醒 agent 开启新 turn；当一轮内积压多条通知时，旧逻辑在每次 `Done` 后**只从 pending 队列弹 1 条**（`pending.remove(index)`），导致 n 条通知 → n 轮 turn 级联雪崩（每轮 `Done` 再触发下一条），现场表现为连续无意义的「收到」。现新增统一注入入口 `App::inject_bg_shell_notifications`：**空闲态**把「本批通知 + 队列中同会话待注入通知」**一次性 drain 拼接为单条**消息提交（n 条压成 1 轮 turn），**推理中**全部入队、待 `Done` 后一次性 drain 合并；`agent_ops` 的 stall 警告与 `shell_command` 的 `!command` / agent shell 两条完成路径统一走该入口，删除三处重复的「首个 submit + 其余入队」分支。合并**仅在消息层拼接**（保留每条 `<background-task-completed>` XML，信息不丢失），其它会话（owner 不匹配）通知仍留在队列交由对应会话消费。配套把完成/等待输入通知的展示解析重构为按文档顺序逐块扫描，支持合并消息含多个块（甚至完成 + 等待输入**混合类型**）时不丢行、不串序。新增 6 项回归用例（空闲合并 / 队列 drain / 跨会话隔离 / `Done` 全量 drain；多条完成合并展示 / 混合类型保序）。

## v0.6.131 — 2026-10-13

### Fixes

- **`/rewind`（`/undo`）不再删掉归档的后台子 Agent 卡片（#468）**：v0.6.130 修「后台分组卡片漏回主时间线」时，把剔除逻辑放在 `apply_pipeline_action` 里对收到的整条 `tail_vms` 做，而 `/rewind` 用 `RebuildAll { prefix_len: 0, 全量历史 }` 表达「替换整条时间线」——剔除于是把用户正在看的归档卡片一并删掉，且没有任何后续事件能把它们放回来（底栏抽屉此时是空的，视觉上就是历史凭空少了几块）。现把剔除下移到「活跃轮次尾部」的唯一产地 `MessagePipeline::build_rebuild_all`，判据落在**动作语义**上而不是会话状态代理，v0.6.130 新增的 `ChatSession::bg_drawer_seen` 字段随之删除（它同样会过度剔除 `/rewind` 的历史重放）。回归用例 `test_bg_group_absent_from_live_rebuild_tail_when_pool_drained`（活跃尾部必须剔除）与 `test_rewind_keeps_archived_bg_cards`（全量重放必须保留）互为反面对照。

## v0.6.130 — 2026-10-13

### Features

- **后台子 Agent 抽屉与独立视口**：按实例 ID 隔离思考、正文、工具、Token、上下文与 Spinner；抽屉导航游标与当前视口分别显示，支持定向消息队列、独立输入草稿与滚动位置、结束任务只读查看。抽屉最多五行，超出后滚动选择，低于十八行的终端使用单行模式。
- **子 Agent 统一为「只有后台模式」**：`Agent` 工具移除 `run_in_background` 参数，子 Agent 一律后台执行——底栏抽屉是它们的唯一呈现，内部过程不再渲染进主 Agent 的消息区（前台同步执行路径已删除）。入参校验（缺 `prompt` / 缺 `subagent_type` / agent 定义不存在 / fork 缺父消息）全部前置，保证模型拿到可处理的错误；并发上限仍为 3；`timeout_secs` 现在显式声明在工具 schema 中（省略或 0 表示不设超时，超时熔断标为 `✗ timeout`）。旧会话里已落库的内嵌分组仍可正常回放。
- **fork 与普通子 Agent 合并为同一条构建路径**：`execute_fork` 的内联构建（约 100 行）删除，fork 复用 `build_agent_from_def`，只保留「注入父级消息 + 包 fork 指令 + 记录分叉点」三个特例；顺带补齐后台任务原先漏掉的 `SubagentStart` hook，`wants_llm_call_payload` 载荷委托改为按实际投递路径条件化（见下方 Fixes）。

### Fixes

- **独立审计复核发现的四处缺陷**：（1）抽屉承载的后台分组剔除判据原先用「`subagent_sessions` 是否为空」作代理，最后一个后台任务完成并被回收后池子归空 → 判据失效，冻结的 `◐ Agent(...)` 卡片会漏回主消息区（底栏抽屉同时是空的，自相矛盾），现改为粘性的 `bg_drawer_seen`（首次登记后台子 Agent 时置位、仅新会话/切 thread 复位，历史回放的归档卡片照旧保留）〔后续演进：该标志本身过度剔除 `/rewind` 的历史重放，已在 v0.6.131 改为在活跃轮次尾部产地剔除并删除此标志〕；（2）子会话回收原先挂在「被摘掉的那一行」上，而行可能已被 `poll_agent` 先摘掉（已停止且结束超过 3s 的行只摘行、留会话），导致会话永久不可达，现回收键回退到 `child_thread_id` 并要求会话已终态；（3）`started` 载荷缺 `isBackground` 时原先默认按前台处理，会把子 Agent 内部过程二次投递进主消息区，现默认按后台；（4）`BackgroundEventHandler` 的 `wants_llm_call_payload` 无条件转述父 handler 声明，而事件实际走会话独立通道时父 handler 根本收不到，导致子 Agent 每次 LLM 调用白拷贝整份历史（下游丢弃 `LlmCallStart`），现仅在事件真的会走 `fallback` 时转述。
- **正在查看的子 Agent 结束时不再抽走视口**：此前完成事件会无条件清空聚焦并摘掉抽屉行，用户正在阅读的内容会「闪走」。现在若用户正在该视口内，视口与抽屉行原地保留（行内 `✓ completed`、圆点与完成标记转绿），按 `Esc`/`Enter` 离开时才回收会话句柄与抽屉行。同时修掉「两个 id 都为 `None` 被误判为正在查看」的幽灵行隐患。
- **抽屉与启动摘要排版配色定稿**：抽屉收敛为「亮白（游标行）+ 灰（其余）」两档前景色，**禁止任何行底色**与终端反色（原先 ↑/↓ 选中行带底色，看起来像已进入视口），青色游标下线；抽屉右侧 token 指标改为跟随流向的**单箭头**（`↓` 取最近一次调用的 output、`↑` 取 input），不再用 `↓ {输入+输出}` 合计冒充单向；启动摘要与完成行不再被注入 `· ` 前缀（原先渲染成 `· • 2 background agents launched …`、`·   ├── …`），改为对齐的 `  ├── 任务` / `  └── 任务` 树形块。
- **`peri/subagent_lifecycle` 通知显式登记**：该通知不带 `notifications/` 前缀，客户端原先按前缀匹配 → 掉进未知通知被丢弃，导致抽屉整块静默失效（服务端一直在发、界面永远为空）。现按方法名显式登记并有回归用例守住。
- **子任务标题与分组标签修正**：标题优先取 `Agent` 工具的 `description`（缺失时才退回 prompt 并跳过 `【…】`/`[…]` 纯指令行），不再把注入给子 Agent 的语言约束当成任务名；分组标签只在真 fork 时写 `fork`，漏参失败调用写 `unknown`，报错分组的指示器改为 `✗` + 红色，不再出现「绿勾 + 红错误行」。同时修正子 Agent 完成后返回给模型的提示：不再指引模型调用并不存在的 `Agent(action="status"|"cancel", task_id=…)` 参数，而是说明抽屉里的管理方式。
- ACP 统一透传来源 ID，生命周期通知单通道发射，主任务结束后继续处理后台事件与审批；子任务完成后回流结果供主任务综合。子任务使用独立持久化线程，会话恢复默认不挂载历史抽屉；并发文件修改检测冲突。

## v0.6.129 — 2026-10-13

### Fixes

- **`@` 文件候选面板立即显示并限制列表规模（#463）**：输入单独的 `@` 即打开面板，默认只枚举当前目录，目录优先、名称排序，最多保留 15 项，不递归扫描整个项目。继续输入关键词可搜索子目录及默认列表外的文件，200ms 防抖保留最后一次输入；被截断的候选及进入新目录前的结果不作为完整过滤缓存。面板最多显示 10 行，按输入框上方可用高度缩小并保持选中项可见，标题显示可见范围；新增搜索中、空目录、无匹配的中英提示。搜索期间清空旧候选并阻止 Enter 误提交，保留方向键选择、Tab/Enter 补全和 Esc 关闭。30 项局部测试覆盖 500 文件上限、中文提示、小终端、快速输入、缓存及键盘链路。

## v0.6.128 — 2026-10-13

### Fixes

- **修复 Windows 下 `cc-code update` 在运行中被误报为 UTF-8 错误（#453）**：`cc-code` 正在运行时执行 `cc-code update` 会失败并报 `stream did not contain valid UTF-8`，实为两层问题叠加：其一，`scripts/install.ps1` 用 `Copy-Item -Force` 覆盖便捷副本 `~/.cc-code/cc-code.exe`，而该 exe 正在运行时被 Windows 文件锁占用 → `IOException`，配合脚本 `$ErrorActionPreference = "Stop"` 直接中止，导致 `current-version.txt` 与便捷副本都未更新；其二，该失败抛出的是中文本地化错误（GBK 字节），而 `update.rs::stream_output` 用严格 UTF-8 的 `BufReader::lines()` 读取子进程输出 → 解码失败，把真实错误整个盖住。现 `update.rs` 改为容错解码逐行读取（`String::from_utf8_lossy`）并在 Windows 子命令前置强制 `[Console]::OutputEncoding = UTF8`；`install.ps1` 便捷副本更新改为锁安全：先复制，失败则把被占用的旧 exe 重命名到 `*.old.<时间戳>` 再复制（对齐 `npm/install.js` 的 `replaceBinary`），仍失败只告警不中止，保证 `current-version.txt` 正常写入。
- **「回到最新」浮层按钮改为按输入框白条居中（#458）**：此按钮原先按「消息区宽度减去最右 1 列（滚动条预留）」居中，相对输入框上方那条整宽白条会整体左偏约 1 列，用户反馈「看着不居中」。现改为以输入框矩形（白条整宽）为居中基准，左右留白对称（80 列下 x=28，左右各 28）；仅视觉调整，显隐、命中区与滚动状态不变。

## v0.6.127 — 2026-10-12

### Features

- **新增「回到最新」浮层按钮（#448）**：鼠标/键盘滚动离开消息区底部且内容溢出时，在消息区最后一行居中显示「↓ 回到最新」浮层按钮；点击按钮或按 `End`（输入框为空）回到底部后按钮自动消失，离开底部期间有新内容（新消息或流式输出）时文案切换为「↓ 有新活动」。按钮为覆盖式浮层，复用既有 `scroll_follow` / `scrollbar_max_offset` 滚动状态，不改变消息区布局与滚动位置；面板/弹窗激活时隐藏且不拦截指针。

### Fixes

- **补齐语言检测、TUI 与 CLI 国际化（#314、#315、#316）**：已保存的 `config.language` 优先；未保存时按 `LC_ALL` → `LANGUAGE` 偏好列表 → `LC_MESSAGES` → `LANG` 选择支持的 `en` / `zh-CN`，`C/POSIX` 消息 locale 忽略 `LANGUAGE` 并保持英文，未知语言回退英文。`/help` 标题、别名、Skills 与快捷键、命令错误、shell 输入提示、空历史 rewind、状态面板和 `/gc` 诊断输出接入 Fluent；命令 dispatch 时刷新描述缓存，修复语言切换后的旧描述。CLI 根命令及嵌套子命令的短/长帮助、格式/范围解析错误、settings 警告和插件文字输出使用同一套语言资源，JSON 字段与执行行为保持原契约。新增语言优先级、双语 CLI 帮助、界面反馈及语言切换回归测试。

## v0.6.124 — 2026-10-12

### Fixes

- **npm postinstall 支持 Windows 进程运行中热替换二进制与历史备份清理（#430、#431）**：在 Windows 环境下，当 `cc-code` 处于运行中时（如在会话内更新或有其它会话未退出），可执行文件具有内核文件锁，npm 安装时的 `node install.js` 直接调用 `unlinkSync` 会抛出 `EBUSY`/`EPERM`，造成全局安装失败回滚。现引入 `replaceBinary` 文件锁退避策略：常规删除失败时，自动将旧二进制重命名为 `${finalPath}.old.<timestamp>` 腾出路径并放入新解压的二进制，实现平滑热替换；同时新增 `cleanOldBinaries` 在安装时顺便尝试清理历史已退出释放的 `.old.*` 备份文件。
- **Jev 规则磁盘缓存单文件归一化、人性化目录命名并解耦模型签名（#433、#434、#436、#437）**：Jev 规则磁盘缓存此前采用「索引 + 内容」两级冗余文件且目录名为 64 位无序哈希，排查困难且切换模型时会强制重新提炼。现彻底重构为一体化架构：其一，落盘彻底废除 `*.index.json`，元数据（生成时间、项目路径、模型名、来源文件指纹）与规则全文一体化写入单文件 `{signature}.json`；其二，目录名优化为人性化的 `{项目名}-{8位短哈希}`（如 `my_project-a1b2c3d4`），直观可辨；其三，缓存签名计算剥离 `provider` 与 `model`，纯由规则文件内容与提示词决定，实现强模型提炼的优质规则跨模型全复用，用户想重新提炼直接删除对应单文件即可。

---

## v0.6.123 — 2026-10-12

### Fixes

- **Bash 运行状态行显示模型请求的 `timeout`，而非硬执行上限（#428）**：`Running… (Xs)    (timeout Nm)` 中的 `timeout` 此前取自 `execution_timeout`（硬执行上限，默认 10m），而模型传的是 `timeout` 参数（前台等待期限，默认 2m）——传 `timeout=300000`（5 分钟）时状态行仍显示 `10m`，标签与展示字段不是同一个，容易误判为「参数没生效」。现将 `timeout_ms` 贯通 `AgentShellRegistration`/`AgentShellSlot`，`MessageViewModel::ToolBlock` 与 `ShellRuntime` 的 `execution_timeout_ms` 字段重命名为 `timeout_ms`，状态行改为取模型传入的 `timeout`（按分钟展示 `2m`/`5m`）。执行与超时语义完全不变。

### Docs

- **README 把 JEV 语义门升格为 Auto 模式重点宣传（#426）**：README（中英）对默认 Auto 权限模式原先只有一行泛称 `auto-classifier`，既未出现 JEV 之名，也未说明它是独立的 LLM 判定服务（默认模型 `openrouter/typesafe/jev-1.13`，走独立端点/密钥）。现新增独立小节「Auto Mode: The JEV Semantic Gate」/「Auto 模式：JEV 语义门」并前置到 Core Capabilities 之前，同时在「Why cc-code?」对比表新增「Safety Gate」一行、Core Capabilities 的 HITL 行改写、顶部导航加锚点，中英一一对应。

## v0.6.121 — 2026-10-12

### Performance

- **diff 缓存加字节预算，`/gc` 补统计渲染缓存与 diff 缓存（#420、#421）**：`/gc` 的「已知合计」此前只有 ~5.9 MB，而 `mimalloc allocated` ~50 MB，存在约 44 MB「未识别」。排查发现两处盲区：其一，`DIFF_CACHE`（`cc-widgets/src/diff/mod.rs`）与 `RENDER_CACHE`（`cc-widgets/src/diff/renderer.rs`）只按条数限流（64 条），但单条可携带接近 `MAX_DIFF_SIZE_BYTES`（1 MB）的文本，实际上限不可控（理论 ~64 MB／每缓存），且完全不在 `/gc` 统计内；其二，渲染线程 `RenderCache`（整段对话渲染行 `lines` + `wrap_map` 逐行纯文本副本与字符宽度）同样未被统计。现新增 `ByteBudgetCache`（条数 + 字节双重上限的 LRU），两个 diff 缓存各设 **8 MB 字节预算**，超预算按 LRU 淘汰（未命中重算，行为不变）；`/gc` 新增 `estimate_render_cache_heap` 与 `diff_cache_stats()` / `render_cache_stats()`，把渲染缓存与两个 diff 缓存计入「已知合计」。剩余「未识别」收窄到语法高亮引擎（SyntaxSet/ThemeSet）、tokio、tracing 与分配器元数据。顺带修复 `renderer.rs` 一处既有注释乱码。

## v0.6.120 — 2026-10-10

### Fixes

- **Windows 传统 CMD 控制码刷屏与星形缺字修复（#415）**：传统 CMD 控制台（conhost）启动后 stdout 的 VT（虚拟终端）模式可能丢失，导致 crossterm 缓存的 ANSI 能力与实际模式不一致——输出退化为控制码刷屏、星形 spinner 缺字（缺字仍占一列却无法被现有列宽探针识别）。现于输出前补齐 VT 必要位并记录恢复、清屏重绘（保留其他控制台模式），`terminal_title_ops`/`terminal_title`/`panel_memory` 保护标题输出与编辑器返回路径，`terminal_backend` 区分原生控制台与 ConPTY 使用不同 ASCII 动画帧并保持逻辑帧原文。同批修复无控制台/重定向环境（如 CI）下 VT 恢复失败提前返回、导致 `last_terminal_title` 缓存未记录与单元测试断言失败的问题（改为先记录缓存状态再执行平台相关 VT 恢复）。（#415）

### Docs

- **活文档与代码漂移校准（#417）**：对照最新代码校准 TUI 命令数（29）、系统提示词段落文件数（14）、核心工具数（11）、cc-widgets 通用组件数（15）与 ReAct 迭代上限（核心默认 10 / 构建 500）；修复 `docs/README.md` 中 7 处指向不存在文件的断链，并在 README 仓库结构树补齐 `TUI-STYLE.md`、`DEVLOG.md`（中英对齐）。（#417）

## v0.6.119 — 2026-10-10

### Fixes

- **手动 `/compact` 摘要补齐 `<system-reminder>` 包裹，界面恢复折叠（#413）**：手动 `/compact` 与自动 compact 各自复制了一份摘要格式化逻辑，手动路径漏掉了 `<system-reminder>` 包裹。而 TUI 依赖该标签决定渲染方式——有标签时摘要折叠成一行「📋 上下文已压缩」，无标签时则被当作普通用户消息把整段摘要铺在界面上；该消息持久化后，load/resume 同样以未折叠形态重现。现将摘要构造抽取为公共函数 `build_compacted_messages`（统一负责 `<system-reminder>` 包裹 + Human 消息构造 + 拼接 re_inject 消息），自动路径（`cc-middlewares/src/compact_middleware.rs`）与手动路径（`cc-acp/src/session/command/compact.rs`）共用同一实现，消除两条路径的格式漂移，并修正「与 auto-compact 路径对齐」的误导性注释。补测试断言两条路径产出的摘要消息结构一致。

## v0.6.118 — 2026-10-10

### Fixes

- **Jev 规则磁盘缓存改为多行 JSON 输出，提升可读性（#412）**：Jev 安全规则的磁盘持久化缓存（`~/.cc-code/jev/peri-<项目哈希>/<规则哈希>.json`）此前用 `serde_json::to_vec` 落盘为**紧凑单行** JSON——规则正文动辄数千字符全挤在一行，用户直接打开查看规则时几乎无法阅读。现将 `write()` 的序列化改为 `serde_json::to_vec_pretty`（2 空格缩进），规则正文文件与 `<签名>.index.json` 索引同步美化为多行。内容哈希对**落盘字节**计算（`digest(&bytes)`），格式化前后各自自洽，缓存仍能正常命中；存量旧缓存因文件名（哈希）与格式化后不一致会 miss 一次并重新提炼，无功能影响。仅影响磁盘落盘格式，内存缓存与 Jev 判定逻辑不变。

## v0.6.117 — 2026-10-10

### Fixes

- **Windows 全局入口直起 exe，修复 npm 安装下 Ctrl+C 直接退出 TUI（#410、#411）**：Windows Terminal（ConPTY）下敲 `cc-code`，npm 全局 wrapper（`<prefix>/cc-code.cmd|.ps1`）由 npm 依据 `package.json` 的 bin（指向 node 脚本 `bin/cc-code`）自动生成，再经 node 以 `execFileSync` 拉起真正的 `cc-code.exe`。Ctrl+C 的 `CTRL_C_EVENT` 会广播给整个控制台进程组（node + `cc-code.exe`），`cc-code.exe` 内部虽用 `SetConsoleCtrlHandler` 拦截了信号（返回 1 阻止默认终止，注入 KeyEvent 走应用层中断 / 空闲双击退出），但 node 父进程没有 handler、按默认行为被直接终止，`execFileSync` 的同步等待随之崩断，把 `cc-code.exe` 一并拖死——表现为 agent 任意状态下按 Ctrl+C 都直接退出，日志在 streaming 中戛然而止、无任何清理记录。现于 `install.js` 的 Windows postinstall 阶段新增 `overwriteNpmGlobalWrapper()`，把 npm 全局入口改写为**直起 `bin/cc-code.exe`**（相对 prefix 根引用，避免硬编码绝对路径），去掉 node 中间层后进程组内只剩 `cc-code.exe`，`SetConsoleCtrlHandler` 正常生效。改写为防御性实现：仅在 exe 存在且能定位到 npm 全局 wrapper 时才覆盖，任何失败都不阻塞安装（此时 exe 同目录 `bin/cc-code.cmd|.ps1` 仍可直起）；`bin/cc-code` node 脚本与非 Windows 路径行为不变。已实机验证：全局入口改为直起 exe 后 Ctrl+C 不再直接退出，TUI 恢复「Ctrl+C 中断 agent / 空闲双击退出」的设计行为。

## v0.6.116 — 2026-10-10

### Fixes

- **Schema 熔断改为滑动窗口计数，修复交替错配逃逸（#404、#408）**：v0.6.110 引入的 Schema 熔断器在**真实失败模式**下从不触发——模型常在多个工具之间交替把参数配错（配错一次 `WebSearch`、配错一次 `WebFetch`、再配错一次 `WebSearch`），且失败之间夹杂成功调用。原因有二：其一，计数口径是「连续失败」，而 `reset()` 在**任意工具**成功时都会清空跨工具聚合计数，聚合阈值 3 因此永远凑不满，单工具阈值 2 也因工具交替而停在 1；其二，`TOOL_SIGNATURE_HINTS` 只有单向条目（表内没有任何一项指向 `WebSearch`），导致把 `query`/`num_results` 打给别的工具时静默无提示，模型在一侧收到「改用 WebFetch」、在另一侧却收不到任何反馈，被提示表**来回推入震荡**。现熔断计数改为**滑动窗口**口径（窗口容量 6，单工具与跨工具计数均自窗口派生，不再单独维护计数器）：`reset()` 只复位该工具的退避状态、**不再清空窗口**，成功调用同样占用窗口位以推动旧失败自然淘汰；同时补全反向条目（`query`/`num_results` → `WebSearch`、`old_string`+`new_string` → `Edit`、`content`+`file_path` → `Write`）。实测以真实 trace 的原始调用序列逐条喂入熔断器，此前 3 次 schema 失败注入 0 次提示，修复后触发 `PerTool { tool_name: "WebSearch", count: 2 }`。

## v0.6.115 — 2026-10-10

### Fixes

- **Jev 规则提炼超时按阶梯重试并改用低思考档，修复磁盘缓存从未落盘（#405、#406）**：v0.6.114 引入的规则提炼持久化缓存（#398、#399）在真实环境里**从未生效**——`~/.cc-code/jev/` 目录始终不存在。三个根因叠加，使提炼结果恒为「不完整」而永远进不了「仅持久化完整非空结果」的分支：其一，提炼调用继承了会话的扩展思考档（`effort=xhigh`），实测单块耗时被拖到 18–73 秒；其二，超时被当作「块太大」去做**对半递归拆分**，而超时的成因是网关慢、并非输入过大，切小只是拿同一时限再赌一次，还把一次等待放大成 7 次（实测单个加载器的一次 `ensure_loaded` 内触发 14 次调用，突变测试测得旧逻辑实际 28 次）；其三，提炼时限默认 15 秒低于实测单块耗时。现提炼改用独立低思考档（`effort=low`、`budget_tokens=1024`，不继承会话配置），实测单块耗时降至 10–15 秒、输出 token 由 6k 级压到 2.5k 级且方差收敛；超时改为**时限阶梯**——首试时限后放宽 2 倍再试一次（默认 15s → 30s），并新增失败原因区分，**超时不再进入对半递归**，该能力仅保留给「回复不可解析 / 为空」这类切小确实有效的失败。提炼时限默认值同步上调至 30 秒。若将来把提炼思考档改为可配置，**必须**将其纳入缓存签名，否则会命中旧档位算出的结果（代码内已留注记）。

## v0.6.114 — 2026-10-10

### Features

- **Jev 安全规则提炼磁盘持久化缓存（#398、#399）**：Auto 模式首次门控将项目、个人、全局规则与 hook 的提炼结果按完整来源签名进行 SHA-256 内容寻址持久化缓存，存储至 `~/.cc-code/jev/peri-<项目哈希>/<规则哈希>.json`，应用重启后跨会话命中缓存即可直接跳过耗时的规则提炼 LLM 请求；来源签名严格覆盖规范化项目路径、展开引用后的完整规则正文、模型 provider/id 与提示词参数；采用文件系统原子写入与所有者权限保护，读取校验版本与内容哈希，损坏自动优雅降级重新提炼。
- **排队消息快捷键优化为 Ctrl+Enter 发送与 Ctrl+X 删除（#400、#401）**：排队消息的操作快捷键优化为 `Ctrl+Enter` 立即发送、`Ctrl+X` 移除排队消息，解除与其他输入态快捷键的潜在冲突，提供更顺畅的输入流水线交互。

### Fixes

- **用户全局指引多候选回退与 Jev 数据源归一（#402、#403）**：修复此前 System Prompt 指引注入中用户全局层硬编码单一文件 `~/.cc-code/AGENTS.md`，导致配置在 `~/.claude/CLAUDE.md` 的个人全局规则无法注入上下文的缺陷；引入有序候选回退列表（`~/.cc-code/AGENTS.md` -> `~/.cc-code/CLAUDE.md` -> `~/.claude/CLAUDE.md` -> `~/.claude/AGENTS.md`），空文件不遮蔽后续候选，自动穿透至有效规则；抽象统一方法 `load_user_global_instruction`，使 Prompt 指引注入与 Jev 安全门规则提炼（`read_global_content()`）彻底对齐同一加载管线。

## v0.6.113 — 2026-10-10

### Fixes

- **交互弹窗 Ctrl+C 中断保护与审批取消及时响应（#396、#397）**：修复提问（Questions）和工具审批（Approval）弹窗此前单次按下 `Ctrl+C` 直接退出 TUI 的问题，改为向 Agent 发送取消中断（`session/cancel`）并解除 transport 等待，保留 TUI 会话且清空退出确认状态；修复配置向导退出缺少防抖的问题，复用普通模式 100ms~2s 双击退出逻辑；在批量 `before_tool` 工具等待中增加取消优先分支，使长时间工具审批在用户中断时可立即响应，保持工具延迟写入约束；后续或迟到的交互请求自动标记取消，避免二次弹出孤立弹窗。
- **PHP 语法高亮内存缩减与空闲工作线程堆内存自动回收（#394、#395）**：将 syntect 代码高亮引擎从 `default-fancy` 切换为静态编译的 `default-onig`（Oniguruma），解决 PHP 等语法正则在 fancy 引擎下引发的大量常驻内存开销（独立测试中单段 PHP RSS 从 89.6 MiB 降至 15.6 MiB，整段会话文本从 118.4 MiB 降至 17.5 MiB）；在 Windows 平台通过 `mi_option_set` 注入有效的 mimalloc purge 参数，并在 Tokio runtime 的 `on_thread_park` 回调中接入工作线程空闲堆回收机制（每线程至多 1s 回收一次）；修正 `/gc` 命令与内存诊断在 resident、metadata、RSS 差额及消息数统计上的口径。

## v0.6.112 — 2026-10-10

### Features

- **消息区底部 Todo 列表默认最多展示 5 项并折叠溢出统计（#392、#393）**：在消息区底部 Spinner 关联的 Todo 列表区域引入 `MAX_VISIBLE_TODOS = 5` 上限。当存在大量待办任务（例如 8~10 个）时，不再全量平铺占满垂直视口，仅渲染前 5 项，超出部分折叠为一行优雅统计（例如 `    ... +3 pending`），并自适应统计 `pending` / `completed` 数量；同步重构 `spinner_extra_count` 及 `todo_render_line_count`，使多任务时的行数计算从无界增长收敛为最多 6 行（5 任务 + 1 统计行），保障视口裁剪、滚动条与实际渲染行数精确对齐。

## v0.6.111 — 2026-10-09

### Features

- **状态栏第二行改为「最近工具」实时摘要（#390）**：原来第二行的运行中工具段只在执行期间存在，且受三个限制——`poll_agent` 每帧把 ACP 通知一次 drain 干净，Read/Glob 这类毫秒级工具的 ToolStart + ToolEnd 常落在同一帧，`◐ Read : x.rs` **一帧都渲染不到**；摘要来源是 11 个工具的白名单，Agent / TodoWrite / AskUserQuestion / MCP 只有裸名字；顺序是「老在左」，最新开始的排在最右。现改为由 `AgentComm.recent_tools` 驱动的一行流水：最新在最左、最多留 2 条（第 3 条挤掉最老的），条目显示时长取 `max(实际执行时长, 300ms)`——快工具垫到 300ms 后消失，慢工具执行结束即刻消失，工具结束只让右侧聚合计数 `✓ Name ×N` +1、不再生成带摘要的完成条目。摘要覆盖扩展到全部工具（AskUserQuestion 取首个问题、Agent 取 description、TodoWrite 显示任务数、其余取第一个非空字符串字段），字符上限由 20 提到 30（首版值），截断仍保持路径语义；`◐`/`✓`/工具名/摘要配色统一、不跟工具身份色。第二行强制单行不折行，整行超宽时按显示列宽（unicode-width）在行尾 `…` 收口，不再被 `Paragraph` 静默裁掉。

## v0.6.110 — 2026-10-09

### Fixes

- **Schema 熔断器跨工具聚合与连续失败按错误类型聚合（#379）**：修复模型在不同工具间轮换猜错参数时单工具计数永远达不到阈值（2 次）导致的熔断逃逸缺陷，新增跨工具聚合连续失败追踪（阈值 3 次）与指数退避机制；修复连续失败检测按完整错误文本做 key 导致参数名差异稀释计数的缺陷，引入类型化 `ToolErrorKind` 枚举并将 key 优化为 `(tool_name, error_kind)`；弃用脆弱的字符串嗅探，在参数校验失败点精准抛出 `AgentError::ToolSchemaValidationFailed` 并结构化传递；调整告警写入时序，确保所有 `ToolResult` 紧随 `AiMessage` 写入后再追加 `System` 提示，杜绝孤立工具结果风险。

## v0.6.109 — 2026-10-09

### Features

- **动词流光动效与纯时间驱动思考状态词色温阶梯（#387、#388）**：参考 Codex CLI 物理余弦衰减模型，在 `cc-widgets` 实现动词独立流光（Shimmer），以 5.0s 为周期舒缓掠过 1.6s 抛光流光（同色系提亮 45%，绝不产生刺眼纯白反差），作为长任务与长 Bash 执行期间的生命体征心跳信号，防止进程假死焦虑；思考状态词与动词动静解耦，状态词完全不闪烁，纯按时间流逝从浅灰逐步加温至琥珀金（0~2.5s 浅灰 `#999999` ➔ 2.5~5s 柔白中间色 `#D0D0D0` ➔ 5~15s 动词暖橙 `#EB9F7F` ➔ 15~60s still thinking 浅金 `#FFD966` ➔ >=60s deep in thought 琥珀金 `RGB 255,152,0`），长 Bash 场景维持 8.0s 极低频独立流光守候，状态词定格显示已结束的思考耗时（置灰静止不抢戏）。

### Fixes

- **ASCII 降级表补 `⎿`（U+23BF）（#386）**：解决在异常列宽终端上工具行前缀因缺少映射降级为问号的问题。

## v0.6.108 — 2026-10-09

### Performance

- **`LlmCallStart` 载荷按需构造，无订阅者不再全量克隆消息历史（#369）**：`llm_step.rs` 在每轮 LLM 调用前无条件 `state.messages().to_vec()`，把整个消息历史（含工具结果正文、图片 base64）深拷贝一份，仅用于构造 `LlmCallStart` 事件的 `messages` 字段。该克隆发生在 `emit()` 的订阅者判断**之前**——Langfuse 未启用时照常执行，长会话下每轮 MB 级、再乘以 ReAct 迭代次数，为无人读取的数据付 O(轮数 × 历史大小) 的分配成本；#306 只修掉了 tracer 侧的第二次深拷贝。由于 `LlmCallStart.messages/tools` 的唯一真实消费者是 Langfuse tracer（TUI 与 ACP mapper 均丢弃该事件），现按「按需快照」实施：`AgentEventHandler` 新增 `wants_llm_call_payload()`（默认 `false`），executor 据此决定是否构造载荷——`false` 时发空载荷、跳过全量拷贝。`cc-acp` 的 `FnEventHandler` 闭包替换为 `PumpHandler`，以「Langfuse 是否启用」声明需求；`cc-middlewares` 的 `SourceAgentIdHandler`（子 Agent 事件包装器）委托 inner 的声明，避免子 Agent 的 Langfuse 输入快照丢失。

### Fixes

- **用户 `!` 命令块渲染对齐工具结果行（#384）**：TUI 中用户直接执行的 `!` shell 命令块与工具结果行的缩进/前缀风格不一致，现统一对齐，消除同一输出区域的视觉割裂。

## v0.6.107 — 2026-10-09

### Features

- **紧凑审批面板与完整参数展示（#368）**：审批面板此前将 Bash 参数单行截断、批量工具重复展示选项与快捷键，长脚本难以核对。现按内容收紧布局、完整换行展示参数（Bash 保留换行/缩进与中文显示列宽，Edit 展示上下文与增删对比），批量显示当前位置与三类计数，没有内部滚动区、超高内容优先当前工具并明确提示剩余未显示行数，快捷键固定在底部；仅改 UI/文案/测试，权限判定、父子 Agent 限制、会话审批记忆、ACP 返回与键盘处理不变。

## v0.6.106 — 2026-10-09

### Features

- **指引文件分层合并，加载模型对齐 dsh（#357）**：项目同时存在 `AGENTS.md`、`CLAUDE.md` 或多级目录指引时，原实现会**漏载规则**。现按项目根 → 工作目录逐层收集同层候选依次加载，记录 provenance（来源路径），同目录按内容去重，并在 `session/new` 时冻结结果（`frozen_instructions`）。用户全局层由旧的 `~/.claude/AGENTS.md` 迁移至 `~/.cc-code/AGENTS.md`。空文件跳过、换行归一；大文件有界读取（单文件默认 1 MiB、最终注入默认 256 KiB），中文切边安全；递归 `@import` 共享输出预算，缺失/不可读/循环引用保留占位符。审计修复：空指引也保存冻结快照；正确解析含 `..` 的 cwd 并保留发现路径供 excludes 使用；候选先内容去重再登记路径，避免别名挡住后续目录的规则（另见 `spec/global/domains/agent-instructions.md`）。
- **子 Agent 继承父权限，消除委派绕过审批（#357）**：Auto 模式下，普通、后台与 fork 子 Agent 现在**共享父级权限模式、Jev 门、规则加载器、分类器及会话审批记忆**，修复了「委派给子 Agent 即可绕过工具审批」的问题。父保留允许 / 拒绝 / 询问三态；子只有允许 / 拒绝，不确定或服务失败默认拒绝，规则提炼失败、不完整或取消也可被识别。检查覆盖真实工具名、间接工具目标（`ExecuteExtraTool` 解包）、工具限制及实际执行目录，并禁止递归委派。
- **HITL 审批界面同时提供三档选择（#357）**：审批弹窗同时显示「同意本次 / 本次会话同意 / 拒绝」，上下键选择、Enter 提交、Tab / Shift+Tab 切换工具、Esc 全部拒绝；滚动保留当前工具的三项选择，参数按终端列宽截断，中英文同步。会话记忆按工具类型细化：本次批准不记忆；文件按工具与路径，Bash 按完整命令 + 执行目录 + 分支，其他工具按完整参数；明确禁止规则优先于记忆。

### Fixes

- **`/export` 导出不再截断工具调用信息（#363）**：导出的 Markdown/PlainText 中，工具调用参数被 `chars().take(200)` 一刀切——真实日志中 210 次 Bash 调用有 **156 次（74%）被切在正好 200 字符**，JSON 未闭合、命令从中间断掉；`Write.content`、`Edit` 的 `old_string`/`new_string` 同样受影响。现去掉截断、完整输出参数 JSON；同时加固 `ContentBlock::ToolResult` 分支（原只写行数统计，改为输出完整正文并标注 `is_error`），并按正文最长连续反引号自适应围栏长度，防止 heredoc 脚本内容破坏 Markdown 结构。（详见 [#362](https://github.com/cc-claws/cc-code/issues/362)）

## v0.6.105 — 2026-10-08

### Fixes

- **日志文件打不开不再 panic，降级到 stderr（#354）**：启动 `cc-code` 时若 `~/.cc-code/logs/{service}.log` 因 ACL 被写坏（空 DACL）而无法打开，`subscriber.rs` 的 `.expect("cannot open log file")` 会**直接 panic 掉整个进程**——日志只是诊断设施，不该是启动硬依赖。现抽 `resolve_log_writer(log_path) -> BoxMakeWriter`：打开失败时 `eprintln!` 警告并退回 `std::io::stderr`（json / 非 json 两条分支共用同一 writer 类型）；`ensure_utf8_bom()` 失败静默忽略（BOM 仅为显示优化）；`set_global_default` 失败降级为提示。修复后日志文件 ACL 坏了也能正常启动。

### Refactoring

- **统一英文内置指令并优化任务执行约束（#356）**：统一 14 个主模板、4 个内置 Agent、核心工具与参数说明、Skills、ACP 命令、压缩/回顾及审批模型的英文指令。清理「四行/一词回复」「编辑后停止」等固定限制，明确授权范围、持续执行、完成结果与验证证据，规定审批与拒绝不得被工具切换绕过。工具说明与实现对齐：Read 默认分页、32 MiB 文件上限、未实现的 PDF 页码；Write/Edit 不再宣称强制先读校验；Bash 使用实际 shell/timeout 契约；Agent 区分独立上下文与继承快照、不承诺 worktree 隔离。Deferred 工具目录按名称排序、每项只保留首个非空描述行（≤160 Unicode 字符），完整描述与 schema 按需经 `SearchExtraTools` 获取。同时修复 HITL 提示段落的开关复用运行时 `is_yolo_mode`（此前默认审批开启却未注入审批说明）。模型可见文本量：核心工具描述 −40.9%、内置 Agent 定义 −31.9%。



### Features

- **Markdown 缓存内存统计（#351）**：`/gc` 此前只显示 Markdown 缓存**条数**（`markdown_cache: 1024/1024 条`），无法判断其字节占用，不足以评估内存节省空间。现新增 `MarkdownCache::stats()`（单次加锁快照，**不克隆解析产物、不提升 LRU 次序、不改缓存策略**），按 `Text.lines` / `Line.spans` / 链接数组及**自有字符串 `capacity()`** 估算堆占用，输出总量、平均/最大条目、渲染行数与 Span 数，并把估算纳入 `/gc` 的「已知合计」；口径明确标注「不含 LRU / 分配器开销，非 RSS」。同时把「未识别」注解由「非泄漏」改为更严谨的「**余量来源待定位，不能据此判断是否泄漏**」，并把原始字节字段经 tracing 输出。**注**：本版仅补统计，**未**实施字节预算 / 容量下调（另见 `spec/issues/2026-10-08-markdown-cache-memory-accounting.md`）。

### Refactoring

- **移除 `~/.peri` 兼容，应用数据统一 `~/.cc-code`（#349，破坏性）**：项目已是 cc-code，不再兼容改名前的旧主目录。此前 #289 为「老用户数据不丢失」保留了「新优先、旧回退」的**逐文件**回退，且具粘性——只要某文件只在 `~/.peri` 就一直使用旧路径（实测 `input-history.json` / `oauth_tokens.json` 长期落 `~/.peri`）。现删除 `cc-agent/src/app_home.rs` 的 `legacy_app_home_dir(_in)`，`app_data_path_in` / `app_data_dir_in` 一律返回 `~/.cc-code/...`；`cc-tui/src/main.rs::inject_env_from_settings` 只读 `~/.cc-code/settings.json`；并清理各处陈旧注释与用户可见报错文案（含 `acp_stdio.rs` 的 provider 缺失提示）。**破坏性变更**：只在 `~/.peri` 存在的数据文件不再被读取（用户需自行迁移）；新写入一律走 `~/.cc-code`。`hitl/jev/policy.rs` 敏感目录名单中的 `.peri` 予以保留（若用户机仍存在该目录，继续阻止工具读取，属安全而非兼容）。

## v0.6.103 — 2026-10-08

### Fixes

- **详细模式运行中工具头前缀不再随指示器闪烁左移（#343）**：详细模式（Ctrl+O）下查看运行中的超长命令时，工具头前缀 `● Bash(` 会随运行指示器**闪烁而左右抖动**——亮帧显示 `● Bash(...`，熄灭帧开头 `● ` 消失、整行左移（观感为「看不到工具名前缀」）。根因三处叠加：`format_indicator()` 让运行中指示器在 `●` 与 `" "` 间闪烁；详细模式 header 走 `wrap_full` 折行；`wrap_line_spans_rich()` 会 **trim 每段行首空白**——熄灭帧首段是空格被 trim，`指示器 + 分隔空格` 前缀整体丢失。现为折行核心增加「保留首段行首空白」路径（`trim_first_lead` 仅控制首段，续行行为不变），工具头 `wrap_full` 分支改用 `push_wrapped_line_keep_first_lead`，其它折行路径行为完全不变。
- **`/gc` RSS 变化符号颠倒 + allocated 差值文案方向修正（#346）**：`/gc` 输出的 `RSS: 197.9 MB → 196.6 MB (+1.3 MB)` 方向是反的——**下降**被显示成 `+`。根因是 `delta = before - after` 却只在 `>= 0` 时加 `+`，使「减少」带上了 `+`（`OS RSS` 同病）；`/gc` 的设计目标正是「消除误导指标」，符号反了等于制造新误导。现统一方向语义为 `after - before`（增加为正），新增 `fmt_signed_delta()` 输出 `+N` / `-N` / `±0` 供两处共用；`allocated - RSS` 改为带符号输出并按方向分支文案。
- **排队消息快捷键 `Ctrl+S` / `Ctrl+X` 迁移为 `Alt+S` / `Alt+X`（#347）**：Windows conhost 会把 `Ctrl+S` 当作终端**流控键**截走，按键根本到不了应用，导致「立即发送」失效。迁移为 `Alt+S`（立即发送）/ `Alt+X`（删除排队消息）。

## v0.6.102 — 2026-10-08

### Chores

- **补全版权署名与 Cargo 包元数据**：LICENSE 在上游 `Copyright 2026 KonghaYao` 之下追加本衍生作品署名 `Copyright 2026 cc-claws (modifications to the Derivative Work)`（上游版权行为 Apache-2.0 §4c 强制保留义务，未删改）；`[workspace.package]` 新增 `authors = ["cc-claws"]` 与 `license = "Apache-2.0"` 作为全仓单一数据源；7 个 workspace crate（cc-agent / cc-middlewares / cc-tui / cc-acp / cc-widgets / cc-lsp / langfuse-client）统一改为 `license.workspace` / `authors.workspace` 继承——此前仅 cc-agent / cc-middlewares 硬编码 license，其余 5 个缺 `license`，全部缺 `authors`。
- **合并 `CORE_VERIFY.md` 到 `human/TESTING.md`**：根目录 8 行的手写冒烟清单（hello 对话 / `/clear` / `/history` / 工具调用完整性 / 多轮失忆 / Ctrl+C 中断 / ask_user）零引用、未收录文档地图，易被忽略。现整理为 `human/TESTING.md` 的「核心功能冒烟清单（发布前人工走查）」段并删除原文件，内容一字未丢。
- **删除 `prompts.md`**：根目录 4 条 `/loop` 命令自用模板，零引用、未收录文档地图，且模板内容引用了并不存在的 `progress.md`。
- **`lefthook.yml` 对齐 CI**：pre-commit 的 clippy 由 `cargo clippy --all-targets -- -W clippy::all` 改为与 CI 一致的 `cargo clippy --workspace --all-targets -- -D warnings`（`-W` 只告警不拦截，是「本地 lefthook 通过、CI 变红」的根因，如 PR #339）；`check` 补 `--workspace`；移除永远空转的 `typos` 命令（`typos --ignore-hidden 2>/dev/null || true`，无配置文件且 `|| true` 保证永不失败）。

### Docs

- **修正 README 与代码不符的陈旧描述**：删除 RISC-V 支持声明（`README.md` 头部、安装平台段及 `npm/README.md` 平台表）——发布的 5 个平台均不含 riscv64（唯一构建 riscv64 的 `release-agent.yml` 因 `agent-v*` tag 从未触发）；`side-projects/` 描述由「llm-gateway 等」更正为实际的 `peri-sync`；仓库结构树把不存在的 `spec/prd/` 更正为 `spec/archive/` 并补上实际存在的 `docs/`、`human/`；Typical Workflow 中不存在的 skill `grill-me`/`improve-codebase-architecture` 更正为真实存在的 `brainstorming`；特性表「更早版本」边界由 v0.6.76 修正为 v0.6.79。
- **维护 v0.6.100 / v0.6.101 文档 + 清理当前状态文档中的 `peri-*` 残留**：CHANGELOG 补 v0.6.100 / v0.6.101 条目；README 中英特性表各 +2 条（一一对应、保持最新 10 条）；`spec/global/domains/tui.md`、`features.md`、`index.md` 同步；新增 `spec/issues/2026-10-08-detail-mode-long-cmd-running-status-overwrites-header.md` 根因分析。另修正 v0.6.95（#333）crate 改名 `peri-*` → `cc-*` 时遗漏的当前状态文档（`spec/global/`、`docs/`、crate 侧 README/CLAUDE.md 等 60 个文件）；历史快照目录（`spec/archive*`、`spec/reviews`、`docs/superpowers/*`、CHANGELOG/DEVLOG 历史条目）按规则保留不改。

## v0.6.101 — 2026-10-08

### Fixes

- **详细模式超长命令运行时，状态刷新不再覆盖 header 续行（#341）**：详细模式（Ctrl+O）下 Bash 命令超长时，ToolBlock header 会折成多行（#264 引入）。渲染线程的 tick 增量刷新仍按「header 恒 1 行」的旧假设处理状态行，导致两处故障：场景 B 写死 `lines.get_mut(1)` 更新 `Running… (Xs)` 秒数，多行 header 下命中的是命令续行——命令文本被就地改写为 `Running…` 并残留右括号，真正的状态行（下标 ≥2）因不在下标 1 而被冻结、秒数不再前进，屏幕上呈现为「两处 `Running…` 且时间不一致」；场景 A 用固定行数 `cached_line_count < 3` 判断「状态行尚未渲染」，多行 header 下行数早已 ≥ 3，判定恒为假，首次跨越 2 秒阈值时状态行永远不会出现。现改为**按内容定位状态行**（新增 `message_render::is_shell_running_status_line()`，与子 Agent 路径已有的内容定位做法一致），两个场景都不再依赖固定下标 / 固定行数。注：本问题由 #264 引入，与 #287 无关（#287 仅调整折行时的词边界回退，反而让长 token 场景更易折成多行、更易触发）。仅影响「详细模式 + 超长命令折行 + 运行中（>2s）」，非详细/短命令路径行为不变。

## v0.6.100 — 2026-10-08

### Fixes

- **`/gc` 内存诊断修正：消除误导指标 + 纳入 `view_messages` 估算（#339）**：`/gc` 输出的内存诊断存在误导性数字。P0 —— `active`/`mapped`/`retained` 语义**分平台**（jemalloc：真实活跃页 / 映射量 / 保留未归还 OS；mimalloc：`page_committed` 历史触及高水位 / `reserved` 虚拟地址 / 虚拟地址空间），旧代码用统一标签，Windows 上 `active=1392 MB` 与派生的「碎片=1332 MB」完全无用（真实 `resident` 仅 100 MB、无碎片问题）。现按 `alloc_name` 条件化标注，并给「碎片」行加忽略提示。P1 —— `estimate_messages_heap` 只统计 `origin_messages` + `completed`，漏掉 `view_messages`（含每块内嵌 `Text<'static>`），正是「未识别 59 MB」的主因。现新增 `estimate_view_messages_heap()` 遍历 `MessageViewModel` 全部变体，「已知合计」改为「消息 X + VM Y」三元展示，`origin vs completed` 完全相同标注为「设计冗余，非泄漏」，并追加注脚说明「未识别」构成（markdown 缓存 / ACP 缓冲 / tokio / tracing），明确非泄漏。附带修正 `estimate_links_heap` 的 clippy `manual_slice_size_calculation` 告警（改用 `std::mem::size_of_val`），该告警曾致 CI 三平台全红。

### Chores

- **移除 ARM32 部署包与构建链路（#340）**：删除 `deploy/peri-arm32-v0.2.0/` 与 `scripts/build-arm32.sh`——ARM32 未纳入 CI / npm 发布链路，属 peri 时代遗留。

---

## v0.6.99 — 2026-10-07

### Fixes

- **后台 shell 通知展示文案接入 i18n（#338）**：`shell_notification_display_text` 把「后台 shell 已完成/超时终止/已取消/已终止/等待输入」硬编码为中文，即使用户语言为英文仍显示中文。根因是该函数处于静态构造路径（`MessageViewModel::user/system/from_base_message*` 内部调用），拿不到 `App`/`ServiceRegistry` 上下文。现新增 6 个 `shell-notify-*` FTL key（en + zh-CN），展示文案改走 `LcRegistry::tr()`；新增进程级语言注册表（`i18n::init_global`/`global`，启动与 `/lang` 切换时同步），供静态构造路径读取当前语言；`FluentBundle` 换用 concurrent 变体使 `LcRegistry` 满足 `Sync`，可跨线程（渲染线程）安全读取。

---

## v0.6.98 — 2026-10-07

### Fixes

- **排队快捷键提示对比度提升（#335, #336）**：排队消息行尾的 `Ctrl+S send now · Ctrl+X delete` 提示用了 MUTED + DIM，在 USER_BG 背景上几乎看不清。改为 TEXT_SOFT 高亮。

---

## v0.6.95 — 2026-10-01

### Breaking

- **crate 改名 peri-* → cc-***：与上游彻底切割，6 个 workspace crate 重命名（peri-agent→cc-agent、peri-middlewares→cc-middlewares、peri-tui→cc-tui、peri-acp→cc-acp、peri-widgets→cc-widgets、peri-lsp→cc-lsp）。二进制名 `cc-code` 不变，npm 用户无感；Rust 侧依赖 peri-* 的需改名。
- **清理无用 side-projects**：删除 git-graph、agent-defect-analyzer、daytona、git-stats、llm-gateway、pty-server（保留 peri-sync，`cc-code sync` 在用）。

### Fixes

- **面板打开时 Ctrl+C 先关面板**：此前 `handle_panels` 无条件把 Ctrl+C 穿透到双击退出逻辑，面板开着时双击/长按 Ctrl+C 会直接退出整个 TUI（2026-06-24 修"面板开着退不出"时摆过来的钟摆）。现改为空闲时先关闭顶层面板（session/global），这次按键不计入双击退出；agent 运行中仍穿透中断。另过滤按住 Ctrl+C 的键盘重复事件，一次物理按下只算一次，避免长按被误判为双击退出。

## v0.6.94 — 2026-10-01

### Fixes

- **后台任务输出文件创建加重试**：`DiskOutput::spawn_writer` 建目录/建文件失败时直接静默退出，导致 `exit_signal` 触发后输出文件不存在（Windows CI 偶发 NotFound flake，如 `test_executor_hard_deadline_survives_every_background_mode`；与 #323 同一家族的 runner 文件系统问题）。现改为最多重试 5 次（100ms 递增退避）再放弃；对应测试的读盘断言改为缺失视为无输出（该测试验证的是硬期限行为）。
- **Windows CI 加 Defender 扫描排除（#323）**：ci.yml 给 `.cargo` / `target` / `TEMP` 加 Defender 扫描排除，治文件锁 flake；测试失败时上传日志 artifact 方便诊断。

### Security Fixes

- **ACP stdio 权限审批转发给客户端**：`cc-code acp` 的 `StdioBroker` 之前直接放行所有工具审批请求（且默认 `PermissionMode::Bypass`），ACP 客户端永远收不到 `session/request_permission`，HITL 名存实亡。现改为 `AcpPermissionBroker`：敏感操作以 `session/request_permission` 交给 IDE 客户端审批（allow-once / allow-always / reject-once / reject-always；allow-always 复用会话级审批记忆），客户端不支持/调用失败/未知选项一律按拒绝处理（fail-closed）；stdio 默认权限模式改为 `AutoMode`，无人值守场景仍可用 `session/set_mode` 显式切到 bypass。

---

## v0.6.93 — 2026-10-01

### Security Fixes

- **HITL 预审批不再执行 rtk 二进制（#288）**：`gate_effective_call()` 为判断改写效果会在审批前执行外部 `rtk`（`verify_rtk_executable` 只校验 `--version` 退出码，PATH/RTK_PATH 劫持可致审批前代码执行）。现以纯字符串预测 `process::predict_rtk_rewrite()` 替代，零子进程；存在性检查 `rtk_rewrite_likely()` 替代 `--version` 探测；审批后的真实执行路径不变。另 `YOLO_MODE=""` 视同未设置。
- **配置目录统一到 `~/.cc-code`（#289）**：新增 `peri-agent::app_home`，新目录优先、仅旧目录存在时回退 `~/.peri`、全新安装用 `~/.cc-code`；迁移 MCP 全局配置、OAuth、threads、skills、历史、sync 共 10 处调用点；Jev `PROTECTED_DIR_SEGMENTS` 补上 `.cc-code`，新配置目录保持写保护。
- **npm 安装包校验 + 迁移确认（#290）**：`verifyChecksum()` 对下载的 tarball 做 sha256 校验（对照 release `checksums.txt`），缺失/不匹配直接失败；参考实现 迁移改为确认制：`CC_CODE_NO_MIGRATE=1` / `--no-migrate` 跳过，非 TTY 跳过并提示手动迁移，TTY 下 `[y/N]` 默认否。
- **项目 hooks 需显式信任（#18）**：`.claude/settings.local.json` 的 hooks 不再自动加载执行（防 clone 即 RCE，PoC 已验证可利用）。需 `CC_CODE_TRUST_PROJECT_HOOKS=1` 或项目列入 `~/.cc-code/trusted_projects`。
- **插件 hooks 需信任（#17）**：未信任插件的 hooks 被过滤，不再自动执行。
- **Sync KDF 改用随机 salt（#21）**：`derive_key(pair_code, salt)`，salt 经协议传输，防预计算攻击。
- **Sync relay 警告（#22）**：sender 显示警告，提示 relay 可解密同步内容（含 API keys）。
- **自更新脚本 SHA256 校验（#19）**：下载后验 hash 才执行，防篡改。

### Fixes

- **内存 Arc 循环引用修复（#306）**：tracer.rs 内存占用从 2.00x 降至 1.00x（100% 确认压测）。
- **消息硬上限 10 万（#307）**：`MAX_MESSAGES = 100_000`，超限 degraded_prune，稳定 500MB 左右。
- **跨平台修复（#308-#311）**：Windows 密钥文件 DACL、多行命令 hard-error、Unix 杀进程组、SIGTERM 恢复终端。
- **Robustness 修复（#317-#319）**：settings.json 非对象 JSON 不再 panic、双 SQLite 失败降级内存模式、配置面板保存失败 UI 报错。

---

## v0.6.92 — 2026-09-30

### Fixes

- **详细模式超长无空格命令的头行折行不再孤立 `●`（#287）**

---

## v0.6.91 — 2026-09-30

### Features

- **Windows 启用 mimalloc 全局分配器（#285）**：`peri-tui` 在 Windows 下启用 mimalloc，顺带修复 `/gc` 诊断。

---

## v0.6.90 — 2026-09-30

### Security Fixes

- **默认启用审批、门控评估实际执行命令（#284）**：`YOLO_MODE` 未设置不再默认免审批（fail-closed），`-y/--yolo` 正确接线；HITL 门控评估 rtk 改写后的实际执行命令；`git clone` 加 `--` 防参数注入；`scripts/install.sh/ps1` 修正改名后的仓库地址与二进制名。

### Features

- **排队消息支持 Ctrl+S / Ctrl+X（#282）**：不用鼠标也能补充/删除排队消息。

### Fixes

- **Thought 归并吸收前置只读计数（#280）**：消除重复或断层的动作摘要。

---

## v0.6.89 — 2026-09-30

### Fixes

- **git 分支探测异步化（#278）**：Agent 工作期间状态栏分支名不再陈旧。

---

## v0.6.88 — 2026-09-30

### Fixes

- **markdown 水平线按可用宽度渲染（#273）**：替代硬编码 60 字符。
- **Edit 工具报错文案改英文（#272）**：消除中英混排。

---

## v0.6.87 — 2026-09-30

### Features

- **前台 `!` 命令结果回流 Agent 上下文（#270）**

### Fixes

- **后台任务面板长命令截断 + 补齐 i18n（#271）**

---

## v0.6.86 — 2026-09-30

### Fixes

- **工具名配色统一（#269）**：状态语义只由 `●` 表达。
- **折行末段可整体容纳时不再多拆一刀（#267）**
- **非详细模式 Bash 命令宽度比例调整为 16/19（#265）**
- **Bash 命令超长改为限制宽度截断（#264）**：详细模式完整折行对齐。

---

## v0.6.85 — 2026-09-29

### Features

- **ACP 协议一致性批次 1（#259）**：补齐错误响应、历史回放、ResourceLink、图片能力。

### Fixes

- **跨平台 Hook 命令路由统一（#263）**：命令路由与后台任务生命周期统一，稳定跨平台 Hook 命令测试，修复跨平台 Clippy 导入。

---

## v0.6.84 — 2026-09-29

### Features

- **HITL 审批弹窗支持三选：一次性 / 本次会话 / 拒绝（#261）**：此前只能二选（批准/拒绝）且逐次生效，而语义门（jev）无状态、每次独立判定——模型对绝对路径的 `local_scope` 条件打分在阈值附近抖动（实测 0.07~0.16），同一文件反复编辑会被反复弹窗。现弹窗提供三档：`allow_once`（仅本次）、`allow_always`（写入**会话级审批记忆**，同 `(工具, 路径)` 本次会话内免问）、`reject_once`；`Space` 循环切换、`Enter` 提交、`Esc` 全部拒绝。

### 说明

- 审批记忆为**路径级 + 会话作用域**：键为 `(工具名, 词法规范化路径)`，随会话销毁丢弃；Bash 等无路径的命令类工具不参与记忆。

---

## v0.6.83 — 2026-09-29

### Fixes

- **卡住检测误判空白 thinking（#256）**：`check_stuck` 的指纹判空仅挡字节空串，模型在工具调用轮常返回 `"\n"` / `" "`，被当作有效指纹入窗后逐轮完全相同，第 3 轮即误报「重复的思考循环」。改为 **trim 后判空**，空白指纹直接跳过检测。
- **卡住检测换策略提示改用英文（#256）**：agent 层无 i18n（`LcRegistry` 在 peri-tui，不可反向依赖），且同文件其余注入提示（连续失败 / schema 熔断 / 动作循环）均为英文，本条硬编码中文是唯一异类。文案提为 `STUCK_HINT` 常量，用户侧语言由 system prompt 保证。
- **spinner 状态词 `· thought for Ns` 配色修正（#252, #254）**：该字段属思考完成态，原随耗时升温变色；按设计应始终 MUTED 灰——仅 `thinking` / `still thinking` / `thinking more` 三个进行中状态词随热度变色。

---

## v0.6.82 — 2026-09-29

### Features

- **Thinking 状态行 + 工具动作汇总，对齐参考实现 非详细模式（#248, #249）**：运行中 spinner 行新增第三字段四态状态机——`· thinking` / `· thought for Ns` / `· still thinking` / `· thinking more`（英文固定输出，`still thinking` 优先于 `thinking more`，10s 阈值可调）；verb 整轮固定不再随工具逐段换词；配色随时间四档升温（5s/15s/30s，仅 verb + 状态词变色，其余 MUTED）。
- **消息区思考行改为秒数 + 动作计数（#248, #249）**：`∴ Thought for N chars` → `Thought for Ns[, read N files] (ctrl+o to expand)`。秒数在 LLM 流式层计时并随消息持久化（`ContentBlock::Reasoning.duration_ms`），历史恢复后仍可用；连续多轮「思考 + 只读工具」合并为一行（秒数/计数累加，跨 Bash 不断开仅不计数）；只读工具折叠为计数文案（含单复数）；纯动作行（无 reasoning 的只读工具组）折叠显示。
- **Bash 非详细模式展示输出摘要（#248, #249）**：显示输出前 3 行 + 截断提示 `... (N more lines) (ctrl+o to expand)`，失败必显（判定依据 `is_error` 标记而非内容前缀）。

### Fixes

- **失败 Bash 输出行颜色口径统一（#250, #251）**：非零退出（`bash_failed`）时圆点/状态已标红 Failed，但输出行与 `⎿` 前缀仍灰白——`result_color`/`border_color` 用 `*is_error` 判定漏了 bash_failed 场景。改为 `state.is_error`，与 header 指示器一致，附 2 个回归测试。

### 说明

- **测试命名规范定死并清理 98 处风格债（#250, #251）**：`CLAUDE.md` 明确测试函数/helper 命名必须全英文 `test_<被测对象>_<场景>` snake_case（禁止中英混排），14 个文件 98 处存量中文命名一次性改为英文，注释与断言消息保持中文不变。
- 设计文档、四轮代码审计记录与渲染效果稿见 `docs/designs/2026-09-29-*`。

---

## v0.6.81 — 2026-09-28

### Features

- **权限模式收敛为 `auto` / `bypass` 两档，默认 Auto（#246, #247）**：实测用户不使用其余模式——`Default`/`DontAsk`/`AcceptEdit` 要么每次弹窗、要么半自动，体验差且不智能。`Shift+Tab` 循环变为 `Auto ↔ Bypass`；TUI 启动默认 Auto，`-p` 默认仍为 Bypass。`PermissionMode::from(u8)` 的**未知取值一律回退 Auto**，绝不因陈旧/异常值意外滑进 Bypass。`--permission-mode` 只认 `bypass`（其余回退 auto），`-a/--approve` 等同 `--permission-mode auto`。
- **规则来源扩展至 `.claude/hooks/*.sh`，并带来源标注**：很多项目的"铁律"并不写在 CLAUDE.md 里，而是直接做成可执行 hook；那些脚本里的 guard 条件与 `BLOCKED` 消息本身就是自然语言规则。采集顺序为个人 `CLAUDE.local.md` → 项目 `CLAUDE.md`/`AGENTS.md` → 项目 hooks → 全局 `~/.claude/CLAUDE.md`，拦截时按来源文件分组展示，用户可直接定位到该改哪个文件。

### Fixes

- **补齐硬黑名单：下载执行（`curl|bash`）及其全部常见变体（#246, #247）**：此前 `curl|bash` 与 `sudo`/`env`/`xargs` 包装、`/bin/bash` 路径前缀、`bash <(curl …)`、`sh -c "$(curl …)"` 等 **8/8 变体全部漏拦**，只挂在"升级语义判定"上——等于用概率墙守最经典的 RCE。现补齐包装命令、进程替换、命令替换三类形态，并覆盖解释器家族 `eval` / `python -c` / `node -e` / `xargs`（实测 5/7 漏拦）。
- **修复写路径穿越可跳过整道门（#246, #247）**：`path.starts_with(cwd)` 为纯词法比较，`cwd/../../etc/passwd` 会被判成"项目内非受保护路径"直接放行，**写保护与策略检查全都没发生**。改为先 `normalize_lexical` 再比较，并配反向对照（真正的项目内路径仍走快车道）。
- **硬黑名单不再误伤日常命令（#246, #247）**：普通 glob（`* ? [] {}`）此前被当作"目标静态不可解析"，导致 `rm -rf build/*`、`rm -rf node_modules/*` 落入**不可申诉**的硬拦。"不可解析"收窄为 `$(…)`/`${…}`/`$VAR`/反引号/`~user`；同时**补上**根目录 glob（`rm -rf /*`）与 Windows 盘符根，防止削弱。
- **判定不可用时不再堵死整个会话（#246, #247）**：原 fail-closed 在端点 503/超时时会把**所有工具调用**拦下（实测）。按上游语义改为**回到人工确认**：网络失败、超时、错误响应、不可读响应体、越界分数一律视为 unknown，多问一次而不是拒绝执行，更不是把 agent 一棍子打死。
- **缺失/越界分数不再被当作"明确违规"（#246, #247）**：此前缺失分数按 `0` 处理 → 判为"安全条件被明确违反" → 硬拦，把"网关响应不完整"伪装成"用户违规"，错误且无从排查。现标记为 `unknown` 并回到人工确认；仅**明确违规**才拦。
- **规则提炼不再静默丢规则或静默失效（#246, #247）**：规则较多的大 CLAUDE.md 会撞输出上限导致**整块产出为空**（本仓库自身实测 0 条）。新增分块提炼、截断抢救（尽力捞回已生成的完整规则对象）、失败自适应对半切分；提炼失败不再与"来源本来就为空"混为一谈，改为可观测并明确告警"规则当前没有被执行"。
- **不再设置 `max_tokens` 进行规则提炼（#246, #247）**：推理模型的**思考同样消耗输出预算**，此前的 4096/8192 会被思考吃光 → `content` 为空 → 一块规则全丢；改为继承 provider 配置（默认 32000）。
- **提炼口径改为「操作级禁令」（#246, #247）**：原口径"面向 AI 编码助手的禁止/约束"把工程规范一并捞入（本仓库提出 33 条，几乎都不是安全门能拦的），稀释判定注意力。现只提取"约束即将执行的操作"且"仅凭本次工具调用参数即可判断"的规则。
- **确定性防线与语义判定解耦（#246, #247）**：此前门只在配置了 API key 时构造，未配 key 时 Auto 模式连硬黑名单一并丢失（零确定性防护）。现门始终构造，Auto 模式改为**先跑确定性层 → 有凭据才走语义 → 否则落回分类器**。关闭语义判定不再等于关闭确定性防线。

### 说明

- 拦截文案按读者拆分：**用户段**为白话、不含内部术语，**agent 段**为结构化说明（`decision` / `rule` / `source` / `retryable`）并明确给出可行路径与"禁止绕过"的指令。
- 审计日志补充判定请求 `id`、`cost`、三态结论与每个条件的 `(概率, 是否未知)`；上下文补充当前 git 分支等事实，使"禁止在 main 上提交"这类**带条件**的规则可被判定。
- 实测留档：**规则数稀释实验**（1/10/40 条规则均正确拦下同一条清晰违规）→ 据此**不做**逐规则判定/预筛的架构改动。

---

## v0.6.80 — 2026-09-28

### Fixes

- **`-c`/`-r` 恢复会话时丢失 recap 与任务完成总结行（#244, #245）**：`✻ Cooked for 25s · done 14:26` 与 `※ recap: ...` 两行原为纯内存展示态、不进 message history 也未持久化，重启恢复即消失。`ThreadMeta` 新增 `latest_recap` 与 `last_task_summary`（`TaskSummary { verb, elapsed_ms, done_at }`）字段并落库（SQLite `threads` 表幂等 `ALTER TABLE` 迁移，旧 `meta.json` 靠 `#[serde(default)]` 兼容）；`ThreadStore` 新增 `update_latest_recap` / `update_last_task_summary` 窄更新接口，SQLite override 为单列 `UPDATE`，避免每轮重写 ~1MB 的 `cached_context`。`handle_recap_completed` 与 `cleanup_agent_state` 分别写回两字段，`open_thread()` 读 meta 回填并无条件覆盖，顺带修复切换 thread 时 recap 残留串台。

---

## v0.6.76 — 2026-09-24

### Features

- **新增 `/recap` 会话回顾命令与终端失焦自动回顾（#237, #238）**：`/recap`（别名 `/away`、`/catchup`）生成一句话回顾——「高层目标 + 当前任务 → 下一步」，单轮禁用工具、不写 history、支持 Ctrl+C 取消；TUI 新增 `AutoRecapState` 调度状态机，终端失焦 + 已完成轮次 ≥3 + 60s 静默后自动触发（两次回顾间至少新增 2 轮，聚焦取消、失败 30s 重试、in-flight 结果按 revision 失效）；`/config` 新增「会话回顾」开关（`config.auto_recap`，默认开），环境变量 `PERI_AUTO_RECAP_DELAY`、`PERI_AUTO_RECAP_MIN_TURNS` 可覆盖。
- **非流式 Anthropic 响应自适应兼容反向代理 OpenAI 格式（#238）**：部分代理网关在非流式请求下返回 OpenAI 格式 JSON（`choices`/`message`/`tool_calls`/`usage.prompt_tokens`），新增 `parse_anthropic_json_response` 统一解析入口，缺少 `content` 字段时回退解析 OpenAI 结构并合成 Anthropic block。

### Fixes

- **recap 模型来源与 compact 解耦（#237, #238）**：原实现复用 `compact_model`，用户关闭「自动压缩」（或设 `DISABLE_COMPACT` / `DISABLE_AUTO_COMPACT`）后 `/recap` 与自动回顾会永久失效；改为独立 `aux_model`，不受 compact 开关影响。
- **recap 渲染接入 i18n 并移除死代码（#238）**：后缀提示原本从 LLM 摘要文本 `rfind`（摘要本身不含该后缀，生产环境永不显示）且硬编码英文，改为 i18n key `app-recap-hint`；删除无任何代码路径触发的 `※ recap:` 特殊渲染分支。

## v0.6.75 — 2026-09-24

### Features

- **工具参数 Schema 校验可读性优化与启发式诊断（#234, #235）**：重构 `validate_against_schema`，对齐参考实现 `formatZodValidationError` 风格，结构化分项输出缺失参数、意外未定义参数与类型不符；基于参数特征指纹对 `WebFetch`（误传 `url`+`prompt`）、`Bash`（误传 `command`）、`Agent`（缺失子 agent 类型且未设 `fork`）提供启发式纠偏建议；新增同一工具连续 2 次参数校验失败熔断拦截（Circuit Breaker），防止模型陷入盲目重试死循环。

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
  - 将用户 `!` 本机命令展示重构为与 参考实现 一致的命令块布局；
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
  - 对齐参考实现 风格的过去式总结行动词与时刻展示（`✻ {verb} for {elapsed} · done {HH:MM}`）；
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

- **npm install 配置迁移字段格式修复**：`migrateFromClaudeCode` 输出改为 Rust 配置期望的 camelCase 字段（`type`、`apiKey`、`baseUrl`），修复新用户从 参考实现 迁移后配置无法识别的问题
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
