//! pi 的 find.limit 在内核边界执行，共享 Glob 的搜索与安全上限保持原样。

use serde_json::{json, Value};

use crate::tools::{BaseTool, FileSnapshot};

pub(super) struct PiFindTool<'a>(pub &'a dyn BaseTool);

#[async_trait::async_trait]
impl BaseTool for PiFindTool<'_> {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn description(&self) -> &str {
        self.0.description()
    }

    fn parameters(&self) -> Value {
        let mut schema = self.0.parameters();
        schema["properties"]["limit"] = json!({
            "type": "number",
            "description": "Maximum files to return (default 1000). Rounded down, with a minimum of 1. The underlying search returns at most 1000 files."
        });
        schema
    }

    fn file_snapshot(&self, input: &Value) -> Option<FileSnapshot> {
        self.0.file_snapshot(input)
    }

    async fn invoke(
        &self,
        mut input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let limit = match input.get("limit") {
            None => 1000,
            Some(value) => value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("limit must be a finite number")?
                .floor()
                .clamp(1.0, 1000.0) as usize,
        };
        if let Some(map) = input.as_object_mut() {
            map.remove("limit");
        }
        let output = self.0.invoke(input).await?;
        Ok(limit_output(output, limit))
    }
}

fn limit_output(output: String, limit: usize) -> String {
    // Glob 的路径列表后以空行分隔警告/落盘提示；这些提示不能算作文件或被丢弃。
    let (paths, footer) = output
        .split_once("\n\n")
        .map(|(paths, footer)| (paths, Some(footer)))
        .unwrap_or((&output, None));
    if paths.lines().count() <= limit {
        return output;
    }
    let mut limited = paths.lines().take(limit).collect::<Vec<_>>().join("\n");
    limited.push_str(&format!(
        "\n\n[Showing first {limit} files; find limit reached.]"
    ));
    if let Some(footer) = footer {
        limited.push_str("\n\n");
        limited.push_str(footer);
    }
    limited
}

#[cfg(test)]
#[path = "find_test.rs"]
mod tests;
