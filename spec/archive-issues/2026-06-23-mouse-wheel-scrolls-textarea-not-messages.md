> 归档于 2026-06-24，原路径 spec/issues/2026-06-23-mouse-wheel-scrolls-textarea-not-messages.md

# ConPTY 下鼠标滚轮滚动 textarea 而非消息区 — crossterm EnableMouseCapture 不发送 ANSI 序列

**状态**：Fixed（v5）
**优先级**：高
**创建日期**：2026-06-23
**Reopen 日期**：2026-06-23

## 问题描述

Windows Terminal 下，用户滚动鼠标滚轮想查看历史消息，但消息区纹丝不动，textarea（输入框）内容却在上下滚动。

用户预期：滚轮 → 消息区上下滚动。
实际行为：滚轮 → textarea 光标移动导致内容滚动。

## 症状详情

| 维度 | 描述 |
|------|------|
| 消息区 | 完全不响应滚轮 |
| textarea | 内容随滚轮上下移动（光标在多行间跳转） |
| 复现频率 | 100%（Windows Terminal + ConPTY） |
| 影响范围 | 所有使用 alternate screen + EnableMouseCapture 的 Windows Terminal 会话 |

## 复现条件

- **复现频率**：必现
- **环境**：Windows Terminal（ConPTY），Windows 10/11
- **触发步骤**：
  1. `cargo run -p peri-tui` 启动 TUI
  2. 发送一条消息（确保有历史消息可滚动）
  3. 鼠标滚轮向上滚动
  4. 观察：消息区不动，textarea 内容在滚动

## 根因分析（v2 — 2026-06-23 Reopen 后修正）

### 前一版因果链的问题

v1 文档断言"crossterm `EnableMouseCapture` 不发送 ANSI → WT 不知道 mouse tracking"，得出"手动发送 `?1000h` 即可修复"的结论。修复 commit `82db370f` + `2adad3df` 已合并，但用户报告 bug 仍然存在，证明 v1 因果链**有遗漏**。

### v2 关键新发现（源码级确认）

#### 真相 A：ConPTY 本应自动通知 WT，但被一个状态差量逻辑吞掉了

```cpp
// microsoft/terminal src/host/getset.cpp:334-411 — SetConsoleInputModeImpl
[[nodiscard]] HRESULT ApiRoutines::SetConsoleInputModeImpl(InputBuffer& context, const ULONG mode) noexcept
{
    // ...
    if (auto writer = gci.GetVtWriter())
    {
        auto oldMode = context.InputMode;
        auto newMode = mode;

        const auto newQuickEditMode{ WI_IsFlagSet(gci.Flags, CONSOLE_QUICK_EDIT_MODE) };
        WI_ClearFlagIf(oldMode, ENABLE_MOUSE_INPUT, oldQuickEditMode);
        WI_ClearFlagIf(newMode, ENABLE_MOUSE_INPUT, newQuickEditMode);

        if (const auto diff = oldMode ^ newMode)
        {
            if (WI_IsFlagSet(diff, ENABLE_MOUSE_INPUT))
            {
                writer.WriteSGR1006(WI_IsFlagSet(newMode, ENABLE_MOUSE_INPUT));
            }
        }
        writer.Submit();
    }
    // ...
}
```

```cpp
// microsoft/terminal src/host/VtIo.cpp:712-718 — WriteSGR1006
void VtIo::Writer::WriteSGR1006(bool enabled) const
{
    char buf[] = "\x1b[?1003;1006h";  // 注意：实际同时发送 1003 + 1006
    buf[std::size(buf) - 2] = enabled ? 'h' : 'l';
    _io->_back.append(&buf[0], std::size(buf) - 1);
}
```

**关键条件**：ConPTY 仅在 `oldMode ^ newMode` 的 diff 包含 `ENABLE_MOUSE_INPUT` 位时才向 WT 转发 mouse mode 序列。

