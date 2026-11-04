//! 当前视口独立保存的显示选项，随消息快照一起提交。
//!
//! 与渲染线程的事件类型解耦：这是纯状态数据，供会话层与渲染层共同使用。

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DisplayFlags {
    pub detail_mode: bool,
    pub diff_visible: bool,
    pub show_tool_messages: bool,
}
