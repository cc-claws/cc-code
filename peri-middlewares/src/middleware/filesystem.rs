use async_trait::async_trait;
use peri_agent::{agent::state::State, middleware::r#trait::Middleware, tools::BaseTool};

use crate::tools::{
    EditFileTool, FolderOperationsTool, GlobFilesTool, GrepTool, LineEditTool, ReadFileTool,
    WriteFileTool,
};

pub struct FilesystemMiddleware {
    line_edit_mode: bool,
}

impl FilesystemMiddleware {
    pub fn new() -> Self {
        Self {
            line_edit_mode: false,
        }
    }

    pub fn with_line_edit_mode(mut self, enabled: bool) -> Self {
        self.line_edit_mode = enabled;
        self
    }

    pub fn build_tools(cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools_with_mode(cwd, false)
    }

    pub fn build_tools_with_mode(cwd: &str, line_edit_mode: bool) -> Vec<Box<dyn BaseTool>> {
        let edit_tool: Box<dyn BaseTool> = if line_edit_mode {
            Box::new(LineEditTool::new(cwd))
        } else {
            Box::new(EditFileTool::new(cwd))
        };

        vec![
            Box::new(ReadFileTool::new(cwd)),
            Box::new(WriteFileTool::new(cwd)),
            edit_tool,
            Box::new(GlobFilesTool::new(cwd)),
            Box::new(GrepTool::new(cwd)),
            Box::new(FolderOperationsTool::new(cwd)),
        ]
    }

    pub fn tool_names() -> Vec<&'static str> {
        vec!["Read", "Write", "Edit", "Glob", "Grep", "folder_operations"]
    }

    pub fn tool_names_line_edit() -> Vec<&'static str> {
        vec![
            "Read",
            "Write",
            "LineEdit",
            "Glob",
            "Grep",
            "folder_operations",
        ]
    }
}

impl Default for FilesystemMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for FilesystemMiddleware {
    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools_with_mode(cwd, self.line_edit_mode)
    }

    fn name(&self) -> &str {
        "FilesystemMiddleware"
    }
}
