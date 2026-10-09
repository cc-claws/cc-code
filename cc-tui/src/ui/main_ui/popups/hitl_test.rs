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
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn make_choice_lines(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .filter(|line| line.contains('○') || line.contains('●'))
        .map(String::as_str)
        .collect()
}

#[tokio::test]
async fn test_hitl_short_command_keeps_header_inline_in_both_languages() {
    for locale in ["en", "zh-CN"] {
        let (mut app, mut handle) =
            make_headless_hitl(120, 30, locale, vec![make_bash_item("git status --short")]).await;
        let lines = make_hitl_snapshot(&mut app, &mut handle);
        let action = app.services.lc.tr("hitl-run-command");
        let directory = app.services.lc.tr("hitl-working-directory");
        let header_index = lines
            .iter()
            .position(|line| {
                line.contains("Bash") && line.contains(&action) && line.contains(&directory)
            })
            .unwrap_or_else(|| {
                panic!(
                    "工具、动作说明和执行目录应在同一行，实际:\n{}",
                    lines.join("\n")
                )
            });
        assert!(
            lines[header_index].contains(r"D:\code\peri"),
            "短目录应完整展示在标题行"
        );
        assert!(
            lines[header_index + 1].contains("git status --short"),
            "标题下一行应直接展示完整命令"
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains(&directory))
                .count(),
            1,
            "执行目录说明不应在命令后重复"
        );
        assert!(
            !lines.iter().any(|line| line.contains("command=")),
            "正文不应退化为参数摘要"
        );
        let title_index = lines
            .iter()
            .position(|line| line.contains(&app.services.lc.tr("hitl-single-title")))
            .expect("审批标题应可见");
        let footer_index = lines
            .iter()
            .position(|line| line.contains("↑↓") && line.contains("Enter"))
            .expect("底部快捷键提示应可见");
        assert!(
            footer_index.saturating_sub(title_index) <= 9,
            "短命令面板应自然收紧，不能保留大片空白"
        );
        assert_eq!(
            lines.iter().filter(|line| line.contains("Enter")).count(),
            1,
            "快捷键提示只能出现一组"
        );
        assert_eq!(
            lines.iter().filter(|line| line.contains("Esc")).count(),
            1,
            "拒绝提示不能重复"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains(&app.services.lc.tr("statusbar-permission-cycle-hint"))),
            "审批期间 Shift+Tab 切换工具，状态栏不能提示它切换权限模式"
        );
    }
}

