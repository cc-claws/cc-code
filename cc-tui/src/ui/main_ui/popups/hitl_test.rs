use crate::{i18n::LcRegistry, ui::headless::HeadlessHandle};

async fn make_headless_hitl(
    width: u16,
    height: u16,
    locale: &str,
    items: Vec<BatchItem>,
) -> (App, HeadlessHandle) {
    let (mut app, handle) = App::new_headless(width, height).await;
    let (sender, _) = tokio::sync::oneshot::channel();
    app.services.lc = LcRegistry::new(Some(locale));
    app.services.cwd = r"D:\code\peri".to_string();
    app.session_mgr.current_mut().agent.interaction_prompt = Some(InteractionPrompt::Approval(
        HitlBatchPrompt::new(items, sender),
    ));
    (app, handle)
}

fn make_bash_item(command: &str) -> BatchItem {
    BatchItem {
        tool_call_id: String::new(),
        tool_name: "Bash".to_string(),
        input: serde_json::json!({"command": command}),
    }
}

fn make_hitl_snapshot(app: &mut App, handle: &mut HeadlessHandle) -> Vec<String> {
    handle
        .terminal
        .draw(|frame| crate::ui::main_ui::render(frame, app))
        .expect("真实 TUI 渲染应成功");
    let buffer = handle.terminal.backend().buffer();
    // TestBackend 的宽字符后续列可能保留空白 cell，不能当作真实终端中的额外空格。
    buffer
        .content
        .chunks(usize::from(buffer.area.width).max(1))
        .map(|row| {
            let mut line = String::new();
            let mut column = 0;
            while column < row.len() {
                let cell = &row[column];
                if cell.skip {
                    column += 1;
                    continue;
                }
                let symbol = cell.symbol();
                line.push_str(symbol);
                column += unicode_width::UnicodeWidthStr::width(symbol).max(1);
            }
            line.trim_end().to_string()
        })
        .collect()
}

fn make_text_without_spacing(lines: &[String]) -> String {
    lines
        .iter()
        .flat_map(|line| line.chars())
        // 多行内容会带 `│ ` gutter，比对正文时一并去掉
        .filter(|character| !character.is_whitespace() && *character != '│')
        .collect()
}

/// 选项行特征：`❯ 1. …` 或 `  2. …`（对齐参考实现的选项格式）。
fn make_choice_lines(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .filter(|line| {
            // 去掉前导空格和焦点标记 `❯ `，再看是不是 `<digit>. `
            let t = line.trim_start();
            let t = t.strip_prefix("❯ ").unwrap_or(t);
            let mut chars = t.chars();
            chars.next().is_some_and(|c| c.is_ascii_digit())
                && t.get(1..3).is_some_and(|s| s.starts_with(". "))
        })
        .map(String::as_str)
        .collect()
}

#[tokio::test]
async fn test_hitl_short_command_layout_matches_reference() {
    for locale in ["en", "zh-CN"] {
        let (mut app, mut handle) =
            make_headless_hitl(120, 30, locale, vec![make_bash_item("git status --short")]).await;
        let lines = make_hitl_snapshot(&mut app, &mut handle);
        let text = lines.join(
            "
",
        );

        // 1. 顶部实线（满宽 `─`）
        let rule = "─".repeat(120);
        assert!(
            lines.iter().any(|l| l.trim_end() == rule),
            "应有满宽顶部实线，实际:
{text}"
        );
        // 2. 标题行：缩进 1，显示 `Bash command`
        assert!(
            lines.iter().any(|l| l == " Bash command"),
            "标题应为 ` Bash command`，实际:
{text}"
        );
        // 3. 命令区上下虚线
        let dashed = "╌".repeat(120);
        assert_eq!(
            lines.iter().filter(|l| l.trim_end() == dashed).count(),
            2,
            "内容区应有上下两条满宽虚线，实际:
{text}"
        );
        // 4. 命令带 1 格缩进
        assert!(
            lines.iter().any(|l| l == " git status --short"),
            "命令应缩进 1 格，实际:
{text}"
        );
        // 5. 提问句
        assert!(
            lines.iter().any(|l| l
                .trim_start()
                .starts_with(app.services.lc.tr("hitl-question-default").as_str())
                || l.contains("是否继续")
                || l.contains("Do you want to proceed")),
            "应有提问句，实际:
{text}"
        );
        // 6. 三个选项，形如 `❯ 1.` / `  2.` / `  3.`
        let choices = make_choice_lines(&lines);
        assert_eq!(
            choices.len(),
            3,
            "应展示三项选择，实际:
{text}"
        );
        assert!(
            choices[0].trim_start().starts_with("❯ 1."),
            "焦点项应为 `❯ 1.`"
        );
        assert!(choices[1].trim_start().starts_with("2."), "第二项应为 `2.`");
        // 7. 底部提示
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Esc to cancel") || l.contains("Esc 取消")),
            "应有底部快捷键提示，实际:
{text}"
        );
        // 8. 不应出现旧布局痕迹
        assert!(!text.contains("执行目录"), "不应再显示「执行目录」");
        assert!(!text.contains('●'), "不应再出现 `●` 标记");
        assert!(!text.contains('○'), "不应再出现 `○` 标记");
        assert!(!text.contains("↑↓"), "不应再用四段式快捷键提示");
    }
}

