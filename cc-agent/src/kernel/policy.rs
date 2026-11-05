//! 内核策略 trait：`should_hide` 等执行期逻辑只依赖此抽象，不依赖具体内核类型。

use crate::agent::react::Reasoning;
use crate::tools::ToolDefinition;

pub trait KernelPolicy: Send + Sync {
    /// 方言名 → 规范名；返回 `&'static str` 以便调用方跨 await 持有。
    fn canonical_name(&self, name: &str) -> Option<&'static str>;
    fn dialect_name(&self, canonical: &str) -> Option<&'static str>;
    fn normalize_reasoning(&self, reasoning: Reasoning) -> Reasoning;
    fn translate_definitions(&self, defs: Vec<ToolDefinition>) -> Vec<ToolDefinition>;
    fn governs(&self, canonical: &str) -> bool;
    fn is_exposed(&self, canonical: &str) -> bool;
    fn is_meta_tool(&self, canonical: &str) -> bool;
    fn exposes_tool_search(&self) -> bool;
}

impl KernelPolicy for super::Kernel {
    fn canonical_name(&self, name: &str) -> Option<&'static str> {
        super::Kernel::canonical_name(self, name)
    }

    fn dialect_name(&self, canonical: &str) -> Option<&'static str> {
        super::Kernel::dialect_name(self, canonical)
    }

    fn normalize_reasoning(&self, reasoning: Reasoning) -> Reasoning {
        super::Kernel::normalize_reasoning(self, reasoning)
    }

    fn translate_definitions(&self, defs: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
        super::Kernel::translate_definitions(self, defs)
    }

    fn governs(&self, canonical: &str) -> bool {
        super::Kernel::governs(self, canonical)
    }

    fn is_exposed(&self, canonical: &str) -> bool {
        super::Kernel::is_exposed(self, canonical)
    }

    fn is_meta_tool(&self, canonical: &str) -> bool {
        super::Kernel::is_meta_tool(self, canonical)
    }

    fn exposes_tool_search(&self) -> bool {
        super::Kernel::exposes_tool_search(self)
    }
}
