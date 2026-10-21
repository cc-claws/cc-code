use super::*;

/// 每一行都必须等宽，否则渲染出的吉祥物会歪斜。
#[test]
fn test_mascot_pixel_rows_are_rectangular() {
    let width = MASCOT[0].len();
    for (i, row) in MASCOT.iter().enumerate() {
        assert_eq!(row.len(), width, "第 {i} 行宽度不一致: {row:?}");
    }
    assert_eq!(MASCOT.len() % 2, 0, "像素行必须成对，才能压成整字符行");
}

/// 只允许三种像素：左青、右橙、空。出现别的字符说明设计稿写错了。
#[test]
fn test_mascot_uses_only_known_pixels() {
    for row in MASCOT {
        for ch in row.chars() {
            assert!(matches!(ch, 'C' | 'O' | '.'), "未知像素 {ch:?} in {row:?}");
        }
    }
}

/// 左右两半必须各自只出现一种颜色，保住「左青右橙」的品牌对分。
#[test]
fn test_mascot_left_half_is_cyan_and_right_half_is_orange() {
    let half = MASCOT[0].len() / 2;
    for (i, row) in MASCOT.iter().enumerate() {
        for (col, ch) in row.chars().enumerate() {
            if col < half {
                assert_ne!(ch, 'O', "第 {i} 行 {col} 列越界成橙色: {row:?}");
            } else {
                assert_ne!(ch, 'C', "第 {i} 行 {col} 列越界成青色: {row:?}");
            }
        }
    }
}

/// 眼睛必须是挖空的：那一格上下两个像素都为空，靠终端底色形成对比。
#[test]
fn test_mascot_eyes_are_punched_through() {
    // 第 5 个像素行是眼睛行；第 2-3 列与第 8-9 列各是一只 2 像素宽的眼睛。
    let eye_row = 5usize;
    for col in [2, 3, 8, 9] {
        let ch = MASCOT[eye_row].as_bytes()[col];
        assert_eq!(ch, b'.', "({eye_row}, {col}) 应为眼睛挖空，实际 {ch:?}");
    }
    // 眼睛左右仍须有脸部像素，否则挖空会连到轮廓外
    assert_ne!(MASCOT[eye_row].as_bytes()[1], b'.', "左眼不能贴到边缘");
    assert_ne!(MASCOT[eye_row].as_bytes()[10], b'.', "右眼不能贴到边缘");
}

/// 压行结果：5 行 × 12 列，且每行字符数一致。
#[test]
fn test_mascot_compresses_to_five_aligned_rows() {
    let lines = mascot_lines();
    assert_eq!(lines.len(), 5);
    let widths: Vec<usize> = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.chars().count()).sum())
        .collect();
    assert!(
        widths.iter().all(|w| *w == widths[0]),
        "各行宽度不一致: {widths:?}"
    );
    assert_eq!(widths[0], 12);
}

/// 空像素不能带背景色，否则会盖住终端底色（浅色主题下变成黑块）。
#[test]
fn test_mascot_empty_pixels_stay_transparent() {
    for line in mascot_lines() {
        for span in &line.spans {
            if span.content == " " {
                assert_eq!(span.style.bg, None, "空格不应设置背景色");
                assert_eq!(span.style.fg, None, "空格不应设置前景色");
            }
        }
    }
}

#[tokio::test]
async fn test_welcome_measurement_matches_left_aligned_card_bounds() {
    for width in [40, 80, 120] {
        let (mut app, mut handle) = App::new_headless(128, 20).await;
        app.services.peri_config = None;
        let height = welcome_height(&app, width);
        let area = Rect::new(3, 2, width, height);
        handle
            .terminal
            .draw(|f| render_welcome(f, &app, area))
            .expect("欢迎卡应能绘制");
        let buffer = handle.terminal.backend().buffer();
        assert_eq!(buffer[(3, 2)].symbol(), "╭", "欢迎卡应从区域左上角开始");
        assert_eq!(
            buffer[(3, 2 + height - 1)].symbol(),
            "╰",
            "测量高度应与实际边界相同"
        );
        assert_eq!(
            buffer[(3 + width.min(CARD_MAX_WIDTH) - 1, 2)].symbol(),
            "╮",
            "宽度应限制在卡片上限"
        );
        assert_eq!(height, if width >= MASCOT_MIN_WIDTH { 7 } else { 6 });
    }
}

#[tokio::test]
async fn test_welcome_localizes_hints_and_keeps_actual_directory_and_version() {
    for (language, hint) in [("en", "Start chatting"), ("zh-CN", "开始对话")] {
        let (mut app, mut handle) = App::new_headless(80, 12).await;
        app.services.lc.switch(language).expect("测试语言可用");
        app.services.cwd = "/project/终端".into();
        app.services.peri_config = None;
        handle
            .terminal
            .draw(|f| {
                let area = f.area();
                render_welcome(f, &app, area);
            })
            .expect("欢迎卡应能绘制");
        let snapshot = handle.snapshot().join("\n");
        assert!(
            snapshot.contains(&format!("CC Code  v{}", env!("CARGO_PKG_VERSION"))),
            "应展示真实版本"
        );
        assert!(
            snapshot.replace(' ', "").contains("/project/终端"),
            "应展示当前目录（忽略 TestBackend 宽字符占位）"
        );
        assert!(
            snapshot.replace(' ', "").contains(&hint.replace(' ', "")),
            "使用提示应采用当前语言（忽略宽字符占位）"
        );
        assert!(snapshot.contains("/help"), "应保留帮助命令入口");
        assert!(
            snapshot.contains("/login"),
            "未配置模型服务时应保留登录引导"
        );
    }
}