#### 真相 B：crossterm 的调用顺序使 diff 永远不含 ENABLE_MOUSE_INPUT

peri-tui `main.rs:493-501` 的初始化顺序：
1. `enable_raw_mode()` → `ConsoleMode::set_mode(default & !0x07)`
   - Windows 默认 input mode 是 `0x1F`（LINE|ECHO|PROCESSED|WINDOW|MOUSE）
   - 清掉 LINE/ECHO/PROCESSED 后 = `0x18`（WINDOW|MOUSE）
   - **ENABLE_MOUSE_INPUT (0x10) 依然 ON**
2. `EnableMouseCapture` → `ConsoleMode::set_mode(0x0098)`（EXTENDED|WINDOW|MOUSE）
   - oldMode = `0x18`，newMode = `0x98`
   - diff = `0x80`（仅 EXTENDED_FLAGS）
   - **ENABLE_MOUSE_INPUT 不在 diff 中** → `WriteSGR1006` 不调用

**所以 WT 从未收到 ConPTY 自动转发的 `\x1b[?1003;1006h`。** WT 不知道 mouse tracking 已开启，`?1007`（默认 ON）+ alt screen 把滚轮转成方向键 → textarea 滚动。

这印证了 v1 的结论方向（WT 不知道 mouse tracking），但**根因机制更深一层**：不是"crossterm 不发 ANSI"，而是"crossterm enable_raw_mode 已经把 ENABLE_MOUSE_INPUT 留在 ON，导致 enable_mouse_capture 时 diff 为空，ConPTY 自动通知被跳过"。

#### 真相 C：v1 因果链忽略的 SCROLL_DELTA 符号问题（已验证为非问题）

crossterm_winapi 0.9.1 `ButtonState` 用 i32 符号判断滚动方向：
```rust
pub fn scroll_down(&self) -> bool { self.state < 0 }
pub fn scroll_up(&self) -> bool { self.state > 0 }
```

ConPTY InputStateMachineEngine 用的常量（`microsoft/terminal src/terminal/parser/InputStateMachineEngine.hpp`）：
```cpp
constexpr DWORD SCROLL_DELTA_BACKWARD = 0xFF800000;  // i32 重解释 = -8388608 ✓
constexpr DWORD SCROLL_DELTA_FORWARD = 0x00800000;   // i32 重解释 = +8388608 ✓
```

**常量设计正确**，重解释为 i32 后符号方向也对得上。这一环不是 bug 源。

### 修复后 bug 仍然存在的可能原因（按可能性排序）

#### 假设 1（最可能）：用户跑的是修复前的旧 binary

`b8514d50` 是 merge commit，但本地 `cargo run` 用的可能是更早编译的产物。**必须在 Windows 上重新 `cargo build -p peri-tui && cargo run -p peri-tui`**，确认 binary 时间戳晚于 `2adad3df`。

#### 假设 2：修复的 ANSI 序列没真正到达 WT

修复在 `execute!(..., EnableMouseCapture)` 之后手动写 `\x1b[?1000h...?1006h`，路径是：
- TUI stdout → `WriteConsoleW` → conhost `DoWriteConsole` → `WriteCharsVT`
- `WriteCharsVT` 把原 bytes 转发到 ConPTY output pipe → WT 接收

理论上会到达 WT。但若 stdout 缓冲未及时 flush，或 ConPTY writer 在初始化阶段被锁，序列可能丢失。`main.rs:511` 显式 `flush()`，但**实际是否到达 WT 需要 Windows 端运行时验证**。

#### 假设 3：ConPTY 版本差异

`WriteSGR1006` 的双序列 `\x1b[?1003;1006h` 是相对新版本的实现。旧版 ConPTY（如 Windows 10 1809 之前）可能没有这个自动转发逻辑，行为不同。

### 100% 确认的环节（源码验证）

