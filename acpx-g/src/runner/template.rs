use std::collections::HashMap;

/// Context available during template interpolation.
pub struct TemplateContext {
    /// Workflow-level inputs (from API or parent's `with`).
    pub inputs: HashMap<String, String>,
    /// Completed upstream node outputs: node_id -> (key -> value).
    pub needs_outputs: HashMap<String, HashMap<String, String>>,
    /// Merged environment: global env + node env.
    pub env: HashMap<String, String>,
}

/// Replace all `{{ expr }}` patterns in `input` using the provided context.
///
/// Supported expressions:
/// - `{{ inputs.x }}` → `ctx.inputs["x"]`
/// - `{{ needs.node_id.outputs.key }}` → `ctx.needs_outputs["node_id"]["key"]`
/// - `{{ env.KEY }}` → `ctx.env["KEY"]`
///
/// Unresolvable expressions are left as-is.
pub fn interpolate(input: &str, ctx: &TemplateContext) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    let bytes = input.as_bytes();

    while let Some(&(i, _)) = chars.peek() {
        // Look for "{{ "
        if i + 3 <= input.len() && &bytes[i..i + 2] == b"{{" && bytes[i + 2] == b' ' {
            // Find closing " }}"
            let search_start = i + 3;
            if let Some(end) = find_close_brace(input, search_start) {
                let expr = &input[i + 3..end];
                let resolved = resolve_expr(expr.trim(), ctx);
                result.push_str(&resolved);
                // Skip past " }}"
                let skip_end = end + 3; // " }}"
                while let Some(&(pos, _)) = chars.peek() {
                    if pos >= skip_end {
                        break;
                    }
                    chars.next();
                }
                continue;
            }
        }
        result.push(chars.next().unwrap().1);
    }

    result
}

/// Interpolate every value in a HashMap.
pub fn interpolate_map(
    map: &HashMap<String, String>,
    ctx: &TemplateContext,
) -> HashMap<String, String> {
    map.iter()
        .map(|(k, v)| (k.clone(), interpolate(v, ctx)))
        .collect()
}

