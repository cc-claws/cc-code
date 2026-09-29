# PRD: Shell/工具执行状态指示器统一改造

## 背景

当前 Shell 命令和工具调用的状态指示存在三个问题：

1. **两套实现不一致**：`peri-widgets` 的 `format_indicator()` 定义了 `●` 系列图标，但 `peri-tui/message_render.rs` 实际使用的是 `◐`/`✓`，widgets 层代码成了死代码
2. **错误状态语义模糊**：错误工具的指示器是绿色 `✓`（对齐 Claude Hub），但用户直觉上红色 = 失败、绿色 = 成功，当前设计违反直觉
3. **Shell 命令没有状态点**：`!command` 用文字 `running` / `exit 0` / `exit N` 表示状态，缺少一目了然的视觉锚点

## 目标

统一 Shell 命令和工具调用的状态指示器，用颜色语义化的 `●` 圆点替代当前混杂的图标体系：

| 状态 | 指示器 | 颜色 | 视觉效果 |
|------|--------|------|----------|
| 执行中 | `●` | 黄色 `YELLOW` #FFCC00 | 闪烁（每 4 tick 隐显一次） |
| 成功 | `●` | 绿色 `SAGE` #4EBA65 | 静态 |
| 失败 | `●` | 红色 `ERROR` #FF6B80 | 静态 |
| 待执行 | `●` | 灰色 `MUTED` #999999 | 静态 |

## 改动范围

### 1. `peri-widgets/src/tool_call/display.rs`

**现状**：`format_indicator()` 返回 `&'static str`，无颜色信息

**改为**：返回 `(Span, &'static str)` 元组，包含带颜色的指示器 Span + 图标字符

```rust
pub fn format_indicator(status: ToolCallStatus, tick: u64) -> (&'static str, Color) {
    match status {
        ToolCallStatus::Pending => ("●", theme::MUTED),
        ToolCallStatus::Running => {
            let visible = (tick / 4).is_multiple_of(2);
            if visible { ("●", theme::YELLOW) } else { (" ", theme::YELLOW) }
        }
        ToolCallStatus::Completed => ("●", theme::SAGE),
        ToolCallStatus::Failed => ("●", theme::ERROR),
    }
}
```

### 2. `peri-tui/src/ui/message_render.rs` — 工具调用指示器

**现状**（L581-591）：
```rust
let indicator = if is_running {
    let tick = ...;
    if (tick / 4).is_multiple_of(2) { "◐" } else { " " }
} else {
    "✓"  // 错误也是绿色 ✓
};
```

**改为**：复用 widgets 层的 `format_indicator()`，或直接对齐语义：
```rust
let (indicator, indicator_color) = if is_running {
    let tick = std::time::Instant::now().elapsed().as_millis() as u64 / 200;
    let visible = (tick / 4).is_multiple_of(2);
    let ch = if visible { "●" } else { " " };
    (ch, theme::YELLOW)
} else if is_error {
    ("●", theme::ERROR)
} else {
    ("●", theme::SAGE)
};
```

**关键变更**：
- 图标：`◐`/`✓` → 统一 `●`
- 错误颜色：`SAGE` 绿 → `ERROR` 红
- 工具名颜色：错误时从 `TEXT` 白 → `ERROR` 红（与指示器一致）

### 3. `peri-tui/src/ui/message_render.rs` — Shell 命令指示器

**现状**（L271-374）：无圆点指示器，只有文字状态 `running` / `exit N`

**改为**：在命令行头部增加 `●` 指示器，紧跟在 `>` 之后：

```
当前：
> !ls -la running · /path/to/cwd
  └ exit code 0

改为：
> ● !ls -la · /path/to/cwd        ← 运行中：黄色闪烁 ●
  └ exit code 0

> ● !ls -la · /path/to/cwd        ← 成功：绿色 ●
  └ exit code 0

> ● !ls -la · /path/to/cwd        ← 失败：红色 ●
  └ exit code 1
```

指示器颜色逻辑：
```rust
let (indicator, indicator_color) = match exit_code {
    None => {
        let tick = std::time::Instant::now().elapsed().as_millis() as u64 / 200;
        let visible = (tick / 4).is_multiple_of(2);
        let ch = if visible { "●" } else { " " };
        (ch, theme::YELLOW)
    }
    Some(0) => ("●", theme::SAGE),
    Some(_) => ("●", theme::ERROR),
};
```

原有的文字状态 `running` 保留，exit code 也保留，指示器是增量增强，不是替换。

### 4. `peri-widgets/src/tool_call/mod.rs` — Widget 组件渲染

确保 `ToolCallWidget` 的 `render()` 方法使用新的 `format_indicator()` 返回带颜色的 Span。

## 不改动

- **Spinner 组件**：`✻` 旋转星号动画独立于状态指示器，不改
- **颜色常量定义**：`theme.rs` 现有颜色够用，不新增
- **折叠/展开逻辑**：与状态指示器无关
- **Shell 输出区域样式**：边框、背景、ANSI 解析不变

## 验收标准

1. 工具执行中：黄色 `●` 闪烁，工具名青色加粗
2. 工具成功：绿色 `●`，工具名白色
3. 工具失败：红色 `●`，工具名红色
4. Shell 运行中：黄色 `●` 闪烁 + `running` 文字
5. Shell 成功：绿色 `●` + `exit 0`
6. Shell 失败：红色 `●` + `exit N`
7. 闪烁频率与当前 spinner 一致（~200ms/tick，每 4 tick 切换）

## 文件清单

| 文件 | 改动 |
|------|------|
| `peri-widgets/src/tool_call/display.rs` | `format_indicator()` 返回颜色 |
| `peri-widgets/src/tool_call/mod.rs` | Widget render 使用带颜色指示器 |
| `peri-tui/src/ui/message_render.rs` | 工具指示器 + Shell 指示器统一 |
