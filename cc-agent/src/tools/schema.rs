//! 工具调度器与延迟工具代理共用的 JSON Schema 预校验。

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValidationMode {
    Strict,
    Schema,
}

/// 内置工具沿用严格字段检查，保持现有调用反馈不变。
pub fn validate_against_schema(
    input: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    validate_arguments(input, schema, ValidationMode::Strict)
}

/// 延迟工具遵循 schema 的动态字段及可空声明。
///
/// 与内置工具共用预校验实现；这不是完整的 JSON Schema 验证器，
/// 未覆盖的约束（如 $ref、patternProperties）仍由目标工具负责校验。
pub fn validate_deferred_arguments(
    input: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    validate_arguments(input, schema, ValidationMode::Schema)
}

fn allows_null(schema: &serde_json::Value) -> bool {
    if schema == &serde_json::Value::Bool(false) {
        return false;
    }
    if let Some(values) = schema.get("enum").and_then(|v| v.as_array()) {
        if !values.iter().any(serde_json::Value::is_null) {
            return false;
        }
    }
    if schema.get("const").is_some_and(|value| !value.is_null()) {
        return false;
    }
    if let Some(value_type) = schema.get("type") {
        let permits_null = match value_type {
            serde_json::Value::String(value) => value == "null",
            serde_json::Value::Array(values) => values.iter().any(|v| v.as_str() == Some("null")),
            _ => true,
        };
        if !permits_null {
            return false;
        }
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(branches) = schema.get(key).and_then(|v| v.as_array()) {
            if !branches.iter().any(allows_null) {
                return false;
            }
        }
    }
    if let Some(branches) = schema.get("allOf").and_then(|v| v.as_array()) {
        if !branches.iter().all(allows_null) {
            return false;
        }
    }
    true
}

