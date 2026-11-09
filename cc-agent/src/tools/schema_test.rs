use super::{validate_against_schema, validate_deferred_arguments};
use serde_json::json;

#[test]
fn test_explicit_additional_properties_preserves_declared_field_validation() {
    let schema = json!({
        "type": "object",
        "properties": {"task_id": {"type": "string"}},
        "required": ["task_id"],
        "additionalProperties": true
    });
    assert!(validate_deferred_arguments(
        &json!({"task_id": "task-1", "extra": {"label": "value"}}),
        &schema
    )
    .is_ok());
    assert!(
        validate_deferred_arguments(&json!({"task_id": 1, "extra": true}), &schema)
            .unwrap_err()
            .contains("'task_id' type is expected as string")
    );
}

#[test]
fn test_additional_properties_schema_validates_dynamic_dictionary_values() {
    let schema = json!({
        "type": "object",
        "additionalProperties": {
            "type": "object",
            "properties": {"enabled": {"type": "boolean"}},
            "required": ["enabled"]
        }
    });
    assert!(
        validate_deferred_arguments(&json!({"dynamic_key": {"enabled": true}}), &schema).is_ok()
    );
    let error = validate_deferred_arguments(&json!({"dynamic_key": {"enabled": "yes"}}), &schema)
        .unwrap_err();
    assert!(error.contains("dynamic_key"));
    assert!(error.contains("'enabled' type is expected as boolean"));
}

#[test]
fn test_additional_properties_false_rejects_keys_without_properties() {
    let schema = json!({"type": "object", "additionalProperties": false});
    assert!(validate_deferred_arguments(&json!({}), &schema).is_ok());
    assert!(validate_deferred_arguments(&json!({"extra": 1}), &schema)
        .unwrap_err()
        .contains("Unexpected parameter 'extra'"));
}

#[test]
fn test_deferred_dynamic_fields_preserve_strict_builtin_behavior() {
    let schema = json!({"type": "object", "properties": {}});
    let input = json!({"dynamic_key": "value"});
    assert!(validate_deferred_arguments(&input, &schema).is_ok());
    assert!(validate_against_schema(&input, &schema)
        .unwrap_err()
        .contains("Unexpected parameter 'dynamic_key'"));
}

#[test]
fn test_deferred_pattern_properties_are_left_to_target_validation() {
    let schema = json!({
        "type": "object",
        "patternProperties": {"^option_": {"type": "string"}},
        "additionalProperties": false
    });
    assert!(validate_deferred_arguments(&json!({"option_name": "value"}), &schema).is_ok());
}

#[test]
fn test_deferred_nullable_required_fields_accept_null_but_require_presence() {
    for property in [
        json!({"anyOf": [{"type": "string"}, {"type": "null"}]}),
        json!({"type": ["string", "null"]}),
        json!({}),
    ] {
        let schema = json!({
            "type": "object",
            "properties": {"value": property},
            "required": ["value"]
        });
        assert!(validate_deferred_arguments(&json!({"value": null}), &schema).is_ok());
        assert!(validate_deferred_arguments(&json!({}), &schema).is_err());
    }
    let schema = json!({
        "type": "object",
        "properties": {"value": {"type": "string"}},
        "required": ["value"]
    });
    assert!(validate_deferred_arguments(&json!({"value": null}), &schema).is_err());
    // required 约束字段存在性，不要求该字段必须同时出现在 properties 中。
    let schema = json!({"type": "object", "required": ["value"]});
    assert!(validate_deferred_arguments(&json!({"value": null}), &schema).is_ok());
    assert!(validate_deferred_arguments(&json!({}), &schema).is_err());
}
