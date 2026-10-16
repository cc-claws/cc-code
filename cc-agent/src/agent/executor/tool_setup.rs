use std::{mem::ManuallyDrop, sync::Arc};

use crate::tools::{BaseTool, ToolContent};

/// 将 Box<dyn BaseTool> 转换为 Arc<dyn BaseTool>
///
/// Rust 标准库不直接支持 `Box<dyn Trait>` → `Arc<dyn Trait>` 转换。
/// 通过一个中间 wrapper struct 持有 `ManuallyDrop<Box<dyn BaseTool>>`，
/// 实现 `BaseTool` trait 透传所有调用，并在最后一个 Arc 释放时析构内部工具。
pub(crate) fn box_to_arc(tool: Box<dyn BaseTool>) -> Arc<dyn BaseTool> {
    struct ToolWrapper(ManuallyDrop<Box<dyn BaseTool>>);

    impl Drop for ToolWrapper {
        fn drop(&mut self) {
            // SAFETY: wrapper 独占内部 Box，且只有此处负责释放，不会重复析构。
            unsafe { ManuallyDrop::drop(&mut self.0) };
        }
    }

    #[async_trait::async_trait]
    impl BaseTool for ToolWrapper {
        fn name(&self) -> &str {
            self.0.name()
        }
        fn description(&self) -> &str {
            self.0.description()
        }
        fn parameters(&self) -> serde_json::Value {
            self.0.parameters()
        }
        async fn invoke(
            &self,
            input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            self.0.invoke(input).await
        }
        async fn invoke_content(
            &self,
            input: serde_json::Value,
        ) -> Result<ToolContent, Box<dyn std::error::Error + Send + Sync>> {
            self.0.invoke_content(input).await
        }
    }

    Arc::new(ToolWrapper(ManuallyDrop::new(tool)))
}

#[cfg(test)]
#[path = "tool_setup_test.rs"]
mod tests;