/// 返回 JSON 值的类型名称（用于错误提示）
fn json_type_name(val: &serde_json::Value) -> &'static str {
    match val {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// 从属性 schema 中提取人类可读的期望类型描述
fn get_expected_type_from_schema(prop_schema: &serde_json::Value) -> String {
    if let Some(t) = prop_schema.get("type") {
        if let Some(s) = t.as_str() {
            return s.to_string();
        }
        if let Some(arr) = t.as_array() {
            let types: Vec<_> = arr.iter().filter_map(|v| v.as_str()).collect();
            if !types.is_empty() {
                return types.join(" | ");
            }
        }
    }
    if let Some(enum_vals) = prop_schema.get("enum").and_then(|e| e.as_array()) {
        let vals: Vec<_> = enum_vals.iter().map(|v| v.to_string()).collect();
        return format!("one of [{}]", vals.join(", "));
    }
    "any".to_string()
}

/// 检查单个类型是否匹配
fn matches_single_type(val: &serde_json::Value, expected: &str) -> bool {
    match expected {
        "string" => val.is_string(),
        "integer" => {
            if let Some(n) = val.as_number() {
                n.is_i64() || n.is_u64()
            } else {
                false
            }
        }
        "number" => val.is_number(),
        "boolean" => val.is_boolean(),
        "array" => val.is_array(),
        "object" => val.is_object(),
        "null" => val.is_null(),
        _ => true,
    }
}

/// 校验单个字段值的类型与枚举
fn validate_value_type(
    val: &serde_json::Value,
    prop_schema: &serde_json::Value,
    field_name: &str,
    mode: ValidationMode,
) -> Result<(), String> {
    // 检查 enum
    if let Some(enum_vals) = prop_schema.get("enum").and_then(|e| e.as_array()) {
        if !enum_vals.contains(val) {
            let vals: Vec<_> = enum_vals.iter().map(|v| v.to_string()).collect();
            return Err(format!(
                "field '{field_name}' value {val} is not one of [{}]",
                vals.join(", ")
            ));
        }
    }

    // 检查 type
    if let Some(type_val) = prop_schema.get("type") {
        let matched = match type_val {
            serde_json::Value::String(s) => matches_single_type(val, s),
            serde_json::Value::Array(arr) => arr.iter().any(|item| {
                if let Some(s) = item.as_str() {
                    matches_single_type(val, s)
                } else {
                    false
                }
            }),
            _ => true,
        };

        if !matched {
            let expected = get_expected_type_from_schema(prop_schema);
            return Err(format!(
                "field '{field_name}' has invalid type: expected {expected}, got {}",
                json_type_name(val)
            ));
        }
    }

    if val.is_object() {
        validate_arguments(val, prop_schema, mode)
            .map_err(|message| format!("Invalid nested arguments at '{field_name}': {message}"))?;
    }
    if let (Some(items), Some(item_schema)) = (val.as_array(), prop_schema.get("items")) {
        for (index, item) in items.iter().enumerate() {
            validate_value_type(item, item_schema, &format!("{field_name}[{index}]"), mode)
                .map_err(|message| {
                    format!("Invalid nested arguments at '{field_name}[{index}]': {message}")
                })?;
        }
    }

    Ok(())
}

/// 基于 JSON Schema 预校验工具入参（结构化错误消息版本）。
///
/// 校验（汇总所有错误，不再 early return）：
/// 1. 顶层是否期望为 Object
/// 2. 必填字段（required）是否存在且非 null（除非明确允许 null）
/// 3. 未定义的意外字段（Unexpected parameters）
/// 4. 已传属性的类型是否符合 properties 定义
fn validate_arguments(
    input: &serde_json::Value,
    schema: &serde_json::Value,
    mode: ValidationMode,
) -> Result<(), String> {
    let schema_obj = match schema.as_object() {
        Some(obj) if !obj.is_empty() => obj,
        _ => return Ok(()),
    };

    let expects_object = schema_obj.get("type").and_then(|t| t.as_str()) == Some("object")
        || schema_obj.contains_key("properties")
        || schema_obj.contains_key("required");

    if expects_object && !input.is_object() {
        return Err(format!(
            "expected an object for arguments, got {}",
            json_type_name(input)
        ));
    }

    let input_map = match input.as_object() {
        Some(map) => map,
        None => return Ok(()),
    };

    let props = schema_obj.get("properties").and_then(|p| p.as_object());
    let mut errors: Vec<String> = Vec::new();

    // 1. 检查必填字段（汇总所有缺失项）
    if let Some(required) = schema_obj.get("required").and_then(|r| r.as_array()) {
        for item in required {
            if let Some(field) = item.as_str() {
                let val = input_map.get(field);
                let is_missing = match val {
                    None => true,
                    Some(serde_json::Value::Null) => {
                        let property_schema = props.and_then(|p| p.get(field));
                        let allows_null = if mode == ValidationMode::Schema {
                            property_schema.is_none_or(allows_null)
                        } else {
                            property_schema.is_some_and(|s| {
                                s.get("type").is_some_and(|t| match t {
                                    serde_json::Value::String(s) => s == "null",
                                    serde_json::Value::Array(arr) => {
                                        arr.iter().any(|v| v.as_str() == Some("null"))
                                    }
                                    _ => false,
                                })
                            })
                        };
                        !allows_null
                    }
                    _ => false,
                };

                if is_missing {
                    let expected_type = props
                        .and_then(|p| p.get(field))
                        .map(get_expected_type_from_schema)
                        .unwrap_or_else(|| "any".to_string());
                    errors.push(format!(
                        "The required parameter '{field}' is missing (expected {expected_type})"
                    ));
                }
            }
        }
    }

    // 2. 内置工具保持严格字段检查；外部 schema 未声明 additionalProperties 时允许动态键。
    let mut allowed: Vec<&str> = props
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default();
    allowed.sort();
    for (key, val) in input_map {
        if props.is_some_and(|p| p.contains_key(key)) {
            continue;
        }
        if mode == ValidationMode::Strict {
            if props.is_some() {
                errors.push(format!(
                    "Unexpected parameter '{key}' was provided (allowed parameters: {allowed:?})"
                ));
            }
            continue;
        }
        // 不把 patternProperties 接受的动态键误判为额外字段；模式匹配留给目标工具。
        if schema_obj.contains_key("patternProperties") {
            continue;
        }
        match schema_obj.get("additionalProperties") {
            Some(serde_json::Value::Bool(true)) => {}
            Some(extra_schema) if extra_schema.is_object() => {
                if let Err(message) = validate_value_type(val, extra_schema, key, mode) {
                    errors.push(message);
                }
            }
            Some(serde_json::Value::Bool(false)) => errors.push(format!(
                "Unexpected parameter '{key}' was provided (allowed parameters: {allowed:?})"
            )),
            _ => {}
        }
    }

    // 3. 检查属性类型（汇总所有类型错误）
    if let Some(props_map) = props {
        for (key, val) in input_map {
            if let Some(prop_schema) = props_map.get(key) {
                if let Err(msg) = validate_value_type(val, prop_schema, key, mode) {
                    // 转换为 Issue 要求的格式
                    let expected = get_expected_type_from_schema(prop_schema);
                    let actual = json_type_name(val);
                    errors.push(format!(
                        "The parameter '{key}' type is expected as {expected}, but received {actual}"
                    ));
                    // 同时保留 enum 不匹配的原始信息（如果是 enum 错误而非类型错误）
                    if msg.contains("not one of") || msg.contains("Invalid nested arguments") {
                        // 替换最后一条为更精确的 enum 信息
                        errors.pop();
                        errors.push(msg);
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod tests;