#[tokio::test]
async fn test_welcome_truncates_chinese_and_emoji_directory_by_display_width() {
    use unicode_width::UnicodeWidthStr;
    let (mut app, mut handle) = App::new_headless(40, 12).await;
    app.services.cwd = format!("/项目/{}", "终端🦀目录".repeat(20));
    let lines = welcome_lines(&app, 36);
    let directory = lines[1].to_string();
    assert!(directory.ends_with('…'), "长目录应以省略号结束");
    assert!(directory.width() <= 36, "中文和 emoji 不能越过分配的显示列");
    handle
        .terminal
        .draw(|f| {
            let area = f.area();
            render_welcome(f, &app, area);
        })
        .expect("中文和 emoji 欢迎卡应能绘制");
    assert!(
        handle.snapshot().join("\n").contains('…'),
        "实际帧应采用相同省略逻辑"
    );
}

#[tokio::test]
async fn test_welcome_short_region_skips_mascot_and_keeps_text() {
    let (app, mut handle) = App::new_headless(80, 12).await;
    handle
        .terminal
        .draw(|f| render_welcome(f, &app, Rect::new(0, 0, 80, 6)))
        .expect("矮区域应能绘制");
    let snapshot = handle.snapshot().join("\n");
    assert!(!snapshot.contains("▀▀▀▀"), "高度不足时必须完整省略吉祥物");
    assert!(snapshot.contains("CC Code"), "文本标题优先保留");
    assert!(snapshot.contains("/help"), "使用提示优先保留");
}

#[tokio::test]
async fn test_welcome_tiny_regions_do_not_overflow() {
    let (app, mut handle) = App::new_headless(16, 12).await;
    for (width, height) in [(0, 0), (1, 1), (2, 2), (3, 4), (4, 3), (10, 3)] {
        handle
            .terminal
            .draw(|f| render_welcome(f, &app, Rect::new(2, 1, width, height)))
            .expect("极小区域不应越界或崩溃");
    }
    assert_eq!(welcome_height(&app, 0), 0, "无可用列时不应要求欢迎区高度");
}

#[tokio::test]
async fn test_welcome_slice_matches_complete_card_at_nonzero_coordinates() {
    for width in [80, 40] {
        let (mut app, mut handle) = App::new_headless(width + 6, 16).await;
        app.services.peri_config = None;
        app.services.cwd = "/项目/终端🦀目录".into();
        let height = welcome_height(&app, width);
        let complete_area = Rect::new(3, 2, width, height);
        handle
            .terminal
            .draw(|f| render_welcome(f, &app, complete_area))
            .expect("完整欢迎卡应能绘制");
        let complete = handle.terminal.backend().buffer().clone();
        for offset in 0..=usize::from(height) + 1 {
            for slice_height in [1, 3, height + 2] {
                let area = Rect::new(3, 2, width, slice_height);
                let mut copied_rows = 0;
                let mut rendered = None;
                handle
                    .terminal
                    .draw(|f| {
                        for cell in &mut f.buffer_mut().content {
                            cell.set_symbol(".");
                        }
                        copied_rows = render_welcome_slice(f, &app, area, offset);
                        // 宽字符后续格在 TestBackend diff 中会被跳过，直接检查实际帧。
                        rendered = Some(f.buffer_mut().clone());
                    })
                    .expect("欢迎卡裁切应能绘制");
                let expected_rows = usize::from(height)
                    .saturating_sub(offset)
                    .min(usize::from(slice_height)) as u16;
                assert_eq!(copied_rows, expected_rows, "应返回实际复制行数");
                let actual = rendered.expect("裁切帧快照");
                for y in 0..16 {
                    for x in 0..width + 6 {
                        if x >= area.x
                            && x < area.right()
                            && y >= area.y
                            && y < area.y + copied_rows
                        {
                            assert_eq!(
                                actual[(x, y)],
                                complete[(x, y + offset as u16)],
                                "宽度 {width}、偏移 {offset} 的裁切必须逐格保留完整卡片"
                            );
                        } else {
                            assert_eq!(actual[(x, y)].symbol(), ".", "不能覆盖复制区域以外的格子");
                        }
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn test_welcome_slice_tiny_and_outside_regions_do_not_overflow() {
    let (app, mut handle) = App::new_headless(8, 6).await;
    for (area, offset, expected_rows) in [
        (Rect::new(2, 1, 0, 2), 0, 0),
        (Rect::new(2, 1, 2, 0), 0, 0),
        (Rect::new(2, 1, 1, 1), 0, 1),
        (Rect::new(2, 1, 2, 2), 1, 2),
        (Rect::new(7, 5, 4, 3), 0, 1),
        (Rect::new(9, 7, 4, 3), 0, 0),
        (Rect::new(2, 1, 1, 1), usize::MAX, 0),
    ] {
        let mut copied_rows = 0;
        handle
            .terminal
            .draw(|f| copied_rows = render_welcome_slice(f, &app, area, offset))
            .expect("极小或越出帧边界的裁切不能崩溃");
        assert_eq!(copied_rows, expected_rows, "只应复制目标帧内存在的行");
    }
}
