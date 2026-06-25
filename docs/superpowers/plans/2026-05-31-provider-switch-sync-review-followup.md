# Provider 切换同步修复 — Code Review 跟进

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `refresh_after_setup` 中的异步竞态问题，提取重复代码为辅助方法，清理冗余的 `set_config_option` double-check。

**Architecture:** 将四处重复的 `block_in_place + block_on { update_config + set_config_option }` 提取为 `App::sync_acp_config()` 辅助方法。`refresh_after_setup` 改为调用该方法。移除冗余的 `set_config_option` 调用（`update_config` 已完整替换 `peri_config` 并重建 `provider`）。

**Tech Stack:** Rust, tokio (`block_in_place` + `block_on`), `parking_lot::RwLock`, ACP MpscTransport

---

### Task 1: 提取 `sync_acp_config` 辅助方法

**Files:**
- Modify: `peri-tui/src/app/mod.rs`

- [ ] **Step 1: 在 `impl App` 中添加 `sync_acp_config` 方法**

在 `refresh_after_setup` 方法附近（L636 后）添加：

```rust
/// 同步等待 ACP Server 更新完整配置，确保 provider 在内存中已更新。
/// 使用 block_in_place + block_on 避免 tokio runtime 死锁。
fn sync_acp_config(&self) {
    let Some(ref acp_client) = self.acp_client else {
        return;
    };
    let cfg = match self.services.peri_config.as_ref() {
        Some(c) => c.clone(),
        None => return,
    };
    let acp = acp_client.clone();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            if let Err(e) = acp.update_config(&cfg).await {
                tracing::error!(error = %e, "sync_acp_config: update_config failed");
            }
        });
    });
}
```

- [ ] **Step 2: 修改 `refresh_after_setup` 使用新方法**

将 `peri-tui/src/app/mod.rs` 中 `refresh_after_setup`（L620-636）替换为：

```rust
pub fn refresh_after_setup(&mut self, cfg: crate::config::PeriConfig) {
    self.services.peri_config = Some(cfg);
    let cfg_ref = self.services.peri_config.as_ref().unwrap();
    if let Some(p) = agent::LlmProvider::from_config(cfg_ref) {
        self.services.provider_name = p.display_name().to_string();
        self.services.model_name = p.model_name().to_string();
    }
    self.sync_acp_config();
}
```

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/mod.rs
git commit -m "refactor: extract sync_acp_config helper, fix refresh_after_setup race

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 2: Panel Login 三处调用改为使用 `sync_acp_config`

**Files:**
- Modify: `peri-tui/src/app/panel_login.rs`

- [ ] **Step 1: 替换 `login_panel_select_provider` 中的内联 block**

将 `panel_login.rs:58-76` 中的 `if let Some(ref acp_client) = self.acp_client { ... }` 块替换为：

```rust
self.sync_acp_config();
```

- [ ] **Step 2: 替换 `login_panel_apply_edit` 中的内联 block**

同样替换该方法的 `if let Some(ref acp_client) = self.acp_client { ... }` 块为：

```rust
self.sync_acp_config();
```

- [ ] **Step 3: 替换 `login_panel_confirm_delete` 中的内联 block**

同样替换该方法的 `if let Some(ref acp_client) = self.acp_client { ... }` 块为：

```rust
self.sync_acp_config();
```

- [ ] **Step 4: 验证编译 + clippy**

Run: `cargo clippy -p peri-tui 2>&1 | grep -E "warning|error"`
Expected: 无 warning/error

- [ ] **Step 5: 运行测试**

Run: `cargo test -p peri-tui --lib -- test_update_config 2>&1 | tail -10`
Expected: 3 tests passed

- [ ] **Step 6: Commit**

```bash
git add peri-tui/src/app/panel_login.rs
git commit -m "refactor: panel_login uses sync_acp_config helper

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 3: Ctrl+Shift+T 快捷键改用 `sync_acp_config`

**Files:**
- Modify: `peri-tui/src/event/keyboard/shortcuts.rs`

- [ ] **Step 1: 替换 Ctrl+Shift+T 中的内联 block**

将 `shortcuts.rs:111-121` 中的 `if let Some(ref acp_client) = app.acp_client { ... }` 块替换为：

```rust
app.sync_acp_config();
```

- [ ] **Step 2: 验证编译 + clippy**

Run: `cargo clippy -p peri-tui 2>&1 | grep -E "warning|error"`
Expected: 无 warning/error

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/event/keyboard/shortcuts.rs
git commit -m "refactor: Ctrl+Shift+T uses sync_acp_config helper

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```

---

### Task 4: 清理 update_config 处理器中的诊断日志

**Files:**
- Modify: `peri-tui/src/acp_server/prompt.rs`（移除 `[diag]` 日志）
- Modify: `peri-tui/src/acp_server/requests.rs`（保留正式 warn 日志，移除多余 info）

- [ ] **Step 1: 移除 prompt.rs 中的诊断日志**

删除 `prompt.rs:119-124` 的 `tracing::info!("[diag] prompt: provider snapshot taken")` 块。

- [ ] **Step 2: 精简 requests.rs update_config 中的日志**

将 `update_config` 处理器中的两条 `info!` 合并保留为一条有意义的 `info!`，去掉冗余的 provider updated info（`warn!` 保留）。

- [ ] **Step 3: 验证编译 + 测试**

Run: `cargo test -p peri-tui --lib -- test_update_config 2>&1 | tail -10`
Expected: 3 tests passed

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/acp_server/prompt.rs peri-tui/src/acp_server/requests.rs
git commit -m "chore: clean up diagnostic logs from provider switch fix

Co-Authored-By: glm-5.1 <zai-org@claude-code-best.win>"
```
