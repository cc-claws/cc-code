# [BUG] CMD 控制台字符列宽不一致导致 TUI 残影

- **状态**：修复验证中（已实现输出层兼容，验证结果见文末）
- **创建日期**：2026-09-20
- **优先级**：高
- **模块**：TUI / Windows 终端兼容
- **GitHub Issue**：未创建；本文为本地调查记录，后续准备 PR 时需创建并关联 Issue。

## 问题现象

用户在传统 CMD 窗口运行 CC Code，多轮对话后出现：

- 思考提示右侧及空白行留下孤立的 `)`。
- 右侧固定列出现多行孤立的 `e`。
- 中文回复、装饰符和标点附近存在错位；正常消息之外的字符没有随刷新消失。

截图包含 `1+1`、`2+2`、`tianwaidihu` 三轮输入，思考提示形如 `∴ Thought for 57 chars (ctrl+o to expand)`。其中 `)` 残留已用真实控制台缓冲区直接复现；`e` 的原始来源未逐帧追踪，不能声称已单独复现整张截图。

## 调查环境与版本

| 项目 | 调查时取值 |
|------|------------|
| Windows | Windows 10，build 19045 |
| 异常控制台字体 | 新宋体，8 × 16 |
| 输入 / 输出代码页 | 936 / 936 |
| 活动输出模式 | 7，包含 `ENABLE_VIRTUAL_TERMINAL_PROCESSING` |
| 本机已安装 npm 包 | `@cc-claw/code` 0.6.56 |
| 工作区版本 | 0.6.57，提交 `aeb298a4` |
| 锁定依赖 | ratatui 0.30.0、ratatui-crossterm 0.1.0、unicode-width 0.2.2 |

已比较 `npm-v0.6.56..HEAD`：本次涉及的 `main.rs`、`conpty.rs`、`message_render.rs`、`message_area.rs` 和 `Cargo.lock` 无差异。此比较是源码比较，未对正在运行的二进制做构建来源校验。

## 根因与证据

### 1. 实际列宽与程序列宽不一致

项目及 Ratatui 使用 `UnicodeWidthStr::width()` 计算列宽。传统 Windows 控制台在当前字体下将部分 East Asian Ambiguous 字符显示为双列，而该函数将它们计作单列。

实测通过独立 Python 子进程附着正在运行的控制台，读取字体和代码页，再用 `CreateConsoleScreenBuffer` 创建**不激活、不显示**的独立缓冲区。以 `WriteConsoleW` 写入字符，用 `GetConsoleScreenBufferInfo` 读取实际光标推进量。未向用户活动缓冲区写入，也未修改用户控制台的字体或代码页。

| 字符 | unicode-width `width()` | 异常控制台实际推进 |
|------|-------------------------|--------------------|
| `A` | 1 | 1 |
| `中` | 2 | 2 |
| `●`、`∴` | 1 | 2 |
| `“`、`”`、`—` | 1 | 2 |
| `·`、`…` | 1 | 2 |
| `✻` | 1 | 1 |

同机另外两个运行中的控制台也使用 CP936，但上述歧义宽度字符实际推进为 1 列。因此，**不能仅凭 Windows 平台、CMD 命令名或 CP936 就全局启用 `width_cjk()`**。

### 2. 直接复现右括号残留

在独立缓冲区中执行以下过程，坐标从 0 开始：

```text
SetConsoleCursorPosition(buffer, (0, 2))
WriteConsoleW(buffer, "∴ Thought for 57 chars (ctrl+o to expand)")
GetConsoleScreenBufferInfo(buffer)
  程序预计列宽：41
  异常控制台实际列宽：42

SetConsoleCursorPosition(buffer, (0, 2))
WriteConsoleW(buffer, " " 重复 41 次)
ReadConsoleOutputCharacterW(buffer, 从 (0, 2) 读取)
  结果：x=41 仍有一个 ')'
```

在另两个单列推进的控制台中，相同过程没有剩余字符。这是对底层宽度和擦除机制的局部验证，未执行完整 TUI 的自动回归。

### 3. 增量绘制使错位字符长期保留

1. `message_render.rs` 输出 `∴ `、`● ` 等前缀，回复正文也可能含弯引号和破折号。
2. Ratatui 的 Buffer 按逻辑列宽记录内容、计算差分和需要覆盖的范围。
3. `ratatui-crossterm` 对相邻逻辑单元格省略 `MoveTo`，连续输出字符，假定实际光标推进与逻辑列宽一致。
4. 控制台实际多推进一列，后续文字向右偏移，尾字符落到逻辑内容之外。
5. 后续差分刷新仍认为那个位置是空白，省略写入，因此物理屏幕保留尾字符。

### 4. 已核对的其他候选原因

