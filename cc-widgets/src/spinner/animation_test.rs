    #[test]
    fn test_tick_to_frame_cycle() {
        for i in 0..20 {
            let frame = tick_to_frame(i);
            assert!(
                BRAILLE_FRAMES.contains(&frame),
                "tick {} returned {:?} not in BRAILLE_FRAMES",
                i,
                frame
            );
        }
    }

    #[test]
    fn test_smooth_increment_convergence() {
        let mut displayed = 0;
        let target = 100;
        for _ in 0..200 {
            displayed = smooth_increment(displayed, target);
            if displayed >= target {
                break;
            }
        }
        assert_eq!(displayed, target);
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(90_000), "1m 30s");
        assert_eq!(format_elapsed(30_000), "30s");
        assert_eq!(format_elapsed(5_000), "5s");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(2200), "2.2k");
        assert_eq!(format_tokens(15000), "15k");
        assert_eq!(format_tokens(0), "0");
    }

    #[test]
    fn test_shimmer_verb_spans_empty_text() {
        let base = Color::Rgb(215, 119, 87);
        let spans = shimmer_verb_spans("", 100, base, base);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_glimmer_index_cycle() {
        // 周期 = 文本宽 + 20，位置从 −10 起、每 50ms 步进一列
        assert_eq!(glimmer_index(0, 9), -10);
        assert_eq!(glimmer_index(49, 9), -10);
        assert_eq!(glimmer_index(50, 9), -9);
        // 一个周期（29 步）后回到起点
        assert_eq!(glimmer_index(29 * 50, 9), -10);
        assert_eq!(glimmer_index(28 * 50, 9), 18);
    }

    #[test]
    fn test_shimmer_verb_spans_parked_phase() {
        // elapsed=0 时流光位置 −10（文本外），整词单段底色
        let base = Color::Rgb(215, 119, 87);
        let shimmer = Color::Rgb(255, 200, 170);
        let spans = shimmer_verb_spans("Thinking…", 0, base, shimmer);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "Thinking…");
        assert_eq!(spans[0].style.fg, Some(base));
    }

    #[test]
    fn test_shimmer_verb_spans_band_is_plus_minus_one_column() {
        // "Thinking…" 宽 9 列；elapsed=750ms → 第 15 步 → 流光位置 5
        // 只有中心列 4/5/6（"kin"）用 shimmer 色，其余底色
        let base = Color::Rgb(215, 119, 87);
        let shimmer = Color::Rgb(255, 200, 170);
        let spans = shimmer_verb_spans("Thinking…", 750, base, shimmer);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "Thin");
        assert_eq!(spans[0].style.fg, Some(base));
        assert_eq!(spans[1].content, "kin");
        assert_eq!(spans[1].style.fg, Some(shimmer));
        assert_eq!(spans[2].content, "g…");
        assert_eq!(spans[2].style.fg, Some(base));
    }

    #[test]
    fn test_shimmer_verb_spans_named_colors_hard_switch() {
        // ANSI 命名色同样按 ±1 列硬切（不插值、不降级为单色）
        let spans = shimmer_verb_spans("Thinking…", 750, Color::Yellow, Color::White);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(spans[1].style.fg, Some(Color::White));
        assert_eq!(spans[2].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_blend_color_rgb_and_named() {
        let black = Color::Rgb(0, 0, 0);
        let white = Color::Rgb(200, 100, 50);
        assert_eq!(blend_color(black, white, 0.5), Color::Rgb(100, 50, 25));
        assert_eq!(blend_color(black, white, 0.0), black);
        assert_eq!(blend_color(black, white, 1.0), white);
        // 命名色降级：t>0.5 取目标色
        assert_eq!(blend_color(Color::Red, Color::Blue, 0.4), Color::Red);
        assert_eq!(blend_color(Color::Red, Color::Blue, 0.6), Color::Blue);
    }

    #[test]
    fn test_stall_intensity_target_cc_thresholds() {
        // CC 2.1.300：无 token 10s 触发、再 10s 渐满；工具活跃永不触发
        assert_eq!(stall_intensity_target(9_999, false), 0.0);
        assert_eq!(stall_intensity_target(10_000, false), 0.0);
        assert_eq!(stall_intensity_target(15_000, false), 0.5);
        assert_eq!(stall_intensity_target(20_000, false), 1.0);
        assert_eq!(stall_intensity_target(120_000, false), 1.0);
        assert_eq!(stall_intensity_target(120_000, true), 0.0);
    }

    #[test]
    fn test_thinking_intensity_cc_thresholds() {
        assert_eq!(thinking_intensity(10_000), 0.0);
        assert_eq!(thinking_intensity(15_000), 0.5);
        assert_eq!(thinking_intensity(20_000), 1.0);
        assert_eq!(thinking_intensity(90_000), 1.0);
    }
