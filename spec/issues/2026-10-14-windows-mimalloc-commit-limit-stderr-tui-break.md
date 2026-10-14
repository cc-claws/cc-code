# Windows 下 mimalloc 遭遇虚拟内存提交限制(error 1455)时裸写 stderr 穿透破坏 TUI 终端渲染

**状态**：Fixed
**优先级**：高（导致终端画面撕裂混乱，且暴露系统级内存耗尽故障）
**创建日期**：2026-10-14
**Issue**：[#483](https://github.com/cc-claws/cc-code/issues/483)

---

## 一、现象与现场

在 Windows 平台进行长时间多轮会话或高负载子 Agent 运行期间，TUI 界面被直接穿透打印的底层错误打花，报错字符与当前渲染行（如状态条、Todo 列表等）重叠撕裂：

```text
Loitering... (thought for 2s) on OS allocation, try again... (attempt 2, 0x400000 bytes, error code: 0x5AF, address: 0x00000000, alignment: 0x10000, flags: 0x3000)
mimalloc: Define SubAgents in .claude/agents/
mimalloc: warning: thread 0x7ED0388000: unable to commit meta-data for OS memorymimalloc: warning: thread 0x7ED0388000: cannot commit OS memory (error: 1455 (0x5AF), address: 0x0277A1D00000, size: 0x400000 bytes)
mimalloc: warning: thread 0x7ED0388000: cannot commit OS memory (error: 1455 (0x5AF), address: 0x0277DD490000, size: 0x400000 bytes)
mimalloc: warning: thread 0x7ED0388000: out-of-memory on OS allocation, try again... (attempt 1, 0x400000 bytes, error code: 0x5AF, address: 0x00000000, alignment: 0x10000, flags: 0x3000)
```

终端备用屏幕（Alternate Screen）渲染被破坏，光标位置混乱。

## 二、根因分析

该现象由两层问题交织引发：

1. **真实系统级虚拟内存提交耗尽（ERROR_COMMITMENT_LIMIT / 1455 / 0x5AF）**：
   - Windows 错误代码 `1455 (0x5AF)` 为 `ERROR_COMMITMENT_LIMIT`（*“页面文件太小，无法完成操作”*）。
   - 这意味着操作系统的虚拟内存提交上限（Commit Limit）已被耗尽，导致内核甚至无法为一个仅 4MB（`size: 0x400000 bytes`）的页面执行 `MEM_COMMIT`。
   - 诱因来自客户端长对话轮次累积、子 Agent 运行与分配器碎片化导致的虚拟内存持续上涨，叠加宿主机环境内存压力。

2. **底座分配器 C 层错误直接裸写 stderr，绕过日志系统破坏 TUI 渲染**：
   - Windows 全局分配器为 `mimalloc`（`libmimalloc-sys`）。
   - 查阅 `cc-tui/src/alloc_config/windows_tuning.rs`，当前启动初始化仅配置了 `OPTION_PURGE_DECOMMITS` 和 `OPTION_PURGE_DELAY`。
   - **未注册自定义输出回调（`mi_register_output`），亦未配置 `mi_option_show_errors = 0`**。
   - 当 mimalloc 发生底层系统调用失败时，其内置逻辑直接调用 C 标准库向操作系统的底层 `stderr` 句柄裸写格式化字符串。
   - 该行为绕过了 Rust 侧的 `tracing` 结构化日志文件，直接击穿并破坏了 Crossterm/Ratatui 的终端渲染流。

## 三、涉及文件

- `cc-tui/src/alloc_config/windows_tuning.rs` —— Windows mimalloc 启动调优配置，缺少输出重定向与错误静音。
- `cc-tui/src/alloc_config.rs` —— 分配器接口层。
- `spec/issues/2026-05-22-memory-linear-growth-no-compact.md` —— 关联的长对话内存持续增长母 Issue。

## 四、修复建议

1. **分配器警告/错误重定向与压制**（`cc-tui/src/alloc_config/windows_tuning.rs`）：
   - 在启动初始化中，使用 `libmimalloc_sys::mi_register_output` 将 mimalloc 的标准输出/错误重定向至 Rust 的 `tracing::warn!` 统一捕获，确保错误落入本地诊断日志文件而非控制台终端。
   - 或配置 `mi_option_show_errors = 0`（以及 `mi_option_verbose = 0`），禁止直接裸打控制台。
2. **长对话内存治理联动**：
   - 结合 `spec/issues/2026-05-22-memory-linear-growth-no-compact.md`，优化分配器回收时机与大对象生命周期管理。

## 五、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-14 | — | Open | agent | 创建 Issue #483 及本地 Spec 文档 |
| 2026-10-14 | Open | Fixed | agent | 修复：压制 mimalloc show_errors 与 verbose 并注册 no-op 输出回调，彻底解决 stderr 穿透打花终端问题 |

## 六、修复记录

- `cc-tui/src/alloc_config/windows_tuning.rs`：在 `init()` 中通过 `set_option` 默认压制 `mi_option_show_errors` (0) 与 `mi_option_verbose` (0)，保留环境变量 `MIMALLOC_SHOW_ERRORS` 与 `MIMALLOC_VERBOSE` 的调试覆盖通道。
- 注册 `noop_output_callback` 封死所有底层旁路向 `stderr` 的字符输出，避免 OOM 上下文调用日志分配器引发重入死锁。
- 新增 `cc-tui/src/alloc_config/windows_tuning_test.rs`：单测覆盖默认静音状态与环境变量覆盖逻辑。

