# cc-compress

CLI 输出压缩过滤器：在执行命令得到的原始输出之上，剥离噪音、折叠重复、
按命令语义保留关键行，减少进入模型上下文的 token。

## 概述

`cc-compress` 是 cc-code 的叶子 crate，特点：

- **纯函数库**：只做字符串进、字符串出——不 spawn 进程、不读环境、不碰文件系统。
  需要外部信息的过滤器由调用方（`cc-tools`）在调用前完成
- **永不反向膨胀**：所有过滤器出口都应经过 `guard::never_worse`，过滤结果比原文
  更费 token 时回退原文
- **按字符而非字节截断**：CJK 按字节切会产生非法 UTF-8，行截断统一走字符级操作
- **零 workspace 内部依赖**：仅依赖 `regex`，与 `cc-widgets` 同级

## 模块

| 模块 | 公开 API | 说明 |
|------|----------|------|
| `ansi` | `strip_ansi(input: &str) -> String` | 剥离 ANSI 转义序列，并把 `\r` 覆盖（进度条）规范化为最终一帧 |
| `guard` | `estimate_tokens(text: &str) -> usize` | 按 `bytes / 4` 估算 token 数 |
| `guard` | `never_worse(raw: &str, filtered: String) -> String` | 比较过滤前后估算 token，过滤更贵时回退原文 |
| `dedup` | `dedup_adjacent_lines(input: &str) -> String` | 折叠相邻 ≥3 行相同内容为 `[xN]`，空行豁免、不跨距离折叠 |

## 使用示例

以下示例的对应用例见 `cc-compress/src/tests.rs`。

### ANSI 剥离

```rust
use cc_compress::ansi::strip_ansi;

let colored = "\x1b[32mSuccess\x1b[0m: \x1b[1;31mError message\x1b[0m";
assert_eq!(strip_ansi(colored), "Success: Error message");
```

终端「原地重绘」的进度条输出（带 `\r`）只保留最后一段终态：

```rust
let progress = "Downloading 10%\rDownloading 50%\rDownloading 100%\nDone";
assert_eq!(strip_ansi(progress), "Downloading 100%\nDone");
```

### token 守卫

```rust
use cc_compress::guard::{estimate_tokens, never_worse};

assert_eq!(estimate_tokens(""), 0);
assert_eq!(estimate_tokens("abcd"), 1);   // bytes / 4，向上取整

// 过滤后变长（小输出被追加标记行）→ 回退原文
let raw = "ok";
let filtered = "ok [+999 lines omitted]".to_string();
assert_eq!(never_worse(raw, filtered), raw);
```

### 相邻重复行折叠

```rust
use cc_compress::dedup::dedup_adjacent_lines;

let raw = "warning: same\nwarning: same\nwarning: same\nwarning: same\ndone";
assert_eq!(dedup_adjacent_lines(raw), "warning: same [x4]\ndone");
```

只折叠**相邻**且完全相同的行（连续 ≥3 次）：非相邻的相同行属不同事件，空行成组原样保留，
连续 2 次重复低于下限、不引入标记。

## 集成方式

`cc-tools/src/tools/output_filter.rs` 的 `filter_command_output` 出口接 `never_worse`，
`strip_ansi` 由本 crate 转出（`cc-tools` 侧保留同名 re-export 供既有调用点使用）。

## 边界

- 本 crate **不提供**按命令定制的过滤器（如 `git log` / `diff` / `show` 的压缩）。
  这类过滤器需要改写 argv 并额外拉取数据判断截断，与 HITL 的
  「门控评估 == 审批展示 == 实际执行」不变量冲突，需单独设计后再引入
- 过滤规则的上游提炼文档见 `docs/2026-11-10-cli-output-compression-rules.md`

## 测试

```bash
cargo test -p cc-compress --lib
```
