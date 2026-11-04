# cc-terminal

跨平台终端底层 crate（2026-11 拆分新增；实施评审后由 `cc-term` 统一改名为 `cc-terminal`）。

## 职责

| 模块 | 内容 |
| :--- | :--- |
| `src/terminal_backend/` | 双缓冲、字形列宽探测、视口兼容、OSC 8 超链接注册 |
| `src/clipboard/` | arboard / OSC 52 / 图片粘贴 / stderr 抑制 |
| `src/conpty.rs` / `conpty_host.rs` | Windows 伪控制台 |
| `src/terminal_color.rs` / `terminal_title.rs` / `keyboard_modes.rs` | 色深、OSC 0 标题、键盘增强模式 |
| `src/shell_exec.rs` | shell 命令执行与流式输出 |

## 依赖方向

依赖 `cc-agent`、`cc-tools`。**不得依赖** `cc-tui` / `cc-tui-core`。

## 垫片：路径 ≠ 物理位置

`cc-tui/src/lib.rs` 以 `pub use cc_terminal::{...}` 同名重导出，
`crate::terminal_backend::*`、`crate::clipboard::*`、`crate::conpty::*` 等路径仍有效。

**未下沉的两处**：`ui/osc8.rs`（依赖 `ui::message_render` / `ui::render_thread`）保留在 `cc-tui`；
`terminal_title` 的 LLM 主题生成（依赖 `cc_acp::provider::LlmProvider`）保留在
`cc-tui/src/terminal_title.rs`。二者下沉会造成底层反向依赖上层。

## 维护注意

- 模块名与 `cc-tui` 保持完全一致，内部 `crate::terminal_color` 等引用才能零改动。
- `pub(crate)` 项若被 `cc-tui` 引用必须提升为 `pub`（跨 crate 不可见）。
- 单测：`cargo test -p cc-terminal -j 1`。
- 拆分依据见 `docs/designs/2026-11-04-rust-crate-architecture-split-plan.md` §4.2 / §9.3 / §9.4 / §9.9。