fn find_close_brace(input: &str, start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut i = start;
    while i + 3 <= input.len() {
        if bytes[i] == b' ' && &bytes[i + 1..i + 3] == b"}}" {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn resolve_expr(expr: &str, ctx: &TemplateContext) -> String {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        // Empty expression like {{ }} — leave as-is (likely a mistake)
        return "{{ }}".to_string();
    }
    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.as_slice() {
        ["inputs", key] => ctx.inputs.get(*key).cloned().unwrap_or_default(),
        ["env", key] => ctx.env.get(*key).cloned().unwrap_or_default(),
        ["needs", node_id, "outputs", key] => ctx
            .needs_outputs
            .get(*node_id)
            .and_then(|m| m.get(*key))
            .cloned()
            .unwrap_or_default(),
        // Leave unresolvable expressions as-is
        _ => format!("{{{{ {expr} }}}}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx() -> TemplateContext {
        let mut inputs = HashMap::new();
        inputs.insert("tag".to_string(), "v1.2.3".to_string());
        inputs.insert("env".to_string(), "staging".to_string());

        let mut build_outputs = HashMap::new();
        build_outputs.insert("artifact_path".to_string(), "./dist/app.tar.gz".to_string());
        build_outputs.insert("repo_dir".to_string(), "./repo".to_string());

        let mut checkout_outputs = HashMap::new();
        checkout_outputs.insert("repo_dir".to_string(), "./repo".to_string());

        let mut needs_outputs = HashMap::new();
        needs_outputs.insert("build".to_string(), build_outputs);
        needs_outputs.insert("checkout".to_string(), checkout_outputs);

        let mut env = HashMap::new();
        env.insert("RUST_BACKTRACE".to_string(), "1".to_string());
        env.insert("DEPLOY_ENV".to_string(), "production".to_string());

        TemplateContext {
            inputs,
            needs_outputs,
            env,
        }
    }

    #[test]
    fn test_interpolate_inputs() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{ inputs.tag }}", &ctx), "v1.2.3");
        assert_eq!(
            interpolate("Deploying {{ inputs.tag }} to {{ inputs.env }}", &ctx),
            "Deploying v1.2.3 to staging"
        );
    }

    #[test]
    fn test_interpolate_needs_outputs() {
        let ctx = make_ctx();
        assert_eq!(
            interpolate("{{ needs.build.outputs.artifact_path }}", &ctx),
            "./dist/app.tar.gz"
        );
        assert_eq!(
            interpolate("{{ needs.checkout.outputs.repo_dir }}", &ctx),
            "./repo"
        );
    }

    #[test]
    fn test_interpolate_env() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{ env.RUST_BACKTRACE }}", &ctx), "1");
        assert_eq!(interpolate("{{ env.DEPLOY_ENV }}", &ctx), "production");
    }

    #[test]
    fn test_interpolate_mixed() {
        let ctx = make_ctx();
        assert_eq!(
            interpolate(
                "Deploy {{ inputs.tag }} artifact {{ needs.build.outputs.artifact_path }} in {{ env.DEPLOY_ENV }}",
                &ctx
            ),
            "Deploy v1.2.3 artifact ./dist/app.tar.gz in production"
        );
    }

    #[test]
    fn test_interpolate_missing_left_as_is() {
        let ctx = make_ctx();
        assert_eq!(
            interpolate("{{ unknown.expression }}", &ctx),
            "{{ unknown.expression }}"
        );
        assert_eq!(interpolate("{{ inputs.nonexistent }}", &ctx), "");
        assert_eq!(interpolate("{{ needs.missing.outputs.key }}", &ctx), "");
    }

    #[test]
    fn test_interpolate_no_templates() {
        let ctx = make_ctx();
        assert_eq!(
            interpolate("plain text no templates", &ctx),
            "plain text no templates"
        );
        assert_eq!(interpolate("", &ctx), "");
    }

    #[test]
    fn test_interpolate_empty_expression() {
        let ctx = make_ctx();
        // Empty {{ }} should be preserved, not silently replaced
        assert_eq!(interpolate("{{ }}", &ctx), "{{ }}");
        assert_eq!(
            interpolate("before {{ }} after", &ctx),
            "before {{ }} after"
        );
    }

    #[test]
    fn test_interpolate_whitespace_only_expression() {
        let ctx = make_ctx();
        // Whitespace-only {{   }} should also be preserved
        assert_eq!(interpolate("{{   }}", &ctx), "{{ }}");
    }

    #[test]
    fn test_interpolate_map() {
        let ctx = make_ctx();
        let mut map = HashMap::new();
        map.insert("tag".to_string(), "{{ inputs.tag }}".to_string());
        map.insert(
            "path".to_string(),
            "{{ needs.build.outputs.artifact_path }}".to_string(),
        );
        map.insert("static".to_string(), "unchanged".to_string());

        let result = interpolate_map(&map, &ctx);
        assert_eq!(result.get("tag").unwrap(), "v1.2.3");
        assert_eq!(result.get("path").unwrap(), "./dist/app.tar.gz");
        assert_eq!(result.get("static").unwrap(), "unchanged");
    }

    #[test]
    fn test_interpolate_multiple_same() {
        let ctx = make_ctx();
        assert_eq!(
            interpolate("{{ inputs.tag }}-{{ inputs.tag }}", &ctx),
            "v1.2.3-v1.2.3"
        );
    }

    #[test]
    fn test_interpolate_nested_node_id() {
        // Reference-expanded node IDs with "/" prefix
        let mut needs_outputs = HashMap::new();
        let mut outputs = HashMap::new();
        outputs.insert("artifact_path".to_string(), "./app".to_string());
        needs_outputs.insert("do-build/build".to_string(), outputs);

        let ctx = TemplateContext {
            inputs: HashMap::new(),
            needs_outputs,
            env: HashMap::new(),
        };

        assert_eq!(
            interpolate("{{ needs.do-build/build.outputs.artifact_path }}", &ctx),
            "./app"
        );
    }
}
