# TUI Style Guide

> **最后更新：2026-10-21**
> 本次更新：工具结果行 / `!` 命令输出里的裸 URL 改为真 OSC 8 超链接——输出层注入、亮蓝 `38;5;12`、无人工下划线（见「消息流 › URL 超链接」）；原 ANSI Cyan + 下划线定稿与 `style_urls()` 已废止。
> 此前更新：新增 `/theme` 命令与 7 套主题（default 为现行配色，其余 6 套与参考源码逐值一致），渲染改走当前调色板；**状态栏为例外**——配色为打磨定稿的固定值，不参与主题化（#523）；「详情渲染」新增 `auto` / `cache` / `virtual` 三态切换（见「/config 面板」）。

## 设计哲学

中性灰层级 + Claude 暖橙品牌色。背景透明（弹窗光标行和用户消息区除外）。信息层级用亮度区分（TEXT/MUTED/DIM），颜色表达状态语义。

## 色板

源码：`cc-tui/src/ui/theme/`（`mod.rs` 主题注册表与当前主题状态、`palettes.rs` 七套调色板数据）、`cc-widgets/src/theme/presets.rs`（DarkTheme / CustomTheme trait 实现，渲染时由 TUI 按当前调色板注入）。

### 主题系统（/theme）

八套主题：`auto`（跟随终端，见下）+ `default`（cc-code 现行配色，即下表色板）、`dark`、`light`、`light-daltonized`、`dark-daltonized`、`light-ansi`、`dark-ansi`。后六套的名称与全部 69 个语义色字段与参考源码的 `src/utils/theme.ts` **逐值一致**（`rgb()` 原样映射、`ansi:*` 映射 ratatui 标准 16 色），由 `palettes_parity_test.rs` 逐字段断言锁定，禁止手改漂移；cc-code 独有元素（抽屉、思考热力、状态徽章、diff 前景等）为调色板尾部的扩展字段，default 取现行值，其余主题从该主题自身字段派生、不引入新色值。渲染代码统一经 `theme::cur().<字段>` 读取当前调色板，禁止再引入编译期颜色常量或裸 `Color::Rgb`（数据驱动的 ANSI 转义解析除外）。**固定配色例外（不随 `/theme` 切换）只有状态栏一处**：`cc-tui/src/ui/main_ui/status_bar.rs` 的配色是打磨定稿的固定值，经模块内固定配色常量 `sb` 输出，渲染结果与主题系统引入之前逐字节一致（#523）。工具结果行 / `!` 命令输出里的 URL 曾是第二处例外（ANSI Cyan + 下划线），自 v0.6.182 起改为真 OSC 8 超链接（亮蓝 `38;5;12`、无人工下划线），见「消息流 › URL 超链接」。`/theme` 打开选择面板（光标移动实时预览、Enter 确认、Esc 还原），`/theme <name>` 直接切换；选择持久化到配置 `theme` 字段，启动时恢复。下表为 default 主题色板。

**`auto`（跟随终端）** 由 `cc-tui/src/terminal_theme/` 解析成上面某套具体主题，两级判定：颜色能力（`TERM_PROGRAM=Apple_Terminal` 只支持 16/256 色 → 选 `*-ansi`；其余终端按真彩色 → 选 `light`/`dark`）+ 背景明暗（启动时给终端发 OSC 11 查询问它自己的背景色，回 `rgb:RRRR/GGGG/BBBB` 后按 sRGB 亮度阈值 0.5 判；终端不应答时 macOS 退回系统外观 `AppleInterfaceStyle`、其余按深色）。**不要**从 Terminal.app 的 `Default Window Settings` profile 推断背景色——那只代表下一个新窗口用哪个 profile，单个窗口可在 `Shell > Show Inspector` 里指定（本机默认 profile 是黑底的 Pro，实际窗口却是白底的 Basic）。探测结果按进程缓存（`theme::cur()` 在渲染热路径上），OSC 11 的回复必须在 `EventReader::start()` 之前读走，因此只在持久化主题为 `auto` 时于 `enable_raw_mode()` 之后预探测一次；运行期再切到 `auto` 时来不及做终端往返，直接走平台回退。`/theme` 面板的 `auto` 行会标出当前解析到的具体主题名。

### 强调色

| 名称 | 色值 | 用途 |
|------|------|------|
| ACCENT | `#D77757` | Claude 暖橙品牌色：用户消息前缀、Welcome Logo、激活边框、光标、关键操作、输入框提示符 |

### 功能色

| 名称 | 色值 | 用途 |
|------|------|------|
| SAGE | `#4EBA65` | 明亮绿：工具调用指示器（⏺）、SubAgent、对勾标记 |
| WARNING | `#FFC107` | 明亮琥珀：次要强调、Markdown 标题、重试状态 |
| ERROR | `#FF6B80` | 明亮红：错误/拒绝、失败工具结果边框、系统错误消息 |
| THINKING | `#A2A9E4` | 标准紫：面板选中行、面板标题、/model 面板光标、Config 编辑高亮 |
| LOADING | `#93A5FF` | 浅蓝紫：Loading spinner、Auto Mode 权限标签 |
| BASH_BORDER | `#FD5DB1` | 粉红：Bash 工具结果边框 |
| MODEL_INFO | `#A0825F` | 棕金：常量保留但**当前未被状态栏使用**（状态栏模型名走 ANSI Cyan，见「状态栏」章节） |
| TOOL_NAME | `= SAGE` | 语义别名：工具名展示色 |
| SUB_AGENT | `= SAGE` | 语义别名：SubAgent 展示色 |

### 文字层级（三级亮度）

| 层级 | 色值 | 用途 |
|------|------|------|
| TEXT | `#FFFFFF` | 纯白：主文字、AI 回复前缀、Running 指示器、展开工具组标题 |
| MUTED | `#999999` | 浅灰：次要文字、标签、路径、工具参数、聚合摘要、Spinner 辅助信息 |
| DIM | `#505050` | 深灰：极弱文字、占位、已完成项、分隔符、折叠工具组参数 |

### 底色

| 名称 | 色值 | 用途 |
|------|------|------|
| USER_BG | `#373737` | 用户消息底色（所有行带底色，与 sticky header 一致） |
| POPUP_BG | `#000000` | 纯黑弹窗底色 |
| CURSOR_BG | `#262626` | 中性暗灰（光标行背景，列表选中行） |
| SELECTION_BG | `#264F78` | 文本选区背景色（深色主题下网页默认选中蓝的暗色版本） |
| SUB_AGENT_BG | `#1E1E26` | SubAgent 嵌套消息背景色（比终端背景略亮，形成视觉容器） |

### 边框

| 名称 | 色值 | 用途 |
|------|------|------|
| BORDER | `#505050` | 中性灰空闲边框、标准面板边框 |
| BORDER_ACTIVE | `= ACCENT` | 激活边框：输入框/panel focus 状态 |
| BORDER_DIM | `#2A2A30` | 非活跃分隔线 |

### Diff 高亮色（diff 内容自动检测时）

| 名称 | 色值 | 用途 |
|------|------|------|
| DIFF_ADD | `#6EB56A` | 绿色：`+` 添加行 |
| DIFF_REMOVE | `#CC463E` | 红色：`-` 删除行 |
| DIFF_HUNK | `Cyan` | 青色：`@@` 上下文标记行 |

## Markdown 渲染

源码：`cc-widgets/src/markdown/`。通过 `DefaultMarkdownTheme` 参数化：

