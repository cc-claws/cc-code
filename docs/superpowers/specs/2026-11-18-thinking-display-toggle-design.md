# 详细模式思考展示：`∴` 内联 + 可关闭配置

## 背景

Ctrl+O 进入详细模式后，思考内容当前渲染成两行：

```
  Thought for 2s
  ⎿  查看 watch 的结果。
```

摘要行（`Thought for 2s`）与正文首行（`⎿  …`）重复占位，且摘要行在详细模式里没有任何额外信息——耗时下方就是同一段思考的正文。用户期望压成一行：

```
∴ 查看 watch 的结果。
```

同时需要给用户一个开关：完全不想看思考正文的人，关闭后详细模式不再渲染思考，普通模式的摘要行也不再挂 `(ctrl+o to expand)` 小尾巴（展开无内容，提示会误导）。

## 配置项

`AppConfig`（`cc-acp/src/provider/config.rs`）新增：

```rust
/// 详细模式（Ctrl+O）展开后是否展示思考正文（缺省 = 展示）
#[serde(default, skip_serializing_if = "Option::is_none", alias = "thinkingUi")]
pub thinking_ui: Option<bool>,
```

- 方法 `thinking_ui_enabled() -> bool`，`unwrap_or(true)`（缺省展示，与 `hooks_ui_enabled` 同口径）。
- `merge_overrides` 增加 `is_some()` 覆盖分支。
- 命名对齐现有 `hooks_ui`（同为「UI 是否展示某类数据」）。**不复用 `thinking` 字段**：`ThinkingConfig` 是推理参数（enabled/budget_tokens/effort/max_tokens），传给 LLM，与界面展示无关。

## 渲染链路

`render_view_model_with_flags` 增加参数 `show_thinking: bool`（紧跟 `hook_rows_visible` 之后）。调用链：

```
AppConfig.thinking_ui_enabled()
  → ChatSession（App 初始化）
  → RenderTask.show_thinking
  → render_one → render_view_model_with_flags
```

运行时切换走新事件 `RenderEvent::ToggleThinking(bool)`（对齐 `ToggleHookRows`）：配置面板保存后同步 `RenderTask.show_thinking` 并补发事件，不重启即生效。

### 渲染键

新增 `thinking_sensitive(vm)`：`AssistantBubble`（可能含 Reasoning block）与 `ToolCallGroup`（`thinking.is_some()`）为真，其余为假。`render_key` 中对敏感消息混入 `show_thinking`，保证开关切换后缓存正确失效、不返回旧的思考行。

## 渲染行为

两个渲染点行为一致：`AssistantBubble` 的 `ContentBlockView::Reasoning` 分支与 `ToolCallGroup` 的 `thinking` 分支。

### 详细模式 + 展示思考（默认）

- **不渲染摘要行**（`Thought for 2s` 删除）
- 思考正文首行前缀 `∴ `，后续行悬挂缩进 2 格（`∴` 与空格各占 1 列，合计 2 列，与正文 `● ` / `  ` 口径一致）
- 全部文字用 `theme::thinking_body()`（#666666）
- 正文结束补一空行（与现状一致，保持段落间距）
- 无内容时（模型只回耗时）保留 `⎿  (No content)` 口径 → 改为 `∴ (No content)`

### 详细模式 + 关闭展示

思考块**整段不渲染**：摘要行、正文、结尾空行都不输出。

### 普通模式（不受详细模式开关影响，始终显示摘要行）

- `show_thinking == true`：`  Thought for 8s, ran 1 shell command (ctrl+o to expand)`
- `show_thinking == false`：`  Thought for 8s, ran 1 shell command`（**去掉尾巴**）
- 活跃态不变：`∴ Thinking for N chars…`

## 配置面板

`ROW_THINKING_DISPLAY` 插在 `ROW_HOOKS_DISPLAY` 之后（值 7），其后 `ROW_DETAIL_RENDER_STRATEGY` 等全部 +1，`ROW_COUNT` 14 → 15。需同步：

- `SCREEN_LAYOUT`（值行 + 描述行各一条）
- `next_editable_row` 的可编辑列表
- `is_text_row`（不涉及，是开关行）
- `field_label_key`
- 面板渲染分支（照抄 `ROW_HOOKS_DISPLAY` 的开关样式）
- `from_config` 初始化 `buf_thinking_display`
- `apply_edit` 写回 `cfg.config.thinking_ui = Some(...)`
- `cycle_thinking_display()`
- 键盘处理三处（Up/Down/Enter/Space 分支）

i18n（`cc-tui-core/locales/{zh-CN,en}/main.ftl`）：

- `config-field-thinking-display` = `思考展示` / `Thinking Display`
- `config-desc-thinking-display` = `（开/关 — 详细模式展开后是否显示思考正文）` / `(ON/OFF — show thinking body when expanded)`

## 测试

需更新（现有断言假设详细模式含 `Thought for Ns`）：

- `cc-tui/src/ui/message_render/message_render_test.rs:4216`、`:4274`
- `cc-tui/src/ui/message_render/bash_file_inspect_test.rs:286`、`:307`
- `cc-tui/src/ui/render_thread_test.rs:830` 附近

新增：

- `show_thinking=false` + 详细模式 → 无 `Thought for`、无思考正文
- `show_thinking=false` + 普通模式 → 无 `(ctrl+o to expand)`
- `show_thinking=true` + 详细模式 → 首行 `∴ ` 前缀、无 `Thought for` 摘要行
- config panel：新行可编辑、保存回读、`SCREEN_LAYOUT` 行数自洽

所有 `render_view_model_with_flags` 调用点（4 处非测试 + 12 处测试）补 `true`（保持既有断言语义）。

## 影响面

| crate | 改动 |
|---|---|
| `cc-acp` | `AppConfig` 新字段 + 方法 + merge |
| `cc-tui-core` | i18n ftl 两条 |
| `cc-tui` | 渲染参数贯通、两个渲染点、渲染键、配置面板、事件、测试 |

约 20 个文件。

## 不做

- 不动 `ThinkingConfig`（推理参数）
- 不动 `Palette`（配色字段受 parity 测试锁定）
- 不改普通模式的活跃态样式
- 不改 `hooks_ui` 既有行为
