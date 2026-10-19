# #622 jemalloc 启动配置验证记录

- GitHub Issue：https://github.com/cc-claws/cc-code/issues/622
- 用户原意：验证并修复 Linux/macOS 分配器调优没有生效的问题。
- 基线：`origin/main`，`d5dbc212`。
- 修复分支：`fix/jemalloc-startup-config`。
- 本地环境：Windows x86_64，Rust 1.99.0；仅安装 Windows Rust target，没有可用的 WSL/Linux 或 macOS 运行环境。
- 验证状态：代码修复已完成，目标平台运行验证待执行；GitHub Issue 保持 Open。

## 已确认的问题

1. `tikv-jemalloc-sys` 的实际配置变量是 `_RJEM_MALLOC_CONF`。旧代码设置的是 `MALLOC_CONF`。
2. jemalloc 在首次分配时解析配置，时机可能早于 `main()`。仅在旧函数中改名不能保证生效。
3. 旧测试只验证环境变量字符串未覆盖，没有读取分配器实际选项，不能证明调优生效。

证据来自本地锁定的 `tikv-jemalloc-sys 0.6.1`：`src/lib.rs` 的 `malloc_conf` 声明和 `jemalloc/src/jemalloc.c` 的配置读取逻辑；静态符号用法与 `tikv-jemallocator 0.6.1/tests/background_thread_enabled.rs` 一致。

## 修复记录

### 修复 1（2026-10-19）

- 操作人：Codex。
- `main.rs`：在二进制中导出 `_rjem_malloc_conf` 静态薄指针，在首次分配前提供默认值；取消非 Windows 的运行时环境注入。
- `alloc_config.rs`：移除失效的非 Windows 初始化函数，更新配置说明；Windows mimalloc 行为保持不变。
- `alloc_config_test.rs`：删除只检查环境变量的旧测试。
- `jemalloc_config_test.rs`：新增 4 个独立子进程用例及一个专用探针，读取真实 `opt.*`、`arenas.*` 和 `background_thread`。
- `spec/global/domains/tui.md`：记录配置优先级、使用方法与验证边界。
- 涉及 commit：本记录随修复分支提交；合并状态以关联 PR 为准。

## 验证记录

| 检查 | 结果 |
|---|---|
| 主代理与独立只读子代理源码审查 | 当前未发现明显问题 |
| `git diff --check` | 通过 |
| 入口、旧测试与新增测试的定向 rustfmt 检查 | 通过 |
| `cargo fmt --all -- --check` | 未通过；基线已有大量格式差异，未批量修改 |
| Windows `cargo test -p cc-tui --lib alloc_config` | 2 个测试通过 |
| Windows `cargo test -p cc-tui --bin cc-code main_test` | 8 个测试通过 |
| Linux/macOS jemalloc 子进程回归 | 未执行，缺少目标平台环境 |

Linux/macOS 的待执行命令：

```bash
cargo test -p cc-tui --bin cc-code jemalloc_config_test
cargo test --release -p cc-tui --bin cc-code jemalloc_config_test
```

预期：4 个父测试通过，专用探针在父进程测试列表中 ignored、在子进程中实际执行；默认 decay 为 `0/0`、后台线程为 `true`，完整和部分 `_RJEM_MALLOC_CONF` 覆盖生效，无前缀变量不参与配置。release 用例同时检查 LTO/strip 后静态符号是否仍生效。

## 限制

- 用户覆盖须在进程启动前通过 `_RJEM_MALLOC_CONF` 设置，不能依赖启动后读取的 settings `env`；不提供旧 `MALLOC_CONF` 转发。
- 保留原有三项调优策略。jemalloc 上游提示初始化时启用后台线程有循环依赖风险，尚需目标平台启动回归；可用 `_RJEM_MALLOC_CONF=background_thread:false` 禁用。
- 未做长会话 RSS A/B 测试，不能将本修复认定为 #26 或 #40 的根因解决。
- Windows 分配器测试只验证 mimalloc 相关回归，不能证明 Linux/macOS jemalloc 修复已运行生效。
