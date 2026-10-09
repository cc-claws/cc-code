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
        let spans = shimmer_verb_spans("", 100, Color::Rgb(215, 119, 87));
        assert!(spans.is_empty());
    }

    #[test]
    fn test_shimmer_verb_spans_non_rgb_fallback() {
        let spans = shimmer_verb_spans("Executing…", 500, Color::Yellow);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "Executing…");
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_shimmer_verb_spans_quiet_phase() {
        // At 3000ms (within 5000ms period, after 1600ms sweep), shimmer is quiet.
        let base = Color::Rgb(215, 119, 87);
        let spans = shimmer_verb_spans("Thinking…", 3000, base);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "Thinking…");
        assert_eq!(spans[0].style.fg, Some(base));
    }

    #[test]
    fn test_shimmer_verb_spans_shimmering_phase() {
        // At 800ms (peak center of 1600ms sweep), shimmer produces grapheme spans.
        let base = Color::Rgb(215, 119, 87);
        let spans = shimmer_verb_spans("Thinking…", 800, base);
        assert_eq!(spans.len(), "Thinking…".chars().count());
        // All spans should have an RGB color.
        for span in spans {
            assert!(matches!(span.style.fg, Some(Color::Rgb(..))));
        }
    }
