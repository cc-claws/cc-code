use super::*;
use serde_json::json;

#[tokio::test]
async fn kernel_batch_preview_matches_actual_edits_and_preserves_line_endings() {
    for newline in ["\n", "\r\n"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("preview.txt");
        let original = ["起始", "alpha", "middle", "beta", "结束", ""].join(newline);
        let expected = ["起始", "beta", "middle", "gamma", "结束", ""].join(newline);
        std::fs::write(&path, &original).unwrap();
        for dialect in [false, true] {
            let input = if dialect {
                json!({"path":path,"edits":[{"oldText":"beta","newText":"gamma"},{"oldText":"alpha","newText":"beta"}]})
            } else {
                json!({"file_path":path,"edits":[{"old_string":"beta","new_string":"gamma"},{"old_string":"alpha","new_string":"beta"}]})
            };
            let preview = preview_batch_edit(&original, &input).unwrap();
            assert_eq!(preview, expected);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            EditFileTool::new(directory.path().to_string_lossy())
                .with_form(MutationForm::BatchReplace)
                .invoke(input)
                .await
                .unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), preview);
            std::fs::write(&path, &original).unwrap();
        }
    }
}

#[tokio::test]
async fn kernel_invalid_batch_has_neither_a_partial_preview_nor_partial_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid.txt");
    let original = "alpha beta beta";
    for edits in [
        json!([{"old_string":"alpha","new_string":"changed"},{"old_string":"missing","new_string":"replacement"}]),
        json!([{"old_string":"beta","new_string":"replacement"}]),
        json!([{"old_string":"alpha","new_string":"changed"},{"old_string":"alpha beta","new_string":"replacement"}]),
    ] {
        std::fs::write(&path, original).unwrap();
        let input = json!({"file_path":path,"edits":edits});
        assert!(preview_batch_edit(original, &input).is_err());
        assert!(EditFileTool::new(directory.path().to_string_lossy())
            .with_form(MutationForm::BatchReplace)
            .invoke(input)
            .await
            .is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }
}
