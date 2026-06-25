> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-env-get-silent-fail-on-non-string.md
# env_get 对非字符串值静默返回空串

**状态**：Closed
**优先级**：低
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

`env_get` 使用 `as_str()` 提取值。如果 `settings.json` 的 `env` 字段中某值是数字或布尔值（如 `{ "ANTHROPIC_API_KEY": true }`），函数静默返回 `""`。用户无法知道配置格式错误，凭据被跳过。

## 症状详情

| 现象 | 详情 |
|------|------|
| 静默跳过 | 非字符串 env 值当作"不存在"处理 |
| 无日志/警告 | 没有 tracing 告警说明配置格式问题 |
| 难以调试 | 用户可能反复检查 key 是否正确，但不知道是 value 类型问题 |

## 根因

`peri-tui/src/app/setup_wizard.rs:443-448`

```rust
fn env_get(env: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    env.get(key)
        .and_then(|v| v.as_str())  // ← 非字符串值时返回 None
        .unwrap_or("")
        .to_string()
}
```

## 期望

非字符串值时至少 `tracing::warn` 记录格式错误，或尝试 `to_string()`。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `env_get()` (line 443-448)
