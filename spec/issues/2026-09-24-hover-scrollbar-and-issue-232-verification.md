# 悬停滚动条恢复与 Issue #232 核验

分支：`fix/hover-scrollbar-input`，基线 `df059c3b`。不修改此前 #229/#230 已提交修复，不自动提交或推送。

## 实现范围

- 消息有溢出时，最右侧 3 列为热区；悬停显示 1 列暗色滑块，离开隐藏。显隐不改变正文宽度。
- 按住滑块拖出热区仍保持显示，松手后按位置决定隐藏；失焦、缩放时清理拖拽状态，交互弹窗不允许操作底层滚动条。
- 独立输入线程持续读取终端，合并相邻被动移动；点击、松手、滚轮、键盘、粘贴、resize 不跨界合并。队列有上限。
- 外部编辑器独占终端时暂停输入读取；退出主循环后、执行退出钩子之前停止线程。
- 模拟粘贴收集保留主动事件边界；被动悬停延后处理，不让 `文字 → 悬停 → 回车` 把粘贴中的换行变成提交。

## Windows 兼容性边界

此前关闭悬停不只是渲染效率问题。本机 Windows 10 19045 的旧系统 ConPTY 会把无按键 SGR 移动报文（button=35）当成键盘片段。独立读取线程不能修好宿主自身的解析器。

实现读取真实控制台宿主的映像和文件版本，不依赖 `WT_SESSION` 猜测。仅为已验证版本的 OpenConsole（1.24 起）及不经过 SGR 解码的传统可见控制台启用 hover。未知/旧宿主不请求无按键移动，降级为常驻的暗色细滑块，保留点击、拖拽和滚轮。

不修改或替换用户安装的终端。压力测试下载的微软 ConPTY SDK 仅存放于忽略的 `target/hover-conpty-sdk/`，不随应用打包。未对所有 Windows 版本、Linux/macOS 做真机验证。