| 元素 | 颜色 | 说明 |
|------|------|------|
| 标题 (H1-H6) | 无前景色 | H1 为 BOLD+ITALIC+UNDERLINED，H2 及以下仅 BOLD（#666 对齐参考实现） |
| 正文 | TEXT | `#FFFFFF` |
| 行内代码 | 主题 `permission()` 色 | dark 下 `#B1B9F9`，对齐参考实现 `formatToken` 的 codespan 分支 |
| 代码块 | TEXT 前景色 + SAGE `│` 前缀 | 多行代码块有行前缀，单行代码块无前缀 |
| 链接 | SAGE | `#4EBA65` |
| 引用块 | `▎` 条 DIM + 正文 ITALIC | 行首空格不带样式 |
| 列表符号 | 无序 `- `；有序按嵌套深度在数字 / 字母 / 罗马数字间切换 | |
| 水平线 | 渲染为 `---`，无颜色 | 删除线渲染关闭 |
| 表格 | 参考实现 `MarkdownTable` 框线（表头居中、每行间补 `├─┼─┤` 分隔） | 列宽受 `MIN_COLUMN_WIDTH=3`、`MAX_ROW_LINES=4` 约束，超限回退纵向键值渲染 |
| 加粗 | BOLD 修饰 | |
| 斜体 | ITALIC 修饰 | |

当 AI 回复内容被检测为 diff 格式时（前 5 行含 `@@` 或 `+++`），自动切换 diff 着色覆盖 markdown 渲染。

## 消息流

源码：`cc-tui/src/ui/message_render.rs`。

### 消息类型与视觉

| 类型 | 前缀 | 前缀色 | 说明 |
|------|------|--------|------|
| 用户消息 | `❯` | ACCENT + BOLD | USER_BG 底色，所有行带底色 |
| AI 回复 | `●` | TEXT (纯白) | 首行文本合并到 `●` 后，支持 markdown 渲染 |
| 思考 (Reasoning) | — | — | 不在消息流中渲染，完全隐藏 |
| 工具调用 | `⏺` | SAGE (绿) | 工具名 TEXT + BOLD，参数 DIM `(...)` |
| 工具聚合组 | `⏺` | SAGE (绿) 前缀 + MUTED 汇总文字 | 仅一行汇总文本，不可展开 |
| AskUserQuestion | `⏺` | SAGE (绿) | 标题 `User answered CC Code's questions:` + `⎿ · header → answer` |
| SubAgent | `●` | SAGE / ERROR | 折叠：名称 + 任务预览；展开：嵌套消息 + 执行结果 |
| 系统消息 | `·` | DIM | 自动检测错误/警告/信息颜色 |

### 间距规则

- 每条有内容的消息后加 **1 个空行**，由 `render_one` 统一管理
- 空内容消息（如纯思考被隐藏的 AssistantBubble）不渲染、不占位
- 消息内部不插入多余空行
- SubAgent 展开时用空行分隔嵌套消息与结果区

### 工具状态指示器

指示器 `⏺` 按状态变化，工具名称统一 TEXT + BOLD，参数用 DIM 色 `(...)` 显示：

| 状态 | 指示器 | 指示器颜色 | 工具名颜色 |
|------|--------|-----------|-----------|
| Running | `⏺` 闪烁（200ms 周期，⏺/空格交替） | SAGE | TEXT + BOLD |
| Completed | `⏺` | SAGE | TEXT + BOLD |
| Failed | `✗` | ERROR | TEXT + BOLD |

工具结果行格式：`  ⎿  ` 前缀（5 列：前 2 空格 + `⎿` + 后 2 空格；INACTIVE 色，失败时 ERROR 色）+ 内容（TEXT_SOFT 色，失败时 ERROR 色）。多行输出的续行使用 5 空格悬挂缩进（`BranchPrefix::HANG`），正文严格左对齐。错误折叠时显示 error_summary（最多 400 Unicode 字符，ERROR 色 `⎿` 前缀）。

### URL 超链接（工具结果 / `!` 命令输出）

纯文本里裸写的 `http://` / `https://` 会变成**真超链接**：消息区登记 URL 命中区及稳定锚点，所有组件和选区绘制后先把链接色写入 frame buffer 样式，再由 `WidthSafeBackend` 在输出时包上 OSC 8 转义序列（`cc-tui/src/ui/osc8.rs` + `cc-tui/src/terminal_backend/hyperlink.rs`）。URL、锚点或启用状态变化时，即使文字相同也只补画受影响的字符；附件等 `recolor=false` 的链接保留自身颜色。**转义序列绝不写进 frame buffer**——`ratatui` 的 `Buffer::diff` 用 `symbol.width()` 决定跳过后续多少个 cell，序列载荷是可见字符，一旦进 buffer，链接后面的整段文字会被跳过不画。

| 项 | 规则 |
|----|------|
| 前景色 | `Color::LightBlue`（crossterm `Blue` → `38;5;12`，亮蓝）；实测该终端下 `34`/`38;5;4` 比参考实现的链接更深，12 号槽才视觉一致。`!` 输出里的非 URL 段仍沿用命令自带 ANSI 着色 |
| 修饰 | 无人工下划线；下划线 / 手型光标由终端在悬停时给出 |
| 可点击 | 悬停提示、Ctrl/Cmd+点击、右键「打开链接」由终端提供；cc-code 自己的单击命中区也登记了同一批 URL（`link_from_parts`） |
| 识别范围 | 只认 `http://` / `https://`，口径对齐参考实现 `OutputLine.tsx::URL_IN_JSON`（`/https?:\/\/[^\s"'<>\\]+/g`），并额外切掉句末 `)` `]` `}` `,` `;` `.` `:` |
| 覆盖范围 | 所有走 `⎿` 前缀的工具结果行（Bash / Read / Grep / Glob 等）+ 用户 `!命令` 输出 + 退化成纯 URL 的 Markdown 链接；**用户自己输入的气泡不参与** |
| 序列格式 | `ESC]8;id=稳定锚点;URL BEL` … `ESC]8;;BEL`；同一命中区共用 ID，独立命中区和后端互不混组，BEL 收尾与参考实现的 `OSC8_END` 一致 |
| 关闭开关 | `DISABLE_HYPERLINKS=1` 时不注入序列（只影响 OSC 8，不影响点击命中区） |

> 注意与 Markdown 链接区分：AI 回复正文里的 `[text](url)` 走 `cc-widgets/src/markdown/render_state.rs` 的 `theme.link()`（= SAGE `#4EBA65` + 下划线，见「Markdown 渲染」），**不受本规则影响**。本条只作用于「裸 URL 文本」。

### AskUserQuestion 渲染

专用渲染路径，独立于普通 ToolBlock：

```
⏺ User answered CC Code's questions:
  ⎿  · header → answer
  ⎿  · header2 → answer2
```

- 标题行：`⏺` (SAGE) + 标题文字 (TEXT)
- 结果行：`  ⎿  ` (INACTIVE) + `· ` (INACTIVE) + `header → answer` (MUTED)
- 解析工具输出 `[问: H]\n回答: V` 格式，重新格式化为 `H → V`
- 错误态：指示器、标题与前缀均使用 ERROR 色

### SubAgent 启动摘要（普通 / 详细双态）

源码：`cc-tui/src/app/agent_ops/subagent.rs`（生成）+ `cc-tui/src/ui/message_render.rs`（渲染）。
同一 `SystemNote` VM 的两份文本（`content` / `detail_content`），由 `detail_mode` 单选渲染：

