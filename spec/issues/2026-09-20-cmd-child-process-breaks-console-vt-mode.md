# [BUG] CMD 控制台下子进程执行破坏 VT 模式导致 ANSI 转义序列泄露为明文与方块

- **状态**：Open
- **创建日期**：2026-09-20
- **优先级**：高 (P2)
- **模块**：TUI / Windows 终端兼容 / 子进程执行 (shell_exec)
- **问题截图凭证**：`C:/Users/adim/AppData/Local/Temp/32a4bd4054324888848587907c0acc69.png`
- **GitHub Issue**：#164 (https://github.com/cc-claws/cc-code/issues/164)

---

## 一、问题现象

用户在 Windows 传统命令提示符（CMD / `conhost.exe`，代码页 CP936，新宋体）下运行 CC Code (`peri-tui`)。

当 Agent 执行耗时前台命令（如 `powershell -NoProfile -Command "Start-Sleep -Seconds 15" && npm view @cc-claw/code dist-tags --json`，运行约 14 秒）时，界面出现严重错位与转义序列泄露：

1. **转义字符退化为方块 `□`**：
   - 屏幕上出现大量缺字方块 `□`（即 ASCII 27 / `0x1B`，ESC 控制符）。
2. **ANSI 控制序列原样以明文喷出**：
   - `□0;CC Code — Running`：设置终端窗口标题的 OSC 0 序列失效，被直接当作文本输出。
   - `□□63;35H` 与 `□51;1H`：CSI H 绝对光标定位序列失效，光标未定位到目标行，文本直接在当前行尾追加。
   - `□38;2;78;186;101;49m8%`、`MEM 83MB`：24 位 TrueColor RGB 颜色设置序列失效，文本作为未着色的普通字符串输出。
3. **界面严重撕裂交错**：
   - 本应在窗口底部渲染的 2~3 行状态栏（Status Bar：含 CPU/内存监控、工具调用统计 `√ Read x29 | √ Grep x7 | Todo x6`）和窗口标题指令，直接插到了主消息区 Spinner 与 Todo 列表的右侧，使整屏排版彻底崩溃。

---

## 二、底层根因分析

### 1. 子进程共享宿主控制台句柄（未设置 `CREATE_NO_WINDOW`）
- 在 `peri-tui/src/shell_exec.rs` 的 `spawn_streaming_child` 以及 `peri-middlewares/src/process/mod.rs` 中，使用 `tokio::process::Command` 启动子进程时，仅将 `stdout` 与 `stderr` 重定向为 `Stdio::piped()`，`stdin` 重定向为 `Stdio::null()`。
- 在 Windows 操作系统中，若未显式指定进程创建标志（Creation Flags，如 `CREATE_NO_WINDOW` = `0x08000000` 或 `DETACHED_PROCESS` = `0x00000008`），子进程（`cmd.exe` / `powershell.exe`）默认**继承并附加在与父进程完全相同的控制台会话（Console Session / conhost）中**。

### 2. PowerShell / CMD 启动与运行时重置 ConsoleMode
- Windows PowerShell 5.1（以及部分 CLI 工具在初始化 CRT 控制台时），当检测到附加在控制台上时，内部会调用 Win32 API `SetConsoleMode` 设置输入/输出模式。
- PowerShell 5.1 是为传统控制台设计的，其默认输出模式通常仅开启 `ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT`，**不会开启（甚至会主动覆盖关闭）`ENABLE_VIRTUAL_TERMINAL_PROCESSING` (0x0004)**。

### 3. 父进程 TUI 高频重绘引发“ANSI 泄洪”
- 在子进程（如 `Start-Sleep -Seconds 15`）运行的 15 秒内，`peri-tui` 主事件循环（`main.rs`）仍在以 30 FPS / 200ms 的频率调用 `draw_app` 和 `SetTitle`，以驱动 Spinner 动画、秒数计时以及状态栏更新。
- 此时宿主控制台的 VT 模式已经被子进程篡改关闭，Ratatui 和 crossterm 发射的所有控制序列（光标移动、RGB 调色、标题修改、隐藏光标）无法再被 conhost 解析：
  - `0x1B` 无法被识别为转义引导字符，被字体渲染为未知字形方块 `□`；
  - 光标定位指令未被执行，光标停留在原处并随着字符输出一直向右推进，最终导致所有 ANSI 字符以明文形式倾泻在屏幕右半部分。

---

## 三、涉及文件

- `peri-tui/src/shell_exec.rs`（`spawn_streaming_child`：流式子进程 spawn 配置）
- `peri-middlewares/src/process/mod.rs`（`shell_command_with_shell` / `shell_command_cmd`：跨平台子进程构建）
- `peri-tui/src/main.rs`（`draw_app`：TUI 帧绘制与终端模式防御）
- `peri-tui/src/conpty.rs`（`enable_vt_processing`：Windows 控制台 VT 模式切换）

---

## 四、建议修复方案（待后续实现）

### 方案 1：子进程彻底脱离控制台（核心根治）
在 `peri-tui/src/shell_exec.rs` 及 `peri-middlewares/src/process/mod.rs` 中为 Windows 平台追加 `CREATE_NO_WINDOW` 标志：

```rust
#[cfg(windows)]
{
    use std::os::windows::process::CommandExt;
    // 0x08000000 = CREATE_NO_WINDOW，彻底阻止子进程附加宿主控制台
    cmd.creation_flags(0x08000000);
}
```

### 方案 2：TUI 绘制帧增加 VT 模式防掉线守护（纵深防御）
在 `peri-tui/src/main.rs` 的 `draw_app` 每次重绘前，确保检查并重置 `ENABLE_VIRTUAL_TERMINAL_PROCESSING`：

```rust
#[cfg(windows)]
{
    let _ = conpty::enable_vt_processing();
}
```
保证即使外部进程在控制台短暂修改了模式，下一帧 TUI 也能立即自愈。

### 方案 3：环境建议
引导用户优先使用基于 ConPTY 架构的 **Windows Terminal**（`wt.exe`），其具备完整的虚拟终端隔离能力。
