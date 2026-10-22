# cc-code

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 Claude Code 全家桶**

[![npm](https://img.shields.io/npm/v/@cc-claw/code)](https://www.npmjs.com/package/@cc-claw/code)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE)

## 最近更新

v0.6.174：**Write 文件内容预览恢复**——修复新建、覆盖成功后不显示内容，以及追加预览重复内容的问题。主、子 Agent 的旧内容在写入前采集，界面通知晚到也能展示正确预览；同内容写入仍不展示空 diff（#654）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.173：**URL 背景与状态栏对齐**——工具结果与 `!` 输出的 URL 使用终端默认背景，避免同为 ANSI Cyan 却与状态栏模型名出现色差（PR #656）。

v0.6.172：**欢迎卡显示模型档位与版本更新提示**——空会话首屏补齐两处信息：标题拆成品牌名（ACCENT）+ 版本号（MUTED），并新增 `model · (effort effort)` 副标题（模型或档位缺失时整行省略）；有新版本时用“Update available! <当前> → <最新>”+ “To update, run: cc-code update”两行**替换**原开始提示，卡片高度恒为 7 行。检查在启动后后台跑一次（只认 `npm-v*` tag、跳过 prerelease、超时 3s、失败静默），渲染不等网络；`CC_CODE_DISABLE_UPDATE_CHECK=1` 可关闭。同版空会话不再显示`Cache - · MEM 19MB`（#653）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.170：**工具结果与 `!` 命令输出里的 URL 链接色定稿 ANSI Cyan**——此前裸 URL 用 `theme::cur().ide`（深色 `#4782C8`），链接色随 `/theme` 漂移，且与正文 `text_soft` 明度接近，扫读时难以快速定位；现 `style_urls()` 固定 `Color::Cyan` + 下划线，不随主题切换，与状态栏模型名 / 分支名同一口径。覆盖所有走 `⎿` 前缀的工具结果行与 `!command` 输出；仍非 OSC 8 真超链接，点击由终端自身 URL 识别提供。Markdown 链接（`[text](url)` → SAGE）是另一条路径，不受影响（#649）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.169：**长会话详情按视口渲染**——`/config` →「详情渲染」可选 `auto`（默认）/ `cache` / `virtual`，即时生效；虚拟路径按需物化窗口，离屏成品按共享 16 MiB 预算淘汰，逻辑锚点支持滚动与跨淘汰复制。预算不等于进程 RSS 上限，原始历史和可见巨型消息仍占内存；可随时切回 `cache`（#629，PR #648）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.157：**`/theme` 新增 `auto`，跟随终端颜色能力与背景明暗**——两级判定：① **颜色能力**（`TERM_PROGRAM=Apple_Terminal` 这类只有 16/256 色的终端 → `light-ansi` / `dark-ansi`，其余真彩色终端 → `light` / `dark`）；② **背景明暗**（OSC 11 查询按回复的 sRGB 相对亮度判定，终端不应答时 macOS 退回系统外观 `AppleInterfaceStyle`、其余平台按深色兜底）。判定结果缓存、仅在持久化主题为 `auto` 时预探测，`/theme` 面板该行显示 `Auto (follow terminal: <解析结果>)`（#604）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.156：**测试基础设施修复，无用户可见行为变更**——11 个 RSS 内存测试此前统一读 `/proc/self/status` 的 `VmRSS`，而 `/proc` 只存在于 Linux：macOS 上读取失败被吞成 0，再被饱和减法洗成「无增长」，断言恒绿（实测 macOS 上打印 `RSS = 0 KB (+0 KB)` 且 `test result: ok`，同一 pid 用 `ps -o rss=` 读到 964 KB）。现改用 `sysinfo` 的 `process.memory()` 取数、采集失败直接 panic 而不返回 0，并新增守护测试（触碰 64 MiB 后 RSS 必须涨 ≥32 MiB）防止测量通道再次退化（#589）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.155：① **状态栏按主题取色**——`/theme` 切到 Light 系列后消息区是浅底深字，状态栏仍沿用深色主题写死的白字（`Rgb(255,255,255)` / `Rgb(153,153,153)`），几乎不可读；现由 `SbColors` 按当前主题解析状态栏一帧所需的全部颜色，`default` 主题逐值保留原字面量、观感零变化，其余主题取 `/theme` palette（#597）；② **markdown 缓存补字节预算**——此前只有 1024 条的**条数**上限、没有**字节**上限，满容量常驻约 52.8 MB（且缓存键含 `max_width`，每次 resize 都会灌入一批新变体）；现加 8 MB 字节预算、条数上限降为兜底，`/gc` 一并显示预算读数（#594）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.154：① **`/gc` 的差值不再被钳成 0**——`OS RSS - jemalloc resident` 与「已知估算 - allocated」此前用饱和减法，而被减的一方并不保证更大：实测 resident 93.6 MB 大于内核 RSS 84.9 MB 时真差是 `-8.7 MB`，却被打成 `= 0 B`，渲染出一个算术不成立的等式，而「差值为负」与「两边完全相等」在诊断上含义相反。现改为有符号差、负值时补解释提示；同批删掉与 `RSS:` 完全重复的 `OS RSS:` 行（同一个 sysinfo 读数，整数 MB 截断后再相减白丢 0.8 MB）、jemalloc stats 读失败不再用 RSS 冒充 `allocated`、macOS 上恒为 0 的 `retained` 改显示「不适用」（#593）；② **`Ctrl+Enter` 在支持键盘增强协议的终端上真正生效**——`keyboard_modes.rs` 从未被声明/调用，应用从未推送 kitty 键盘协议，现接通后在 iTerm2 ≥3.5 / WezTerm / Ghostty / kitty / Alacritty 上可用；排队提示同时按平台显示真能按下的键：Windows `Ctrl+Enter`、macOS `⌥S`、Linux `Alt+S`（#592、#591）；③ **macOS 启动时清屏一次**，修复 Terminal.app 因 alternate screen buffer 跨程序保留导致的旧画面/黑条残留（#582）；④ 另有 TUI 用户可见文案全面接线到 i18n（中文界面不再残留英文、英文界面不再出现中文）、状态栏三行行首缩进统一 2 空格、等待后台子 Agent 期间终端标题保持动画。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.151：① **后台 Bash 完成通知改为运行中插入**——agent 运行中后台化（`Ctrl+B` 或前台超时自动转后台）的命令跑完后，通知此前一律积压到轮次结束才注入，延迟上限等于任务剩余时长（实测十几分钟）；而轮末优先处理用户排队消息会跳过通知的 drain，中断/报错分支根本不消费队列。现复用既有 steering 通道，在通知产生时即插入当前轮次（运行中每帧回收回执，空闲仍合并为一条，跨会话仍留给对应会话），并把消费回执的确认时机钉在「真正写入 state 之前」，因此 `-32001`/`-32002` 都代表未送达、可安全重试；无法判定的错误只保留不重投；中断/报错后不自动唤醒新一轮，滞留通知随下一次主动提交送出（#571）；② **系统提示写入后立即重绘**——空闲态输入 `/dream` 这类未知命令回车后界面毫无反应：提示其实已写进消息状态，但写法只更新 `view_messages`、不发渲染事件，要等下次按键才显示；现把重绘内置于 `push_system_note`，同类提示路径（`/rewind` 无历史、排队消息 steer 失败、切换会话失败等）一并修复（#573）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.150：① **`/history` 改为「你输入过的内容」列表**——CLI 语境里 `history` 指用户输入历史（shell 的 `history` 就是列历史命令），此前 `/history` 打开的却是历史会话列表，想查自己输入过什么只能 `Ctrl+↑/↓` 一条条回溯。新增输入历史面板：`↑`/`↓`/`j`/`k`/`PageUp`/`PageDown` 导航、`Enter` 只把选中项回填输入框（不代发）、`Esc` 关闭，数据源与 `Ctrl+↑/↓` 完全同一份（跨会话持久化）。**注意语义变更：原「历史会话浏览」迁到 `/resume`**（#566）；② **状态栏系统提示至少停留 5 秒**——Jev 语义门降级播报（429/超时触发的兜底分类器提示）此前会因紧随其后的 `ToolStart` 等事件被立刻清除，一闪而过读不到原因，现按「最短可见时长」范式垫 5 秒（#564）；③ **修复多轮对话后工具资源持续累积**——工具包装器未析构内部 `Box`、延迟工具代理与子 Agent 模型工厂反向持有注册表/会话池形成引用环，现补唯一析构并改弱引用，最后一个持有者释放后资源正常回收（#568）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.149：**会话中心详情面板重排 + 状态栏会话计数 +「内联 Diff」开关收敛**——① 状态栏 `← {n} agents` 改按会话中心 `Ready` 口径统计：只算当前目录 24h 内有更新、且未进入 Needs you / Working 的根会话，长期使用的目录不再显示累计历史数（实测 194 → 10）（#554）；② `/agents` 详情面板重排：`Model:` / `Reasoning:` 标签与值同行，`Project` / `Branch` / `Last message` / `Prompt` 标签独占一行、块间留空行，值沿用状态栏配色（模型青 / 项目黄 / 分支青），`Last message` / `Prompt` 预览改走 markdown 渲染并折行（长链接不再被 `…` 截断），高度上除 `Prompt` 预留 3 行外全部给 `Last message`，删除确认弹层底色改用 `popup_bg`（#554）；③「内联 Diff」开关（`config.diff_enabled`）不再顺带展开整页——开启后普通页面只多出 Write/Edit 的 diff，shell 输出 / 工具折叠 / 参数折行 / reasoning 一律保持原样（#558）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.148：**Bash 失败退出码置顶与正文整齐缩进**——TUI 终端层优化失败展示体验：非零退出码提升为首行置顶 `  ⎿ Error: Exit code {code}`（红色醒目展示），自动剥离末尾冗余的 `[Exit code: N]` 标记并将输出正文以 4 列悬挂缩进对齐，长输出时不再淹没退出原因；同版完成 `message_render.rs`（90KB+）目录级解耦拆分为独立的 `decorators/`（Bash / Glob / Read / AskUser）模块，并清扫全仓历史测试命名为 100% 全英文 snake_case。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.147：新增跨项目 **Agent Command Center**：`/agents` 或空输入框 `←` 查看全部项目根会话，支持分组、五档状态、分页、详情、搜索、重命名与二次确认永久删除。Enter 恢复目标项目，`n` 在选中项目新建空会话；存在运行任务或审批时保持原会话。原 agent 定义入口改为 `/agent-design`，`/tasks` 入口由中心替代。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.146：修复 Windows CI 上 cc-tui 测试的间歇性失败（**内部改动，无用户可见行为变化**）——这些用例用固定墙钟超时等待真实子进程（`cmd`/git-bash/嵌套 powershell），libtest 并发时互相抢进程与 IO，在 4 核 runner 上被放大成超时；现用全局锁串行化真起进程的用例并统一放宽等待预算，不改任何用例语义（#538）。

v0.6.145：三项 TUI 输入修复——① 输入框内 `/command` 用 suggestion 蓝高亮，规则对齐 Claude Code（`!` shell 态跳过，避免把 `/etc` 之类路径着色）（#529）；② Rewind 对话回滚从「双击 `Esc`」改为正式命令 `/rewind`（别名 `/undo`）——`Esc` 是取消/中断键，连按两次会误弹出回滚面板，而输入 `/rewind` 反而无效（#531）；③ Agent 运行中按 `Enter` 不再把命令静默变成模型提问——此前 `/exit` 不退出、`/gc` 不回收内存、未知命令反被模型回答。判定收敛到唯一入口 `CommandSystem::resolve`：普通消息 / Skill / ACP 命令 / 转发型命令入队等本轮结束后执行，需要空闲 UI 的命令、`!` 输入与未知命令一律拒绝并**保留输入框内容**（#533）。

v0.6.144：修复后台子 Agent 抽屉「正在做什么」列的闪动——该列不再被工具名（`Bash`/`Grep`）覆盖，只显示子 Agent 最近一句助手文本；并新增 200ms 刷新节流（渲染快照，丢渲染帧不丢状态，结束时立即同步）。

v0.6.143：修复两个 `/theme` 相关问题——① 主题面板补齐键盘路由，此前 `↑`/`↓` 不移动光标、`Esc` 关不掉面板（#521）；② 状态栏配色固定、不再随主题切换，恢复打磨定稿的固定观感（#523）。

v0.6.141：**Spinner tips 调度对齐 Claude Code**——tips 改为带会话冷却的调度模型：每轮开始挑选一条并扣留、轮结束才发布（每轮最多一条），挑选取「距上次展示最久」者，历史持久化到 `~/.cc-code/spinner-tips.json`；新增 `spinner_tips_enabled` 与 `spinner_tips_override` 自定义配置。另：MCP 协议库 rmcp 升级至 3.5.1，适配 3.x 数据模型与接口。

v0.6.140：**`/theme` 主题命令**——7 套可切换配色（现行配色为 `default`，其余 6 套与 Claude Code 逐值一致），选择面板实时预览、持久化恢复；同时 **Spinner 变色对齐 Claude Code**：动词改走成对流光（底色 + 流光色，±1 列窄带扫过），新增停滞超 10 秒平滑转红，删除自研的「时间热度四档」。

v0.6.139：**自动记忆系统（Auto-Memory）落地**——模型可跨会话记住项目事实：纯文件架构、零专有工具，模型用 `Write`/`Edit` 直接维护 Markdown 记忆文件，入口索引 `MEMORY.md` 有界（≤200 行 / ≤25KB），与 `~/.claude/projects/`、`~/.cc-code/projects/` 既有记忆资产双向兼容；安全门为记忆路径设豁免，会话创建时冻结索引以保障 Prompt Cache 稳定。同版还有：主 Agent 可用 `Agent(action="stop", task_id=…)` 精确取消后台子 Agent（停止行区分 `by user` / `by main Agent`、等待计数行实时递减）；系统提示词静态段落对齐 CC 行为约束（Scope discipline、授权与影响判定、结果诚实性）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.138：核心工具描述对齐 Claude Code 官方措辞。① Bash/WebFetch 改为 **NOT 反向枚举**（读用 `Read` NOT `cat`；内容检索用 `Grep` NOT `grep`/`rg`；写用 `Write` NOT `echo>` 等），WebFetch 补「仅远程 http(s)，无点主机名不支持」边界；② 补齐 `Grep`/`Read`/`TodoWrite`/`AskUserQuestion` 边界句（Grep 禁用 `grep`/`rg`、Read 不能列目录、TodoWrite 单一进行项、AskUserQuestion 决策边界）；③ **`Write` 改为 NEVER 语义**——默认不再主动创建 `.md`/README，仅用户明确要求时创建。另修复 ReAct 纠正提示（连续失败/Schema 熔断/动作循环）此前以 System 消息注入、被适配器抽到请求顶层导致**模型看不见、导出也丢**，现并入所属 ToolResult 正文（#495）；`-p --output-format stream-json` 的 `tool_use` 事件补齐真实 `input` 参数（此前恒为 `null`）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.137：修复后台 shell 完成通知的**多轮级联唤醒**。通过 `Bash(run_in_background=true)` 启动的任务完成后会唤醒 agent，但旧逻辑在一轮内积压多条通知时每次 `Done` 只消费 1 条，导致 n 条通知触发 n 轮 turn（每轮再触发下一条），表现为连续无意义的「收到」。现按批次**合并注入**：空闲态把本批 + 队列中同会话待注入通知一次性 drain 拼接为**单条消息**（n 条压成 1 轮 turn），推理中统一入队、`Done` 后合并；合并仅在消息层拼接、保留每条通知 XML，信息不丢失（#491）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.136：一批 TUI 修复在同一版本落地。① 切入刚跑完的后台子 Agent 独立视口时，底部现常驻显示 `✻ Brewed for 32s · done 16:30` 完成总结行（此前完全空白），被停止/失败的子 Agent 不显示成功文案（#485）。② Windows 下默认静音全局分配器 mimalloc 的底层 `stderr` 裸打，避免长时间或高负载会话被 `mimalloc: cannot commit OS memory` 打花撕裂界面（#483）。③ 行首状态圆点统一为公共常量 `●`，消除子 Agent 行比其他工具「小一号」的字形割裂（#487）。④ 修复 auto-compact 被误判为手动 compact、中途提前关闭 loading 而引发的 Tip/spinner 消失、无法中断、回车反转等连锁问题（#481）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.130：子 Agent 统一为**只有后台模式**，底栏抽屉是它们的唯一呈现。空输入框按 `↓` 浏览后台任务，`↑/↓` 移动游标、`Enter` 进入独立视口（独立滚动位置、输入草稿与工具统计）、`x` 停止选中任务、`Ctrl+X Ctrl+K` 停止全部；正在阅读的子 Agent 结束时留在屏幕上（该行转绿显示 `✓ completed`），离开视口才回收。启动摘要改为对齐的 `├── / └──` 树形块，抽屉只用亮白/灰两档文字、游标行不设底色，右侧 token 指标为跟随流向的单箭头。`fork` 与普通子 Agent 合并为同一条构建路径，`run_in_background` 参数退场。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.129：输入 `@` 即显示文件候选。默认仅列当前目录，最多 15 项；面板最多显示 10 行并适配终端高度，继续输入关键词可搜索更多文件。搜索中、空目录和无匹配结果均有提示。

## 安装

[![install](https://img.shields.io/badge/npm_install-g%20@cc--claw%2Fcode-47d147?style=for-the-badge&logo=npm&logoColor=white)](https://www.npmjs.com/package/@cc-claw/code)

```bash
npm install -g @cc-claw/code
```

> [!WARNING]
> **npm 12+（2026 年 7 月发布）默认阻止依赖包的 `postinstall` 脚本，而本包的平台二进制正是在该脚本里下载的。** 安装会显示成功，但运行 `cc-code` 时报 `spawnSync ...cc-code.exe ENOENT`（bin 里只有 node 脚本、没有 exe）。请显式放行：
>
> ```bash
> npm install -g --allow-scripts=@cc-claw/code @cc-claw/code
> ```
>
> 对所有全局安装长期放行：`npm config set allow-scripts=@cc-claw/code --location=user`
>
> 用 **pnpm / bun**？它们同样阻止 build script —— 先 `pnpm approve-builds`，否则装不到二进制。

## 升级

```bash
npm update -g @cc-claw/code
```

npm 12+ 需带同样的参数：`npm update -g --allow-scripts=@cc-claw/code @cc-claw/code`。

## 快速开始

```bash
# 启动交互式 TUI
cc-code

# 直接给任务
cc-code "解释这个项目的目录结构"

# 指定模型
cc-code --model deepseek/deepseek-chat "重构这个函数"
```

## 支持的平台

| 平台 | 架构 |
|------|------|
| Linux | x86_64, aarch64 |
| macOS | x86_64 (Intel), aarch64 (Apple Silicon) |
| Windows | x86_64 |

## 常用快捷键

| 快捷键 | 功能 |
|--------|------|
| `Shift+Tab` | 循环切换权限模式（auto / bypass，共两档） |
| `Ctrl+P` / `Alt+P` | 打开命令面板（Provider / Model / Effort 配置） |
| `Ctrl+B` | 将正在运行的前台 Shell 命令转入后台执行 |
| `Alt+V` / `Ctrl+V` | 从剪贴板粘贴图片附件（`Alt+V` 专用于穿透 Windows Terminal 等现代终端对 `Ctrl+V` 的拦截） |
| `Ctrl+O` | 切换详细模式（Verbose） |
| `Ctrl+U` / `Ctrl+D` | 消息列表快速半页滚动（输入框为空时） |
| `Ctrl+C` | 中断执行（执行中）/ 双击退出（空闲时） |

## 常用 Slash 命令

| 命令 | 说明 |
|------|------|
| `/model` | 浏览并选择 Provider/模型，或直接切换 |
| `/commit` | 基于当前代码改动一键生成 commit 并提交 |
| `/review` | 针对当前 PR 或分支代码发起代码审查 |
| `/export` | 将当前会话导出为干净的 Markdown 文档 |
| `/gc` | 手动释放内存并输出内存占用诊断信息 |
| `/lang <lang>` | 切换系统提示与界面语言（en / zh-CN） |
| `/recap` | 生成一句话会话回顾（目标 + 当前任务 + 下一步），别名 `/away`、`/catchup` |
| `/rewind` | 打开会话历史回滚选择器，回退到某条历史消息，别名 `/undo` |
| `/clear` | 清空屏幕开启新会话 |

## 为什么选 cc-code？

| 对比项 | 其他终端 Agent | cc-code |
|--------|---------------|------|
| 运行时 | Node.js / Bun，动辄吃 1GB 内存 | Rust 原生，启动快，~50MB 内存 |
| 模型绑定 | 锁死一家 LLM | 随便换：Anthropic、OpenAI 兼容、DeepSeek、GLM |
| Prompt 缓存 | 每轮重算，token 白烧 | 冻结 system prompt，95-99% 缓存命中率 |
| Claude Code 生态 | 不兼容 | 直接用 `.claude/` 配置、agents、skills、hooks、MCP |

## 链接

- [GitHub](https://github.com/cc-claws/cc-code)
- [Issues](https://github.com/cc-claws/cc-code/issues)
- [English README](https://github.com/cc-claws/cc-code#readme)