**普通模式**：极简树形，标题 `●`（`theme::SYMBOL_DOT`）用 SAGE、任务名 `TEXT_SOFT`、分支 `├──`/`└──` 用 DIM，缩进 2 列使**字形**与标题文字同列；该形状由 `split_tree_row()` 识别，**不得**注入 `· ` 前缀。

```
● 2 background agents launched (↓ to manage)
  ├── 广州天气查询(先休眠3分钟)
  └── 厦门天气查询(先休眠2分钟)
```

> **圆点统一契约**：全界面行首状态圆点一律引用 `crate::ui::theme::SYMBOL_DOT`（`●` `U+25CF`，以 `● Bash(...)` 为唯一样式基准），**严禁**使用小圆点 `•`（`U+2022`）等异形符号造成字形一大一小。

**详细模式（`Ctrl+O`）**：每个任务一张卡片，标题 `●` 用 **SAGE + 大实心圆点 `U+25CF`**（与 `● Bash(...)` 同字形，**禁止**用小圆点 `•` `U+2022`），`Agent` 用 TEXT + BOLD、`(任务预览)` 用 TEXT_SOFT；层级前缀 `  └ ` 与 `Backgrounded agent` 用 DIM/MUTED，`Prompt:` 用 SAGE + BOLD；指令正文 6 列悬挂缩进、按显示宽度折行。卡片之间空一行。

```
● Agent(广州天气查询(先休眠3分钟))
  └ Backgrounded agent
  └ Prompt:
      请先在终端执行 sleep 180 (休眠 3 分钟)。
      随后查询广州天气并整理成中文报告。

● Agent(厦门天气查询(先休眠2分钟))
  └ Backgrounded agent
  └ Prompt:
      请先在终端执行 sleep 120 (休眠 2 分钟)。
      随后查询厦门天气并整理成中文报告。
```

实现约束：逐行解析必须用 `str::strip_prefix`，禁止 `&line[4..]` 等字节下标切片（`└` 占 3 字节，会 `not a char boundary` panic）；`SystemNote` 的 `PartialEq`/`Hash` 必须包含 `detail_content`，否则切换详细模式不触发重渲染。完整契约见 `docs/prd/subagent-drawer-and-virtual-routing.md` §4.17。

## 只读工具聚合折叠

read_file、search_files_rg、glob_files 等只读工具自动聚合：

- **相邻的同类型工具**合并为一组（无其他消息穿插时）
- 仅显示一行汇总：`⏺ Read 3 files`（⏺ SAGE + 文字 MUTED），不可展开
- 出错工具即使在折叠态也显示 error_summary（ERROR 色）

摘要格式：

| 工具 | 单数 | 复数 |
|------|------|------|
| read_file | Read 1 file | Read N files |
| search_files_rg | Searched for 1 pattern | Searched for N patterns |
| glob_files | Matched 1 pattern | Matched N patterns |

## SubAgent 渲染

- 折叠：`● agent_id`（SAGE + BOLD）+ `task_preview…`（MUTED，截断 50 字符）
- 展开：名称 + 任务描述 + 缩进 2 空格的嵌套消息 + 空行 + 结果行 ` │ `（MUTED，最多 20 行，超长截断 80 字符 + `…`）
- 错误态：前缀和名称使用 ERROR 色，显示 error_summary

## Welcome Card

源码：`cc-tui/src/ui/welcome.rs`。欢迎卡从消息区左上角开始，宽度随终端适配、上限 64 列；自然高度 5～7 行，与终端高度无关。常规单行输入加空闲状态栏后，启动区域约 10～12 行。

| 区域 | 样式 |
|------|------|
| 边框 | DIM 圆角，左右各留 1 列内边距 |
| 标题与版本 | ACCENT + BOLD `CC Code  v<实际包版本>` |
| 模型与档位 | MUTED `model · (effort)`，无模型名或未配 effort 时整行省略 |
| 当前目录 | MUTED，超宽按显示列宽以 `…` 收口 |
| 使用提示 | TEXT，中英文提示开始对话或 `/help` 查看命令 |
| 更新提示（有新版本时） | 两行**替换**使用提示：WARNING `Update available! <当前> → <最新>` + MUTED `To update, run: cc-code update` |
| 首次引导（无 Provider 时） | WARNING，简短 `/login` 配置提示 |
| 吉祥物（宽度与高度足够时） | 12 列 × 5 行半块像素，位于文字左侧；左半 CYAN / 右半 ACCENT |

帮助内容统一从 `/help` 获取；欢迎卡仅保留上述信息，与首条用户消息间隔一行。`welcome_height()` 与渲染共用文本生成逻辑，中文与 emoji 均按显示列宽省略，目录中的换行及控制字符先清洗为单行内容。

更新提示是**替换**而非追加：卡片内高由吉祥物的 5 行决定，开始提示占 1 行、更新提示占 2 行，有更新时正好补满剩余空位，因此有吉祥物时卡片高度恒为 7 行，不受是否提示更新影响。

更新检查（`cc-tui/src/app/update_check.rs`）在启动后异步查一次 GitHub release（只认 `npm-v*` tag，跳过 prerelease），整个请求压在 3s 超时内；失败时保持正常的欢迎页，不显示任何更新提示。每个进程只查一次，`CC_CODE_DISABLE_UPDATE_CHECK=1` 可关闭。

### 吉祥物

源码：`welcome.rs` 的 `MASCOT`（12×10 像素稿）+ `mascot_lines()`（压成 5 行）。

- **构成**：左右对分的方块小人，左半 CYAN、右半 ACCENT，头顶两根同色天线，两条腿。
  配色直接取 `theme::cur().cyan` / `theme::cur().claude`，不新增色值，随 `/theme` 切换。
- **半块字符**：像素行两两压成一格（`▀` + 前景/背景；只有下半时用 `▄`），
  因此 12 列即可表达 12×10 的图形。只输出 `" "`、`U+2580`、`U+2584`，
  沿用已有像素稿，不增加新的图形资源。
- **眼睛是挖空**：两个像素都不着色，露出终端底色，因此 light / dark / daltonized /
  `*-ansi` 各主题下都有对比度，无需第三种颜色。
- **纯装饰，可省略**：卡片宽度至少 56 列，且内部高度至少 5 行时显示。
  窄屏或矮区域整块跳过，优先保留标题、目录和使用提示。

## 终端视口（v0.6.171，#650）

交互模式从当前 shell 输出下方启动，先在正常屏中绘制紧凑欢迎卡、输入框及状态栏。输入、消息、工具结果与面板增加时，输入区随内容向下移动；下方空间不足时原生滚动必要行数并上移视口起点，内容或已占用高度达到整个宿主高度才切换一次 alternate screen，随后输入区固定在底部，消息区沿用原有内部滚动。

