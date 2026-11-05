use async_trait::async_trait;
use cc_agent::kernel::MutationForm;
use cc_agent::{agent::state::State, middleware::r#trait::Middleware, tools::BaseTool};

use crate::tools::{EditFileTool, GlobFilesTool, GrepTool, ReadFileTool, WriteFileTool};

pub struct FilesystemMiddleware {
    /// Edit 的变更形态，由内核 ToolSpec.mutation_form 决定（默认 SingleReplace）。
    form: MutationForm,
}

impl FilesystemMiddleware {
    pub fn new() -> Self {
        Self {
            form: MutationForm::default(),
        }
    }

    pub fn with_form(mut self, form: MutationForm) -> Self {
        self.form = form;
        self
    }

    pub fn build_tools(cwd: &str, form: MutationForm) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(ReadFileTool::new(cwd)),
            Box::new(WriteFileTool::new(cwd)),
            Box::new(EditFileTool::new(cwd).with_form(form)),
            Box::new(GlobFilesTool::new(cwd)),
            Box::new(GrepTool::new(cwd)),
        ]
    }

    pub fn tool_names() -> Vec<&'static str> {
        vec!["Read", "Write", "Edit", "Glob", "Grep"]
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
        Self::build_tools(cwd, self.form)
    }

    fn name(&self) -> &str {
        "FilesystemMiddleware"
    }
}
