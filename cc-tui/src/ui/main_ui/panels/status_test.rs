    fn make_record(hit_rate_percent: u32) -> RequestRecord {
        // 构造一条记录，使得 cache_hit_rate() ≈ hit_rate_percent / 100.0
        let input_tokens = 1000u32;
        RequestRecord {
            input_tokens,
            output_tokens: 100,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: input_tokens * hit_rate_percent / 100,
        }
    }

    #[test]
    fn test_build_cache_rate_lines_adaptive_y_axis() {
        // 命中率集中在 85-99% 区间，y 轴应自适应而非固定 0-100%
        let records = vec![
            make_record(85),
            make_record(90),
            make_record(92),
            make_record(95),
            make_record(99),
        ];
        let lines = build_cache_rate_lines(&records, 5, 4);
        assert_eq!(lines.len(), 5); // 4 行 + 1 底部 x 轴

        // 顶部标签应接近 100%（nice_ceil），底部标签应 ≤ 85%
        let top_line = &lines[0];
        let top_spans = top_line.spans.iter().collect::<Vec<_>>();
        let top_label = top_spans[0].content.as_ref();
        assert!(top_label.contains("%"), "顶部标签应包含 %：{top_label}");

        // 底部 x 轴标签应 ≤ 85%（自适应下界）
        let bottom_line = &lines[4];
        let bottom_spans = bottom_line.spans.iter().collect::<Vec<_>>();
        let bottom_label = bottom_spans[0].content.as_ref();
        assert!(
            bottom_label.contains("%"),
            "底部标签应包含 %：{bottom_label}"
        );

        // 柱子不应全等高——至少有一个行包含空格（最低值的柱子比最高值矮）
        let has_space_row = lines
            .iter()
            .take(4)
            .any(|line| line.spans.iter().skip(1).any(|s| s.content.as_ref() == " "));
        assert!(has_space_row, "柱子不应全等高，至少有一行存在空格");
    }

    #[test]
    fn test_build_cache_rate_lines_all_same() {
        // 所有命中率相同，y 轴应加 padding
        let records = vec![make_record(95), make_record(95), make_record(95)];
        let lines = build_cache_rate_lines(&records, 3, 4);
        assert_eq!(lines.len(), 5);

        // 底部标签应 < 95%（有 padding）
        let bottom_spans = lines[4].spans.iter().collect::<Vec<_>>();
        let bottom_label = bottom_spans[0].content.as_ref();
        let bottom_val: u64 = bottom_label
            .trim_end_matches("%┼")
            .trim()
            .parse()
            .unwrap_or(100);
        assert!(
            bottom_val < 95,
            "所有值相同时底部标签应有 padding，实际：{bottom_label}"
        );
    }

    #[test]
    fn test_build_cache_rate_lines_empty() {
        let lines = build_cache_rate_lines(&[], 5, 4);
        assert!(lines.is_empty());
    }