- 增长高度取当前所需视觉行数与已占用高度的较大值；删除草稿、折叠结果、任务结束、`/clear` 及切换会话都不主动缩回。
- 首条消息及全屏切换后，启动欢迎卡作为纯 UI 前缀保留，并随消息一起滚动；不写入消息历史、系统提示词或消息渲染缓存。空会话仍使用紧凑欢迎内容，恢复已有历史时不补插启动卡片。
- 高度使用当前宽度的消息渲染缓存，加上 UI 欢迎前缀、实际可见的思考、总结、Todo、附件、面板、队列、输入和状态栏；输入区上限、面板高度以宿主终端尺寸计算。宽度变化及全屏切换前先完成重排；切屏、resize 与首帧同步输出，合并重复清屏，避免先展示空屏再等待排版。
- 宽度重排在后台准备，未就绪时保留旧屏、继续处理输入；稳定布局滚动复用确认。交互、后台更新和动画共用 33ms 绘制期限，停下输入后的最后一次更新仍须呈现；拖选释放前先呈现最终选区再复制。
- 启动及后续每次增长都只滚动所缺的行数；底部启动不因起点以下余量少而提前全屏。终端连最小布局也放不下时直接全屏。全屏锁定属于当前终端运行实例，放大窗口或新建会话均不解除。
- 首次配置向导和 Agent Center 等全页界面锁定全屏；普通面板按实际所需高度扩展。外部编辑器返回后恢复挂起前的模式。
- 增长阶段只清理应用拥有的区域，鼠标、选区与链接沿用真实终端坐标；退出时逐行清除欢迎、消息、输入和状态栏，光标回到正常屏应用起点，仅留下 Session ID、Resume 和 shell 提示符。全屏退出先恢复正常屏，再清除进入全屏前的应用区域，不擦除其上方 shell 历史。

需求与验收场景见 [PRD](docs/prd/growing-terminal-viewport.md)。已随 v0.6.171 发布（PR #652），三平台 CI 全绿；内存帧与状态测试不能替代 Windows Terminal / PowerShell 及 Unix/macOS 的真实终端验收。

## Spinner

源码：`cc-widgets/src/spinner/`。位于消息区域底部：

| 模式 | 显示格式 | 颜色 |
|------|---------|------|
| Loading | `✻ verb (Xm Xs · ↓ X.Xk tokens · <状态词>)` | 动词走底色/流光色对（默认 `claude` + `claudeShimmer`；compact 时底色 THINKING） |
| 完成 | `✻ Brewed for Xm Xs` | MUTED |

配色机制（2026-10-15 起对齐参考实现 2.1.300 实证，见 issue #510；原「时间热度四档」已废弃）：

- **流光**：底色 + 流光色成对，流光为 ±1 列窄带、50ms 步进、周期 = 文本宽 + 20、自左向右扫过（停泊区在文本外）；字形与动词同色对。
- **停滞转红**：无新 token 超过 10s 后流光熄火，字形与动词整体向 ERROR 色平滑插值（再 10s 渐满、每 50ms 向目标靠拢 10%）；新 token 到达立即清零，工具执行期间不触发。
- **状态词配色**：走 thinkingIntensity——思考段 10s 内恒 MUTED，超过后自 MUTED 向 WARNING 线性插值（20s 满）；`thought for Ns` 恒 MUTED；`elapsed` / `tokens` 恒 MUTED。

第三字段（v0.6.82+，思考状态词，英文固定输出）：

| 状态词 | 触发 | 颜色 |
|--------|------|------|
| `thinking` | 进入思考段 | thinkingIntensity 插值 |
| `thought for Ns` | 思考段结束 | 始终 MUTED |
| `still thinking` | 同一段思考超阈（10s 可调，优先级最高） | thinkingIntensity 插值 |
| `thinking more` | 本轮被打断后再次思考 | thinkingIntensity 插值 |

动画帧：16 个 Unicode 符号循环（`✳✴✵✶✷✸✹✺✻✼❃❊…`），每 200ms 推进一帧。
Verb 列表：128 个中文烹饪/动作动词随机选择。
Spinner 下方附加 Tip 行：`  ⎿  Tip: ...`（5 列前缀，与工具结果行共享 `BranchPrefix` 规范；INACTIVE 色）。调度对齐参考实现（issue #511）：每轮挑选一条并扣留、轮结束才发布展示，挑选按会话冷却（内置 10 个会话）取「最久未展示」者，历史持久化于 `~/.cc-code/spinner-tips.json`；本轮运行 >30min 时被 /clear 覆盖 tip 顶替；可用配置 `spinner_tips_enabled` 关闭、`spinner_tips_override` 追加自定义条目。

## Todo 列表

在 loading 状态时跟在 Spinner + Tip 之后显示：

| 状态 | 图标 | 图标样式 | 文字样式 |
|------|------|---------|---------|
| InProgress | `◼` | ACCENT + BOLD | TEXT |
| Completed | `✔` | SAGE | MUTED + CROSSED_OUT |
| Pending | `◻` | MUTED | MUTED + `(可开始)` 提示 |

## 布局

### 垂直布局（单 Session）

```
┌─────────────────────────────────────┐
│ Messages Area (Min(1))              │  ← 消息列表 + 滚动条 + Spinner
│ Attachment Bar (0/3 行)             │  ← 有附件时 3 行
│ Panel / Popup (0~60% 屏幕高度)      │  ← 面板/弹窗区
│ Queued Messages (0~3 行)            │  ← loading 时待发送消息预览
│ Input Area (3~40% 屏幕高度)         │  ← 输入框 + `❯` 提示符
│ Status Bar (3 行固定)               │  ← 状态信息
└─────────────────────────────────────┘
```

> **Sticky Header 已禁用（v0.6.71）**：`cc-tui/src/ui/main_ui/mod.rs` 中 `let sticky_header_height: u16 = 0;`（注释「鸡肋功能，高度固定为 0 不渲染」）。原顶部固定消息条（显示最后一条用户消息摘要、动态高度）当前不渲染，关联 headless 测试已标 `#[ignore]`。保留 `USER_BG` 与 sticky header 一致的配色约定即可。

### 面板高度

| 面板类型 | 最大高度 |
|---------|---------|
| 标准面板 | 屏幕高度 60% |
| 插件面板 | 屏幕高度 70% |
| HITL 审批 | 按命令/参数实际换行数自适应，最高为屏幕高度 60%；一组选择与快捷键固定可见 |
| AskUser | 自适应（考虑文字换行） |
| OAuth | 9 行固定 |

## 状态栏

源码：`cc-tui/src/ui/main_ui/status_bar.rs`。高度动态 **2~3 行**：第二行（activity）只在有内容时展开（`has_hud_activity()`：最近工具 / 工具统计 / 后台 Agent / Todo 任一非空），否则折叠为 2 行。

### 第一行（左→右）

| 元素 | 样式 | 条件 |
|------|------|------|
| `[model]` | ANSI Cyan，切换后 3 秒 BOLD + SLOW_BLINK | 始终 |
| context 进度条 `████░░░░░░ 62%` | Green(<70%) / Yellow(70~85%) / Red(>=85%) | 始终 |
| ` cwd` | ANSI Yellow | 始终 |
| `git:(branch*)` | 前缀 Magenta + 分支 Cyan + dirty `*` | git 仓库且分支缓存命中 |
| `tok: 1.4M (in: 96.3k, out: 588)` | DIM | 有 token 统计时 |
| `⏱ 6m53s` | DIM | 有会话开始时间时 |

### 第二行（activity 行，单行不折行）

左→右四段：**最近工具** │ **工具调用统计** │ **🤖 后台 Agent** │ **📋 Todo 进度**

