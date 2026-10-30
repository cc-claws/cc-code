    #[test]
    fn highlight_rust_code() {
        let result = highlight_code_block("rust", &["fn main() {}".to_string()], "base16-ocean.dark");
        assert!(result.is_some(), "rust 代码应被识别");
        let lines = result.unwrap();
        assert_eq!(lines.len(), 1);
        let has_content = lines[0]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
            .contains("fn main");
        assert!(has_content, "应有代码内容");
        let has_syntax_color = lines[0].spans.iter().any(|s| s.style.fg.is_some());
        assert!(has_syntax_color, "应有非前缀颜色的语法着色 span");
    }

    #[test]
    fn highlight_unknown_lang() {
        let result = highlight_code_block("unknown_lang_xyz", &["hello".to_string()], "base16-ocean.dark");
        assert!(result.is_none(), "未识别语言应返回 None");
    }

    #[test]
    fn highlight_empty_lang() {
        let result = highlight_code_block("", &["hello".to_string()], "base16-ocean.dark");
        assert!(result.is_none(), "空语言标签应返回 None");
    }

    #[test]
    fn highlight_multiline() {
        let lines = vec![
            "fn main() {".to_string(),
            "    println!(\"hello\");".to_string(),
            "}".to_string(),
        ];
        let result = highlight_code_block("rust", &lines, "base16-ocean.dark");
        assert!(result.is_some(), "多行 rust 代码应被识别");
        assert_eq!(result.unwrap().len(), 3, "输出行数应等于输入行数");
    }

    // ── Monokai Extended 颜色对齐测试（对齐 CC diff 配色）────────────

    /// 从单行高亮结果中提取 (text, fg) 对
    fn collect_spans(line: &ratatui::text::Line) -> Vec<(String, Option<ratatui::style::Color>)> {
        line.spans
            .iter()
            .map(|s| (s.content.to_string(), s.style.fg))
            .collect()
    }

    fn rgb(r: u8, g: u8, b: u8) -> Option<ratatui::style::Color> {
        Some(ratatui::style::Color::Rgb(r, g, b))
    }

    /// 找到包含目标子串的 span 的前景色
    fn fg_of(line: &ratatui::text::Line, needle: &str) -> Option<ratatui::style::Color> {
        let spans = collect_spans(line);
        spans
            .iter()
            .find(|(text, _)| text.contains(needle))
            .and_then(|(_, fg)| *fg)
    }

    #[test]
    fn test_monokai_extended_theme_registered() {
        // 主题已注册到 THEME_SET
        assert!(
            THEME_SET.themes.contains_key("Monokai Extended"),
            "Monokai Extended 主题应已注册"
        );
        assert!(
            THEME_SET.themes.contains_key("Monokai Extended Light"),
            "Monokai Extended Light 主题应已注册"
        );
        // 主题默认前景应为 CC 同款 #f8f8f2
        let dark = &THEME_SET.themes["Monokai Extended"];
        let fg = dark.settings.foreground.unwrap();
        assert_eq!(
            (fg.r, fg.g, fg.b),
            (248, 248, 242),
            "Monokai Extended 默认前景应为 #f8f8f2"
        );
    }

    #[test]
    fn test_monokai_extended_keyword_color_matches_cc() {
        // CC MONOKAI_SCOPES.keyword = rgb(249, 38, 114) 粉红
        let result = highlight_code_block("rust", &["if x == 1 {}".to_string()], "Monokai Extended");
        let line = &result.unwrap()[0];
        assert_eq!(fg_of(line, "if"), rgb(249, 38, 114), "`if` keyword 应为粉红 #F92672");
    }

    #[test]
    fn test_monokai_extended_storage_keyword_color_matches_cc() {
        // CC 把 const/fn/struct 等 storage 单独映射为青色 rgb(102, 217, 239)
        let result = highlight_code_block("rust", &["fn main() {}".to_string()], "Monokai Extended");
        let line = &result.unwrap()[0];
        let fn_fg = fg_of(line, "fn");
        let main_fg = fg_of(line, "main");
        assert_eq!(fn_fg, rgb(102, 217, 239), "`fn` storage keyword 应为青色 #66D9EF");
        assert_eq!(main_fg, rgb(166, 226, 46), "函数名应为绿色 #A6E22E");
    }

    #[test]
    fn test_monokai_extended_comment_color_matches_cc() {
        // CC MONOKAI_SCOPES.comment = rgb(117, 113, 94) 灰褐
        let result = highlight_code_block(
            "rust",
            &["// comment here".to_string()],
            "Monokai Extended",
        );
        let line = &result.unwrap()[0];
        assert_eq!(
            fg_of(line, "//"),
            rgb(117, 113, 94),
            "注释应为灰褐色 #75715E"
        );
    }

    #[test]
    fn test_monokai_extended_string_color_matches_cc() {
        // CC MONOKAI_SCOPES.string = rgb(230, 219, 116)
        let result = highlight_code_block(
            "rust",
            &["let s = \"hello\";".to_string()],
            "Monokai Extended",
        );
        let line = &result.unwrap()[0];
        let str_fg = fg_of(line, "hello");
        assert_eq!(str_fg, rgb(230, 219, 116), "字符串应为 #E6DB74");
        // let 也是 storage → 青色
        assert_eq!(fg_of(line, "let"), rgb(102, 217, 239), "`let` 应为青色 #66D9EF");
    }

    #[test]
    fn test_monokai_extended_number_color_matches_cc() {
        // CC MONOKAI_SCOPES.number = rgb(190, 132, 255) 紫色
        let result = highlight_code_block("rust", &["let x = 42;".to_string()], "Monokai Extended");
        let line = &result.unwrap()[0];
        assert_eq!(fg_of(line, "42"), rgb(190, 132, 255), "数字应为紫色 #BE84FF");
    }

    #[test]
    fn test_theme_set_contains_cc_syntax_themes() {
        // 对齐 CC `defaultSyntaxThemeName`：light → GitHub、ansi → ansi
        for name in ["GitHub", "ansi", "Monokai Extended", "Monokai Extended Light"] {
            assert!(
                THEME_SET.themes.contains_key(name),
                "主题集应包含 {name}"
            );
        }
        // GitHub 主题可实际着色
        let result = highlight_code_block("rust", &["fn main() {}".to_string()], "GitHub");
        assert!(result.is_some(), "GitHub 主题应能渲染 Rust 代码");
        let result = highlight_code_block("rust", &["fn main() {}".to_string()], "ansi");
        assert!(result.is_some(), "ansi 主题应能渲染 Rust 代码");
    }

    #[test]
    fn test_monokai_extended_light_theme_colors() {
        // Light 版默认前景应为主题定义值（非空即可，具体值随上游主题）
        let light = &THEME_SET.themes["Monokai Extended Light"];
        assert!(light.settings.foreground.is_some(), "Light 主题应有前景色");
        // light 主题下注释色（上游 Monokai Extended Light 沿用与 dark 相同的注释色 #75715E）
        let result = highlight_code_block(
            "rust",
            &["// comment".to_string()],
            "Monokai Extended Light",
        );
        let line = &result.unwrap()[0];
        assert_eq!(
            fg_of(line, "//"),
            Some(ratatui::style::Color::Rgb(117, 113, 94)),
            "Light 主题注释色应为 #75715E"
        );
    }

    #[test]
    fn test_syntect_color_to_ratatui_ansi_convention() {
        use syntect::highlighting::Color as SynColor;
        // alpha == 1 应映射为 None（终端默认色，自适应黑白背景）
        assert_eq!(
            syntect_color_to_ratatui(SynColor { r: 0, g: 0, b: 0, a: 1 }),
            None,
            "alpha=1 应被解析为 None 终端默认前景色"
        );
        // alpha == 0 应映射为 ANSI 16 色
        assert_eq!(
            syntect_color_to_ratatui(SynColor { r: 1, g: 0, b: 0, a: 0 }),
            Some(ratatui::style::Color::Red),
            "alpha=0, r=1 应映射为 Red"
        );
        assert_eq!(
            syntect_color_to_ratatui(SynColor { r: 5, g: 0, b: 0, a: 0 }),
            Some(ratatui::style::Color::Magenta),
            "alpha=0, r=5 应映射为 Magenta"
        );
        // alpha == 255 应映射为真实 RGB
        assert_eq!(
            syntect_color_to_ratatui(SynColor { r: 248, g: 248, b: 242, a: 255 }),
            Some(ratatui::style::Color::Rgb(248, 248, 242)),
            "alpha=255 应保留 24 位真实 RGB"
        );
    }

    #[test]
    fn test_ansi_theme_does_not_render_pitch_black() {
        let code = "$colNames = array_map(function($c) { return $c->Field; }, $columns);";
        let result = highlight_code_block("php", &[code.to_string()], "ansi");
        assert!(result.is_some(), "ansi 主题应能解析 php 代码");
        let line = &result.unwrap()[0];
        for span in &line.spans {
            if let Some(fg) = span.style.fg {
                assert!(
                    !matches!(fg, ratatui::style::Color::Rgb(0..=6, 0, 0)),
                    "ansi 主题下不应存在 RGB 值为 0~6 的伪纯黑 span: {:?}",
                    span
                );
            }
        }
        let has_none_fg = line.spans.iter().any(|s| s.style.fg.is_none());
        assert!(has_none_fg, "未匹配 scope 的默认文本前景色应为 None 自适应终端");
    }



