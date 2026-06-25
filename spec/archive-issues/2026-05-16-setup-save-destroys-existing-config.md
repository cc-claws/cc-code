> 归档于 2026-05-16，原路径 spec/issues/2026-05-16-setup-save-destroys-existing-config.md

# save_setup 覆盖已有配置文件导致数据永久丢失

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-16
**修复日期**：2026-05-16

## 问题描述

`save_setup` 函数在合并配置前先将 wizard 数据写入目标路径，覆盖整个配置文件，然后立即从同一路径加载刚被覆盖的空壳配置做 merge。merge 逻辑成为空操作，原有配置中所有非 provider 字段（`skills_dir`、`thinking`、`env` 等）永久丢失。

## 症状详情

| 现象 | 详情 |
|------|------|
| 配置数据丢失 | 已配置用户通过 `/setup` 重新配置后，`skills_dir`、`thinking`、`env` 等字段消失 |
| 覆盖再读取 | `save_setup_to` 用 `PeriConfig::default()` + wizard provider 覆盖文件，`config::load()` 后 merge 逻辑对比的是同一份数据 |
| 首次安装不受影响 | 首次安装时 `config::load()` 返回 `PeriConfig::default()`（无已有数据可丢失） |

## 根因

`peri-tui/src/app/setup_wizard.rs:838-861`

```rust
pub fn save_setup(wizard: &SetupWizardPanel) -> anyhow::Result<PeriConfig> {
    let path = config_path();
    let cfg = save_setup_to(wizard, &path)?;  // ← 已覆盖文件
    if let Ok(existing) = config::load() {     // ← 读到刚写的数据
        let mut merged = existing;
        for new_provider in &cfg.config.providers {
            // 此时 existing.providers 与 cfg.config.providers 完全相同
            // merge 是空操作
        }
        config::save(&merged)?;
        return Ok(merged);
    }
    Ok(cfg)
}
```

## 期望

先 `config::load()` 读取原始配置，再合并 wizard provider 数据，最后 `save_setup_to` 写入合并后的完整配置。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— `save_setup()` (line 838-861), `save_setup_to()` (line 790-835)

## 修复方案

1. 提取纯函数 `build_wizard_config(wizard)` —— 从 wizard 数据构建 `PeriConfig`，无磁盘 I/O
2. `save_setup_to` 调用 `build_wizard_config` + `crate::config::store::save_to`（atomic write）
3. `save_setup` 先 `crate::config::load()` 加载已有配置，合并 wizard 数据（按 id 去重 provider），更新 language/active_alias/active_provider_id，最后 `crate::config::save()`
