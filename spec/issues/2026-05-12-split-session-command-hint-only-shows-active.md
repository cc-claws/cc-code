# 分屏模式下非活跃 Session 命令浮层显示异常

**状态**：Fixed
**修复日期**：2026-05-17
**优先级**：中
**创建日期**：2026-05-12

## 问题描述

使用 `/split` 分屏后，左侧（非活跃）session 输入 `/` 时，命令浮层显示异常。具体表现为：
- 左侧 session 输入 `/` 后，命令浮层缺失部分内置命令
- 右侧（活跃） session 可以正常显示完整的命令列表

## 症状详情

### 预期行为
- 在任一 session 输入 `/` 后，应显示该 session 的完整命令列表（包括所有内置命令）
- 或：点击非活跃 session 时自动切换焦点，然后显示命令浮层

### 实际行为
- 左侧 session 输入 `/` 后，命令浮层缺失部分内置命令
- 右侧 session 命令浮层显示正常

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI
  2. 输入 `/split` 创建分屏
  3. 在左侧 session 输入 `/`
  4. 观察命令浮层显示的命令列表
- **环境**：多 session 分屏模式

## 相关代码

- `peri-tui/src/ui/main_ui.rs:297-299` — 命令浮层只在 `is_active == true` 时渲染
  ```rust
  if is_active {
      // 统一命令/Skills 提示条
      popups::hints::render_unified_hint(f, app, chunks[5]);
  }
  ```

- `peri-tui/src/ui/main_ui.rs:80-82` — `render_session_column` 临时切换 active
  ```rust
  let prev_active = app.session_mgr.active;
  app.session_mgr.active = session_idx;
  ```

- `peri-tui/src/ui/main_ui/popups/hints.rs:24-33` — `render_unified_hint` 读取 active session 的输入和命令列表
  ```rust
  let first_line = app.session_mgr.sessions[app.session_mgr.active]
      .ui
      .textarea
      .lines()
      .first()
      .map(|s| s.as_str())
      .unwrap_or("");
  ```

## 根因分析

问题由**两个独立 bug** 叠加导致：

### Bug 1：命令浮层只在活跃 session 渲染（已修复）

`main_ui.rs:297` 的 `if is_active` 守卫导致非活跃 session 完全不渲染命令提示浮层。

**修复**：移除 `if is_active` 守卫，依赖 `render_session_column` 已有的临时 `active` 切换机制（第 82 行 `app.session_mgr.active = session_idx`）。

### Bug 2：`/split` dispatch 后命令注册表被归还到错误 session（根因）

`event.rs:812-820` 的 `std::mem::take` + `dispatch` + 归还模式存在 session index 竞态：

```
// 执行 /split 前: session 0 是 active，command_registry = [24 命令]
let registry = std::mem::take(
    &mut app.session_mgr.sessions[app.session_mgr.active]  // ← 从 session 0 取出
        .commands.command_registry,
);
let known = registry.dispatch(app, "/split");
// ↑ dispatch 内部: SplitCommand::execute → app.new_session() → active 变为 1
app.session_mgr.sessions[app.session_mgr.active]  // ← BUG: 此时 active=1，归还到 session 1
    .commands.command_registry = registry;
```

结果：
- **Session 0（左侧）**：CommandRegistry 被 `take` 后为空壳（`Vec::new()`），永未恢复
- **Session 1（右侧）**：获得完整的 24 个内置命令（原属于 session 0 的 registry）
- Skills 不受影响（独立的 `Vec<SkillMetadata>`，不走 `std::mem::take`）

**修复**：在 `take` 前保存 `let session_idx = app.session_mgr.active;`，归还时使用保存的 `session_idx`。

## 修复详情

| 文件 | 行 | 修改 |
|------|-----|------|
| `peri-tui/src/ui/main_ui.rs` | 297-298 | 移除 `if is_active` 守卫 |
| `peri-tui/src/event.rs` | 811-820 | take 前保存 session_idx，归还时使用保存值 |

## 验证

- 527 个测试全部通过
- 新增回归测试 `test_split_command_preserves_session0_command_registry`：验证 dispatch `/split` 后 session 0 的 CommandRegistry 仍包含 model/mcp/memory 命令

## 影响范围

- 所有使用 `/split` 分屏功能的用户
- 特别是在左侧 session 输入命令时

## 经验教训

1. **`std::mem::take` + 归还模式有 session index 竞态风险**：任何在 `dispatch` 期间可能改变 `app.session_mgr.active` 的命令（如 `/split`、`/loop` 等）都会导致归还目标错误。核心原则：**在 take 前保存 index，归还时使用保存值**。
2. **Hint 渲染依赖 `active` 临时切换**：非活跃 session 的渲染通过 `render_session_column` 临时切换实现。所有渲染函数都应无条件执行，不应有 `is_active` 守卫——数据隔离依赖临时切换，视觉区分依赖 `is_active` 传参处理边框颜色和光标样式。