| 段 | 形态 | 规则 |
|----|------|------|
| 最近工具 | `◐ Bash : sleep 15` / `◐ Read : src/main.rs` | 由 `AgentComm.recent_tools` 驱动：**最新在最左**、最多 2 条（第 3 条挤掉最老）。全程 `◐`（ANSI Yellow），**不区分运行 / 完成**——快工具若在结束时翻成 `✓`，执行态一帧都画不出来，同一位置会出现两套形态；完成与否由条目到期消失体现。工具名 ANSI Cyan、摘要 DIM。条目显示时长 = `max(实际执行时长, TOOL_MIN_VISIBLE_MS)`：快工具垫满该窗口后消失，慢工具执行结束即刻消失 |
| 工具统计 | `✓ Bash ×22` | 会话累计、按次数倒序，最多 4 条，超出显示 `+N more`。工具结束只让它 +1，不产生带摘要的完成条目 |
| 后台 Agent | `🤖 name(run) ...` | 存在 `background_agents` 时 |
| Todo 进度 | `📋 [##---] 2/5` | 存在 todo 时 |

- 摘要字符上限 **30**（`TOOL_TARGET_MAX_LEN`），沿用路径语义截断（`truncate_tool_target`，如 `src/ui/main_ui/status_bar.rs` → `.../status_bar.rs`）。
- 摘要覆盖全部工具：白名单（Bash / Read / Write / Edit / Glob / Grep / WebSearch / WebFetch / ExecuteExtraTool / SearchExtraTools / LSP）+ 通用兜底（AskUserQuestion → 首个问题、Agent → description、TodoWrite → `N 项任务`、其余 → 第一个非空字符串字段）。
- 整行超宽时按显示列宽（unicode-width）在行尾 `…` 收口，**禁止折行**。

### 最后一行（HUD 展开时为第三行，折叠时为第二行）

左→右：权限模式标签（`⏵⏵` 前缀符号 + Auto / Bypass，切换后 1500ms BOLD + SLOW_BLINK，后接 `(Shift+Tab to cycle)`）→ 后台任务 pill（`1 shell, 1 agent` + `↓ to view`）→ 重试状态 → MCP 初始化进度 → LSP 诊断计数 → `Cache 87% · MEM 74MB`；最右侧为上下文相关的快捷键提示，右对齐、超宽时截断右侧。

右侧快捷键上下文：

| 上下文 | 快捷键 |
|--------|--------|
| 面板打开 | 面板自提供 `status_bar_hints()` |
| OAuth 弹窗 | `Ctrl+O` 打开浏览器 + `Enter` 提交 + `Esc` 取消 |
| Approval 弹窗 | 状态栏不显示（审批面板底部已展示完整快捷键） |
| Questions 弹窗 | `Tab` 切换 + `↑↓` 移动 + `Space` 选择 + `Enter` 确认 |
| Rewind 回滚选择器 | `↑↓` 移动 + `Tab` 切换回退文件 + `Enter` 确认 + `Esc` 取消 |
| 退出确认 | `Ctrl+C` 关闭 + 其他键取消 |
| 详细模式 | `Ctrl+O` 退出详细模式 |
| 默认 | `Ctrl+O` 详细模式 + `Ctrl+P` 设置（Provider & Model） |

按键 MUTED + BOLD，说明 MUTED。右侧右对齐，超宽时截断右侧。

## 面板系统

源码：`cc-tui/src/ui/main_ui/panels/`。基于 `PanelManager` + `PanelComponent` trait 组件化架构。

### 面板列表

| 命令 | 面板 | 作用域 | 边框 |
|------|------|--------|------|
| `/login` | LoginPanel | Session | Browse=BORDER, Edit/New/Delete=动态色 |
| `/model` | ModelPanel | Session | BORDER |
| `/agent-design` | AgentPanel | Session | BORDER |
| `/history` | InputHistoryPanel | Session | BORDER |
| `/resume` | ThreadBrowserPanel | Session | BORDER |
| `/cron` | CronPanel | Session | BORDER |
| `/config` | ConfigPanel | Session | Browse=BORDER, Edit=WARNING |
| `/memory` | MemoryPanel | Session | BORDER |
| `/hooks` | HooksPanel | Session | BORDER |
| `/mcp` | McpPanel | Session | BORDER |
| `/plugin` | PluginPanel | Global | BORDER |
| `/cost` | StatusPanel (Cost tab) | Session | BORDER |
| `/context` | StatusPanel (Context tab) | Session | BORDER |

### 选中行样式

所有面板/弹窗的选中行统一使用 `fg(THINKING) #A2A9E4` 标准紫文字，**不使用底色**。光标指示器使用 `❯` 符号（THINKING 色）。/model 面板例外：选中项（✔）标签使用 SAGE 绿色，优先级高于紫色。

### 边框颜色规则

| 类型 | 边框颜色 | 面板 |
|------|----------|------|
| 标准面板 | BORDER (`#505050`) | model, agent, cron, thread_browser, config, memory, hooks, mcp, status |
| 编辑态 | WARNING | login(Edit/New/Delete), config(Edit) |
| 引导流程 | ACCENT | setup_wizard 步骤 1-2（Done=SAGE） |
| 警告弹窗 | WARNING | HITL 审批 |
| 提问弹窗 | THINKING | AskUser |
| OAuth | BORDER | oauth |

### 面板标题

统一 `BorderedPanel` 组件，标题 THINKING + BOLD，格式 `" 标题 "` 左右留空。

### 快捷键提示位置

所有面板的快捷键提示统一放在列表/表单**底部**，状态栏第二行右侧通过 `status_bar_hints()` 自描述。

**例外**：Queued Messages 区不是面板、也不进 `status_bar_hints()`，它的 `Ctrl+Enter` / `Ctrl+X` 提示以 DIM 灰字行内尾部形式渲染在**队列最下一条可见消息行**右侧（`queue-keys-tip`，见 `cc-tui/src/ui/main_ui/queued_messages.rs`）。空间不足时整条省略，不挤压消息预览。

### /model 面板样式

```
───────────── Select model ──────────────────
  Switch between models. Applies to this session.

  ❯ 1. Opus  ✔   claude-opus-4-7
    2. Sonnet      claude-sonnet-4-6
    3. Haiku       claude-haiku-4-5
    4. Fable       <gateway-mapped tier>

    ● High effort ← → to adjust

  Enter to confirm · Esc to exit
──────────────────────────────────────────────
```

| 元素 | 颜色 |
|------|------|
| 边框 | BORDER (`#505050`) |
| 标题 "Select model" | THINKING + BOLD |
| 描述文字 | MUTED |
| 光标箭头 `❯` | THINKING |
| 选中项标签（✔ 所在行） | SAGE + BOLD |
| 光标行标签（非选中项） | THINKING + BOLD |
| 普通行标签 | TEXT + BOLD |
| 对勾 `✔` | SAGE |
| 模型名称（右侧） | MUTED |
| Effort 圆点 `●` | ACCENT |
| Effort 文字 | MUTED + BOLD |

### /memory 面板

每行格式：`❯ [✓] label    ...path`

- 存在：`✓` SAGE；不存在：`✗` MUTED
- 不存在 + 光标行：额外提示 `按 Enter 创建并编辑`（MUTED）

### /mcp 面板

- 按 ConfigSource 分组（Project MCPs / User MCPs / Plugin MCPs）
- 分组标题 MUTED，服务器计数 MUTED
- 连接状态：Connected=SAGE, Failed=ERROR, Connecting=WARNING, Disabled=MUTED

### /hooks 面板

- 只读面板，标题 `Hooks` 或 `Hooks (none configured)`
- 事件列表：序号 + 事件名(THINKING+BOLD) + hook 数量

