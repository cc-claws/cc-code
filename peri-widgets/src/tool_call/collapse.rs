pub const READ_ONLY_TOOLS: &[&str] = &["Read", "Glob", "Grep", "AskUserQuestion"];

pub const MAX_RESULT_LINES: usize = 20;

pub fn should_collapse_by_default(tool_name: &str) -> bool {
    READ_ONLY_TOOLS.contains(&tool_name)
}

pub fn truncate_result(lines: &[String], max: usize) -> (Vec<String>, Option<usize>) {
    if lines.len() <= max {
        return (lines.to_vec(), None);
    }
    (lines[..max].to_vec(), Some(lines.len() - max))
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("collapse_test.rs");
}
