use super::*;

#[test]
fn test_parse_ask_user_qa_items_single_pair() {
    let content = "[问: header1]\n回答: value1";
    assert_eq!(
        parse_ask_user_qa_items(content),
        vec!["header1 → value1".to_string()]
    );
}

#[test]
fn test_parse_ask_user_qa_items_multiple_pairs() {
    let content = "[问: header1]\n回答: value1\n\n[问: header2]\n回答: value2";
    assert_eq!(
        parse_ask_user_qa_items(content),
        vec![
            "header1 → value1".to_string(),
            "header2 → value2".to_string()
        ]
    );
}

#[test]
fn test_parse_ask_user_qa_items_empty_input() {
    assert!(parse_ask_user_qa_items("").is_empty());
}