### /config 面板

- Browse：字段列表，光标行 THINKING + BOLD，值 TEXT
- Edit：编辑字段高亮，开/关用 THINKING / MUTED 对比
- 「详情渲染」使用左右键或 Space 在 `auto` / `cache` / `virtual` 间切换，即时生效并持久化；`auto` 自动适配长会话，`cache` 保留全量渲染，`virtual` 只保留视口附近成品与有预算的离屏缓存。此选项独立于 `Ctrl+O` 详情开关与「内联 Diff」。

### /plugin 面板

- Tab 栏：活动 tab THINKING 底色 + TEXT + BOLD，非活动 MUTED
- 最多 70% 屏幕高度（其他面板 60%）

## 弹窗

### HITL 审批弹窗

- 边框 WARNING 色
- 标题按语言显示工具审批 / Tool Approval（THINKING + BOLD）
- 同时显示三个选项：同意本次 / 本次会话同意 / 拒绝；`↑↓` 直接选择，默认同意本次
- 工具名称、动作说明、执行目录同行；长标题按显示列宽省略，目录不在正文重复显示
- 命令默认展开并按终端列宽换行，多行脚本保留换行与缩进；Edit 显示上下文及 `−` 删除 / `+` 新增
- 工具入参带 description 时在标题下方单独显示说明行；Write/Edit 改为文件级全文 diff 预览（新建文件走带行号与语法高亮的代码预览），超限、读不到文件或片段定位不到时回退片段 diff（v0.6.206）
- 仅当前工具显示一组审批选项；选中项用 `❯` 与 THINKING 色高亮，待提交工具不显示成功 `✓`
- 选项说明宽屏同行，窄屏显示当前选择的说明；命令会话提示为“本次会话内，相同命令不再询问”
- `Tab` / `Shift+Tab` 切换工具并保留各项选择；批量显示当前工具位置及本次/会话/拒绝三种计数
- 面板没有内部滚动区，`↑↓` 只用于审批选择；超高内容优先展示当前工具，并提示未显示行数与扩大终端
- 底部固定按键提示：`↑↓ 选择 · Tab 切换工具 · Enter 确认 · Esc 全部拒绝`（DIM）
- 本次会话同意只在当前会话记忆：文件按真实工具与路径，Bash 按完整命令、执行目录和分支；参数或执行位置变化需重新批准，明确禁止规则仍优先

### AskUser 批量问答弹窗

- 边框 THINKING 色
- Tab 栏：活动 tab THINKING 底色 + TEXT，非活动 MUTED
- 分隔线 `──────────────`（MUTED）
- 选项：`▶ ○` 单选 / `▶ ☐` 多选
- 自定义输入区

### OAuth 弹窗

- 边框 BORDER 色
- URL 区域 MUTED
- Ctrl+O / Enter / Esc 快捷键提示

### Setup Wizard

- 步骤 1-2：ACCENT 边框
- 完成：SAGE 边框
- 全屏覆盖，优先于所有正常界面

## Widget 库

源码：`cc-widgets/src/`。独立 crate，零内部依赖。

| Widget | 样式要点 |
|--------|---------|
| BorderedPanel | TOP+BOTTOM 边框，标题居中 |
| ScrollableArea | 右侧滚动条 MUTED 色 |
| SelectableList | `▶` 光标标记，2 空格缩进 |
| InputField | `Label  Value█` 模式，2 空格缩进 |
| CheckboxGroup | `✓` 选中 / `✗` 未选中 |
| RadioGroup | `●` 选中 / `○` 未选中 |
| TabBar | `│` 分隔符，indicator char |
| Spinner | 16 帧 Unicode 符号动画 |

## 快捷键全览

### 快捷键设计规则

- **禁止 Shift + 字母**：`Shift + 字母` 在编辑状态下等同于输入大写字母，二者不可区分
- 全局操作优先使用 `Ctrl + 字母` 或功能键（如 `Esc`、`PageUp`）
- 面板内操作优先使用 `↑/↓`、`Space`、`Enter`、`Esc` 等无冲突按键

### 按键语义统一规则

| 按键 | 语义 | 说明 |
|------|------|------|
| `Enter` | 确认 / 进入深度 / 保存 | 不用于 toggle 等切换操作 |
| `Space` | 选中切换 | toggle checkbox/radio/枚举值、列表项启用/禁用 |
| `Backspace` | 删除 | 仅用于文本删除（搜索框、编辑字段） |
| `Ctrl+D` | 删除项 | 删除面板中的条目（Provider/Cron/Plugin/MCP 等） |

### 全局快捷键

源码：`cc-tui/src/event/keyboard/shortcuts.rs`、`keyboard.rs`、`normal_keys.rs`。

| 按键 | 行为 | 说明 |
|------|------|------|
| `Ctrl+C` | 中断 Agent（loading 时）/ 双击退出（idle 时） | 提问和审批期间中断当前轮次，保留 TUI；空闲时两次有效按键间隔为 100ms～2s |
| `Esc` | 中断 Agent（loading 时）/ 关闭弹窗、浮层与历史浏览 | 与 `Ctrl+C` 的中断语义一致，但不会退出程序；**分层关闭**：候选浮层可见时先关浮层（不中断 Agent），再按一次才中断；关闭时记录 `hint_dismissed_for`，输入文本不变不再弹出，编辑后恢复 |
| `Enter` | 提交消息（idle）/ 缓冲消息（loading） | loading 时消息排队等待 |
| `Ctrl+Enter` | 立即补充队首排队消息 | 等价点击 `[Send now]`；同时兼容 `Alt+S`；队列为空时不拦截该按键。**前台命令宽限期**：steer 时若恰有一条前台命令在跑，先给 2 秒宽限（短命令自行结束即收口），到时仍在前台则自动移交后台（等价 `Ctrl+B`）并提示「前台命令已转入后台，补充信息将立即生效」；多条前台并行不自动选择，重复 steer 不重置计时器。不用 `Ctrl+S`：Windows 控制台把 Ctrl+S 当暂停键（XOFF）在宿主层截走。**仅 Windows 能可靠区分 `Ctrl+Enter`**：macOS（Terminal.app 不支持 kitty 键盘协议/modifyOtherKeys）与 Linux（终端默认未开启）下 `Ctrl+Enter` 与 `Enter` 同字节，队列提示行改显示 `⌥S` / `Alt+S`（`ui/main_ui/queued_messages.rs` 的 `keys_tip_key`） |
| `Ctrl+X` | 删除队首排队消息 | 等价点击 `[×]`；同时兼容 `Alt+X`；队列为空时不拦截该按键 |
| `Alt+Enter` | 插入换行 | |
| `Shift+Tab` | 循环切换权限模式 | Auto → Bypass → Auto（只剩两档） |
| `Ctrl+P` / `Alt+P` | 开关命令面板（Provider & Model 选择） | **模型切换统一走此入口** |
| `Ctrl+O` | 切换详细模式（detail mode） | OAuth 弹窗激活时不响应 |
| `Ctrl+B` | 后台化前台 shell / 聚焦底部后台任务入口 | |
| `Alt+V`（或 `Ctrl+V`） | 粘贴剪贴板图片附件 | 优先图片，回退文字；Alt+V 为现代终端穿透主键 |
| `↑` | 浮层导航 / 历史恢复 | 浮层激活时导航候选，否则恢复上一条输入 |
| `↓` | 浮层导航 / 历史恢复 | 浮层激活时导航候选，否则恢复下一条输入 |
| `Tab` | 命令/Skills 提示浮层导航 | 选中后 Enter 补全 |
| `PageUp/PageDown` | 消息区半页滚动（每次 20 行） | 仅当输入框为空时生效；输入框有内容时光标在输入框内移动 |
| `Del` | 删除最后一个待发送附件 | |
| `MouseScrollUp/Down` | 消息区滚动 | |

