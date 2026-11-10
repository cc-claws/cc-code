use super::format_status_directory;
use std::path::Path;
use unicode_width::UnicodeWidthStr;

#[test]
fn test_directory_preserves_full_path_and_normalizes_windows_separators() {
    assert_eq!(
        format_status_directory(r"D:\code\worktrees\task-a\project", None, 100),
        "D:/code/worktrees/task-a/project"
    );
    assert_eq!(format_status_directory("/", None, 100), "/");
}

#[test]
fn test_directory_abbreviates_home_only_at_component_boundaries() {
    let home = Path::new("/home/example");
    assert_eq!(
        format_status_directory("/home/example", Some(home), 100),
        "~"
    );
    assert_eq!(
        format_status_directory("/home/example/work/project", Some(home), 100),
        "~/work/project"
    );
    assert_eq!(
        format_status_directory("/home/example-other/project", Some(home), 100),
        "/home/example-other/project"
    );
    assert_eq!(format_status_directory("/", Some(Path::new("/")), 100), "~");
}

#[test]
fn test_directory_truncation_distinguishes_worktrees_with_same_repository_name() {
    let first = format_status_directory("/long/source/worktrees/task-a/project", None, 30);
    let second = format_status_directory("/long/source/worktrees/task-b/project", None, 30);
    assert_ne!(first, second, "相同仓库名不能丢失 worktree 目录区别");
    assert!(first.starts_with("/long"));
    assert!(first.ends_with("task-a/project"));
    assert!(first.contains('…'));
    assert!(first.width() <= 30);
    for width in [14, 16, 20] {
        assert_ne!(
            format_status_directory("/long/source/worktrees/task-a/project", None, width),
            format_status_directory("/long/source/worktrees/task-b/project", None, width)
        );
    }
}

#[test]
fn test_directory_truncation_respects_narrow_and_wide_character_widths() {
    for path in [
        "/源代码/多个目录/任务甲/项目",
        "D:/code/worktrees/task-a/project",
    ] {
        for width in 0..40 {
            let display = format_status_directory(path, None, width);
            assert!(display.width() <= width, "宽度 {width} 越界: {display}");
            if width > 0 {
                assert!(!display.is_empty());
            }
        }
    }
}

#[test]
fn test_directory_sanitizes_control_characters_before_width_calculation() {
    let display = format_status_directory("/work/line\nbreak/\u{1b}[31mproject", None, 100);
    assert!(!display.contains('\n'));
    assert!(!display.contains('\u{1b}'));
}

#[tokio::test]
async fn test_status_row_keeps_directory_visible_with_long_model_on_narrow_terminal() {
    for width in [40, 80] {
        let (mut app, mut handle) = crate::app::App::new_headless(width, 4).await;
        app.services.cwd = "/long/source/worktrees/task-a/project".to_string();
        app.services.model_name = "long-model-name-".repeat(10);
        handle
            .terminal
            .draw(|frame| {
                super::render_status_bar(frame, &app, ratatui::layout::Rect::new(0, 0, width, 2));
            })
            .unwrap();
        let row = &handle.snapshot()[0];
        assert!(row.contains("a/project"), "窄屏应保留真实目录末尾: {row}");
        assert!(row.width() <= width as usize);
    }
}
