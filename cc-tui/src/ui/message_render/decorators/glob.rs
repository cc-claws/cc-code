/// 生成 Glob 工具输出结果摘要。
pub fn glob_summary(content: &str) -> Option<String> {
    if content.is_empty() {
        return None;
    }
    let count = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    Some(format!("Found {} files", count))
}

#[cfg(test)]
#[path = "glob_test.rs"]
mod glob_test;