- 当前初始化已开启 alternate screen，并启用 VT；本次证据不支持“未开启 ANSI”作为原因。
- 当前为全屏模式，不再调用旧的 `insert_before` / `flush_scrollback_history` 链路；2026-06-15、2026-06-16 的 Inline 残影文档不适用于本次根因。
- `render_session_column` 和 `render_messages` 已调用 `Clear` widget。它清理的是逻辑绘图缓冲区，不能纠正真实终端列宽偏差。
- 在另建的隐藏控制台中保持新宋体 8 × 16，分别使用 CP936 和 CP65001，`●`、`∴`、`“`、`—`、`·` 实测均为 2 列。单独执行 `chcp 65001` 不能解决本机已复现的宽度问题。

## 代码定位

行号以 `aeb298a4` 为准，后续修改后应按符号定位。

| 文件 / 位置 | 作用 |
|-------------|------|
| `peri-tui/src/ui/message_render.rs:755` | 回复首行使用 `● ` |
| `peri-tui/src/ui/message_render.rs:778` | 思考提示使用 `∴ ` |
| `peri-tui/src/ui/render_thread.rs:145` | 使用 `UnicodeWidthStr::width()` 计算字符宽度 |
| `peri-tui/src/main.rs:526` | raw mode、alternate screen 和后端初始化 |
| `peri-tui/src/main.rs:1092` | `draw_app`，条件清屏后调用 `terminal.draw()` |
| `peri-tui/src/conpty.rs:164` | `enable_vt_processing` |
| `peri-tui/src/ui/main_ui/mod.rs` | `render_session_column` 清理逻辑帧 |
| `peri-tui/src/ui/main_ui/message_area.rs` | `render_messages` 清理并渲染消息区域 |
| 依赖 `ratatui-core-0.1.0/src/buffer/buffer.rs` | `Buffer::diff` 按 `symbol().width()` 计算跳过和覆盖范围 |
| 依赖 `ratatui-crossterm-0.1.0/src/lib.rs:237` | 相邻逻辑单元格省略光标定位，随后输出字符 |

## 原始修复设想（调查阶段）

### 推荐方向：统一终端实际列宽、布局和差分输出

1. **探测实际终端能力**：在 Windows 初始化阶段探测代表字符的实际推进宽度，优先复用此次“不激活的独立缓冲区”方法；探测失败时保留默认行为并记录诊断信息。不得修改用户活动屏幕、字体或代码页，也不得只按 CP936 判断。
2. **确定一致的列宽策略**：探测结果必须同时约束消息折行、截断、输入光标、鼠标选区，以及 Ratatui 的布局、Buffer 和 diff。仅修改 `render_thread.rs` 的宽度函数不够。
3. **先验证底层可行性**：当前锁定的 Ratatui 链路使用自身的 `width()`。实施前需验证受控依赖适配或其他兼容后端能否贯穿整个宽度计算链路，再确定最小补丁范围；不能假设增加一个应用层开关就能解决。
4. **保留正常终端行为**：单列终端继续使用原策略。字体或终端能力变化后，重新探测时应同步重建渲染缓存和物理画面，避免混用新旧宽度。
5. **覆盖普通回复正文**：不仅处理 `∴`、`●` 等装饰符，也必须处理正文中的弯引号、破折号等字符；显示适配不得篡改保存的消息或复制内容。

上述是调查阶段的设计方向。实施时确认，仅修改应用层列宽不能覆盖 Ratatui 内部计算；全局更改依赖列宽会扩大影响，因此本次采用文末的输出边界兼容方案。

### 缓解方式与不充分的修复

- 可尝试使用 Windows Terminal 承载 CMD，保持默认单列歧义字符行为；具体用户窗口仍需回归确认。
- 将装饰符替换为 ASCII 只能降低触发概率，无法覆盖任意回复正文。
- 每帧 `terminal.clear()` 可能清除已有残影，但不能修正定位和折行，还可能引入闪烁；不作为最终方案。
- 只在每个字符前增加 `MoveTo` 仍可能覆盖双列字符的后一半，也不能独立解决布局问题。
- 不应直接全局切换为 `width_cjk()`，否则会破坏已正常的单列终端。

## 验收计划

| 场景 | 预期 |
|------|------|
| 传统 conhost + 新宋体，CP936 / CP65001 | 实际输出和逻辑列宽一致，思考行移除后无 `)` 残留 |
| Windows Terminal 承载 CMD / PowerShell | 正常显示，不出现额外空格、截断或光标偏移 |
| `∴`、`●` 与正文中的弯引号、破折号混排 | 前缀、正文、行尾均正确，消息原文和复制内容不变 |
| 流式增长、长行缩短、连续多轮、消息滚动 | 旧内容完整擦除，无固定列残留 |
| 调整窗口大小、展开/折叠思考内容 | 折行、缓存和差分同步更新 |
| 中文输入、鼠标定位、文本选区 | 显示列和字符索引转换正确 |
| 非 Windows 终端 | 原有渲染行为不变 |

先增加可重现宽度差异的局部回归，再在真实 conhost 和 Windows Terminal 验证。仅 TestBackend 或逻辑 Buffer 测试通过不能证明物理终端问题已修复。

## 调查产物与边界

