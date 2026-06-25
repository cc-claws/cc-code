> 归档于 2026-05-27，原路径 spec/issues/2026-05-27-ctrl-o-toggle-inline-diff.md

# Ctrl+O 开关 Write/Edit 工具的内联 diff 显示，默认关闭并提供 config 选项

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-27
**修复日期**：2026-05-27

## 问题描述

Write/Edit 工具结果中已实现了内联 diff 渲染（`5fdfef4`），但目前 diff 始终显示，无法关闭。需要提供快捷键切换 diff 显隐，默认关闭，并由用户通过 `~/.peri/settings.json` 配置默认行为。

## 期望功能

1. **快捷键**：`Ctrl+O` 切换消息流中 Write/Edit 工具结果的内联 diff 显隐
2. **默认关闭**：diff 默认不显示
3. **持久化配置**：`~/.peri/settings.json` 中增加选项控制默认值

## 快捷键冲突说明

`Ctrl+O` 当前在 OAuth 流程中用于「在浏览器中打开链接」。该功能仅在 OAuth 弹窗激活时触发（`popups/oauth.rs:39`、`status_bar.rs:363`）。非 OAuth 场景下 `Ctrl+O` 应切换 diff 显隐。

## 配置设计

`~/.peri/settings.json` 中 `config` 段新增：

```json
{
  "config": {
    "diffEnabled": false
  }
}
```

- 字段名 `diffEnabled`（布尔，默认 `false`）
- 启动时读取，设置 diff 初始显隐状态
- 运行时 `Ctrl+O` 切换不写回配置文件（仅会话级切换）

## 涉及文件

- `peri-tui/src/app/ui_state.rs` —— 新增 `diff_visible: bool` 状态字段
- `peri-tui/src/app/thread_ops.rs` —— 新增 `toggle_diff()` 方法
- `peri-tui/src/event/keyboard.rs` —— 绑定 `Ctrl+O` 到 diff 切换（非 OAuth 场景）
- `peri-tui/src/ui/message_view/mod.rs` —— 根据 `diff_visible` 决定是否渲染 diff_lines
- `peri-tui/src/ui/message_render.rs` —— diff 渲染逻辑
- `peri-acp/src/provider/config.rs` —— `AppConfig` 新增 `diff_enabled: bool` 字段

## 实现说明

### 数据流

```
settings.json diffEnabled ──→ AppConfig.diff_enabled ──→ UiState.diff_visible (初始值)
                                                               │
Ctrl+O ──→ shortcuts.rs (跳过 OAuth) ──→ toggle_diff() ────────┤
                                                               │
                        RenderEvent::ToggleDiff ──→ RenderTask.diff_visible
                                                               │
                        render_view_model(vm, .., diff_visible) ──→ 跳过 diff_lines
```

### 变更文件

| 文件 | 变更 |
|------|------|
| `peri-acp/src/provider/config.rs` | `AppConfig` 新增 `diff_enabled: bool`（`#[serde(default)]`） |
| `peri-tui/src/app/ui_state.rs` | 新增 `diff_visible: bool`，`new()` 增加参数 |
| `peri-tui/src/app/chat_session.rs` | `new()` 接受 `diff_enabled` 参数 |
| `peri-tui/src/app/mod.rs` | 从 config 读取 `diff_enabled`，初始化 session |
| `peri-tui/src/app/thread_ops.rs` | 新增 `toggle_diff()` |
| `peri-tui/src/app/panel_ops.rs` | 测试用 `UiState::new()` 签名更新 |
| `peri-tui/src/event/keyboard/shortcuts.rs` | `Ctrl+O` 快捷键绑定（OAuth 激活时跳过） |
| `peri-tui/src/ui/render_thread.rs` | 新增 `ToggleDiff` 事件、`diff_visible` 字段、hash 清空强制重渲染 |
| `peri-tui/src/ui/message_render.rs` | `render_view_model()` 接受 `diff_visible` 参数，条件跳过 diff_lines |
| `peri-tui/src/app/config_panel.rs` | 新增 `ROW_DIFF=4`（10 行布局）、`buf_diff`、`cycle_diff()` |
| `peri-tui/src/ui/main_ui/panels/config.rs` | `ROW_DIFF` 渲染（ON/OFF 开关） |
| `peri-tui/locales/en/main.ftl` | `config-field-diff`、`config-desc-diff` |
| `peri-tui/locales/zh-CN/main.ftl` | `config-field-diff`、`config-desc-diff` |
| `peri-tui/src/app/config_panel_test.rs` | 新增 `test_config_panel_cycle_diff`、`test_config_panel_apply_edit_diff_enabled` |
| `peri-tui/src/ui/headless_test.rs` | `render_view_model` 调用更新（5 处） |

### /config 面板布局

```
ROW_GENERAL_HEADER  = 0  General
ROW_AUTOCOMPACT     = 1  Autocompact       [ON]  OFF
ROW_THRESHOLD       = 2  Compact Threshold  85
ROW_LANGUAGE        = 3  Language          English  [简体中文]
ROW_DIFF            = 4  Inline Diff       ON  [OFF]          ← 新增
ROW_PROACTIVENESS   = 5  Proactiveness     low  [medium]  high
ROW_SEPARATOR       = 6  (空行)
ROW_OVERRIDES_HEADER= 7  Prompt Overrides
ROW_PERSONA         = 8  Persona           ...
ROW_TONE            = 9  Tone              ...
```
