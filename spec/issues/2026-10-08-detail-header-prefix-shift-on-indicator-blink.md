# 详细模式下运行中工具头前缀 `● Bash(` 随指示器闪烁左移

**状态**：Fixed
**优先级**：中（仅影响显示、不影响命令执行；但详细模式正是为「看清完整命令」而设，前缀随闪烁抖动/消失影响可读性）
**创建日期**：2026-10-08
**分支**：`fix/detail-header-prefix-blink-shift`
**Issue**：[#342](https://github.com/cc-claws/cc-code/issues/342)
**PR**：[#343](https://github.com/cc-claws/cc-code/pull/343)

---

## 一、问题描述

详细模式（Ctrl+O）下查看**运行中的超长命令**时，工具头前缀 `● Bash(` 会随运行状态指示器**闪烁而左右抖动**：

```text
亮帧：● Bash(powershell -NoProfile -Command " Get-ChildItem 'C:\Users\adim' -Force -Directory …
灭帧：Bash(powershell -NoProfile -Command " Get-ChildItem 'C:\Users\adim' -Force -Directory …   ← ● 前缀消失、整行左移 2 列
```

表现：

1. 指示器亮帧显示 `● Bash(...`，闪烁熄灭帧开头 `● ` 消失、整行左移；
2. 用户观感为「详细模式看不到工具名前缀 / Bash 被吃掉了」。

非详细模式（外部视图）**正常**——只有进入 Ctrl+O 详细模式才复现。

> 复现要点：详细模式 + 超长命令（header 折行）+ 命令运行中（指示器闪烁，`TOOL_INDICATOR_TICK_INTERVAL` 每 200ms 一次）。

## 二、根因

三处叠加：

1. **指示器闪烁**：`cc-widgets/src/tool_call/display.rs` 的 `format_indicator()` 让运行中指示器在 `●` 与 `" "`（空格）之间闪烁：

   ```rust
   ToolCallStatus::Running => {
       let visible = (tick / 4).is_multiple_of(2);
       let ch = if visible { "●" } else { " " };   // 熄灭帧首 span 是空格
       (ch, Color::Rgb(153, 153, 153))
   }
   ```

2. **详细模式折行**：详细模式且非 Glob 时，工具头走 `wrap_full` 分支（`cc-tui/src/ui/message_render.rs`），把 `(完整命令)` 折成多行。

3. **行首 trim**：`wrap_line_spans_rich()` 在每段折行时 **trim 行首空白**（`seg_start` 跳过前导 whitespace）。

熄灭帧时 header 首 span 是空格 → 被 trim → `指示器 + 分隔空格` 前缀整体丢失、整行左移。

非详细模式 header 不折行、不 trim，所以外部正常。

```
亮帧（首段首列是 ●，非空白，trim 不生效）
    ● Bash(powershell ...        →  trim 后仍为 "● Bash(powershell ..."
灭帧（首段首列是空格，trim 生效）
    " Bash(powershell ..."  ← 首列空格    →  被 trim  →  "Bash(powershell ..."（● 与空格都没了）
```

## 三、修复

工具头的**结构性前缀**（指示器占位 + 分隔空格）不应参与折行的行首 trim。为折行核心增加「保留首段行首空白」的路径：

- `wrap_line_spans_rich_impl(line, max_width, trim_first_lead)`：把原实现抽为带开关的内部函数，`trim_first_lead` 只控制**首段**（`pos == 0`），续行行为不变（断行点推进时其后空白已跳过）。
- 新增保留变体：`wrap_line_spans_rich_keep_first_lead` / `wrap_line_spans_keep_first_lead` / `push_wrapped_line_keep_first_lead`。
- 工具头 `wrap_full` 分支改用 `push_wrapped_line_keep_first_lead`；其它折行路径（reasoning、batch summary 等）保持 `trim_first_lead = true`，行为完全不变。

修复后：亮帧 `● Bash(`、灭帧 `  Bash(`（指示器占位为空格），前缀**不再左移抖动**。

## 四、回归测试

`cc-tui/src/ui/message_render_test.rs`：

- `test_detail_header_prefix_stable_across_indicator_blink`：tick ∈ {0,2,4,6}（亮/灭帧）× width ∈ {40,80,100,120,137,160}，断言首行前 2 列恒为 `● ` 或 `  `，且首行含 `Bash`。
- 反向验证：回退修复后该测试失败；恢复后通过。

## 五、影响面

- 仅详细模式 + 超长命令折行场景；非详细 / 短命令路径不变。
- 仅显示层，不影响命令执行。
