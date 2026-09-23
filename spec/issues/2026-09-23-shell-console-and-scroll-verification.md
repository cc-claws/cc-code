# #229 / #230 验证与修复记录

- Issue：<https://github.com/cc-claws/cc-code/issues/229>、<https://github.com/cc-claws/cc-code/issues/230>
- 基线：`6f621e75`（npm v0.6.72）
- 分支：`fix/windows-shell-and-scroll-stutter`

## #229：Windows 子进程导致物理清屏

独立隐藏控制台中的 PHP 7.4.3 CLI 实测：父控制台代码页为 936 时，未隔离的
`cmd /C php -r "usleep(300000);"` 会将输入/输出代码页改成 65001，退出时恢复 936。
Python 对照命令没有改变代码页。父控制台本来是 65001 时，PHP 也没有产生代码页变化。
因此问题成立，但 issue 中“所有环境 100% 必现”的说法不准确。

代码链路为 `ConsoleFingerprint.code_page` 改变 → `ConsoleWidthProbe.refresh()` →
`refresh_widths()` → `draw_app()` 中的 `terminal.clear()`。

修复统一 shell 构造：Windows 下的 cmd、PowerShell、Git Bash 使用
`CREATE_NO_WINDOW`，保留原有管道输入输出、取消和工作目录配置。TUI captured/streaming、
inline executor 及共用 shell helper 的 hooks/MCP 均复用该隔离策略；非 Windows 行为不变。
真实字体/代码页改变后的列宽缓存失效与清屏恢复机制保持原样，避免引入旧的字符残影问题。

## #230：滚轮事件积压

旧代码只合并拖拽事件；每个滚轮事件返回 `Action::Redraw`，主循环每次立即绘制。
连续 120 个滚轮事件会产生 120 次绘制，绘制越慢，排队输入消化越慢。
旧拖拽合并还会用紧随其后的释放/键盘事件覆盖最后一次拖拽坐标。

另外，原有 `thumb_click_drift_runtime_proof` 在 30 个采样位置中复现了 13 个滑块点击漂移：
渲染端无上下箭头，映射仍减去两行；按下可见滑块会将精确内容偏移重新量化，导致跳动。
现改为从实际绘制结果读取滑块命中区域，按住滑块不改变偏移，并保存鼠标行/内容偏移锚点。
拖拽按锚点差值计算，来回拖动不积累舍入误差；空白轨道按完整高度映射，resize 取消旧锚点。
原“断言 bug 存在”的测试已替换为真实渲染下 35 个采样位置的正向回归测试。

修复为有界批处理：每批最多读取 128 个已排队事件，滚轮逐步更新状态但整批只重绘一次。
保留方向、鼠标位置、修饰键及上下边界语义；连续拖拽保留最终坐标。
遇到释放、键盘、resize 等边界事件暂存到下一轮，确保先绘制最终选区后再处理释放复制。
达到批次上限就返回主循环，避免持续滚动饿死 Agent/shell 轮询。

没有引入固定延时、滚动惯性或加速度。现有 `viewport_clip()` 已使用二分定位可见逻辑行，
不能把问题归因为每帧重排全部历史；`render_cache.write()` 也已在绘制前释放。
长历史在短滚动条上每格对应大量内容仍受字符坐标分辨率限制；点击漂移与丢失最终拖拽坐标已修复。

## 可复现验证

```powershell
cargo test -p peri-tui --lib mouse_batch -- --test-threads=1
cargo test -p peri-tui --lib scrollbar_test -- --test-threads=1
cargo test -p peri-tui --lib shell_exec::tests -- --test-threads=1
cargo test -p peri-tui --lib terminal_backend::compatible::tests
cargo test -p peri-middlewares --lib process::process_test -- --test-threads=1
cargo check -p peri-tui --bins
```

滚轮回归包含一万行缓存内容，比较旧逐事件绘制与新批处理的最终画面和偏移量；
120 个事件的绘制次数从 120 次变为 1 次，目标是可重复的队列与画面验证，不是终端 FPS 基准。

真实控制台专项（需要 PHP CLI 和 Git Bash；使用 cargo 输出的实际测试二进制路径）：

```powershell
./scripts/test-shell-console.ps1 -TestBinary ./target/debug/deps/peri_tui-<hash>.exe
./scripts/test-cmd-rendering.ps1 -TestBinary ./target/debug/deps/peri_tui-<hash>.exe
```

第一项记录 cmd/PowerShell/Git Bash 的隔离前后代码页与真实列宽探针失效次数，
并覆盖 TUI captured/streaming 两条执行路径；第二项复核既有字体/代码页切换残影回归。
两项均在专属隐藏控制台运行，证据写入独立 JSON 文件。

这些验证不等价于在 Windows Terminal / VS Code 中实际操作鼠标的视觉验收。

## 本机执行结果（2026-09-23）

- 定向自动测试共 62 项通过：process 31、shell_exec 8、compatible backend 7、
  mouse_batch 6、event 4、message_area 5、scrollbar 1（内部覆盖 35 个采样位置）。
- 独立真实控制台 8 个 shell/executor 用例通过。cmd、PowerShell、Git Bash 的
  PHP 旧路径均触发两次列宽探针失效，隔离后均为零；captured/streaming 均正常输出。
  本机证据：`target/shell-console-229-evidence.json`。
- 既有控制台残影专项 4 个字体/代码页组合通过。
  本机证据：`target/console-ghosting-229-evidence.json`。
- `cargo clippy -p peri-tui -p peri-middlewares --lib --bins --no-deps -- -D warnings`
  通过，覆盖最新库与 TUI 二进制代码；修改的 Rust 文件格式检查及 `git diff --check` 通过。
- 未运行工作区完整测试、其他操作系统测试或 Windows Terminal / VS Code 手动视觉验收；
  未提交、推送或发布，用户原有未跟踪文件保持不变。