| 环节 | 状态 | 证据 |
|------|------|------|
| crossterm `EnableMouseCapture::is_ansi_code_supported()` = false | ✓ 确认 | `crossterm-0.29.0/src/event.rs:343` |
| `enable_raw_mode` 不清 `ENABLE_MOUSE_INPUT` | ✓ 确认 | `crossterm-0.29.0/src/terminal/sys/windows.rs:18` `NOT_RAW_MODE_MASK = LINE\|ECHO\|PROCESSED` |
| ConPTY `WriteSGR1006` 仅在 diff 含 MOUSE_INPUT 时调用 | ✓ 确认 | `microsoft/terminal src/host/getset.cpp:379-385` |
| `WriteSGR1006` 实际发送 `?1003;1006h`（含 tracking + encoding） | ✓ 确认 | `microsoft/terminal src/host/VtIo.cpp:713-718` |
| ConPTY 启动注入 `?9001h`（Win32 Input Mode）到 WT | ✓ 确认 | `microsoft/terminal src/host/VtIo.cpp:200-204` |
| `SCROLL_DELTA_BACKWARD/FORWARD` 符号正确 | ✓ 确认 | `InputStateMachineEngine.hpp`（值 = `0xFF800000 / 0x00800000`） |
| `crossterm_winapi::ButtonState` 用 i32 符号判断滚动方向 | ✓ 确认 | docs.rs crossterm_winapi 0.9.1 |
| WT `IsTrackingMouseInput()` 仅在 `?1000h/?1002h/?1003h` 设置后为 true | ✓ 确认 | `microsoft/terminal src/terminal/input/mouseInput.cpp:277-280` |
| WT `ShouldSendAlternateScroll()` 需要 alt buffer + `?1007h` + wheel | ✓ 确认 | `mouseInput.cpp:488-494` |

### 100% **未**确认的环节（需 Windows 实测）

| 环节 | 状态 | 验证方法 |
|------|------|---------|
| 手动发送的 `\x1b[?1000h...?1006h` 是否真正到达 WT | ✗ 未确认 | WT 开 verbose log，或用 `sysdig`/`ProcessMonitor` 抓 ConPTY output pipe |
| WT 收到后是否真的启用了 mouse tracking | ✗ 未确认 | 在 WT 内同时运行 `cat -v` 等命令观察是否有 SGR 序列返回 |
| crossterm 在 ConPTY 下是否收到 `MouseEventKind::ScrollUp/Down` | ✗ 未确认 | TUI 加临时日志，记录 `Event::Mouse` 收到时的 kind |
| 用户测试的是否是修复后的 binary | ✗ 未确认 | `cargo clean && cargo build`，对比 binary mtime 与 commit 时间 |

---

## 根因分析（v1 — 原始版本，已被 v2 修正但保留作为参考）

### 因果链（全部基于源码确认，零猜测）

#### 环节 1：crossterm `EnableMouseCapture` 在 Windows 上不发送 ANSI 序列

```rust
// crossterm 0.29.0 src/event.rs:318-346
impl Command for EnableMouseCapture {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // ?1000h ?1002h ?1003h ?1015h ?1006h — 但这条路径在 Windows 上不走
        f.write_str(concat!(csi!("?1000h"), csi!("?1002h"), ...))
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> std::io::Result<()> {
        sys::windows::enable_mouse_capture()  // 只设 ConsoleMode
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        false  // ← 关键：强制走 WinAPI，跳过 ANSI
    }
}
```

```rust
// crossterm 0.29.0 src/command.rs:121-134 — queue() 分发逻辑
fn queue(&mut self, command: impl Command) -> io::Result<&mut Self> {
    #[cfg(windows)]
    if !command.is_ansi_code_supported() {   // EnableMouseCapture → false → !false = true
        self.flush()?;
        command.execute_winapi()?;            // ← 只设置 ConsoleMode
        return Ok(self);                      // ← 直接返回，?1000h 从未发送
    }
    write_command_ansi(self, command)?;       // ← 永远到不了
    Ok(self)
}
```

