# /gc 内存诊断：RSS 变化符号颠倒 + allocated 差值文案误导

**状态**：Fixed
**优先级**：中（仅显示层；但 `/gc` 的设计目标正是「消除误导指标」，符号反了等于制造新的误导）
**创建日期**：2026-10-08
**分支**：`fix/gc-rss-delta-sign`
**Issue**：[#345](https://github.com/cc-claws/cc-code/issues/345)
**PR**：[#346](https://github.com/cc-claws/cc-code/pull/346)

---

## 一、问题描述

`/gc` 输出的 RSS 变化方向**与事实相反**：

```text
RSS: 197.9 MB → 196.6 MB  (+1.3 MB)      ← 明明是「下降 1.3 MB」，却显示 "+1.3 MB"
· OS RSS: 197 MB → 196 MB (+1 MB)        ← 同上
```

`197.9 → 196.6` 是**减少**，`+` 号把方向彻底搞反。使用者会误以为内存在增长。

另有一处**文案与数值方向不符**：

```text
· mimalloc allocated: 200.1 MB (与 RSS 差 3.4 MB；RSS 更大 = 栈/映射文件等非分配器占用)
```

此处 `allocated 200.1 > RSS 196.6`，是 **allocated 更大**，但文案「RSS 更大 = …」写的却是相反情形，驴唇不对马嘴。

## 二、根因

`cc-tui/src/command/core/gc.rs`（#339 `e0fcf5c3` 引入）：

```rust
// RSS 汇总
let delta = before.current_rss as isize - after.current_rss as isize;  // before - after
let sign  = if delta >= 0 { "+" } else { "" };                         // delta>=0 → "+"
lines.push(format!("RSS: {} → {} ({sign}{})",
    fmt_bytes(before.current_rss), fmt_bytes(after.current_rss),
    fmt_bytes(delta.unsigned_abs())));
```

1. `delta = before - after`：**内存减少**（`after < before`）时 delta 为正。
2. `sign = if delta >= 0 { "+" }`：于是「减少」被加上 `+`。

两处叠加 → 方向 100% 反向。`OS RSS` 那段（`before as isize - after as isize`）同病。

`alloc_delta = allocated - RSS` 方向本身没错，但文案**只按「RSS 更大」一种情形**写，
当 `allocated > RSS`（本例）时说明与实际相反。

## 三、修复

- 统一方向语义为 `after - before`（增加为正）；新增 `fmt_signed_delta()` 输出
  `+1.3 MB` / `-1.3 MB` / `±0 B`，`RSS` 与 `OS RSS` 两处共用，杜绝再次写反。
- `allocated - RSS` 改为**带符号**输出（`allocated-RSS = +3.4 MB`），并按方向分支文案：
  - `> 0`：allocated 大于当时 RSS（采样时刻/记账口径差，仅供参考）；
  - `< 0`：RSS 更大 = 栈/映射文件等非分配器占用。

## 四、回归测试

`cc-tui/src/command/core/gc.rs` 内联 `mod tests`：

- `fmt_signed_delta_direction`：`-1.3MB → "-1.3 MB"`、`+1.3MB → "+1.3 MB"`、`0 → "±0 B"`。
- `fmt_signed_delta_mb_direction`：同理校验整 MB 路径。

## 五、影响面

仅 `/gc` 命令的显示文案与符号；不涉及内存回收行为本身。
