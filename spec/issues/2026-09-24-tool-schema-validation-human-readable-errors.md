# [peri-agent] 工具参数 Schema 校验可读性优化与工具错选启发式诊断（对齐 Claude Code formatZodValidationError）

**状态**：Open  
**优先级**：P0 / P1  
**创建日期**：2026-09-24  
**模块**：Agent 核心执行器 (`peri-agent` / `tool_dispatch`)  
**GitHub Issue**：[cc-claws/cc-code#234](https://github.com/cc-claws/cc-code/issues/234)  

---

## 1. 问题背景

在 ReAct 自主循环任务执行中，大语言模型（LLM）在高负载上下文或跨领域任务交替时，偶发将参数签名与工具名称混淆。例如：
- 想要执行网页抓取或查询，却向 `Grep` 或 `Read` 误传了 `url` 与 `prompt` 参数；
- 想要执行终端命令，却向 `Grep` 误传了 `command` 参数；
- 想要分发子任务调用 `Agent`，但遗漏了必选的 `subagent_type` 且未声明 `fork: true`。

当前 `peri-agent` 的工具参数校验逻辑主要存在以下缺陷：
1. **未显式区分与拦截多余字段（Unexpected / Unrecognized keys）**：当模型传入了完全不存在于 properties 中的非法参数时，校验逻辑默认忽略或缺乏明确的报错分类，未提示合法可用字段列表。
2. **错误可读性与引导不足**：遇到参数错误时直接早退（fail-fast），未汇总所有错误项；错误文本未对齐 Claude Code 的 `formatZodValidationError` 标准模式，引导性弱，模型难以在一次反馈中完整纠偏。
3. **缺少启发式推断（Heuristics）**：当入参具有鲜明特征（如 `url` + `prompt` 或 `command`）时，系统没有基于特征指纹给出潜在的正确工具推荐，导致模型容易陷入反复试错的盲目重试循环。
4. **连续失败缺乏强阻断机制**：针对相同工具的 Schema 校验失败缺乏低阈值（如连续 ≥2 次）熔断拦截，导致模型在错误入参上空耗轮次。

---

## 2. 改进目标与设计方案

### P0 - 结构化错误消息 (Structured Schema Validation Errors)

重构 `validate_against_schema`，支持批量汇总所有校验问题并进行结构化分项格式化，对齐 Claude Code 的 `formatZodValidationError` 风格：

1. **缺失必填字段**：
   `"The required parameter '<field>' is missing (expected <type>)"`
2. **未定义意外字段**：
   `"Unexpected parameter '<field>' was provided (allowed parameters: [<allowed_list>])"`
3. **字段类型不符**：
   `"The parameter '<field>' type is expected as <expected>, but received <actual>"`
4. **错误汇总输出**：
   多个错误项分行编号展示，清晰告知 LLM 当前调用的所有 Schema 违规点。

### P1 - 工具错配启发式诊断 (Tool Misalignment Heuristic Diagnostics)

在参数校验失败或特征匹配阶段，检测显著特征字段并附加明确的纠偏建议：
- **WebFetch 错配**：非 `WebFetch` 工具收到 `url` 与 `prompt` 字段组合时，追加提示：
  `"Did you mean to use 'WebFetch'?"`
- **Bash 错配**：非 `Bash` 工具收到 `command` 字段时，追加提示：
  `"Did you mean to use 'Bash'?"`
- **Agent 分发缺失**：调用 `Agent` 工具时缺少 `subagent_type` 且未指定 `fork: true`，追加提示：
  `"Missing 'subagent_type' or 'fork'. Please specify subagent_type from available subagents or set 'fork: true'."`

### P1 - 连续失败熔断拦截 (Consecutive Schema Failure Breaker)

- 针对相同工具的 Schema 校验连续失败次数进行追踪。
- 当同一工具的 Schema 校验失败连续达到 **≥2 次**时，立即在 `tool_error` 返回内容中注入强烈的纠正阻断提示（Circuit Breaker Prompt），明确阻止盲目重试并强制模型反思工具定义与参数。

---

## 3. 涉及核心文件

- `peri-agent/src/agent/executor/tool_dispatch.rs`
- `peri-agent/src/agent/executor/tool_dispatch_test.rs`

---

## 4. 详细技术实现方案

### 4.1 校验重构：`validate_against_schema`

修改当前单一返回第一个错误的逻辑，改为多错误收集与分段构建：

```rust
pub(crate) fn validate_against_schema(
    tool_name: &str,
    input: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), String> {
    let schema_obj = match schema.as_object() {
        Some(obj) if !obj.is_empty() => obj,
        _ => return Ok(()),
    };

    // 1. 顶层对象类型校验
    let expects_object = schema_obj.get("type").and_then(|t| t.as_str()) == Some("object")
        || schema_obj.contains_key("properties")
        || schema_obj.contains_key("required");

    if expects_object && !input.is_object() {
        return Err(format!(
            "The parameter is expected as object, but received {}",
            json_type_name(input)
        ));
    }

    let input_map = match input.as_object() {
        Some(map) => map,
        None => return Ok(()),
    };

    let props = schema_obj.get("properties").and_then(|p| p.as_object());
    let mut errors: Vec<String> = Vec::new();

    // 2. 必填字段校验
    if let Some(required) = schema_obj.get("required").and_then(|r| r.as_array()) {
        for item in required {
            if let Some(field) = item.as_str() {
                let val = input_map.get(field);
                let is_missing = match val {
                    None => true,
                    Some(serde_json::Value::Null) => {
                        let allows_null = props
                            .and_then(|p| p.get(field))
                            .and_then(|p_schema| p_schema.get("type"))
                            .is_some_and(|t| match t {
                                serde_json::Value::String(s) => s == "null",
                                serde_json::Value::Array(arr) => {
                                    arr.iter().any(|v| v.as_str() == Some("null"))
                                }
                                _ => false,
                            });
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

    // 3. 字段存在性与类型校验
    if let Some(props_map) = props {
        let mut allowed_keys: Vec<&str> = props_map.keys().map(|k| k.as_str()).collect();
        allowed_keys.sort();

        for (key, val) in input_map {
            match props_map.get(key) {
                Some(prop_schema) => {
                    if let Err(type_err) = validate_value_type(val, prop_schema, key) {
                        errors.push(type_err);
                    }
                }
                None => {
                    // 未识别的多余字段
                    let allowed_str = allowed_keys.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(", ");
                    errors.push(format!(
                        "Unexpected parameter '{key}' was provided (allowed parameters: [{allowed_str}])"
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        // 4. 结合启发式推断生成最终诊断
        let mut full_msg = errors.join("\n");
        if let Some(suggestion) = detect_tool_misalignment(tool_name, input_map) {
            full_msg.push_str("\n\nSuggestion: ");
            full_msg.push_str(&suggestion);
        }
        Err(full_msg)
    }
}
```

### 4.2 工具错配启发式诊断函数：`detect_tool_misalignment`

```rust
fn detect_tool_misalignment(
    tool_name: &str,
    input_map: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    // 启发式 1：非 WebFetch 出现 url + prompt
    if !tool_name.eq_ignore_ascii_case("WebFetch")
        && input_map.contains_key("url")
        && input_map.contains_key("prompt")
    {
        return Some("Did you mean to use 'WebFetch'?".to_string());
    }

    // 启发式 2：非 Bash 出现 command
    if !tool_name.eq_ignore_ascii_case("Bash") && input_map.contains_key("command") {
        return Some("Did you mean to use 'Bash'?".to_string());
    }

    // 启发式 3：Agent 缺少 subagent_type 且未开启 fork
    if tool_name.eq_ignore_ascii_case("Agent") {
        let has_subagent = input_map.contains_key("subagent_type");
        let has_fork = input_map.get("fork").and_then(|v| v.as_bool()).unwrap_or(false)
            || input_map.get("subagent_type").and_then(|v| v.as_str()) == Some("fork");
        if !has_subagent && !has_fork {
            return Some(
                "Missing 'subagent_type' or 'fork'. Please specify subagent_type or set 'fork: true'.".to_string()
            );
        }
    }

    None
}
```

### 4.3 连续 Schema 校验失败熔断机制

在 `dispatch_tools` 或 `collect_tool_results` 中：
- 追踪每个工具的参数 Schema 校验连续失败次数 `consecutive_schema_failures: HashMap<String, usize>`。
- 成功执行后清零该工具的 Schema 失败计数。
- 当连续失败次数达到 2 次时，在返回的错误文本末尾追加强阻断警告：

```text
[SCHEMA VALIDATION CIRCUIT BREAKER]
Tool '<tool_name>' schema validation has failed 2 consecutive times.
Do NOT retry the same parameter format immediately.
Carefully review the tool's parameter definitions and fix the call format.
```

---

## 5. 验收标准与测试用例

1. **单测覆盖 (P0)**：
   - `test_validate_schema_missing_required_readable`：缺少必选参数时返回包含 `"The required parameter '<field>' is missing (expected <type>)"`。
   - `test_validate_schema_unexpected_parameter`：传入未定义参数时返回包含 `"Unexpected parameter '<field>' was provided (allowed parameters: [...])"`。
   - `test_validate_schema_type_mismatch_readable`：类型错误时返回包含 `"The parameter '<field>' type is expected as <expected>, but received <actual>"`。
   - `test_validate_schema_multiple_errors_collected`：同时存在缺失参数、意外参数与类型错误时，所有错误均被收集并完整输出。
2. **单测覆盖 (P1 启发式)**：
   - `test_heuristic_misalignment_webfetch`：向 Grep 传 `url` 和 `prompt` 时，错误包含 `"Did you mean to use 'WebFetch'?"`。
   - `test_heuristic_misalignment_bash`：向 Grep 传 `command` 时，错误包含 `"Did you mean to use 'Bash'?"`。
   - `test_heuristic_misalignment_agent_missing_subagent`：向 Agent 传参缺少 `subagent_type` 且无 `fork` 时提示明确建议。
3. **单测覆盖 (P1 熔断)**：
   - `test_consecutive_schema_failures_circuit_breaker`：同一工具连续两次触发 Schema 校验错误，第二次应注入熔断阻断强提示。

---

## 6. 关联链接

- GitHub Issue: https://github.com/cc-claws/cc-code/issues/234
- 对齐规范：Claude Code `formatZodValidationError` 模式
