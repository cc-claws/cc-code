> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-setup-needs-setup-incomplete-check.md
# needs_setup 未校验 provider_id/base_url/models 非空

**状态**：Closed
**优先级**：中
**创建日期**：2026-05-16
**关闭日期**：2026-05-17

## 问题描述

`needs_setup` 仅检查 `providers.is_empty()` 和 `api_key.is_empty()`。一个 provider 即使 `provider_id`、`base_url` 为空但 `api_key` 非空，也会被判定为"不需要设置"。这与 `save_setup_to` 中校验 `provider_id.trim().is_empty()` 的逻辑不一致。

## 症状详情

| 现象 | 详情 |
|------|------|
| 误判不需要设置 | 损坏的配置（无 provider_id 但有 api_key）跳过 setup |
| 运行时静默失败 | LLM 调用时才知道配置不完整 |
| 两处逻辑不一致 | `needs_setup` 宽松，`save_setup_to` 严格 |

## 根因

`peri-tui/src/app/setup_wizard.rs:465-481`

```rust
pub fn needs_setup(config: &crate::config::types::AppConfig) -> bool {
    if config.providers.is_empty() { return true; }
    for provider in &config.providers {
        if provider.api_key.is_empty() {
            let key_env = ...;
            if std::env::var(key_env).unwrap_or_default().is_empty() {
                return true;
            }
        }
        // provider_id / base_url 完全不检查
    }
    false
}
```

## 期望

至少检查 `provider_id` 非空，理想情况下校验与 `save_setup_to` 的过滤逻辑（`is_complete()`）一致。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `needs_setup()` (line 465-481)
- `peri-tui/src/main.rs:203-211` —— 调用点
