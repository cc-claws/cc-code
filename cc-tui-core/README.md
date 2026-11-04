# cc-tui-core

会话核心与展示模型层（2026-11 拆分新增）。

## 职责

| 模块 | 内容 |
| :--- | :--- |
| `src/config/`、`src/i18n/`、`src/export/`、`src/shell_history.rs` | 配置类型、国际化、会话导出、输入/shell 历史 |
| `src/theme/`、`src/markdown/`、`src/message_view/`、`src/terminal_theme/` | 主题/调色板、markdown 解析、`MessageViewModel`、终端主题跟随 |
| `src/tool_display.rs`、`src/shell_notification.rs`、`src/text_width.rs` | 工具参数格式化、后台通知展示、显示列宽截断 |
| `src/display_flags.rs`、`src/ui_state.rs` | `DisplayFlags`、`UiStateCore`（约 40 个纯状态字段） |

## 依赖方向

依赖 `cc-agent`、`cc-acp`、`cc-terminal`、`cc-tools`、`cc-widgets`。
**不得依赖 `cc-tui`**，否则出现循环依赖。

## 垫片：路径 ≠ 物理位置

`cc-tui` 侧同名重导出：

- `ui/mod.rs`：`pub use cc_tui_core::{theme, markdown, message_view};`
- `app/mod.rs`：`pub use cc_tui_core::tool_display;`
- `lib.rs`：`pub use cc_tui_core::{config, export, i18n, shell_history, terminal_theme};`
- `ui/render_thread.rs`：`pub use cc_tui_core::display_flags::DisplayFlags;`

因此 `crate::ui::message_view::*`、`crate::app::tool_display::*`、`crate::i18n::*` 等路径仍有效。

## UiState 类型归属

纯状态在 `UiStateCore`（本 crate）；`TextArea`、`ratatui::Rect` 命中几何、`ScreenSnapshot`、
`text_selection`、`at_mention` 属表现层，留在 `cc-tui::UiState` 侧车，通过
`Deref` / `DerefMut` 委托，`ui.scroll_offset` 等访问保持原样。

## 维护注意

- 本 crate 可依赖 ratatui / cc-widgets（展示模型含渲染缓存），但不得依赖 `cc-tui`。
- 新增共享展示类型优先放这里；判断口诀见根目录 `CLAUDE.md`「模块归属速查」。
- 单测：`cargo test -p cc-tui-core -j 1`。
- 暂缓的 `PanelController` / `PanelViewRenderer` 拆分与真实收益评估见 `docs/designs/2026-11-04-rust-crate-architecture-split-plan.md` §9.8 / §9.12。
