    #[test]
    fn test_should_collapse_read() {
        assert!(should_collapse_by_default("Read"));
    }

    #[test]
    fn test_should_not_collapse_bash() {
        assert!(!should_collapse_by_default("Bash"));
    }

    #[test]
    fn test_truncate_result_short() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {}", i)).collect();
        let (result, omitted) = truncate_result(&lines, 20);
        assert_eq!(result.len(), 10);
        assert!(omitted.is_none());
    }

    #[test]
    fn test_truncate_result_long() {
        let lines: Vec<String> = (0..30).map(|i| format!("line {}", i)).collect();
        let (result, omitted) = truncate_result(&lines, 20);
        assert_eq!(result.len(), 20);
        assert_eq!(omitted, Some(10));
    }
