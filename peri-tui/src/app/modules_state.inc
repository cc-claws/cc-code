// ── State Management ────────────────────────────────────────────────────────
mod global_ui_state;
mod service_registry;
pub use global_ui_state::GlobalUiState;
pub use service_registry::ServiceRegistry;

mod session_manager;
pub use session_manager::SessionManager;

mod ui_state;
pub use ui_state::UiState;

mod at_mention;
pub use at_mention::AtMentionState;

mod message_state;
pub use message_state::MessageState;
