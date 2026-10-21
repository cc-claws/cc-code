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

/// 边框必须逐行闭合，中文与英文、各种内容组合下都不能被文字挤开。
///
/// 断言「每一行最后一个非空格子」正好落在右边框列——宽字符（中文）会占两格，
/// 若按字符数而非显示列宽排版，右边框就会被推到更右边而破框。
#[tokio::test]
async fn test_welcome_border_closes_on_every_row_in_both_languages() {
    use crate::config::{AppConfig, PeriConfig, ThinkingConfig};

    for lang in ["en", "zh-CN"] {
        for (has_update, has_provider) in [(false, true), (true, true), (false, false)] {
            let (mut app, mut handle) = App::new_headless(66, 12).await;
            app.services.model_name = "claude-fable-5".to_string();
            app.services.cwd = r"D:\code\peri".to_string();
            app.services.lc.switch(lang).expect("测试语言可用");
            app.services.peri_config = has_provider.then(|| PeriConfig {
                schema: None,
                config: AppConfig {
                    providers: vec![crate::config::ProviderConfig {
                        id: "t".into(),
                        name: Some("T".into()),
                        ..Default::default()
                    }],
                    thinking: Some(ThinkingConfig {
                        enabled: false,
                        budget_tokens: 8000,
                        effort: "max".to_string(),
                        max_tokens: 32000,
                    }),
                    ..Default::default()
                },
            });
            if has_update {
                app.services
                    .update_checker
                    .lock()
                    .set_status_for_test(UpdateStatus::Available {
                        latest: "0.6.999".to_string(),
                    });
            }
            handle
                .terminal
                .draw(|f| render_welcome(f, &app, f.area()))
                .expect("欢迎卡应能绘制");

            let buffer = handle.terminal.backend().buffer().clone();
            let height = welcome_height(&app, 66);
            let width = 66u16.min(CARD_MAX_WIDTH);
            let case = format!("lang={lang} update={has_update} provider={has_provider}");

            // 四角
            assert_eq!(buffer[(0, 0)].symbol(), "╭", "{case} 左上角");
            assert_eq!(buffer[(width - 1, 0)].symbol(), "╮", "{case} 右上角");
            assert_eq!(buffer[(0, height - 1)].symbol(), "╰", "{case} 左下角");
            assert_eq!(
                buffer[(width - 1, height - 1)].symbol(),
                "╯",
                "{case} 右下角"
            );

            for y in 0..height {
                // 左右边框：首末行是圆角，中间行是竖线
                let (left, right) = if y == 0 {
                    ("╭", "╮")
                } else if y == height - 1 {
                    ("╰", "╯")
                } else {
                    ("│", "│")
                };
                assert_eq!(buffer[(0, y)].symbol(), left, "{case} row {y} 左边界");
                // 每行最后一个非空格必须正好是右边界列——宽字符排错时会右移
                let last_nonblank = (0..width).rev().find(|&x| buffer[(x, y)].symbol() != " ");
                assert_eq!(
                    last_nonblank,
                    Some(width - 1),
                    "{case} row {y} 最后一个非空格应落在右边界列 {}",
                    width - 1
                );
                assert_eq!(
                    buffer[(width - 1, y)].symbol(),
                    right,
                    "{case} row {y} 右边界"
                );
            }

            // 卡片右侧（终端比卡片宽时）不能有溢出内容
            for x in width..66u16 {
                for y in 0..height {
                    assert_eq!(
                        buffer[(x, y)].symbol(),
                        " ",
                        "{case} 卡片外 x={x} y={y} 不应有内容"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn test_welcome_tiny_regions_do_not_overflow() {    let (app, mut handle) = App::new_headless(16, 12).await;
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

// ── 副标题：model · (effort) ─────────────────────────────────────────────────

/// 构造一份带 thinking 配置的 app，用于副标题与更新提示的断言。
fn app_with_thinking(model: &str, effort: &str) -> impl std::future::Future<Output = App> {
    use crate::config::{AppConfig, PeriConfig, ThinkingConfig};
    let model = model.to_string();
    let effort = effort.to_string();
    async move {
        let (mut app, _handle) = App::new_headless(80, 12).await;
        app.services.model_name = model;
        app.services.peri_config = Some(PeriConfig {
            schema: None,
            config: AppConfig {
                providers: vec![crate::config::ProviderConfig {
                    id: "test".into(),
                    name: Some("Test".into()),
                    ..Default::default()
                }],
                thinking: Some(ThinkingConfig {
                    enabled: false,
                    budget_tokens: 8000,
                    effort,
                    max_tokens: 32000,
                }),
                ..Default::default()
            },
        });
        app
    }
}

/// 标题行：品牌名 ACCENT+BOLD，版本号降级 MUTED（同一行两个 Span）。
#[tokio::test]
async fn test_welcome_title_splits_brand_accent_and_version_muted() {
    let (app, _handle) = App::new_headless(80, 12).await;
    let lines = welcome_lines(&app, 60);
    let title = &lines[0];
    assert_eq!(title.spans.len(), 2, "标题应拆成品牌名与版本号两段");

    assert_eq!(title.spans[0].content.as_ref(), "CC Code  ");
    assert_eq!(
        title.spans[0].style.fg,
        Some(theme::cur().claude),
        "品牌名应为 ACCENT"
    );
    assert!(
        title.spans[0].style.add_modifier.contains(Modifier::BOLD),
        "品牌名应保持 BOLD"
    );

    assert_eq!(
        title.spans[1].content.as_ref(),
        format!("v{}", env!("CARGO_PKG_VERSION")),
        "第二段只应包含版本号"
    );
    assert_eq!(
        title.spans[1].style.fg,
        Some(theme::cur().inactive),
        "版本号应为 MUTED"
    );
    assert!(
        !title.spans[1].style.add_modifier.contains(Modifier::BOLD),
        "版本号不应继承 BOLD"
    );
}

#[tokio::test]
async fn test_welcome_subtitle_shows_model_and_effort() {
    let app = app_with_thinking("claude-fable-5", "max").await;
    let lines = welcome_lines(&app, 60);
    let subtitle = lines[1].to_string();
    assert_eq!(
        subtitle, "claude-fable-5 · (max effort)",
        "第二行应为 `model · (effort effort)`，实际: {subtitle:?}"
    );
}

#[tokio::test]
async fn test_welcome_subtitle_omitted_without_model_name() {
    let app = app_with_thinking("", "max").await;
    let lines = welcome_lines(&app, 60);
    assert!(
        !lines.iter().any(|line| line.to_string().contains("· (")),
        "没有模型名时不应渲染半截副标题: {:?}",
        lines.iter().map(|l| l.to_string()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_welcome_subtitle_omitted_when_thinking_disabled() {
    let app = app_with_thinking("claude-fable-5", "").await;
    let lines = welcome_lines(&app, 60);
    assert!(
        !lines.iter().any(|line| line.to_string().contains("· (")),
        "未配置 effort 时不应编造档位: {:?}",
        lines.iter().map(|l| l.to_string()).collect::<Vec<_>>()
    );
}

/// 无 Provider 时副标题整行省略，改用一行 `/login` 引导——两者抵消，行数不变。
#[tokio::test]
async fn test_welcome_omits_subtitle_and_shows_login_hint_without_provider() {
    let (mut app, _handle) = App::new_headless(80, 12).await;
    app.services.model_name = "claude-fable-5".to_string();
    app.services.peri_config = None;

    let lines = welcome_lines(&app, 60);
    let joined = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.contains("effort"),
        "未配置 Provider 时不应显示模型/档位副标题:\n{joined}"
    );
    assert!(
        joined.contains("/login"),
        "未配置 Provider 时应保留登录引导:\n{joined}"
    );
    assert_eq!(
        lines.len(),
        4,
        "标题 + 目录 + 开始提示 + 登录引导，共 4 行:\n{joined}"
    );
}

/// 卡片高度不变量：有吉祥物时恒为 7 行（内高 5 + 上下边框），
/// 与「是否有更新」「是否配置 Provider」都无关。
#[tokio::test]
async fn test_welcome_card_height_is_always_seven_rows_with_mascot() {
    const WIDE: u16 = 80;
    for has_provider in [false, true] {
        for has_update in [false, true] {
            let (mut app, _handle) = App::new_headless(WIDE, 12).await;
            app.services.model_name = "claude-fable-5".to_string();
            if has_provider {
                use crate::config::{AppConfig, PeriConfig, ThinkingConfig};
                app.services.peri_config = Some(PeriConfig {
                    schema: None,
                    config: AppConfig {
                        providers: vec![crate::config::ProviderConfig {
                            id: "test".into(),
                            name: Some("Test".into()),
                            ..Default::default()
                        }],
                        thinking: Some(ThinkingConfig {
                            enabled: false,
                            budget_tokens: 8000,
                            effort: "max".to_string(),
                            max_tokens: 32000,
                        }),
                        ..Default::default()
                    },
                });
            } else {
                app.services.peri_config = None;
            }
            if has_update {
                app.services
                    .update_checker
                    .lock()
                    .set_status_for_test(UpdateStatus::Available {
                        latest: "0.6.999".to_string(),
                    });
            }
            assert_eq!(
                welcome_height(&app, WIDE),
                7,
                "provider={has_provider} update={has_update} 时卡片应恒为 7 行"
            );
        }
    }
}

// ── 更新提示 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_welcome_hides_update_hint_when_unknown_or_up_to_date() {
    for status in [UpdateStatus::Unknown, UpdateStatus::UpToDate] {
        let (app, mut handle) = App::new_headless(80, 12).await;
        app.services
            .update_checker
            .lock()
            .set_status_for_test(status);
        handle
            .terminal
            .draw(|f| render_welcome(f, &app, f.area()))
            .expect("欢迎卡应能绘制");
        let snapshot = handle.snapshot().join("\n");
        assert!(
            !snapshot.contains("cc-code update"),
            "无更新或状态未知时不应显示升级命令:\n{snapshot}"
        );
    }
}

#[tokio::test]
async fn test_welcome_shows_update_hint_with_versions_when_available() {
    let (app, mut handle) = App::new_headless(80, 12).await;
    app.services
        .update_checker
        .lock()
        .set_status_for_test(UpdateStatus::Available {
            latest: "0.6.999".to_string(),
        });
    handle
        .terminal
        .draw(|f| render_welcome(f, &app, f.area()))
        .expect("欢迎卡应能绘制");
    let snapshot = handle.snapshot().join("\n");
    assert!(
        snapshot.contains("0.6.999"),
        "应展示线上最新版本号:\n{snapshot}"
    );
    assert!(
        snapshot.contains("cc-code update"),
        "应给出升级命令:\n{snapshot}"
    );
}

/// 有吉祥物时，更新提示**替换**开始提示，卡片高度必须不变。
///
/// 内高由吉祥物的 5 行撑住：无更新时文本 4 行（标题/模型/目录/开始提示），
/// 有更新时文本 5 行（标题/模型/目录/更新提示/升级命令）。两者都在 5 行以内，
/// 因此外框高度恒定。窄卡片没有吉祥物撑高，文本多一行自然高一行——不算缺陷。
#[tokio::test]
async fn test_welcome_update_hint_replaces_start_hint_without_growing_card() {
    const WIDE: u16 = 80;
    let (mut app, _handle) = App::new_headless(WIDE, 12).await;
    app.services.peri_config = None;
    let base = welcome_height(&app, WIDE);

    app.services
        .update_checker
        .lock()
        .set_status_for_test(UpdateStatus::Available {
            latest: "0.6.999".to_string(),
        });
    assert_eq!(
        welcome_height(&app, WIDE),
        base,
        "有吉祥物时更新提示替换开始提示，卡片高度不应变化"
    );

    // 替换语义：更新提示出现时，「开始对话」提示必须同时消失。
    let lines = welcome_lines(&app, 60);
    let joined = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.contains("开始对话") && !joined.contains("Start chatting"),
        "有更新时不应再显示开始提示:\n{joined}"
    );
}

/// 更新提示沿用主题调色板：警告色 + 次要色，不写死 RGB。
#[tokio::test]
async fn test_welcome_update_hint_uses_theme_palette() {
    let (app, _handle) = App::new_headless(80, 12).await;
    app.services
        .update_checker
        .lock()
        .set_status_for_test(UpdateStatus::Available {
            latest: "0.6.999".to_string(),
        });
    let lines = welcome_lines(&app, 60);
    let index = lines
        .iter()
        .position(|line| line.to_string().contains("0.6.999"))
        .expect("应存在更新提示行");
    assert_eq!(
        lines[index].spans[0].style.fg,
        Some(theme::cur().warning),
        "更新提示行应使用主题 WARNING 色"
    );
    assert_eq!(
        lines[index + 1].spans[0].style.fg,
        Some(theme::cur().inactive),
        "升级命令行应使用主题 MUTED 色"
    );
}
