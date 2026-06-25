# 实施计划：消除 `include!` `.inc` 文件——`app/mod.rs` 模块组织标准化

## 背景

`peri-tui/src/app/mod.rs` 使用 4 个 `include!` 宏引入 `.inc` 文件来按类别声��子模块：

```
include!("modules_panels.inc");   // 23 个面板模块
include!("modules_state.inc");    // 6 个状态模块
include!("modules_agent.inc");    // 18 个 agent 模块
include!("modules_system.inc");   // 6 个系统模块
```

### 问题

1. **IDE 导航断裂**：rust-analyzer 无法正确解析 `include!` 宏中的 `mod`/`pub mod` 声明，跳转到模块定义时经常失效
2. **非惯用 Rust**：`.inc` 文件不是 Rust 生态的标准实践，新贡献者需要额外学习成本
3. **全项目唯一**：代码库中仅此一处使用 `.inc` 文件，是孤例模式
4. **工具链兼容差**：`rustfmt`、`cargo doc`、IDE 重构工具对 `include!` 的支持不如标准 `mod`

### 当前 .inc 文件内容摘要

| 文件 | 行数 | 声明数 | 功能 |
|------|------|--------|------|
| `modules_panels.inc` | 27 行 | 23 个 mod | 面板 UI 组件（15 pub + 8 private） |
| `modules_state.inc` | 18 行 | 6 个 mod + 5 个 pub use | 状态管理 |
| `modules_agent.inc` | 22 行 | 18 个 mod + 3 个 pub use | Agent 通信/操作 |
| `modules_system.inc` | 15 行 | 6 个 mod + 3 个 pub use | 系统基础设施 |

**总计**：4 个文件、82 行、53 个 mod 声明 + 11 个 pub use。

## 方案选择

### 方案 A：内联回 mod.rs（推荐 ✅）

将 `.inc` 文件内容直接内联回 `mod.rs`，保留分类注释。

**优点**：
- 改动最小（删除 4 文件，修改 1 文件）
- 与 Rust 标准实践完全一致
- IDE/rust-analyzer 完美支持
- 82 行不会让 mod.rs 膨胀（当前 mod.rs 569 行，合并后 ~651 行）

**缺点**：
- mod.rs 前 82 行全是 mod 声明，略长但可接受（有分类注释分隔）

### 方案 B：引入子模块中间层

创建 `panels/mod.rs`、`state/mod.rs`、`agent_comm/mod.rs`、`system/mod.rs` 作为中间层，将声明下沉。

**优点**：
- 更强的分类封装

**缺点**：
- 大量文件需要移动或重新导出
- 修改所有 `use crate::app::xxx` 的引用路径
- 53 个模块的路径变更影响面巨大（不划算）
- 破坏现有 `use crate::app::agent_ops::xxx` 等路径

### 方案 C：按类别拆分为多个 mod 声明文件

创建 `modules_panels.rs`（而非 `.inc`），用 `mod modules_panels;` 引入。

**缺点**：
- Rust 的 `mod modules_panels;` 会创建新的模块命名空间（`crate::app::modules_panels::agent_panel`），改变所有引用路径
- 除非所有声明都是 re-export，但那等同于方案 B

### 决策

**选择方案 A**：内联回 mod.rs。理由：
- 最小改动、最低风险
- 82 行模块声明是可接受的（大多数 Rust 项目直接这样做）
- 不改变任何模块路径，零破坏性
- 解决了所有 IDE 和工具链问题

## 实施步骤

### Step 1：内联 .inc 文件内容到 mod.rs

**修改文件**：`peri-tui/src/app/mod.rs`
**删除文件**：`modules_panels.inc`、`modules_state.inc`、`modules_agent.inc`、`modules_system.inc`

将 4 个 `include!` 宏替换为内联的模块声明，保留分类注释分隔符：

```rust
// ── Panel Modules ────────────────────────────────────────────────────────────
pub mod agent_panel;
pub mod config_panel;
pub mod hooks_panel;
pub mod login_panel;
pub mod memory_panel;
pub mod model_panel;
pub mod plugin_panel;
pub mod setup_wizard;
pub mod status_panel;
pub mod tasks_panel;
pub mod mcp_panel;
pub mod panel_component;
pub mod panel_list;
pub mod panel_manager;
pub mod panel_plugin;

// Panel private modules
mod panel_agent;
mod panel_config;
mod panel_hooks;
mod panel_login;
mod panel_memory;
mod panel_model;
mod panel_ops;
mod panel_status;

// ── State Management ─────────────────────────────────────────────────────────
mod global_ui_state;
mod service_registry;
pub use global_ui_state::GlobalUiState;
pub use service_registry::ServiceRegistry;

mod session_manager;
pub use session_manager::SessionManager;

mod ui_state;
pub use ui_state::UiState;

pub(crate) mod at_mention;
pub use at_mention::AtMentionState;

mod message_state;
pub use message_state::MessageState;

// ── Agent Communication ──────────────────────────────────────────────────────
mod agent_comm;
mod agent_compact;
mod agent_events_bg;
mod agent_events_oauth;
mod agent_events_plugin;
mod agent_ops;
mod agent_ops_interaction;
mod agent_render;
mod agent_submit;
mod ask_user_ops;
mod ask_user_prompt;
pub use ask_user_prompt::AskUserBatchPrompt;
mod cron_ops;
mod cron_state;
mod hint_ops;
mod history_ops;
mod history_persistence;
mod hitl_ops;
mod hitl_prompt;
pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};

// ── System Infrastructure ────────────────────────────────────────────────────
mod chat_session;
mod command_system;
mod session_metadata;
pub use chat_session::ChatSession;
#[cfg(test)]
pub(crate) use chat_session::RunningBgAgent;
pub use command_system::CommandSystem;
pub use session_metadata::SessionMetadata;

mod langfuse_state;
mod oauth_prompt;
pub use oauth_prompt::OAuthPrompt;
mod thread_ops;

// ── Other Modules ─────────────────────────────────────────────────────────────
pub mod agent;
pub mod events;
pub mod message_pipeline;
mod provider;
pub mod text_selection;
pub mod tool_display;
```

### Step 2：验证构建

```bash
cargo build -p peri-tui
cargo test -p peri-tui --lib
```

**验证检查点**：
- 编译通过（零路径变更，理论上零风险）
- 所有测试通过
- 无 clippy 警告

### Step 3：清理

删除 4 个 `.inc` 文件：
```bash
rm peri-tui/src/app/modules_panels.inc
rm peri-tui/src/app/modules_state.inc
rm peri-tui/src/app/modules_agent.inc
rm peri-tui/src/app/modules_system.inc
```

## 风险评估

| 风险 | 可能性 | 影响 | 缓解 |
|------|--------|------|------|
| 编译失败 | 极低 | 低 | 纯文本替换，无路径变更 |
| 测试失败 | 极低 | 低 | 模块可见性不变 |
| 代码审查摩擦 | 无 | 无 | 标准 Rust 实践 |

## 影响范围

- **修改**：1 个文件（`peri-tui/src/app/mod.rs`）
- **删除**：4 个文件（`.inc` 文件）
- **移动**：0 个文件
- **路径变更**：0 条

## 预估工作量

~10 分钟（机械性替换 + 构建验证）