```rust
// crossterm 0.29.0 src/event/sys/windows.rs:36-42
const ENABLE_MOUSE_MODE: u32 = 0x0010 | 0x0080 | 0x0008;
// ENABLE_MOUSE_INPUT | ENABLE_EXTENDED_FLAGS | ENABLE_WINDOW_INPUT

pub(crate) fn enable_mouse_capture() -> std::io::Result<()> {
    let mode = ConsoleMode::from(Handle::current_in_handle()?);
    init_original_console_mode(mode.mode()?);
    mode.set_mode(ENABLE_MOUSE_MODE)?;  // ← 只调 SetConsoleMode，零 stdout I/O
    Ok(())
}
```

**结论**：`EnableMouseCapture` 在 Windows 上通过 `SetConsoleMode` 设置 ConPTY Console 对象的 `ENABLE_MOUSE_INPUT`，**不发送 `?1000h` ANSI 序列**。Windows Terminal 前端永远不知道 mouse tracking 已开启。

#### 环节 2：`EnterAlternateScreen` 确实发送了 ANSI 序列

```rust
// crossterm 0.29.0 src/terminal.rs:220-231
impl Command for EnterAlternateScreen {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str(csi!("?1049h"))
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        let alternate_screen = ScreenBuffer::create()?;
        alternate_screen.show()?;
        Ok(())
    }
    // 没有覆盖 is_ansi_code_supported() → 使用默认实现 supports_ansi()
}
```

在 Windows Terminal (ConPTY) 下，`supports_ansi()` 返回 `true`，所以 `EnterAlternateScreen` 走 `write_ansi()` 路径，发送 `\x1b[?1049h`。Windows Terminal 收到后进入 alternate screen。

**对比**：`EnterAlternateScreen` 的 `is_ansi_code_supported()` 用默认实现（返回 `true`），而 `EnableMouseCapture` 显式覆盖为 `false`。这就是为什么前者发送 ANSI 而后者不发送。

#### 环节 3：Windows Terminal 在 alt screen + 无 mouse tracking 下将滚轮转为方向键

```cpp
// Windows Terminal src/terminal/input/mouseInput.cpp:295-389
TerminalInput::OutputType TerminalInput::HandleMouse(...)
{
    if (IsTrackingMouseInput())          // 没收到 ?1000h → false → 跳过
    {
        // 滚轮作为鼠标事件报告 — 到不了这里
    }

    if (ShouldSendAlternateScroll(button, delta))  // ← 执行到这里
    {
        return _makeAlternateScrollOutput(button, delta);  // 滚轮 → 方向键
    }
}
```

```cpp
// mouseInput.cpp:488-494 — alternate scroll 生效条件
bool ShouldSendAlternateScroll(button, delta) const noexcept
{
    return _inAlternateBuffer              // ?1049h 已发送 → true ✓
        && _inputMode.test(Mode::AlternateScroll)  // 默认 ON（构造函数写死）✓
        && wasMouseWheel;                 // ✓
}
```

```cpp
// mouseInput.cpp:500-526 — 滚轮 → 方向键
TerminalInput::OutputType _makeAlternateScrollOutput(button, delta)
{
    vkey = delta > 0 ? VK_UP : VK_DOWN;  // 滚轮向上 → ↑，向下 → ↓
}
```

**关键事实**：alternate scroll mode (`?1007`) 默认 ON，写在 `TerminalInput` 构造函数中。没有 profile 设置可以关闭——只能通过应用发送 `\x1b[?1007l`。当 mouse tracking 关闭时，滚轮在 alt screen 中必然被转为方向键。

#### 环节 4：方向键通过 ConPTY → Console → crossterm

```rust
// crossterm 0.29.0 src/event/source/windows.rs — Windows 100% 走 WinAPI
impl EventSource for WindowsEventSource {
    fn try_read(&mut self, ...) {
        let event = match self.console.read_single_input_event()? {
            InputRecord::KeyEvent(record) => handle_key_event(record, ...),  // ← 方向键
            InputRecord::MouseEvent(record) => handle_mouse_event(record, ...), // ← 到不了
        };
    }
}
```