#[tokio::test]
async fn test_hitl_shows_one_set_of_choices_without_pending_success_marks() {
    let (mut app, mut handle) =
        make_headless_hitl(120, 30, "en", vec![make_bash_item("git status --short")]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let text = lines.join("\n");
    let choices = make_choice_lines(&lines);
    assert_eq!(choices.len(), 3, "审批只展示一组当前工具的三项选择");
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
    assert!(text.contains("↑↓") && text.contains("Enter") && text.contains("Esc"));
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
    assert!(compact.contains(&token), "无空格长串应换行后完整保留");
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
    assert_eq!(make_choice_lines(&lines).len(), 3);
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
    assert!(lines[first + 2].trim().is_empty(), "脚本中的空行应保留");
    assert!(lines[first + 3].ends_with("    printf 'line two'"));
    assert_eq!(lines[first + 4].trim(), "fi", "脚本各行不应被合并");
}

#[tokio::test]
async fn test_hitl_chinese_command_wraps_by_display_width() {
    let content = format!("开始{}结束", "中文目录甲乙丙丁".repeat(10));
    let command = format!("echo {content}");
    let (mut app, mut handle) =
        make_headless_hitl(64, 44, "zh-CN", vec![make_bash_item(&command)]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    assert!(
        make_text_without_spacing(&lines).contains(&content),
        "中文命令应按显示列宽换行，不能字节截断或丢失字符"
    );
    assert_eq!(make_choice_lines(&lines).len(), 3);
    assert!(lines.iter().any(|line| line.contains("Enter")));
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
    let header_index = lines
        .iter()
        .position(|line| line.contains("Bash") && line.contains("Run command"))
        .expect("工具标题应存在");
    assert!(
        lines[header_index].ends_with('…'),
        "过长标题应单行省略并以省略号闭合"
    );
    assert!(lines[header_index + 1].contains("git status --short"));
}

#[tokio::test]
async fn test_hitl_edit_diff_displays_context_deletions_and_additions() {
    let item = BatchItem {
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
        initial.contains("src/approval.rs"),
        "非当前编辑仍应显示路径"
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
                    tool_name: "Edit".to_string(),
                    input: serde_json::json!({"file_path":"a.rs", "old_string":"old", "new_string":"new"}),
                },
                "hitl-description-session-file",
            ),
            (
                BatchItem {
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
            assert!(
                lines
                    .iter()
                    .any(|line| line.contains(&app.services.lc.tr(key))),
                "会话提示应明确说明相同命令、同工具同文件或相同调用不再询问，实际:\n{}",
                lines.join("\n")
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
            assert_eq!(
                make_choice_lines(&lines).len(),
                3,
                "多工具也只展示一组三项选择"
            );
            assert!(lines
                .iter()
                .any(|line| line.contains("Tab") && line.contains("Enter")));
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
        assert_eq!(
            make_choice_lines(&lines).len(),
            3,
            "极长脚本不能挤掉三项选择"
        );
        assert!(lines
            .iter()
            .any(|line| line.contains("↑↓") && line.contains("Enter") && line.contains("Esc")));
        let title = app.services.lc.tr("hitl-single-title");
        let panel_start = lines
            .iter()
            .position(|line| line.contains(&title))
            .expect("审批面板标题应存在");
        let panel_end = lines
            .iter()
            .rposition(|line| line.contains("Enter") && line.contains("Esc"))
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
            assert_eq!(make_choice_lines(&lines).len(), 3, "窄终端仍需显示三个选项");
            for key in ["↑↓", "Tab", "Enter", "Esc"] {
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
    let shown_rows = lines
        .iter()
        .filter(|line| {
            (line.contains("Bash") && line.contains("Run command"))
                || line.contains("printf batch_line_")
        })
        .count();
    let remainder = app.services.lc.tr_args(
        "hitl-hidden-lines",
        &[("count".into(), (20usize - shown_rows).into())],
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("printf batch_line_09")),
        "优先显示当前工具"
    );
    assert!(
        lines.iter().any(|line| line.contains(&remainder)),
        "剩余行数应包含其他未展示的批量工具"
    );
    assert_eq!(make_choice_lines(&lines).len(), 3);
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
        tool_name: "Edit".to_string(),
        input: serde_json::json!({
            "file_path": "src/long.rs",
            "old_string": old,
            "new_string": new,
        }),
    };
    let (mut app, mut handle) = make_headless_hitl(120, 20, "en", vec![item]).await;
    let lines = make_hitl_snapshot(&mut app, &mut handle);
    let shown_rows = lines
        .iter()
        .filter(|line| {
            (line.contains("Edit") && line.contains("Edit file"))
                || line.contains("src/long.rs")
                || line.contains(&app.services.lc.tr("hitl-diff-title"))
                || line.contains("old_line_")
                || line.contains("new_line_")
        })
        .count();
    let remainder = app.services.lc.tr_args(
        "hitl-hidden-lines",
        &[("count".into(), (83usize - shown_rows).into())],
    );
    assert!(
        lines.iter().any(|line| line.contains(&remainder)),
        "长 diff 未显示新增内容时必须明确提示剩余行数"
    );
    assert_eq!(make_choice_lines(&lines).len(), 3);
    assert!(lines
        .iter()
        .any(|line| line.contains("Enter") && line.contains("Esc")));
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