### 后台子任务抽屉

空输入框按 `↓` 进入抽屉，`↑/↓` 移动游标 `>`，`Enter` 切换视口 `•`。`Esc` 先离开抽屉；在子视口中再次按 `Esc` 返回主视口。主视口只显示分发摘要与完成通知，各子任务的思考、工具、Token、Spinner 和滚动位置分别保存。

子视口用亮青色输入框边界及任务标签标识，输入 `Message @agent` 后的内容在当前工具结束、下一次模型调用前消费。结束的任务只读。`x` 在抽屉中停止选中的子任务；`Ctrl+X Ctrl+K` 停止全部子任务。运行期间 `Ctrl+C` 中断当前视口，500ms 内再次按下会中断全部任务。存在子任务时 `Ctrl+X` 优先作为任务控制组合键前缀。

后台审批在任意视口显示，`y/n` 同意或拒绝，`Enter` 进入提出审批的子视口；普通提问继续使用问答面板。抽屉最多五行（溢出行计入上限），全部任务均可键盘选择；终端总高度低于十八行时仅占一行。停止任务显示三秒后归档，正在阅读的任务保留到离开视口。

**主 Agent 侧的取消**：用户可用自然语言让主 Agent 停止某个子任务，模型走 `Agent(action="stop", task_id="bg-…")`（`task_id` 取自 spawn 回执 `Background task bg-<uuid> started ...`）。**只接受精确 `task_id`**，不做任务名/描述模糊匹配——兄弟任务描述常高度相似（「查询广州天气」/「查询厦门天气」），模糊匹配会停错对象。停止与用户按键走同一条终态链路。

**停止与等待的视觉**：

| 位置 | 形态 |
|------|------|
| 时间线停止行 | `• Agent "x" was stopped by main Agent`（用户按键停止则为 `stopped by user`） |
| 抽屉停止行 | **红色实心 `●`**（`#FF6B80`）+ 红色 `stopped by user\|main Agent`；取代原橙色 `✗ stopped`——红实心比橙 `✗` 更醒目，一眼确认已停 |
| 等待计数行 | `✻ Waiting for N background agents to finish`（单复数随数量变化；活跃数归零即移除）。样式与 `✻ Cogitated for …` 同族：MUTED 前景 + `✻` 前缀，不加额外强调色 |
| 抽屉完成态 | `✓ completed`（SAGE 绿），与停止的红 `●` 形成对立语义 |

### 通用面板按键约定

| 按键 | 行为 |
|------|------|
| `↑/↓` | 竖向列表导航 |
| `←/→` | 横向切换（枚举字段） |
| `Enter` | 确认/进入/保存 |
| `Space` | 选中/切换 |
| `Esc` | 关闭/取消 |
| `Ctrl+V` | 粘贴剪贴板内容 |

### HITL 审批弹窗

| 按键 | 行为 |
|------|------|
| `↑/↓` | 选择同意本次 / 本次会话同意 / 拒绝 |
| `Tab` / `Shift+Tab` | 切换待审批工具，保留各项选择 |
| `Enter` | 按当前选择确认提交 |
| `Esc` | 全部拒绝并提交 |
| `Ctrl+C` | 中断当前 Agent 轮次，保留 TUI |

### AskUser 批量问答弹窗

| 按键 | 行为 |
|------|------|
| `Tab` / `Shift+Tab` | 切换问题 Tab |
| `↑/↓` | 当前问题内选项光标移动 |
| `Space` | 切换当前选项选中状态 |
| `Enter` | 提交所有答案 |
| 普通字符 | 自定义文本输入 |
| `Backspace` | 删除字符 |
| `Ctrl+C` | 中断当前 Agent 轮次，保留 TUI |

## 命令系统

源码：`cc-tui/src/command/mod.rs`。TUI 命令注册表共 **32 个命令**。

### 命令列表

#### 会话类

| 命令 | 说明 |
|------|------|
| `/clear` | 清空消息列表（别名 `reset`、`new`） |
| `/commit` | 一键 git commit，自动生成提交信息（别名 `ci`） |
| `/context` | 查看上下文使用率和会话统计 |
| `/cost` | 查看当前会话费用和 token 消耗 |
| `/exit` | 退出应用（别名 `quit`） |
| `/export` | 导出对话到文件或剪贴板（别名 `save`） |
| `/history` | 查看输入历史（`Ctrl+↑/↓` 回溯的列表版，同一份数据） |
| `/recap` | 生成 1-2 句会话回顾：高层目标 + 当前任务 → 下一步（别名 `away`、`catchup`） |
| `/rename [name]` | 查看或修改当前会话标题 |
| `/resume` | 打开历史对话浏览面板，选 thread 恢复 |
| `/review` | PR 代码审查（别名 `pr`） |
| `/rewind` | 打开对话回滚选择器，回退到某条历史消息（可选逆向恢复文件变更，别名 `undo`） |
| `/streaming [mode]` | 查看或切换时间线渲染模式（`streaming` / `block` / `none`），调试用 |
| `/gc` | 手动触发内存回收，显示 RSS 变化与数据结构诊断 |

#### 面板 / 交互类

| 命令 | 说明 |
|------|------|
| `/agents` | 打开跨项目全页会话中心（独立于面板系统） |
| `/agent-design` | 打开 Agent 定义选择面板 |
| `/help` | 列出所有可用命令 |
| `/model` | 打开模型选择面板（Provider + 级别 + Thinking）；带参数时直接切换别名 |
| `/plugin` | 管理插件（浏览、安装、卸载） |
| `/setup` | 打开配置向导，设置 Provider |
| `/theme` | 打开主题选择面板切换配色（`auto` 跟随终端，`default` 为现行配色，另有 `dark`/`light`/`light-daltonized`/`dark-daltonized`/`light-ansi`/`dark-ansi`）；带参数时直接切换 |

#### 配置类

| 命令 | 说明 |
|------|------|
| `/channel` | 管理 MCP 频道连接：`open <source>` / `close` / `status`（别名 `ch`） |
| `/config` | 全局配置（autocompact、语言、系统提示词覆盖）（别名 `settings`） |
| `/doctor` | 诊断配置完整性 |
| `/effort <level>` | 查看或设置推理力度（`low`/`medium`/`high`/`xhigh`/`max`） |
| `/init` | 生成或优化项目 CLAUDE.md 知识库 |
| `/lang` | 切换并保存界面语言（`en` / `zh-CN`）；未保存时启动检测 locale，CLI 帮助沿用该选择 |
| `/login` | 管理 Provider 配置（新建/编辑/删除） |
| `/memory` | 编辑用户/项目级 CLAUDE.md 记忆文件 |
| `/mcp` | 管理 MCP 服务器连接 |

#### 任务 / 工具类

| 命令 | 说明 |
|------|------|
| `/cron` | 查看和管理定时任务 |
| `/hooks` | 查看 Hook 配置 |
| `/loop` | 注册定时循环任务（自然语言描述，如 `/loop 每隔5分钟提醒我喝水`） |

### ACP 层命令