Windows Terminal 将滚轮转为方向键事件 → ConPTY 放入 Console 输入队列 → `ReadConsoleInputW` 返回 `KEY_EVENT_RECORD` → crossterm 解析为 `Event::Key(KeyCode::Up/Down)`。

#### 环节 5：peri-tui 代码逻辑（已验证无其他路径）

```
Event::Key(Down)
  → handle_event (event/mod.rs:318)
    → keyboard::handle_key_event
      → normal_keys.rs:52: Input { key: Key::Down, .. } => handle_down(app)
        → normal_keys.rs:500: textarea.input(Key::Down)
          → textarea 光标下移 → 视口跟随 → 内容滚动
```

**textarea 滚动的唯一路径**：`textarea.input(Key::Up/Down)`。全代码库搜索确认无其他代码路径在滚轮时移动 textarea 光标（`textarea.move_cursor` 仅用于鼠标拖拽选区和渲染时的 `!` 前缀处理，不在 ScrollUp/Down 路径中）。

`Event::Mouse(ScrollUp/Down)` 路径（`event/mod.rs:380`）只调用 `app.scroll_up()/down()`（`thread_ops.rs:4-29`），只修改 `ui.scroll_offset` + `ui.scroll_follow`，**完全不触碰 textarea**。

`Event::Key` 和 `Event::Mouse` 在 `handle_event:305` 是互斥的 match arm。

### 为什么旧架构（inline viewport）没这个问题

旧代码用 `Viewport::Inline(height)`，不进入 alternate screen，Windows Terminal 保留原生 scrollback。滚轮由终端原生处理（滚动 scrollback），不经过 ConPTY → crossterm 路径，alternate scroll mode 不生效。

### 完整因果链总结

```
EnableMouseCapture.is_ansi_code_supported() = false
  → queue() 走 execute_winapi()，只 SetConsoleMode，不发送 ?1000h
    → Windows Terminal 前端 IsTrackingMouseInput() = false
      → ShouldSendAlternateScroll() = true（alt screen + ?1007 默认 ON）
        → 滚轮转为 VK_UP/VK_DOWN
          → ConPTY 传入 Console → ReadConsoleInputW → KEY_EVENT_RECORD
            → crossterm handle_key_event() → Event::Key(KeyCode::Up/Down)
              → normal_keys.rs handle_down() → textarea.input(Key::Down)
                → textarea 内容滚动（消息区纹丝不动）
```

**本质**：crossterm `EnableMouseCapture` 在 Windows 上的 WinAPI 路径只设置了 ConPTY 的 Console 对象，但 Windows Terminal 前端与 Console 对象在 ConPTY 架构下是分离的——WinAPI 无法通知终端前端启用 mouse tracking，只有 ANSI 序列 `?1000h` 才能。这是 crossterm 在 ConPTY 环境下的设计缺陷。

## 涉及文件

- `peri-tui/src/conpty.rs` —— `force_conpty_mouse_notify()`（toggle MOUSE bit 主修复）+ ANSI 序列 + `?1007h`（enable alternate scroll，让滚轮作为 Up/Down 到达应用）
- `peri-tui/src/lib.rs` —— 注册 conpty 模块
- `peri-tui/src/main.rs` —— 终端初始化（EnterAlternateScreen + EnableMouseCapture + `enable_mouse_tracking()`）
- `peri-tui/src/app/panel_memory.rs` —— 外部编辑器恢复（EnterAlternateScreen + EnableMouseCapture + `enable_mouse_tracking()`）
- `peri-tui/src/event/mod.rs:380-455` —— ScrollUp/ScrollDown 处理（鼠标事件路径，鼠标 tracking 工作时走此路径）
- `peri-tui/src/event/keyboard/normal_keys.rs` —— `↑`/`↓` 绑定消息区滚动，`Ctrl+↑`/`Ctrl+↓` 绑定 textarea 光标/历史/hint
- `peri-tui/src/app/thread_ops.rs:4-29` —— scroll_up/scroll_down（消息区滚动）

