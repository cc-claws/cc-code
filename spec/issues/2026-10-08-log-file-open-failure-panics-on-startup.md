# 日志文件打不开导致 `cc-code` 启动即 panic

**状态**：Fixed
**优先级**：高（日志不可用会把整个 TUI 直接崩掉，用户完全无法启动）
**创建日期**：2026-10-08
**分支**：`fix/log-open-panic`
**Issue**：[#353](https://github.com/cc-claws/cc-code/issues/353)
**PR**：[#354](https://github.com/cc-claws/cc-code/pull/354)

---

## 一、现象

启动 `cc-code` 直接崩溃：

```
thread 'main' panicked at cc-agent\src\telemetry\subscriber.rs:75:10:
cannot open log file: Os { code: 5, kind: PermissionDenied, message: "拒绝访问。" }
```

## 二、根因

两层问题叠加：

1. **环境**：`~/.cc-code/logs/{service}.log` 的 ACL 被写坏成**空 DACL**（无任何 ACE）→ 新进程以 append 打开时得到 `PermissionDenied`。目录本身 ACL 正常，坏的是**日志文件**。
2. **代码（真 bug）**：`cc-agent/src/telemetry/subscriber.rs` 用 `.expect("cannot open log file")` 打开日志文件——**日志打不开就 panic 掉整个进程**。日志只是诊断设施，绝不该成为启动的硬依赖。

同一函数内还有两处同类 `.expect()`：
- `ensure_utf8_bom()`：读 metadata / 打开写 BOM 都用 `.expect()`；
- `set_global_default(...).expect("Unable to set global subscriber")`：subscriber 已设置时（如重复初始化）会 panic。

## 三、修复

「日志不可用 → 优雅降级，绝不 panic」：

- 抽出 `resolve_log_writer(log_path) -> BoxMakeWriter`：打开文件失败时 **`eprintln!` 警告并退回 `std::io::stderr`**；用 `BoxMakeWriter` 统一 writer 类型，json / 非 json 两条分支共用。
- `ensure_utf8_bom()` 改为失败静默忽略（BOM 只是显示优化）。
- `set_global_default` 失败降级为 `eprintln!` 提示，不 panic。

修复后：日志文件 ACL 坏了也**能正常启动**（日志走 stderr），不再"启动即崩"。

> 注：本 PR 只解决"日志不可用崩程序"的代码 bug；日志文件 ACL 被外部反复写坏的**根因**另案跟踪（`SetNamedSecurityInfo` 只在 DB/历史/凭据调用，未涉及日志）。

## 四、回归测试

- 新增 `test_resolve_log_writer_bad_path_falls_back_without_panic`：用「目录」当日志路径（打开必失败），断言 `resolve_log_writer` 不 panic（旧实现 `.expect()` 会崩）。
- 既有 `test_init_tracing_creates_default_log_file` 仍通过（修复后是「先建文件再装 subscriber」，文件仍会创建）。

## 五、验证

- `cargo test -p cc-agent --lib`：534 passed / 0 failed。
- `cargo clippy -p cc-agent --lib --tests -- -D warnings`：干净。