新版宿主的 `GetConsoleWindow` 可能是客户端自己的消息代理窗口，不能仅看窗口所属 PID。[微软文档](https://learn.microsoft.com/en-us/windows/console/getconsolewindow) 也说明伪控制台返回的是消息队列窗口。实现先查询当前进程的 `ProcessConsoleHostProcess`，再核对宿主文件版本；该 Native 查询不是稳定 Win32 契约，失败时保守降级。

**旧宿主未覆盖项：** 本机系统 ConPTY 直接注入 SGR 点击/拖动/松手/滚轮，不产生原生鼠标 INPUT_RECORD。用修改前的开关序列跑原生 INPUT_RECORD 对照，结果相同。因此旧宿主只确认不请求 hover、键盘没有鼠标乱码；不能声称其完整鼠标链路通过。应用层保留既有点击/拖拽处理，不在本任务内替换或修补系统 ConPTY。

## Issue #232：事实与缺陷不能混为一谈

原文：[GitHub #232](https://github.com/cc-claws/cc-code/issues/232)。原始用户反馈只有“不丝滑”；文中的细分症状是推断，不能视为用户实测。

1. **两套分母确实存在，但不能直接证明点击/拖拽错位。** 点击轨道是绝对定位，拖拽是以抓取点为锚的相对位移。测试覆盖 30 行轨道、可滚动范围 20/60/1000、起点/中点/终点的空白轨道点击；调用 ratatui 实际绘制验证鼠标是否落在滑块内。另外调用实际事件处理与消息渲染，检查点击后原地拖动、向上拖动和回锚点。不要求滑块中心始终对齐鼠标，因为抓取点不一定在中心。
2. **未显式设置 viewport 属实，声称因此导致滑块异常的归因不成立。** 对当前依赖 ratatui-widgets 0.3.0，未指定时使用绘制区域高度。测试逐格比较省略与显式设置相同高度的 Buffer，结果相同；长历史的最小 1 格滑块是比例/字符栅格的结果。按 Issue 建议改成 `content_length=visual_total`，测试能复现“内容到底、滑块不到底”，不应照搬该修法。
3. **渲染清理拖拽状态属实，需区分正常取消与异常中断。** 测试切换 loading/spinner 时，只要仍有滚动范围，拖动保持；当内容完全容纳、滚动范围归零时，滑块消失并取消拖动是正常清理。弹窗接管输入也不应继续拖底层内容。仅删除渲染清理不能解决缺失 metrics，因为事件层同样会取消。

以上是实际事件/渲染代码测试，不是手工代入公式。但 headless 绘制和 ConPTY 自动化不等于用户肉眼观察的跟手性、掉帧录屏；**不据此宣称所有“不丝滑”都不存在**。尚无证据把这三处代码差异定性为用户手感问题的根因。

## 可复现验证

普通回归（不需要 LLM 请求）：

```powershell
cargo test -p peri-tui --lib event:: -- --test-threads=2
cargo test -p peri-tui --lib conpty::
cargo clippy -p peri-tui --lib --bins --no-deps -- -D warnings
```

Windows 真实 ConPTY：先 `cargo test -p peri-tui --lib --no-run` 取得测试 exe 路径；在独立 PowerShell 中运行以下命令，避免环境变量影响用户启动的应用。

```powershell
$env:PERI_EXPECT_HOVER='0'
python scripts/test-hover-conpty.py <test-binary.exe> target/hover-legacy.json

$env:PERI_EXPECT_HOVER='1'
$env:PERI_CONPTY_DLL=(Resolve-Path 'target/hover-conpty-sdk/sdk/runtimes/win-x64/native/conpty.dll').Path
python scripts/test-hover-conpty.py <test-binary.exe> target/hover-modern.json
```

脚本创建专属伪控制台，核对实际发给前端的 `1003` 模式，模拟 UI 消费暂停 500ms 和 100 次输出，再检查 20,001 次悬停报告的最终位置、键盘文本及点击/拖拽/释放/滚轮顺序。旧宿主必须不启用/不注入 hover；不伪称旧宿主也通过悬停压力测试。

`PERI_HOVER_BATCHES` 可调整每批 100 次报告的批数，`PERI_HOVER_BATCH_DELAY` 可设置批间隔。`PERI_HOVER_NATIVE_PROBE=1` 是独立诊断模式，强制注入悬停并记录原生 INPUT_RECORD，用于隔离宿主解析错误；不要把该诊断模式视为正常应用行为。

## 本轮已完成结果（2026-09-24）

- `event::`：47 项通过；1 项专属 ConPTY 测试在普通运行中忽略，另由脚本执行。包含 4 项 #232 审核用例及原有滑块 35 组位置回归。
- `conpty::`：5 项通过。
- `cargo clippy -p peri-tui --lib --bins --no-deps -- -D warnings`：通过。
- `cargo build -p peri-tui --bin cc-code`：通过。可在兼容终端中运行 `./target/debug/cc-code.exe -c` 继续本地会话体验；无需替换已安装版本。
- 本次修改涉及的 Rust 文件 `rustfmt --check`、`git diff --check`：通过。未顺手格式化基线其他文件。
- 新版 OpenConsole 1.24.2607.10001：20,001 次悬停报告合并后送达 1 个最终位置；带 1ms 批间隔的 100,001 次报告送达 714 个移动，最终位置均为 `(79,9)`。两次键盘文本均精确为 `beforeafter`，四种主动鼠标事件顺序完整，实际前端 `1003` 模式与宿主判定一致。
- 旧系统 conhost 10.0.19041：hover 判定为 false，实际不请求 hover，键盘精确为 `beforeafter`。鼠标事件未送达，限制及基线对照见上文，不计为鼠标交互通过。
- 输入线程生命周期/粘贴边界经过独立只读审查，发现的悬停打断粘贴问题已修复并补测试；第二轮当前未发现明确问题。审查未修改文件。

真实终端证据文件位于忽略的构建目录（清理 target 后会消失）：

- `target/hover-conpty-modern-verified.json` / 同名 `.json.log`
- `target/hover-conpty-modern-100k-verified.json` / 同名 `.json.log`
- `target/hover-conpty-legacy-verified.json` / 同名 `.json.log`
- `target/hover-native-legacy-baseline.json`（使用原始开关的原生记录对照）

未跑 workspace 全量测试、没有真人连续拖动录屏或完整 LLM 输出端到端观测；不以这些局部结果声称所有主观“手感”已确认。未修改 GitHub Issue 状态或内容。