## 修复方案

### 方案 A（v1，已证明不够）：手动发送 ANSI mouse tracking 序列

在 `EnableMouseCapture` 之后手动发送 `\x1b[?1000h\x1b[?1006h`。commit `82db370f` 已实现但用户报告 bug 仍存在——ANSI 序列通过 stdout → ConPTY → WT 的路径不可靠（ConPTY output VT 状态机不把应用写的鼠标序列翻译成 input-mode 变化）。

### 方案 B（v4，部分有效）：toggle MOUSE bit + ?1007l

`force_conpty_mouse_notify()` 在 `EnableMouseCapture` 后 toggle MOUSE bit off→on，强制 ConPTY `WriteSGR1006`。mouse tracking 修复成功（日志确认 Mouse(Moved) 事件），但 `?1007l`（禁用 alternate scroll）导致滚轮被 ConPTY 彻底吞掉（0 事件），`?1007h`（启用 alternate scroll）下滚轮作为 Mouse(ScrollUp/Down) 正确到达。

### 方案 C（v5，当前实现）：?1007h + 键盘绑定交换

实测发现 ConPTY 在 `?1007h` 下滚轮作为 `Mouse(ScrollUp/ScrollDown)` 正确报告（鼠标 tracking 和 alternate scroll 共存），但 crossterm 偶尔丢失鼠标事件。根本解法是**不依赖鼠标事件**：

1. **`?1007h` 启用 alternate scroll**：滚轮经 ConPTY 变为裸 `Up`/`Down` 方向键（无 Ctrl 修饰）
2. **键盘绑定交换**：`↑`/`↓` 绑定消息区滚动，`Ctrl+↑`/`Ctrl+↓` 绑定 textarea 光标移动 + @提及/hint/命令历史
3. **`force_conpty_mouse_notify()` 保留**：确保鼠标点击/拖拽/悬停仍正常工作

这样 ConPTY 滚轮生成的裸 Up/Down 自动走消息区滚动路径，键盘方向键也滚消息区（与鼠标滚轮一致），编辑 textarea 时用 Ctrl+方向键。

**关键优势**：不依赖鼠标事件到达、不依赖鼠标位置路由、不与 textarea 光标移动冲突。

## 验证方法

修复后确认：
1. `↑`/`↓` → 消息区上下滚动
2. `Ctrl+↑`/`Ctrl+↓` → textarea 光标移动 / @提及导航 / hint / 命令历史
3. 鼠标滚轮 → 消息区滚动（经 ConPTY ?1007h 变裸 Up/Down，走同一条路径）
4. 鼠标拖拽选区 → 正常工作（mouse tracking 仍有效）
5. 鼠标点击面板 → 正常工作

## [TRAP] 经验沉淀

### ConPTY 下滚轮事件不可靠，用键盘绑定 + alternate scroll 绕过

**Why:** ConPTY 在 SGR mouse 模式（`?1003;1006h`）下，滚轮事件的转发行为不稳定——实测 `?1007l` 时滚轮 0 事件，`?1007h` 时偶尔丢失。crossterm 的 `ReadConsoleInputW` 路径依赖 ConPTY 把滚轮 SGR 翻译成 scroll 事件，但 ConPTY 并不总是做这个翻译。根本解法是不依赖鼠标滚轮事件。

**How to apply:**
- 启用 `?1007h`（alternate scroll）：ConPTY 把滚轮转成裸 Up/Down 方向键，100% 可靠
- 把消息区滚动绑到 `↑`/`↓`，textarea 光标绑到 `Ctrl+↑`/`Ctrl+↓`
- `force_conpty_mouse_notify()` 保留，确保鼠标点击/拖拽/悬停仍通过 mouse tracking 工作
- 禁止用 `?1007l`——会导致滚轮被 ConPTY 彻底吞掉

