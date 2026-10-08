# 移除 `~/.peri` 兼容：应用数据统一 `~/.cc-code`

**状态**：Fixed
**优先级**：中（改名已久，兼容分支长期存在；旧目录 `~/.peri` 还留着 stale 密钥与坏 ACL 文件，继续兼容弊大于利）
**创建日期**：2026-10-08
**分支**：`refactor/drop-peri-compat`
**Issue**：[#348](https://github.com/cc-claws/cc-code/issues/348)
**PR**：[#349](https://github.com/cc-claws/cc-code/pull/349)

---

## 一、背景

项目于 v0.6.86 起把应用主目录从 `~/.peri` 迁移到 `~/.cc-code`（#289）。为「老用户数据不丢失」，当时保留了**逐文件回退**：

```rust
// cc-agent/src/app_home.rs（移除前）
pub fn app_data_path_in(home, file_name) -> PathBuf {
    let new = app_home_dir_in(home).join(file_name);
    let legacy = legacy_app_home_dir_in(home).join(file_name);
    if new.exists() || !legacy.exists() { new } else { legacy }
}
```

回退是**逐文件 + 粘性**的：只要某文件只在 `~/.peri`，就一直用它。实测在用户机器上表现为 `input-history.json` / `oauth_tokens.json` 长期落在 `~/.peri`。

## 二、决策

项目已经是 cc-code，不再兼容 `~/.peri`。**直接硬移除**（不做自动迁移）：代码只读写 `~/.cc-code`，不再读、不再回退旧目录。

## 三、改动

### 核心兼容面（会读 `~/.peri` 的地方，共 2 处）

1. **`cc-agent/src/app_home.rs`**
   - 删除 `legacy_app_home_dir` / `legacy_app_home_dir_in`；
   - `app_data_path_in` / `app_data_dir_in` 不再回退，直接 `home/.cc-code/...`。

2. **`cc-tui/src/main.rs::inject_env_from_settings`**
   - 只读 `~/.cc-code/settings.json`，删掉 `~/.peri/settings.json` 的读取。

### 顺带清理（陈旧注释 / 用户可见文案）

- `cc-tui/src/acp_stdio.rs`：报错文案 `configure ~/.peri/settings.json` → `~/.cc-code/settings.json`；
- `cc-tui/src/app/ui_state.rs`、`shell_history.rs`、`sync/{scanner,writer,protocol}.rs`、`app/history_persistence.rs`、`app/panel_ops.rs`；
- `cc-middlewares/src/mcp/{auth_store,config}.rs`、`skills/mod.rs`；
- `cc-acp/src/provider/config.rs`、`cc-agent/src/thread/filesystem.rs`。

### 测试夹具

- `cc-tui/src/sync/{scanner_test,writer_test}.rs`、`cc-middlewares/src/mcp/config_test.rs`、`cc-middlewares/src/mcp/auth_store_test.rs`、`cc-tui/src/ui/main_ui/panels/mcp_test.rs`：夹具路径 `.peri` → `.cc-code`；断言收紧为「必须 `.cc-code`」。

## 四、保留项（非兼容）

- `cc-middlewares/src/hitl/jev/policy.rs` 的敏感目录名单里保留 `.peri`：若用户机上仍存在 `~/.peri`（含 stale 密钥），**继续阻止**工具读取是有益的，与「兼容」无关。
- `peri_config` 等**内部标识符命名残留**（155 处）本次不动——纯命名，与数据目录兼容无关，可另开清理 PR。

## 五、影响

- **行为变化（破坏性）**：只在 `~/.peri` 存在的数据文件将**不再被读取**（用户需自行迁移到 `~/.cc-code`）。
- 新写入一律走 `~/.cc-code`，与移除前一致。

## 六、验证

- `cargo check --workspace --all-targets` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- 测试：cc-agent 533 ✅ / cc-tui 1067 ✅ / cc-middlewares mcp 99 ✅
- 新增测试 `test_data_paths_never_use_legacy_peri`：断言所有数据路径落在 `~/.cc-code` 且不含 `.peri`。