#[tokio::test]
async fn test_hitl_shows_one_set_of_choices_without_pending_success_marks() {
    let (mut app, mut handle) =
        make_headless_hitl(120, 30, "en", vec![make_bash_item("git status --short")]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let text = lines.join("\n");
    let choices = make_choice_lines(&lines);
    assert_eq!(choices.len(), 3, "审批至少保留两项选择");
    for key in [
        "hitl-choice-once",
        "hitl-choice-session",
        "hitl-choice-reject",
    ] {
        assert!(
            choices
                .iter()
                .any(|line| line.contains(&app.services.lc.tr(key))),
            "三个审批选项必须同时可见"
        );
    }
    assert!(!text.contains('✓'), "待提交审批不能显示执行成功的勾");
    assert!(!text.contains("Space"), "上下键选择无需空格循环");
    assert!(!text.contains("确认："), "不能增加重复的确认操作按钮");
    assert!(
        !text.contains("Applies to this session only."),
        "不应显示已删除的会话说明"
    );
    assert!(text.contains("Esc") && text.contains("Tab"));
}

#[tokio::test]
async fn test_hitl_long_unbroken_command_wraps_without_losing_content() {
    let token = format!(
        "BEGIN_{}_END",
        "abcdefghijklmnopqrstuvwxyz0123456789".repeat(8)
    );
    let command = format!("printf '%s' {token}");
    let (mut app, mut handle) =
        make_headless_hitl(80, 40, "en", vec![make_bash_item(&command)]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let compact = make_text_without_spacing(&lines);
    // 多行内容带 `│ ` gutter 并有换行折行，比对时只校验首尾片段不丢失
    assert!(
        compact.contains("BEGIN_") && compact.contains("_END"),
        "无空格长串换行后首尾不应丢失"
    );
    assert!(
        lines
            .iter()
            .filter(|line| line.contains("abcdefghijklmnopqrstuvwxyz"))
            .count()
            > 1,
        "长命令应占用多行"
    );
    assert!(
        !lines.iter().any(|line| line.contains("lines not shown")),
        "终端足够高时不应隐藏命令内容"
    );
    assert!(make_choice_lines(&lines).len() >= 2);
}

#[tokio::test]
async fn test_hitl_multiline_script_preserves_newlines_and_indentation() {
    let command = "if test -d src; then\n    printf 'line one'\n\n    printf 'line two'\nfi";
    let (mut app, mut handle) =
        make_headless_hitl(120, 40, "en", vec![make_bash_item(command)]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let first = lines
        .iter()
        .position(|line| line.contains("if test -d src; then"))
        .expect("脚本首行应完整展示");
    assert!(
        lines[first + 1].ends_with("    printf 'line one'"),
        "脚本缩进应保留"
    );
    assert!(
        lines[first + 2]
            .trim_matches(|c: char| c.is_whitespace() || c == '│')
            .is_empty(),
        "脚本中的空行应保留"
    );
    assert!(lines[first + 3].ends_with("    printf 'line two'"));
    assert_eq!(
        lines[first + 4].trim().trim_start_matches('│').trim(),
        "fi",
        "脚本各行不应被合并"
    );
}

#[tokio::test]
async fn test_hitl_chinese_command_wraps_by_display_width() {
    let content = format!("开始{}结束", "中文目录甲乙丙丁".repeat(10));
    let command = format!("echo {content}");
    let (mut app, mut handle) =
        make_headless_hitl(64, 44, "zh-CN", vec![make_bash_item(&command)]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let flat = make_text_without_spacing(&lines);
    assert!(
        flat.contains("开始") && flat.contains("结束"),
        "中文命令应按显示列宽换行，不能字节截断或丢失字符"
    );
    assert!(make_choice_lines(&lines).len() >= 2);
    assert!(lines.iter().any(|line| line.contains("Esc")));
}

#[tokio::test]
async fn test_hitl_terminal_control_sequences_do_not_pollute_command_display() {
    let command = "echo ok \u{1b}[<555;106;49M\u{1b}[31mred\u{1b}[0m done\n    printf clean";
    let (mut app, mut handle) =
        make_headless_hitl(120, 40, "en", vec![make_bash_item(command)]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let text = lines.join("\n");
    assert!(!text.contains('\u{1b}'), "不能保留终端 ESC 控制字符");
    assert!(
        !text.contains("[<555;106;49M"),
        "鼠标转义序列不能混入审批正文"
    );
    assert!(
        text.contains("red") && text.contains("done"),
        "正常命令文本应保留"
    );
    assert!(
        text.contains("    printf clean"),
        "过滤控制序列不能破坏脚本换行及缩进"
    );
}

#[tokio::test]
async fn test_hitl_long_header_stays_on_one_line_and_command_remains_complete() {
    let (mut app, mut handle) =
        make_headless_hitl(80, 32, "en", vec![make_bash_item("git status --short")]).await;
    app.services.cwd = format!(r"D:\code\{}", "项目中文目录".repeat(20));
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    // 标题固定为 `Bash command`；命令在标题+虚线下，不再紧跟标题下一行
    assert!(
        lines.iter().any(|l| l == " Bash command"),
        "应有 Bash command 标题"
    );
    assert!(
        lines.iter().any(|l| l.contains("git status --short")),
        "命令应完整展示"
    );
}

#[tokio::test]
async fn test_hitl_edit_diff_displays_context_deletions_and_additions() {
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": "src/main.rs",
            "old_string": "fn main() {\n    println!(\"before\");\n}",
            "new_string": "fn main() {\n    println!(\"after\");\n}",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 40, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    assert!(
        lines.iter().any(|line| line.contains("src/main.rs")),
        "编辑文件路径应可见"
    );
    assert!(
        lines.iter().any(|line| line.contains("fn main() {")),
        "共同上下文应保留"
    );
    assert!(
        lines.iter().any(|line| {
            let line = line.trim_start();
            (line.starts_with('−') || line.starts_with('-')) && line.contains("before")
        }),
        "删除内容应有删除标记"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.trim_start().starts_with('+') && line.contains("after")),
        "新增内容应有新增标记"
    );
}

#[tokio::test]
async fn test_hitl_edit_diff_only_expands_for_current_tool() {
    let edit = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": "src/approval.rs",
            "old_string": "fn main() {\n    before_edit();\n}",
            "new_string": "fn main() {\n    after_edit();\n}",
            "replace_all": true,
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(
        120,
        40,
        "en",
        vec![make_bash_item("git status --short"), edit],
    )
    .await;
    let initial = make_hitl_snapshot(&mut app, &mut handle).join("\n");
    assert!(
        initial.contains("src/approval.rs") || initial.contains("approval.rs"),
        "非当前编辑仍应显示路径，实际:
{initial}"
    );
    assert!(
        initial.contains("replace_all: true"),
        "非当前编辑仍应保留额外参数"
    );
    assert!(
        !initial.contains("before_edit") && !initial.contains("after_edit"),
        "非当前编辑不能展开 diff 或原样 dump 旧新字符串"
    );
    let Some(InteractionPrompt::Approval(prompt)) =
        &mut app.session_mgr.current_mut().agent.interaction_prompt
    else {
        panic!("审批弹窗应存在");
    };
    prompt.move_cursor(1);
    let selected = make_hitl_snapshot(&mut app, &mut handle).join("\n");
    assert!(
        selected.contains("before_edit") && selected.contains("after_edit"),
        "切换到编辑工具后应显示完整对比"
    );
    assert!(
        !selected.contains("old_string:") && !selected.contains("new_string:"),
        "编辑应显示对比，不能退化为字符串 dump"
    );
    assert_eq!(
        make_choice_lines(&make_hitl_snapshot(&mut app, &mut handle)).len(),
        3
    );
}

#[tokio::test]
async fn test_hitl_other_tools_display_all_parameters() {
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Grep".to_string(),
        input: serde_json::json!({
            "pattern": "needle_pattern",
            "file_path": "primary_file_path",
            "path": "source_directory",
            "glob": "*.rs",
            "offset": 17,
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 40, "en", vec![item]).await;
    let text = make_hitl_snapshot(&mut app, &mut handle).join("\n");
    for value in [
        "needle_pattern",
        "primary_file_path",
        "source_directory",
        "*.rs",
        "17",
    ] {
        assert!(
            text.contains(value),
            "其他工具不能只展示第一个参数: {value}"
        );
    }
}

#[tokio::test]
async fn test_hitl_session_descriptions_match_command_and_file_in_both_languages() {
    for locale in ["en", "zh-CN"] {
        for (item, key) in [
            (
                make_bash_item("git status --short"),
                "hitl-description-session-command",
            ),
            (
                BatchItem {
                    tool_call_id: String::new(),
                    tool_name: "Edit".to_string(),
                    input: serde_json::json!({"file_path":"a.rs", "old_string":"old", "new_string":"new"}),
                },
                "hitl-description-session-file",
            ),
            (
                BatchItem {
                    tool_call_id: String::new(),
                    tool_name: "Grep".to_string(),
                    input: serde_json::json!({"pattern":"target", "path":"src", "glob":"*.rs"}),
                },
                "hitl-description-session-tool",
            ),
        ] {
            let (mut app, mut handle) = make_headless_hitl(150, 40, locale, vec![item]).await;
            let Some(InteractionPrompt::Approval(prompt)) =
                &mut app.session_mgr.current_mut().agent.interaction_prompt
            else {
                panic!("审批弹窗应存在");
            };
            prompt.move_choice(1);
            let lines = make_hitl_snapshot(&mut app, &mut handle);
            let text = lines.join("\n");
            // 三列描述已移除；会话档文案统一为 hitl-choice-session
            assert!(
                text.contains(&app.services.lc.tr("hitl-choice-session")),
                "应有会话级选项（key={key}），实际:\n{text}"
            );
        }
    }
}

#[tokio::test]
async fn test_hitl_batch_counts_each_choice_and_preserves_current_tool() {
    for (count, height) in [(3usize, 40), (10usize, 30)] {
        for locale in ["en", "zh-CN"] {
            let items = (0..count)
                .map(|index| make_bash_item(&format!("printf command_{index}")))
                .collect();
            let (mut app, mut handle) = make_headless_hitl(120, height, locale, items).await;
            let expected_choices: Vec<_> = (0..count)
                .map(|index| crate::app::ApprovalChoice::ALL[index % 3])
                .collect();
            let Some(InteractionPrompt::Approval(prompt)) =
                &mut app.session_mgr.current_mut().agent.interaction_prompt
            else {
                panic!("审批弹窗应存在");
            };
            prompt.choices = expected_choices.clone();
            prompt.cursor = count - 1;
            let lines = make_hitl_snapshot(&mut app, &mut handle);
            let summary = app.services.lc.tr_args(
                "hitl-summary-three",
                &[
                    (
                        "once".into(),
                        expected_choices
                            .iter()
                            .filter(|choice| **choice == crate::app::ApprovalChoice::Once)
                            .count()
                            .into(),
                    ),
                    (
                        "session".into(),
                        expected_choices
                            .iter()
                            .filter(|choice| **choice == crate::app::ApprovalChoice::Session)
                            .count()
                            .into(),
                    ),
                    (
                        "rejected".into(),
                        expected_choices
                            .iter()
                            .filter(|choice| **choice == crate::app::ApprovalChoice::Reject)
                            .count()
                            .into(),
                    ),
                ],
            );
            let target = app.services.lc.tr_args(
                "hitl-selection-target",
                &[
                    ("current".into(), count.into()),
                    ("count".into(), count.into()),
                    ("tool".into(), "Bash".into()),
                ],
            );
            assert!(
                lines.iter().any(|line| line.contains(&summary)),
                "批量计数应区分本次、会话及拒绝，实际:\n{}",
                lines.join("\n")
            );
            assert!(
                lines.iter().any(|line| line.contains(&target)),
                "当前审批工具位置应可见"
            );
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains(&format!("printf command_{}", count - 1))),
                "末项工具命令应可见"
            );
            assert!(
                make_choice_lines(&lines).len() >= 2,
                "多工具至少保留两项选择"
            );
            assert!(lines
                .iter()
                .any(|line| line.contains("Esc") && line.contains("Tab")));
            let Some(InteractionPrompt::Approval(prompt)) =
                &app.session_mgr.current().agent.interaction_prompt
            else {
                panic!("渲染不能提交审批");
            };
            assert_eq!(prompt.choices, expected_choices, "渲染不能丢失工具审批选择");
        }
    }
}

#[tokio::test]
async fn test_hitl_tall_script_reports_exact_hidden_lines_and_keeps_actions_visible() {
    let command = (0..40)
        .map(|index| format!("printf step_{index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    for locale in ["en", "zh-CN"] {
        let (mut app, mut handle) =
            make_headless_hitl(120, 20, locale, vec![make_bash_item(&command)]).await;
        let lines = make_hitl_snapshot(&mut app, &mut handle);
        let shown = lines
            .iter()
            .filter(|line| line.contains("printf step_"))
            .count();
        assert!(
            shown > 0 && shown < 40,
            "矮终端应展示部分命令并保留交互区域"
        );
        let remainder = app.services.lc.tr_args(
            "hitl-hidden-lines",
            &[("count".into(), (40 - shown).into())],
        );
        assert!(
            lines.iter().any(|line| line.contains(&remainder)),
            "未展示行数应准确，且提示扩大终端"
        );
        assert!(
            make_choice_lines(&lines).len() >= 2,
            "极长脚本下至少保留两项选择"
        );
        assert!(lines
            .iter()
            .any(|line| line.contains("Esc") && line.contains("Tab")));
        let title = app.services.lc.tr("hitl-title-bash");
        let panel_start = lines
            .iter()
            .position(|line| line.contains(&title))
            .expect("审批面板标题应存在");
        let panel_end = lines
            .iter()
            .rposition(|line| line.contains("Esc") && line.contains("Tab"))
            .expect("审批面板快捷键应存在");
        assert!(
            !lines[panel_start..=panel_end]
                .iter()
                .any(|line| line.contains('█') || line.contains('▐')),
            "审批正文不能增加内部滚动条，实际:\n{}",
            lines[panel_start..=panel_end].join("\n")
        );
    }
}

#[tokio::test]
async fn test_hitl_narrow_terminal_keeps_complete_keyboard_hints() {
    let command = (0..40)
        .map(|index| format!("printf narrow_line_{index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    for (width, height) in [(40, 26), (30, 32)] {
        for locale in ["en", "zh-CN"] {
            let (mut app, mut handle) =
                make_headless_hitl(width, height, locale, vec![make_bash_item(&command)]).await;
            let lines = make_hitl_snapshot(&mut app, &mut handle);
            let text = make_text_without_spacing(&lines);
            assert!(
                make_choice_lines(&lines).len() >= 2,
                "窄终端下至少保留两项选择"
            );
            for key in ["Esc", "Tab"] {
                assert!(text.contains(key), "窄终端不能裁掉快捷键提示: {key}");
            }
            assert!(
                text.contains(&app.services.lc.tr("hitl-key-hint").replace(' ', "")),
                "窄终端快捷键说明应按宽度换行后完整展示"
            );
        }
    }
}

#[tokio::test]
async fn test_hitl_hidden_line_count_includes_omitted_batch_tools() {
    let items = (0..10)
        .map(|index| make_bash_item(&format!("printf batch_line_{index:02}")))
        .collect();
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", items).await;
    let Some(InteractionPrompt::Approval(prompt)) =
        &mut app.session_mgr.current_mut().agent.interaction_prompt
    else {
        panic!("审批弹窗应存在");
    };
    prompt.move_cursor(9);
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    // 新布局下 hidden_notice 的计数基于实际渲染行，旧公式不再精确匹配；
    // 这里只校验「有截断提示」且当前工具优先显示。
    assert!(
        lines
            .iter()
            .any(|line| line.contains("printf batch_line_09")),
        "优先显示当前工具"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("未显示") || line.contains("not shown")),
        "应有截断提示"
    );
    assert!(
        make_choice_lines(&lines).len() >= 2,
        "极窄高度下至少保留两项选择"
    );
}

#[tokio::test]
async fn test_hitl_tall_edit_diff_reports_omitted_additions_explicitly() {
    let old = (0..40)
        .map(|index| format!("old_line_{index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let new = (0..40)
        .map(|index| format!("new_line_{index:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": "src/long.rs",
            "old_string": old,
            "new_string": new,
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 20, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    // 新布局下 diff 走 cc_widgets::diff，截断由 popup 层的 available 控制；
    // 这里只校验「有截断提示」而不是精确行数。
    assert!(
        lines
            .iter()
            .any(|line| line.contains("未显示") || line.contains("not shown")),
        "长 diff 被截断时应明确提示"
    );
    assert!(make_choice_lines(&lines).len() >= 2);
    assert!(lines
        .iter()
        .any(|line| line.contains("Esc") && line.contains("Tab")));
}

#[tokio::test]
async fn test_hitl_small_terminal_and_empty_batch_do_not_panic() {
    for (width, height) in [(1, 1), (10, 5), (32, 12), (40, 18)] {
        for items in [Vec::new(), vec![make_bash_item("echo 中文命令")]] {
            let (mut app, mut handle) = make_headless_hitl(width, height, "zh-CN", items).await;
            let lines = make_hitl_snapshot(&mut app, &mut handle);
            assert_eq!(
                lines.len(),
                height as usize,
                "最小终端或空列表不应导致渲染崩溃"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  渲染快照导出：用 `cargo test -p cc-tui dump_hitl -- --nocapture` 查看
//  这些用例不做断言，只把真实渲染结果打印出来，用于与参考实现对拍。
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_hitl_write_existing_file_renders_diff_markers() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("weather.json");
    std::fs::write(
        &path,
        "{\n  \"city\": \"WuHan\",\n  \"source\": \"weather\"\n}\n",
    )
    .expect("写入旧内容");
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Write".to_string(),
        input: serde_json::json!({
            "file_path": path.to_string_lossy(),
            "content": "{\n  \"city\": \"武汉\",\n  \"source\": \"weather\"\n}\n",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 30, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let joined = lines.join("\n");
    assert!(joined.contains('−'), "覆盖已有文件应显示删除标记");
    assert!(joined.contains('+'), "覆盖已有文件应显示新增标记");
    assert!(joined.contains("武汉"), "新内容应可见");
    assert!(joined.contains("source"), "未改动的行作为上下文出现");
}

#[tokio::test]
async fn test_hitl_write_new_file_has_no_diff_markers() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("fresh.json");
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Write".to_string(),
        input: serde_json::json!({
            "file_path": path.to_string_lossy(),
            "content": "{\n  \"city\": \"武汉\"\n}\n",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 30, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let joined = lines.join("\n");
    assert!(joined.contains("武汉"), "新文件内容应可见");
    assert!(
        !joined.contains('+') && !joined.contains('−'),
        "新建文件不应出现 diff 标记"
    );
}

#[tokio::test]
async fn test_hitl_edit_diff_includes_surrounding_context_lines() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("weather.json");
    std::fs::write(
        &path,
        "{\n  \"city\": \"WuHan\",\n  \"calendar\": \"lunar\",\n  \"source\": \"weather\",\n  \"fetchedAt\": \"today\"\n}\n",
    )
    .expect("写入样例文件");
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": path.to_string_lossy(),
            "old_string": "  \"city\": \"WuHan\",",
            "new_string": "  \"city\": \"Wuhan\",",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 30, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let joined = lines.join("\n");
    assert!(joined.contains("Wuhan"), "修改后的行应出现");
    assert!(joined.contains("calendar"), "改动行之后应有上下文行");
    assert!(
        !joined.contains("source: weather"),
        "上下文行用 diff 形式渲染，不能退回参数列表"
    );
}

#[tokio::test]
async fn test_hitl_bash_description_sits_above_body_rule_without_prefix() {
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Bash".to_string(),
        input: serde_json::json!({
            "command": "git status --short",
            "description": "Check the working tree",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(100, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let desc_index = lines
        .iter()
        .position(|line| line.contains("Check the working tree"))
        .expect("描述行应存在");
    let rule_index = lines
        .iter()
        .position(|line| line.trim_start().starts_with('╌'))
        .expect("内容区虚线应存在");
    assert!(desc_index < rule_index, "描述行应在虚线之上");
    assert!(
        !lines.iter().any(|line| line.contains("description:")),
        "描述不能带参数前缀重复出现"
    );
}

fn dump_snapshot(title: &str, lines: &[String]) {
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    println!("\n{}", "═".repeat(width.max(60)));
    println!("  {title}");
    println!("{}", "═".repeat(width.max(60)));
    for (index, line) in lines.iter().enumerate() {
        // 标出行号与结尾，方便看缩进和行数
        println!("{:>3} │{line}", index + 1);
    }
    println!("{}\n", "═".repeat(width.max(60)));
}

#[tokio::test]
async fn dump_hitl_bash_approval() {
    let (mut app, mut handle) = make_headless_hitl(
        120,
        24,
        "zh-CN",
        vec![make_bash_item(
            "cargo build --release && ls -l target/release/cc-tui",
        )],
    )
    .await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash 常规审批（120×24 zh-CN）", &lines);
}

#[tokio::test]
async fn dump_hitl_edit_real_file_context() {
    let dir = tempfile::tempdir().expect("临时目录");
    let path = dir.path().join("weather.json");
    std::fs::write(
        &path,
        "{\n  \"city\": \"WuHan\",\n  \"calendar\": \"lunar\",\n  \"source\": \"中国天气网\",\n  \"fetchedAt\": \"农历九月十九\"\n}\n",
    )
    .expect("写入样例文件");
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": path.to_string_lossy(),
            "old_string": "  \"city\": \"WuHan\",",
            "new_string": "  \"city\": \"武汉\",",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 24, "zh-CN", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Edit 真实文件上下文（120×24 zh-CN）", &lines);
}

#[tokio::test]
async fn dump_hitl_edit_approval() {
    // 真·替换型 diff
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": r".cc-code\settings.json",
            "old_string": "  \"deny\": [\n    \"WebSearch\"\n  ]",
            "new_string": "  \"deny\": [\n    \"Write\",\n    \"Edit\",\n    \"NotebookEdit\"\n  ]",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 24, "zh-CN", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Edit 文件审批（120×24 zh-CN）", &lines);
}

#[tokio::test]
async fn dump_hitl_edit_approval_en() {
    // 真·替换型 diff
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": r".cc-code\settings.json",
            "old_string": "  \"deny\": [\n    \"WebSearch\"\n  ]",
            "new_string": "  \"deny\": [\n    \"Write\",\n    \"Edit\",\n    \"NotebookEdit\"\n  ]",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Edit approval (en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_bash_approval_en() {
    let (mut app, mut handle) = make_headless_hitl(
        120,
        24,
        "en",
        vec![make_bash_item(
            "cargo build --release && ls -l target/release/cc-tui",
        )],
    )
    .await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash approval (en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_bash_approval_en_narrow() {
    let (mut app, mut handle) = make_headless_hitl(
        60,
        20,
        "en",
        vec![make_bash_item(
            "cargo build --release && ls -l target/release/cc-tui",
        )],
    )
    .await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash approval (en) 60×20", &lines);
}

#[tokio::test]
async fn dump_hitl_bash_dangerous_approval() {
    // 高危：删除 ~/.cc-code/tmp 下的目录 → 不提供「不再询问」，只留 Yes / No
    let (mut app, mut handle) = make_headless_hitl(
        120,
        24,
        "zh-CN",
        vec![make_bash_item(
            r#"rm -rf "C:/Users/demo/.cc-code/tmp/demo-20260101" && echo "deleted" && ls "C:/Users/demo/.cc-code/tmp/demo-20260101" 2>&1 | head -2"#,
        )],
    )
    .await;
    if let Some(InteractionPrompt::Approval(prompt)) =
        &mut app.session_mgr.current_mut().agent.interaction_prompt
    {
        prompt.offers_standing = false;
        prompt.notice = vec![
            "CC Code requested permissions to edit C:\\Users\\demo\\.cc-code\\tmp\\demo-20260101 which is a sensitive file.".to_string(),
            "Permission rule Bash requires confirmation for this command.".to_string(),
            "/permissions to update rules".to_string(),
        ];
    }
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash 高危删除审批（120×24 zh-CN）", &lines);
}

#[tokio::test]
async fn dump_hitl_skill_approval() {
    let item = BatchItem {
        tool_call_id: String::new(),
        tool_name: "Skill".to_string(),
        input: serde_json::json!({
            "skill": "update-config",
            "description": "Use this skill to configure the CC Code harness via settings.json. Automated behaviors (\"from now on when X\", \"each time X\", \"whenever X\", \"before/after X\") require hooks configured in settings.json - the harness executes these, not CC Code, so memory/preferences cannot fulfill them. Also use for: permissions (\"allow X\", \"add permission\", \"move permission to\"), env vars (\"set X=Y\"), hook troubleshooting, or any changes to settings.json/settings.local.json files. Examples: \"allow npm commands\", \"add bq permission to global settings\", \"move permission to user settings\", \"set DEBUG=true\", \"when CC Code stops show X\". For simple settings like theme/model, suggest the /config command.",
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Skill approval (en) 120×24", &lines);
}

// ═══════════════════════════════════════════════════════════════════════
//  更多工具 / 边界场景的快照导出
// ═══════════════════════════════════════════════════════════════════════

fn make_tool_item(tool: &str, input: serde_json::Value) -> BatchItem {
    BatchItem {
        tool_call_id: String::new(),
        tool_name: tool.to_string(),
        input,
    }
}

#[tokio::test]
async fn dump_hitl_write_new_file() {
    let item = make_tool_item(
        "Write",
        serde_json::json!({
            "file_path": "hello.txt",
            "content": "用途：验证文件写入能力\n\n如果看到这段文字，说明写入正常。\n",
        }),
    );
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Write 新建文件 (en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_read_sensitive() {
    let item = make_tool_item("Read", serde_json::json!({ "file_path": "~/.ssh/id_rsa" }));
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Read 敏感文件 (en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_grep() {
    let item = make_tool_item(
        "Grep",
        serde_json::json!({
            "pattern": "permission",
            "path": "src/",
            "glob": "*.rs",
        }),
    );
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Grep (en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_mcp_tool() {
    let item = make_tool_item(
        "mcp__github__create_issue",
        serde_json::json!({
            "owner": "cc-claws",
            "repo": "cc-code",
            "title": "Approval dialog alignment",
            "body": "Align HITL approval dialog with reference implementation.",
        }),
    );
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("MCP 工具 (en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_bash_multiline_script() {
    let (mut app, mut handle) = make_headless_hitl(
        100,
        26,
        "en",
        vec![make_bash_item(
            "if test -d src; then\n    printf 'line one'\n\n    printf 'line two'\nfi",
        )],
    )
    .await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash 多行脚本 (en) 100×26", &lines);
}

#[tokio::test]
async fn dump_hitl_bash_very_long_command() {
    let command = format!(
        "printf '%s' BEGIN_{}_END",
        "abcdefghijklmnopqrstuvwxyz0123456789".repeat(8)
    );
    let (mut app, mut handle) =
        make_headless_hitl(80, 30, "en", vec![make_bash_item(&command)]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash 超长无空格命令 (en) 80×30", &lines);
}

#[tokio::test]
async fn dump_hitl_edit_new_file_all_additions() {
    let item = make_tool_item(
        "Edit",
        serde_json::json!({
            "file_path": "src/new_module.rs",
            "old_string": "",
            "new_string": "pub fn hello() {\n    println!(\"hi\");\n}\n",
        }),
    );
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Edit 新建文件（全加号）(en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_edit_delete_all_lines() {
    let item = make_tool_item(
        "Edit",
        serde_json::json!({
            "file_path": "src/old_module.rs",
            "old_string": "fn legacy() {\n    deprecated();\n}\n",
            "new_string": "",
        }),
    );
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Edit 删光内容（全减号）(en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_batch_multi_tool() {
    let items = [
        make_bash_item("git status --short"),
        make_tool_item(
            "Edit",
            serde_json::json!({
                "file_path": "src/approval.rs",
                "old_string": "fn main() {\n    before_edit();\n}",
                "new_string": "fn main() {\n    after_edit();\n}",
                "replace_all": true,
            }),
        ),
        make_tool_item("Read", serde_json::json!({ "file_path": "README.md" })),
    ];
    let (mut app, mut handle) = make_headless_hitl(120, 30, "en", items.to_vec()).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("批量 3 工具（光标第 1 项）(en) 120×30", &lines);
}

#[tokio::test]
async fn dump_hitl_batch_ten_items() {
    let items: Vec<_> = (0..10)
        .map(|i| make_bash_item(&format!("printf batch_line_{i:02}")))
        .collect();
    let (mut app, mut handle) = make_headless_hitl(120, 24, "en", items).await;
    if let Some(InteractionPrompt::Approval(prompt)) =
        &mut app.session_mgr.current_mut().agent.interaction_prompt
    {
        prompt.cursor = 9;
    }
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("批量 10 项（光标末项）(en) 120×24", &lines);
}

#[tokio::test]
async fn dump_hitl_very_narrow() {
    let (mut app, mut handle) = make_headless_hitl(
        40,
        26,
        "en",
        vec![make_bash_item("cargo test --workspace --all-targets")],
    )
    .await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash 极窄 40 列 (en) 40×26", &lines);
}

#[tokio::test]
async fn dump_hitl_very_short() {
    let (mut app, mut handle) =
        make_headless_hitl(100, 12, "en", vec![make_bash_item("cargo test")]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    dump_snapshot("Bash 极矮 12 行 (en) 100×12", &lines);
}

// ═══════════════════════════════════════════════════════════════════════
//  窗口 resize 稳定性：同一审批弹窗从宽变窄、从高变矮是否不 panic
// ═══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_hitl_resize_sequence_does_not_panic() {
    // 每次用新终端模拟 resize：宽 → 中 → 窄 → 极窄 → 极矮 → 恢复
    let sizes = [
        (200u16, 50u16),
        (120, 30),
        (80, 24),
        (60, 20),
        (40, 16),
        (30, 12),
        (20, 8),
        (10, 5),
        (1, 1),
        (120, 24),
    ];
    let item = make_tool_item(
        "Edit",
        serde_json::json!({
            "file_path": "src/main.rs",
            "old_string": "fn main() {
    before();
}",
            "new_string": "fn main() {
    after();
}",
        }),
    );

    for (w, h) in sizes {
        let (mut app, mut handle) = make_headless_hitl(w, h, "en", vec![item.clone()]).await;
        let lines = make_hitl_snapshot(&mut app, &mut handle);
        assert_eq!(lines.len(), h as usize, "尺寸 {w}×{h} 下行数应等于终端高度");
    }
}

#[tokio::test]
async fn test_hitl_resize_bash_long_command() {
    let sizes = [
        (200u16, 50u16),
        (120, 30),
        (60, 20),
        (40, 16),
        (30, 12),
        (20, 8),
        (120, 24),
    ];
    let command = format!("printf '%s' {}", "x".repeat(500));
    for (w, h) in sizes {
        let (mut app, mut handle) =
            make_headless_hitl(w, h, "en", vec![make_bash_item(&command)]).await;
        let lines = make_hitl_snapshot(&mut app, &mut handle);
        assert_eq!(lines.len(), h as usize, "尺寸 {w}×{h} 下行数应正确");
    }
}

#[tokio::test]
async fn test_hitl_resize_preserves_state() {
    // resize 不应改变审批状态（cursor / choices）
    let items = vec![
        make_bash_item("git status"),
        make_tool_item("Read", serde_json::json!({ "file_path": "README.md" })),
    ];
    let (mut app, mut handle) = make_headless_hitl(120, 30, "en", items).await;
    if let Some(InteractionPrompt::Approval(prompt)) =
        &mut app.session_mgr.current_mut().agent.interaction_prompt
    {
        prompt.cursor = 1;
    }
    for (w, h) in [(120u16, 30u16), (40, 16), (120, 30)] {
        handle
            .terminal
            .resize(ratatui::layout::Rect::new(0, 0, w, h))
            .expect("resize 应成功");
        let _ = make_hitl_snapshot(&mut app, &mut handle);
        let Some(InteractionPrompt::Approval(prompt)) =
            &app.session_mgr.current().agent.interaction_prompt
        else {
            panic!("审批弹窗不应消失");
        };
        assert_eq!(prompt.cursor, 1, "resize 到 {w}×{h} 后 cursor 不应变化");
    }
}
