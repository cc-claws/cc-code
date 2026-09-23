# Windows 下执行 PHP 命令触发 TUI 全屏黑白闪烁 (PHP Child Process Triggers Terminal Clear Redraw)

**状态**：Open
**优先级**：高
**创建日期**：2026-09-23
**GitHub Issue**：#229 (https://github.com/cc-claws/cc-code/issues/229)

## 问题描述

在 Windows 平台下，Agent 每执行一次 PHP 命令（如 `php think ...` 或 `php -r "sleep(1);"`），TUI 界面就会出现一次极其刺眼的整屏黑白闪烁（物理清屏）。执行相同耗时的 Python 命令则完全平稳正常，不会发生闪烁。

## 症状详情

1. **执行 PHP 100% 出现黑白闪烁**：无论 PHP 命令耗时长短（1 秒、2 秒还是更长），每次执行都会触发一次全屏黑白闪屏。
2. **执行 Python 完全平稳**：执行 `python -c "import time; time.sleep(1);"` 或 `time.sleep(3)` 全程画面无任何闪烁。

## 根因定位

1. **直接触发点**：`peri-tui/src/main.rs:1114`
   ```rust
   #[cfg(windows)]
   if terminal.backend_mut().refresh_widths() {
       app.session_mgr.current_mut().ui.request_terminal_clear_redraw();
   }
   if app.session_mgr.current_mut().ui.take_terminal_clear_redraw() {
       terminal.clear()?; // 向标准输出发射 \x1b[2J 强制物理清屏
   }
   ```
2. **深层原因（子进程与宿主控制台未隔离）**：
   - `peri-tui/src/shell_exec.rs` 及 `peri-middlewares/src/process/mod.rs` 中使用 `tokio::process::Command` 启动 Windows 子进程时，未显式指定 `cmd.creation_flags(0x08000000)`（`CREATE_NO_WINDOW`），子进程与父进程 TUI 共享同一个控制台会话（Console Session）。
   - Windows 平台下的 `php.exe`（及其依赖的 `php7.dll` / `php8.dll` 的 `win32/console.c` 与 `readline` 扩展）在启动和执行过程中，会显式调用 Win32 API `SetConsoleOutputCP` / `SetConsoleCP` 修改控制台代码页。
   - 父进程 TUI 主循环在每一帧 `draw_app` 时通过 `refresh_widths()` 检测到控制台代码页发生变化（`ConsoleFingerprint` 不一致），判定终端配置被篡改，触发自愈保护机制调用 `terminal.clear()` 物理清屏（`\x1b[2J`），导致整屏瞬间黑白闪烁。
   - 而 Python 在 Windows 下的运行时库不包含此类侵入式控制台代码页修改逻辑，因此 Python 执行时完全平稳。

## 复现条件

- **复现频率**：Windows 平台执行 PHP 命令 100% 必现
- **触发步骤**：
  1. 在 Windows 终端运行 CC Code TUI。
  2. 让 Agent 执行任意一条 `php` 命令（例如 `php -r "sleep(1);"`）。
  3. 观察命令执行前后的 TUI 屏幕画面。
- **环境**：
  - OS: Windows (conhost / Windows Terminal)
  - PHP: PHP CLI (Win32 NTS / TS)

## 涉及文件

- `peri-tui/src/main.rs` —— `draw_app` 中的 `terminal.clear()` 物理清屏调用
- `peri-tui/src/terminal_backend/windows.rs` —— `ConsoleFingerprint::read()` 与 `refresh_widths()`
- `peri-tui/src/shell_exec.rs` —— Windows 子进程 spawn 缺少 `CREATE_NO_WINDOW` 隔离标志
- `peri-middlewares/src/process/mod.rs` —— `shell_command_cmd` 子进程参数与属性设置

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-23 | — | Open | agent | 创建 issue |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
