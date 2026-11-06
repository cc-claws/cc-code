use super::slash_command_ranges;

fn ranges(text: &str) -> Vec<String> {
    slash_command_ranges(text)
        .into_iter()
        .map(|(s, e)| text.chars().skip(s).take(e - s).collect())
        .collect()
}

#[test]
fn test_bare_slash_before_cjk_is_command() {
    assert_eq!(ranges("按 / 输入命令，Tab 补全"), vec!["/"]);
}

#[test]
fn test_bare_slash_between_keys_is_not_command() {
    assert_eq!(ranges("Ctrl+P / Alt+P 打开命令面板"), Vec::<String>::new());
}

#[test]
fn test_leading_command_is_matched() {
    assert_eq!(ranges("/clear 清空当前对话"), vec!["/clear"]);
    assert_eq!(ranges("/compact 压缩上下文节省 token"), vec!["/compact"]);
}

#[test]
fn test_command_after_fullwidth_punctuation_is_matched() {
    assert_eq!(
        ranges("Ctrl+B 后台化前台任务，/resume 浏览与切换历史会话"),
        vec!["/resume"]
    );
}

#[test]
fn test_path_is_not_command() {
    assert_eq!(
        ranges("在 .claude/skills/ 中添加自定义 Skills"),
        Vec::<String>::new()
    );
    assert_eq!(ranges("/home/user/x 不存在"), Vec::<String>::new());
    assert_eq!(ranges("C:/Users/adim/.cc-code"), Vec::<String>::new());
}

#[test]
fn test_url_is_not_command() {
    assert_eq!(ranges("https://api.deepseek.com"), Vec::<String>::new());
}

#[test]
fn test_help_list_line_is_matched() {
    assert_eq!(ranges("  /help        Show help"), vec!["/help"]);
}

#[test]
fn test_multiple_commands_in_one_hint() {
    assert_eq!(
        ranges("用 /model 切换模型，/config 打开配置"),
        vec!["/model", "/config"]
    );
}