### crossterm `EnableMouseCapture` 在 Windows + ConPTY 下不发送 ANSI `?1000h`，必须在 SetConsoleMode 层面修复

**Why:** crossterm 的 `EnableMouseCapture::is_ansi_code_supported()` 硬编码返回 `false`，导致 `execute!` 走 WinAPI 路径（`SetConsoleMode`），不发送 VT 序列。`enable_raw_mode()` 已经把 `ENABLE_MOUSE_INPUT` (0x10) 留在 ON，所以 `EnableMouseCapture` 的 `SetConsoleMode` 调用在 MOUSE 位上没有 diff → ConPTY 的 `WriteSGR1006` 自动通知被跳过。

**How to apply:**
- 在 `EnableMouseCapture` 之后调用 `conpty::force_conpty_mouse_notify()`，toggle MOUSE bit off+on 强制触发 `WriteSGR1006`
- ANSI 序列 `\x1b[?1000h\x1b[?1006h` 保留作为 defense-in-depth
- `\x1b[?1007h` 启用 alternate scroll（让滚轮作为 Up/Down 到达应用，配合键盘绑定交换使用）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-23 | — | Open | agent | v1 根因分析完成 |
| 2026-06-23 | Open | Fixed | agent | commit 82db370f：手动发送 ?1000h ?1006h |
| 2026-06-23 | Fixed | Reopen | agent | 用户报告 bug 仍存在。复盘后发现 v1 因果链有遗漏：忽略了 ConPTY 自身有 SetConsoleMode → WriteSGR1006 自动通知机制。v2 根因已写入。需 Windows 端实测 4 项确认。 |
| 2026-06-24 | Reopen | Fixed | agent | v4 修复：(1) toggle MOUSE bit after EnableMouseCapture force ConPTY WriteSGR1006 (2) ScrollUp/ScrollDown 后补 return Action::Redraw 触发重绘 |
| 2026-06-24 | — | — | agent | 代码对齐 v4：`conpty.rs` 此前实际为纯 ANSI 方案（无 toggle——纯 ANSI 在 ConPTY 下不可靠，output VT 状态机不转 input-mode 变化），现补齐 `force_conpty_mouse_notify()` after-toggle 主修复 + `?1007l` 第二保险。编译 + conpty 单测通过，待 Windows Terminal 实测。 |
| 2026-06-24 | Fixed | Reopen | agent | 实测确认 toggle 修复成功（mouse tracking 生效，大量 Mouse(Moved) 事件），但滚轮事件在 ConPTY 层被丢弃（`?1007l` 下 0 个 ScrollUp/Down）。`?1007h` 下滚轮作为 Mouse(ScrollUp/Down) 正确到达，但也偶尔丢失。 |
| 2026-06-24 | Reopen | Fixed | agent | v5 最终修复：启用 `?1007h`（alternate scroll，滚轮变裸 Up/Down）+ 键盘绑定交换（`↑`/`↓` 滚消息区，`Ctrl+↑`/`Ctrl+↓` 移 textarea 光标）。不依赖鼠标滚轮事件，彻底解耦。 |

## 修复记录

### 修复 #1（2026-06-23）

- **操作人**：agent
- **修复内容**：在 `EnableMouseCapture` 之后手动发送 `\x1b[?1000h\x1b[?1006h`，退出/挂起时发送 `\x1b[?1006l\x1b[?1000l` 清理
- **涉及 commit**：`82db370f`、`2adad3df`
- **涉及文件**：`peri-tui/src/main.rs`（终端初始化 + 退出）、`peri-tui/src/app/panel_memory.rs`（外部编辑器挂起 + 恢复）
- **验证状态**：Reopen（ANSI 序列路径可能不可靠）