`/compact` 走 ACP 层而非 TUI registry。`cc-acp/src/session/command/` 注册了 7 个命令：`compact`、`clear`、`rewind`、`init`、`recap`、`commit`、`review`。其中 `/clear`、`/recap`、`/commit`、`/review`、`/init` 两侧均有，由 ACP 拦截处理。

`/rewind` 两侧分工：TUI registry 的 `rewind` 只负责打开回滚选择器（无参入口），用户在面板确认后 TUI 再向 ACP 提交带 JSON 参数的 `/rewind {target_message_id, revert_files}`，由 ACP 侧执行截断 history + 逆向恢复文件。

### `/recap` 特别说明

- 生成 1-2 句会话回顾（高层目标 + 当前任务 → 下一步）
- 单轮执行，**禁用工具**、**不写入 history**，支持 `Ctrl+C` 取消

### 命令匹配

匹配优先级：精确匹配(name) > 别名精确匹配 > 前缀唯一匹配（如 `/m` 匹配 `/model`）。

判定入口唯一：`CommandSystem::resolve`（`cc-tui/src/app/command_system.rs`）负责判定一段输入的归属，`App::route_slash_input`（空闲派发）与 `App::handle_busy_input`（运行中入队/拒绝）共用它；`CommandRegistry::find` 供判定使用，`dispatch` 复用同一份匹配规则执行，判定与执行不会各写一套而漂移。

### 运行中敲命令的语义

Agent 运行中（`loading`）按 `Enter` 时，输入按归属分流，**绝不静默变成发给模型的普通消息**：

| 输入 | 运行中行为 |
|------|-----------|
| 普通消息 / Skill | 入队，本轮结束后发送 |
| 本地「转发型」命令（`/commit`、`/review`、`/recap`、`/init`、`/streaming`） | 入队，本轮结束后执行 |
| ACP 侧命令（如 `/compact`） | 入队，本轮结束后由 ACP 执行 |
| 需要空闲 UI 的本地命令（`/theme`、`/model`、`/exit`、`/rewind`、`/gc`、`/help` 等） | 拒绝，提示原因并**保留输入框内容** |
| `!` shell 命令 | 拒绝，同上 |
| 未知命令 / 前缀歧义 | 拒绝并提示 |

命令能否延后由 `Command::blocked_while_busy` 声明（默认 `true` = 需要空闲 UI）。**新增命令时若其 `execute` 仅调用 `App::submit_message`，必须显式返回 `false`**，否则运行中会被误拒；反之把需要面板/选择器的命令标成 `false`，会导致它在运行中入队后于错误的时机弹窗。`cc-tui/src/command/mod_test.rs` 有白名单回归测试锁死这份分类。

已入队的命令在消费时重新走 `route_slash_input`（即按命令派发），因此 `Ctrl+Enter`「立即插入当前轮次」对命令不适用：`steer` 会把内容插入正在执行的轮次，命令经此路径会退化成普通文本，故被拒绝并提示等本轮结束。

### Skills 浮层

输入 `#` 前缀触发 Skills 浮层，`Tab` / `↑↓` 导航，`Enter` 补全为 `#skill-name`。
输入 `/` 前缀触发命令提示浮层，`Tab` / `↑↓` 导航。

**运行中（loading）交互**：候选存在时 `↑↓`/`Tab` 即可导航，不再被 `!loading` 守卫挡掉（对齐参考实现 `useTypeahead.tsx`）；`Enter` 仅在已选中候选时执行补全（只改输入框文本不提交），未选中则走排队语义（`handle_busy_input`）；`Esc` 先关浮层再中断（见全局快捷键 `Esc` 行）；`↓` 优先级为浮层导航 > 后台 shell 入口聚焦 > `@` 提及导航 > 滚动消息区。

### 文件提及浮层

输入 `@`（行首或空白后）立即显示候选；默认仅枚举当前目录，目录优先、按名称排序，排除构建/依赖目录，最多 15 项。继续输入关键词可搜索子目录及默认列表外的文件，200ms 防抖保留最后一次查询。

面板最多显示 10 行候选，按输入框上方可用高度缩小，不覆盖输入框；标题展示可见范围。`↑↓` 选择、`Tab` / `Enter` 补全、`Esc` 关闭。搜索中、空目录、无匹配都有独立提示；搜索时清空旧候选，防止误选。

## 权限模式

源码：`cc-middlewares/src/hitl/shared_mode.rs`。

通过 `Shift+Tab` 循环切换，状态栏首列实时显示。**只剩两档**：`AutoMode(0)` / `Bypass(1)`。

原 `Default` / `DontAsk` / `AcceptEdit` 已移除——实测用户不愿"天天确认"，
非 Auto 档要么每次弹窗、要么半自动，体验差且不智能。

**循环顺序**：Auto → Bypass → Auto。

状态栏标签前固定显示 `⏵⏵` 前缀符号（对齐参考实现 `PermissionMode.ts` 的 symbol 字段，如 `⏵⏵ auto`），符号本身不参与主题化取色。

| 模式 | 标签 | 颜色 | 说明 |
|------|------|------|------|
| AutoMode（默认） | Auto | WARNING | 语义门按调用逐个判定，**只有判不准时才弹窗** |
| Bypass | Bypass | ERROR | 所有都允许（危险） |

Auto 模式下，父 Agent 有“允许、拒绝、询问用户”三种结果。子 Agent 共享父级权限模式与判定规则，但只接受“允许、拒绝”：需要询问、无法判定或判定失败时拒绝执行，不弹审批。普通、后台和 fork 子 Agent 均遵循此规则；间接工具调用也检查真实目标与子 Agent 的工具限制。

`u8` → 模式的映射中，**未知取值一律回退 `AutoMode`**，绝不因陈旧/异常值意外滑进 Bypass。

模式切换后标签 1500ms BOLD + SLOW_BLINK 高亮。状态栏首列始终显示（两档都有标签）。

## Agent Command Center

`/agents` 打开独立全页视图；原 Agent 定义入口为 `/agent-design`，`/tasks` 不再注册，定时任务仍用 `/cron`。空草稿且焦点、根会话和无弹窗等门控满足时，裸 `←` 的 Press 打开中心（Repeat 不触发）；状态栏 `← N agents` 数当前项目根会话。

| 键 | 行为 |
|---|---|
| ↑/↓、Home/End | 移动选择并保持可见 |
| Tab / Shift+Tab | All / Needs you / Working / Ready / Inactive 过滤，零查询 |
| g | Project → Status → Model 分组，重开重置 |
| / | 已加载会话标题搜索，关闭清空 |
| Enter | 打开选中会话，或 Show more 加载下一页 10 条 |
| o | 当前项目历史选择器，与所选行无关 |
| n | 在选中项目新建空草稿，首次发送前不落库 |
| r | 重命名；Enter 提交，Esc 取消 |
| Backspace / Delete | 永久删除确认；y 确认不可恢复，Esc/n 取消 |
| ? / Esc | 开关帮助；Esc 优先关闭帮助、确认/重命名、搜索，然后返回会话 |

中心只有 tab 下方一条横线；宽度至少 90 列显示详情栏，窄屏单栏。计数涵盖全部项目，搜索和过滤作用于已加载页，组头数当前可见组行数。Working/Needs you 只反映本进程；其它会话按保存状态与 24h 阈值归 Ready/Inactive。切换不打断任务：存在运行任务、Shell、排队消息或审批时保持原会话。
