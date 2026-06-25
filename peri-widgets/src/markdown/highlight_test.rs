    #[test]
    fn highlight_rust_code() {
        let result = highlight_code_block("rust", &["fn main() {}".to_string()]);
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
        let result = highlight_code_block("unknown_lang_xyz", &["hello".to_string()]);
        assert!(result.is_none(), "未识别语言应返回 None");
    }

    #[test]
    fn highlight_empty_lang() {
        let result = highlight_code_block("", &["hello".to_string()]);
        assert!(result.is_none(), "空语言标签应返回 None");
    }

    #[test]
    fn highlight_multiline() {
        let lines = vec![
            "fn main() {".to_string(),
            "    println!(\"hello\");".to_string(),
            "}".to_string(),
        ];
        let result = highlight_code_block("rust", &lines);
        assert!(result.is_some(), "多行 rust 代码应被识别");
        assert_eq!(result.unwrap().len(), 3, "输出行数应等于输入行数");
    }
