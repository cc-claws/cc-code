# 实施计划：清理 plugin_panel/mod.rs 死代码（44 个未使用的 App 包装方法）

## 背景

`plugin_panel/mod.rs` 有 828 行，其中约 470 行是 `impl App` 块中 44 个公有方法（356-820 行）。

### 关键发现：这 44 个方法全是死代码

实际的事件处理流程：

```
用户按键 → PanelComponent::handle_key()
         → PluginPanel::handle_key()（mod.rs:201）
         → handlers/plugin_handlers/*.rs（直��操作 self + ctx）
```

44 个 App 包装方法（如 `plugin_panel_move_up()`、`discover_enter_search()` 等）**从未被生产代码调用**。唯一的调用方是测试文件 `plugin_panel_test.rs`。

### 证据

1. handler 通过 `ctx`（`PanelContext`）直接操作 `PluginPanel`，不经过 App
2. Grep 搜索所有 `.rs` 文件：44 个方法仅在 `plugin_panel_test.rs` 中出现
3. `panel_plugin.rs` 中的 4 个方法（`open_plugin_panel`、`close_plugin_panel`、`marketplace_add_and_save`、`marketplace_delete_and_save`）**确实在使用**——这些不在此次清理范围

## 当前结构

```
plugin_panel/mod.rs (828 行)
├── impl PluginPanel          (行 21-192)  ← 13 个 pub fn — 保留
├── impl PanelComponent       (行 196-352) ← trait 实现 — 保留
├── impl App（44 个方法）      (行 356-821) ← 死代码 — 删除
└── #[cfg(test)] mod tests    (行 823-828) ← 需要重写
```

## 实施步骤

### Step 1：删除 44 个死代码方法

**修改文件**：`peri-tui/src/app/plugin_panel/mod.rs`

删除 `mod.rs` 第 354-821 行（整个 `impl App` 块 + 上方的注释分隔线）。

保留：
- `impl PluginPanel`（行 21-192）
- `impl PanelComponent for PluginPanel`（行 196-352）
- `#[cfg(test)] mod tests`（行 823-828）

删除后 mod.rs 从 828 行缩减到约 355 行（-57% 体积）。

### Step 2：重写测试

**修改文件**：`peri-tui/src/app/plugin_panel/plugin_panel_test.rs`

当前 7 个测试通过 App 包装方法测试面板行为。删除这些包装方法后需要重写为直接测试 `PluginPanel`：

| 旧测试 | 新测试策略 |
|--------|-----------|
| `test_plugin_panel_new` | 不变（直接用 PluginPanel） |
| `test_plugin_panel_move_cursor` | 直接操作 `panel.installed_list.move_cursor()` |
| `test_plugin_panel_tab_cycles_views` | 直接调用 `panel.view.next()` + `panel.sync_current_view_items()` |
| `test_plugin_panel_shift_tab_cycles_back` | 直接调用 `panel.view.prev()` |
| `test_plugin_panel_close` | 删除（测试的是 App::global_panels.close()，非面板逻辑） |
| `test_plugin_panel_request_cancel_delete` | 直接设置/检查 `panel.confirm_delete` |
| `test_plugin_panel_toggle_enabled` | 直接操作 `panel.entries[idx].enabled` |
| `test_plugin_panel_errors_view` | 直接检查 `panel.current_list_len()` |

重写后的测试：
- 不再需要 `App::new_headless()` 构造——更快、更独立
- 直接测试 `PluginPanel` 方法——更接近实际使用方式（handler 也是直接操作）
- 预计从 193 行缩减到约 80 行

### Step 3：验证构建和测试

```bash
cargo build -p peri-tui
cargo test -p peri-tui --lib
cargo clippy -p peri-tui -- -D warnings
```

### Step 4：确认无残留引用

```bash
grep -rn "plugin_panel_move_up\|plugin_panel_move_down\|plugin_panel_tab\|..." peri-tui/src/
```

确保 44 个方法名在代码库中完全消失。

## 风险评估

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| 某个方法被遗漏调用 | 极低 | 编译失败 | grep 验证 |
| 测试重写逻辑错误 | 低 | 测试失败 | 保持测试覆盖相同场景 |
| 破坏外部依赖 | 无 | 无 | 所有方法均为 pub(crate) 级别 |

## 影响范围

- **修改**：2 个文件（`mod.rs` + `plugin_panel_test.rs`）
- **删除代码**：约 470 行（44 个方法）+ 约 113 行（旧测试）≈ 580 行
- **新增代码**：约 80 行（新测试）
- **净减少**：约 500 行

## 预估工作量

~15 分钟（机械性删除 + 测试重写 + 验证）

## 不在范围内

- `panel_plugin.rs` 中的 4 个方法（确实在使用）
- `PluginPanel` 的 `pub` 字段可见性调整（独立优化）
- handler 内部逻辑重构