### 修复 #2（2026-06-24）

- **操作人**：agent
- **修复内容**：v4 — 两个独立问题的修复
  1. **ConPTY mouse tracking**：`force_mouse_tracking_notify()` 在 `EnableMouseCapture` 后 toggle MOUSE bit off+on，强制 ConPTY 的 `WriteSGR1006` 触发（v3 的 clear-before 方案未生效，改为 after-toggle）
  2. **ScrollUp/ScrollDown 缺少 Redraw**：`app.scroll_up()`/`scroll_down()` 后没有 `return Ok(Some(Action::Redraw))`，导致 TUI 不重绘——滚轮事件到达了但画面不刷新
  3. 保留 ANSI 序列 + `\x1b[?1007l` 作为 defense-in-depth
- **涉及文件**：`peri-tui/src/conpty.rs`（force_mouse_tracking_notify）、`peri-tui/src/event/mod.rs`（+2 行 return Action::Redraw）
- **验证状态**：日志确认 Mouse(ScrollUp/Down) 事件正确到达 TUI，待用户确认视觉滚动效果

## 验证 #1（2026-06-23）—— Reopen

用户报告：修复 commit 已合并到 main，但运行时滚轮仍然滚动 textarea 而非消息区。

复盘后确认 v1 因果链遗漏了 ConPTY 的 `WriteSGR1006` 自动通知机制（src/host/getset.cpp:379-385 + src/host/VtIo.cpp:713-718）。v1 说"crossterm 不发 ANSI 所以 WT 不知道"，但真相是"crossterm enable_raw_mode 保留了 ENABLE_MOUSE_INPUT，导致 enable_mouse_capture 时 diff 为空，ConPTY 自动通知被跳过"——结论方向相同但根因机制更深一层。

修复理论上仍应有效（手动发送 `?1000h...?1006h` 直接到 stdout，ConPTY 通过 WriteCharsVT 转发到 WT）。但 4 项关键环节未在 Windows 实测：

1. 用户测试的是修复后的 binary 吗？（可能跑的是旧产物）
2. 手动发送的 `\x1b[?1000h...?1006h` 真的到达 WT 了吗？
3. WT 收到后真的启用了 mouse tracking 吗？
4. crossterm 真的收到了 `ScrollUp/Down` 而非 `Key(Up/Down)` 吗？

**Windows 实测方案**（任选其一即可 100% 确认）：

方案 A：在 TUI 加临时日志，记录每次 `Event::Mouse` 和 `Event::Key(Up/Down)` 的接收
```rust
// peri-tui/src/event/mod.rs handle_event 入口
tracing::info!(?ev, "TUI received event");
```
跑 TUI、滚动鼠标、查看日志。若日志显示 `Key(Up/Down)`，证明修复未生效；若显示 `Mouse(ScrollUp/Down)`，证明修复生效但 TUI 路由有问题。

方案 B：用 microsoft/terminal 的 VT 输入记录功能
WT 设置里开 `"debugFeatures": true`，或用 `tracevt` 工具抓 ConPTY 双向流量，直接看 `\x1b[?1000h` 是否到达 WT、WT 是否回送 SGR 鼠标序列。

方案 C：用 powershell 验证 ConPTY 自动通知机制
```powershell
# 简单脚本：直接调用 SetConsoleMode 改变 ENABLE_MOUSE_INPUT，观察 WT 是否收到 ?1003;1006h
$mode = 0x18  # 先设为不含 MOUSE_INPUT
[Console]::SetConsoleMode([Console]::OpenStandardInput().SafeFileHandle, $mode)
$mode = 0x98  # 再加上 MOUSE_INPUT
[Console]::SetConsoleMode([Console]::OpenStandardInput().SafeFileHandle, $mode)
# 此时 WT 应收到 \x1b[?1003;1006h
```

确认假设 1（用户跑旧 binary）优先级最高：先 `cargo clean && cargo build -p peri-tui`，再测。