最初调查只完成源码审查、独立交叉审查和真实控制台独立缓冲区探针。后续用户授权修复后的实现和验证记录见下文；尚未创建 GitHub Issue、提交或推送。

调查时的临时探针产物位于 `C:/Users/adim/AppData/Local/Temp/`：

- `peri-width-probe.py` / `peri-width-probe-results.json`：三个控制台的字体、代码页、列宽和擦除残留对照。
- `peri-width-cp-launch.py` / `peri-width-cp-isolated.py` / `peri-width-cp-isolated-results.json`：隐藏控制台中的 CP936 / CP65001 对照。

临时文件可能被系统清理；核心过程和结果已写入本文。探针中的 PID 是当时的进程号，复用脚本前需重新定位目标进程，不能直接照搬。

微软记录了传统控制台歧义字符宽度依赖字体、与 Windows Terminal 行为不同的历史背景：[Console: Potential Breaking Changes](https://github.com/microsoft/terminal/wiki/Console:-Potential-Breaking-Changes)。本次结论以本机实测和锁定依赖源码为主。

## 修复实现（2026-09-20）

开发分支：`fix/cmd-render-ghosting`。

### 最终方案：在输出边界保证逻辑列宽

- 新增 `terminal_backend` 模块，Windows 下用 `TuiBackend` 包装原 `CrosstermBackend`；非 Windows 仍直接使用原后端。
- Windows 创建不激活的独立控制台缓冲区，测量完整 `cell.symbol()` 的实际光标推进。缓冲区继承字体，并预留足够容量，避免小窗口下测量时发生滚动。
- 按完整符号缓存结果，ASCII 直接透传，最多保留 4096 个缓存项。实测列宽与逻辑列宽一致时，字符保持原样。
- 只有宽度不一致的输出单元格被复制并替换：如 `∴` 显示为 `.`、`●` 为 `*`、弯引号为 ASCII 引号、破折号为 `-`；其他无法等宽显示的符号用 `?` 并按原逻辑列宽补空格。
- 适配覆盖整个后端输出，包含普通回复、用户输入、面板和状态行。原始消息、Ratatui Buffer、选区快照及复制文本均不改写，RGB 颜色和样式透传。
- 每次绘制前检查实际字体、字号、字重和输出代码页指纹。变化时重建探针、清除宽度缓存，并请求一次物理清屏重绘；正常连续刷新继续使用差分，不增加每帧清屏。

这个方案通过使“最终输出字符的实际列宽”匹配现有布局，保持布局、光标、选区和差分坐标一致；不改写整个依赖栈的宽度算法。

### 改动文件

| 文件 | 改动 |
|------|------|
| `peri-tui/src/terminal_backend/mod.rs` | 平台后端别名和构造入口 |
| `peri-tui/src/terminal_backend/compatible.rs` | 输出副本适配、宽度缓存、原后端方法透传 |
| `peri-tui/src/terminal_backend/windows.rs` | 独立缓冲区探针、字体指纹和句柄自动释放 |
| `peri-tui/src/terminal_backend/compatible_test.rs` | 模拟单列/双列终端、原文及样式保留、差分擦除与缓存回归 |
| `peri-tui/src/terminal_backend/windows_test.rs` | 显式隔离运行的真实 Windows 控制台回归 |
| `peri-tui/src/main.rs` | 接入后端，字体能力变化后重绘 |
| `peri-tui/src/lib.rs` | 导出后端模块 |
| `peri-tui/Cargo.toml` | 显式启用 `CreateConsoleScreenBuffer` 所需 `Win32_Security` feature |
| `scripts/test-cmd-rendering.ps1` | 在独立隐藏控制台运行物理回归并保存 JSON 证据 |

### 兼容边界

- 异常终端中的部分符号会显示为兼容字符，原文和复制内容仍保留 Unicode。这是明确的显示降级；不承诺旧 conhost 支持其字体无法正确按指定列宽渲染的所有字形。
- 探针不可用时保留原后端行为并记录初始化失败日志；没有自动改变用户字体、全局设置或代码页。
- 单个符号超过 256 个 UTF-16 单元或包含控制字符时，探针不测量；不将这类异常输入纳入本次修复覆盖承诺。
- Windows 对 `SetCurrentConsoleFontEx` 返回成功并不保证采用请求的字体。回归需记录实际字体和实际宽度，不能把“请求了 Consolas”直接当成已经覆盖单列终端。

### 可复现验证命令

```powershell
cargo check -p peri-tui --lib --bin cc-code --locked --offline
cargo test -p peri-tui --lib terminal_backend --locked --offline
# 使用上一条输出的最新 peri_tui 测试二进制；哈希随构建变化。
./scripts/test-cmd-rendering.ps1 -TestBinary ./target/debug/deps/peri_tui-<hash>.exe
```

真实控制台测试默认 `ignored`，必须通过隔离脚本显式运行。脚本创建隐藏的独立控制台，测试仅修改该专属控制台的字体和代码页，不连接用户活动窗口；JSON 证据默认保存到本机临时目录。
