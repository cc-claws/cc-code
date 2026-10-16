/// 解析 AskUserQuestion 的多问题应答格式：`[问: H]\n回答: V\n\n[问: H2]\n回答: V2`
/// 返回格式化后的条目列表（例如 `["H → V", "H2 → V2"]`）。
pub fn parse_ask_user_qa_items(content: &str) -> Vec<String> {
    let mut items = Vec::new();
    if content.is_empty() {
        return items;
    }

    for block in content.split("\n\n") {
        let mut header = String::new();
        let mut answer = String::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("[问: ") {
                header = rest.trim_end_matches(']').to_string();
            } else if let Some(a) = line.strip_prefix("回答: ") {
                answer = a.to_string();
            }
        }
        header = header.replace(['\n', '\r'], " ");
        answer = answer.replace(['\n', '\r'], " ");
        let text = if !header.is_empty() {
            format!("{} → {}", header, answer)
        } else if !answer.is_empty() {
            answer
        } else {
            block.lines().collect::<Vec<_>>().join(" ")
        };
        if !text.is_empty() {
            items.push(text);
        }
    }

    items
}

#[cfg(test)]
#[path = "ask_user_test.rs"]
mod ask_user_test;
