# ACP 配置协议纯净化 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 废除 TUI↔ACP Server 的 Arc 共享内存，所有配置变更走 ACP 协议。

**Architecture:** TUI 侧乐观更新本地 PeriConfig 后通过 `tokio::spawn(acp.set_config_option / acp.update_config)` fire-and-forget 同步到 ACP Server。ACP Server 自行 `store::load()` 初始化，新增 `config_path` 字段支持持久化。`store.rs` 从 `peri-tui` 迁移到 `peri-acp`。

**Tech Stack:** Rust, tokio async, ACP JSON-RPC protocol, serde_json, parking_lot::RwLock

**Design doc:** `docs/superpowers/specs/2026-05-29-acp-config-protocol-design.md`

---

## 文件结构

### 迁移
- 移动: `peri-tui/src/config/store.rs` → `peri-acp/src/provider/store.rs`
- 移动: `peri-tui/src/config/store_test.rs` → `peri-acp/src/provider/store_test.rs`

### 修改（peri-acp）
- `peri-acp/src/provider/mod.rs` — 新增 `pub mod store;`
- `peri-acp/Cargo.toml` — 确认 `dirs-next` 已存在

### 修改（peri-tui TUI 侧）
- `peri-tui/src/config/mod.rs` — re-export 从 `peri_acp::provider::store`
- `peri-tui/src/config/store.rs` — 删除
- `peri-tui/src/config/store_test.rs` — 删除（迁移到 peri-acp）
- `peri-tui/src/app/service_registry.rs` — 删除 `acp_peri_config`/`acp_provider` 字段 + `sync_peri_config_to_acp()`
- `peri-tui/src/app/mod.rs` — 删除 Arc 赋值 + `refresh_after_setup` 改用 ACP
- `peri-tui/src/main.rs` — 删除 Arc clone，ACP Server 自行初始化
- `peri-tui/src/event/keyboard/shortcuts.rs` — 替换 `sync_peri_config_to_acp()` 为 ACP 调用
- `peri-tui/src/app/panel_model.rs` — 替换同步调用
- `peri-tui/src/app/model_panel.rs` — 替换 `sync_peri_config_to_acp()` 为 ACP 调用
- `peri-tui/src/command/session/effort.rs` — 替换为 ACP 调用
- `peri-tui/src/command/panel/model.rs` — 替换为 ACP 调用
- `peri-tui/src/app/panel_login.rs` — 替换为 `update_config`
- `peri-tui/src/app/login_panel/component.rs` — 替换 `sync_peri_config_to_acp()` 为 ACP 调用
- `peri-tui/src/app/panel_ops.rs` — 删除 Arc None 赋值
- `peri-tui/src/acp_client/client.rs` — 新增 `update_config` 方法

### 修改（ACP Server 侧）
- `peri-tui/src/acp_server/mod.rs` — `AcpServerConfig` 新增 `config_path` 字段
- `peri-tui/src/acp_server/requests.rs` — 扩展 `set_config_option` handler + 新增 `update_config` handler

---

### Task 1: 迁移 store.rs 到 peri-acp

**Files:**
- 移动: `peri-tui/src/config/store.rs` → `peri-acp/src/provider/store.rs`
- 移动: `peri-tui/src/config/store_test.rs` → `peri-acp/src/provider/store_test.rs`
- 修改: `peri-acp/src/provider/mod.rs` — 新增 `pub mod store;` + re-export
- 修改: `peri-tui/src/config/mod.rs` — re-export 从 `peri_acp`
- 删除: `peri-tui/src/config/store.rs`
- 删除: `peri-tui/src/config/store_test.rs`

- [ ] **Step 1: 在 peri-acp 中创建 store 模块**

将 `peri-tui/src/config/store.rs` 的内容复制到 `peri-acp/src/provider/store.rs`，修改 import：

```rust
// peri-acp/src/provider/store.rs
use super::config::PeriConfig;
use anyhow::Result;
use std::path::{Path, PathBuf};

// 其余函数（config_path, workspace_config_path, load, load_from, save, save_to）保持不变
```

- [ ] **Step 2: 迁移测试文件**

将 `peri-tui/src/config/store_test.rs` 复制到 `peri-acp/src/provider/store_test.rs`，修改 import：

```rust
// peri-acp/src/provider/store_test.rs
use super::store::load_from;
// 其余代码不变
```

在 `store.rs` 底部添加：

```rust
#[cfg(test)]
#[path = "store_test.rs"]
mod store_tests;
```

- [ ] **Step 3: 更新 peri-acp/src/provider/mod.rs**

在 `pub mod config;` 后添加：

```rust
pub mod store;

pub use store::{config_path, load, load_from, save, save_to, workspace_config_path};
```

- [ ] **Step 4: 更新 peri-tui/src/config/mod.rs**

```rust
pub mod store;  // 删除此行

// Re-export config types from peri-acp (single source of truth)
pub use peri_acp::provider::{
    AppConfig, PeriConfig, ProviderConfig, ProviderModels, ThinkingConfig,
};

// Re-export store functions from peri-acp
pub use peri_acp::provider::{config_path, load, load_from, save, save_to, workspace_config_path};

#[cfg(test)]
#[path = "types_test.rs"]
mod tests;
```

- [ ] **Step 5: 删除旧文件**

删除 `peri-tui/src/config/store.rs` 和 `peri-tui/src/config/store_test.rs`。

- [ ] **Step 6: 编译验证**

Run: `cargo build -p peri-acp && cargo build -p peri-tui`
Expected: 编译通过，无错误

- [ ] **Step 7: 运行测试**

Run: `cargo test -p peri-acp -- provider::store`
Expected: 所有 store 测试通过

Run: `cargo test -p peri-tui -- config`
Expected: 所有 config 测试通过（types_test.rs）

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor: migrate store.rs from peri-tui to peri-acp

Moves PeriConfig disk I/O (load/save) to peri-acp crate alongside
the type definition. peri-tui re-exports via pub use."
```

---

### Task 2: ACP Server 侧扩展 set_config_option + 新增 update_config

**Files:**
- 修改: `peri-tui/src/acp_server/mod.rs`
- 修改: `peri-tui/src/acp_server/requests.rs`

- [ ] **Step 1: AcpServerConfig 新增 config_path 字段**

在 `peri-tui/src/acp_server/mod.rs` 的 `AcpServerConfig` 结构体中添加：

```rust
pub struct AcpServerConfig {
    // ...existing fields...
    pub config_path: std::path::PathBuf,
}
```

- [ ] **Step 2: 添加 persist_config 辅助函数**

在 `peri-tui/src/acp_server/requests.rs` 顶部添加：

```rust
use crate::config::save_to;

/// 将当前 PeriConfig 持久化到 config_path 指定的磁盘路径
fn persist_config(cfg: &AcpServerConfig) {
    let c = cfg.peri_config.read();
    if let Err(e) = save_to(&c, &cfg.config_path) {
        tracing::warn!(error = %e, "Failed to persist config");
    }
}
```

- [ ] **Step 3: 扩展 thought_effort handler 添加持久化**

在 `requests.rs` 的 `set_config_option` match 中，`"thinking_effort"` 分支末尾添加持久化调用：

```rust
"thinking_effort" => {
    apply_thinking_effort(&cfg.peri_config, value);
    persist_config(cfg);
    info!(effort = %value, "Thinking effort changed via configOption (persisted)");
}
```

注意：当前的 configId 是 `"thinking_effort"` 而非 `"thought_effort"`。根据设计决策，统一使用 `"thinking_effort"`（与 `apply_thinking_effort` 函数名对齐）。如果当前代码匹配的是 `"thinking_effort"` 则无需改名。

- [ ] **Step 4: 新增 context_1m handler**

在 `requests.rs` 的 `set_config_option` match 中，`"thinking_effort"` 之后添加：

```rust
"context_1m" => {
    let enabled = value == "true" || value == "1";
    {
        let mut c = cfg.peri_config.write();
        c.config.context_1m = Some(enabled);
    }
    persist_config(cfg);
    info!(enabled = %enabled, "Context 1M changed via configOption (persisted)");
}
```

- [ ] **Step 5: 新增 session/update_config handler**

在 `requests.rs` 的 `handle_request` match 中，`"session/fork"` 之后、`_` 之前添加：

```rust
"session/update_config" => {
    let session_id = extract_session_id(params, "");
    let new_cfg: crate::config::PeriConfig =
        serde_json::from_value(params.get("config").cloned().unwrap_or_default())
            .map_err(|e| AcpError::new(-32602, format!("Invalid config: {e}")))?;

    // 校验
    if new_cfg.config.providers.is_empty() {
        return Err(AcpError::new(-32602, "providers cannot be empty"));
    }
    let active_pid = new_cfg.config.active_provider_id.as_str();
    if !active_pid.is_empty()
        && !new_cfg.config.providers.iter().any(|p| p.id == active_pid)
    {
        return Err(AcpError::new(
            -32602,
            format!("active_provider_id '{active_pid}' not found"),
        ));
    }

    // 应用到内存
    *cfg.peri_config.write() = new_cfg.clone();

    // 重算 provider
    if let Some(p) = LlmProvider::from_config(&new_cfg) {
        *cfg.provider.write() = p;
    }

    // 持久化
    persist_config(cfg);

    // 返回完整 configOptions
    let config_options = {
        let c = cfg.peri_config.read();
        let p = cfg.provider.read();
        build_config_options(&c, &p, cfg.permission_mode.load())
    };
    send_config_option_update(transport, session_id, cfg).await;
    serde_json::to_value(SetSessionConfigOptionResponse::new(config_options))
        .map_err(|e| AcpError::new(-32603, format!("Serialize failed: {e}")))
}
```

- [ ] **Step 6: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过（config_path 暂未在 main.rs 中传入，先传 `std::path::PathBuf::new()` 占位）

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(acp): extend set_config_option with persistence + add update_config handler

- thought_effort now persists to disk
- New context_1m configOption with persistence
- New session/update_config for full PeriConfig CRUD
- AcpServerConfig gains config_path field"
```

---

### Task 3: AcpTuiClient 新增 update_config 方法

**Files:**
- 修改: `peri-tui/src/acp_client/client.rs`

- [ ] **Step 1: 新增 update_config 方法**

在 `AcpTuiClient` impl 块中，`set_config_option` 方法之后添加：

```rust
/// Update the full PeriConfig on the ACP server (for Login panel CRUD).
pub async fn update_config(&self, config: &crate::config::PeriConfig) -> Result<(), String> {
    let session_id = self
        .current_session_id
        .lock()
        .unwrap()
        .clone()
        .ok_or("no active session")?;
    let params = json!({
        "sessionId": session_id,
        "config": config,
    });
    let _ = self
        .transport
        .send_request("session/update_config", params)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(tui): add AcpTuiClient::update_config for full PeriConfig sync"
```

---

### Task 4: 删除 Arc 共享 + ServiceRegistry 清理

**Files:**
- 修改: `peri-tui/src/app/service_registry.rs`
- 修改: `peri-tui/src/app/mod.rs`
- 修改: `peri-tui/src/app/panel_ops.rs`
- 修改: `peri-tui/src/main.rs`

- [ ] **Step 1: 删除 ServiceRegistry 中的 Arc 字段和 sync 方法**

在 `peri-tui/src/app/service_registry.rs` 中，删除以下字段：

```rust
// 删除这两个字段:
pub acp_peri_config: Option<Arc<parking_lot::RwLock<PeriConfig>>>,
pub acp_provider: Option<Arc<parking_lot::RwLock<crate::app::agent::LlmProvider>>>,
```

删除整个 `sync_peri_config_to_acp` 方法。

检查文件顶部的 import，如果有 `use std::sync::Arc;` 且不再被使用则删除。

- [ ] **Step 2: 更新 main.rs 中的 AcpServerConfig 构造**

在 `peri-tui/src/main.rs` 中：

1. 删除 Arc clone 行：
```rust
// 删除这两行:
app.services.acp_provider = Some(server_config.provider.clone());
app.services.acp_peri_config = Some(server_config.peri_config.clone());
```

2. 添加 `config_path` 字段到 `AcpServerConfig` 构造：
```rust
// 在 server_config 构造中添加:
config_path: crate::config::store::config_path(),
```

注意：`store` 模块已迁移到 `peri-acp`，通过 re-export 可用 `crate::config::config_path()`。

3. 将 `AcpServerConfig` 构造中的 `provider` 和 `peri_config` 改为自行初始化：
```rust
// 原来（TUI 侧 clone Arc）:
provider: Arc::new(parking_lot::RwLock::new(provider)),
peri_config: Arc::new(parking_lot::RwLock::new(peri_config)),

// 改为（ACP Server 自行 load）:
provider: Arc::new(parking_lot::RwLock::new(
    crate::app::agent::LlmProvider::from_config(&peri_config)
        .unwrap_or_else(|| crate::app::agent::LlmProvider::from_env()
            .expect("No provider configured"))
)),
peri_config: Arc::new(parking_lot::RwLock::new(peri_config)),
```

实际上 `peri_config` 和 `provider` 变量已经在 main.rs 中计算好了（setup wizard 之后），只需保留传值方式不变。关键是删除 Arc clone 到 ServiceRegistry 的两行。

- [ ] **Step 3: 更新 app/mod.rs 中的 refresh_after_setup**

```rust
// 原来:
pub fn refresh_after_setup(&mut self, cfg: crate::config::PeriConfig) {
    self.services.peri_config = Some(cfg);
    let cfg_ref = self.services.peri_config.as_ref().unwrap();
    if let Some(p) = agent::LlmProvider::from_config(cfg_ref) {
        self.services.provider_name = p.display_name().to_string();
        self.services.model_name = p.model_name().to_string();
    }
    self.services.sync_peri_config_to_acp();
}

// 改为:
pub fn refresh_after_setup(&mut self, cfg: crate::config::PeriConfig) {
    self.services.peri_config = Some(cfg);
    let cfg_ref = self.services.peri_config.as_ref().unwrap();
    if let Some(p) = agent::LlmProvider::from_config(cfg_ref) {
        self.services.provider_name = p.display_name().to_string();
        self.services.model_name = p.model_name().to_string();
    }
    // 通过 ACP 协议同步完整配置到 Server
    if let Some(ref acp_client) = self.acp_client {
        let acp = acp_client.clone();
        let cfg_clone = cfg_ref.clone();
        tokio::spawn(async move {
            let _ = acp.update_config(&cfg_clone).await;
        });
    }
}
```

- [ ] **Step 4: 更新 panel_ops.rs 测试初始化**

在 `peri-tui/src/app/panel_ops.rs` 中删除：
```rust
// 删除这两行:
acp_peri_config: None,
acp_provider: None,
```

- [ ] **Step 5: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译错误只会出现在仍在调用 `sync_peri_config_to_acp()` 的地方（shortcuts.rs、panel_model.rs 等），这些在 Task 5 中修复。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor: remove Arc sharing between TUI ServiceRegistry and ACP Server

- Delete acp_peri_config/acp_provider fields from ServiceRegistry
- Delete sync_peri_config_to_acp() method
- AcpServerConfig gains config_path for self-managed persistence
- refresh_after_setup uses ACP update_config instead of Arc write"
```

---

### Task 5: 替换所有 sync_peri_config_to_acp 调用点为 ACP 协议调用

**Files:**
- 修改: `peri-tui/src/event/keyboard/shortcuts.rs`
- 修改: `peri-tui/src/app/panel_model.rs`
- 修改: `peri-tui/src/app/model_panel.rs`
- 修改: `peri-tui/src/command/session/effort.rs`
- 修改: `peri-tui/src/command/panel/model.rs`
- 修改: `peri-tui/src/app/panel_login.rs`
- 修改: `peri-tui/src/app/login_panel/component.rs`

所有替换遵循同一模式：

```rust
// 旧:
app.services.sync_peri_config_to_acp();

// 新（set_config_option 类）:
if let Some(ref acp_client) = app.acp_client {
    let acp = acp_client.clone();
    let val = value.to_string();
    tokio::spawn(async move {
        let _ = acp.set_config_option("config_id", &val).await;
    });
}

// 新（update_config 类 — Login 面板）:
if let Some(ref acp_client) = app.acp_client {
    let acp = acp_client.clone();
    let cfg = app.services.peri_config.as_ref().unwrap().clone();
    tokio::spawn(async move {
        let _ = acp.update_config(&cfg).await;
    });
}
```

- [ ] **Step 1: 替换 shortcuts.rs 中的 Ctrl+T 和 Ctrl+Shift+T**

`peri-tui/src/event/keyboard/shortcuts.rs`

Ctrl+T（约第67行），替换 `app.services.sync_peri_config_to_acp();` 为：

```rust
if let Some(ref acp_client) = app.acp_client {
    let acp = acp_client.clone();
    let alias = next.to_string();
    tokio::spawn(async move {
        let _ = acp.set_config_option("model", &alias).await;
    });
}
```

Ctrl+Shift+T（约第99行），替换 `app.services.sync_peri_config_to_acp();` 为：

```rust
if let Some(ref acp_client) = app.acp_client {
    let acp = acp_client.clone();
    let alias = cfg.config.active_alias.clone();
    tokio::spawn(async move {
        let _ = acp.set_config_option("model", &alias).await;
    });
}
```

- [ ] **Step 2: 替换 panel_model.rs 中的面板确认**

`peri-tui/src/app/panel_model.rs`

替换 `self.services.sync_peri_config_to_acp();` 和冗余 ACP 调用（第70-84行）为：

```rust
self.services.sync_peri_config_to_acp();  // 删除此行
self.session_mgr.sessions[self.session_mgr.active]
    .session_panels
    .close_if(PanelKind::Model);

// 统一走 ACP 协议
if let Some(ref acp_client) = self.acp_client {
    let acp = acp_client.clone();
    let alias = alias_label.clone().to_lowercase();
    let effort_val = effort.clone();
    tokio::spawn(async move {
        let _ = acp.set_config_option("model", &alias).await;
        let _ = acp.set_config_option("thinking_effort", &effort_val).await;
    });
}
```

- [ ] **Step 3: 替换 model_panel.rs 中的 apply_1m_context 和 apply_confirm**

`peri-tui/src/app/model_panel.rs`

`apply_confirm`（约第406行）中 `ctx.services.sync_peri_config_to_acp();` 替换为：

```rust
if let Some(ref acp_client) = ctx.app.acp_client {
    let acp = acp_client.clone();
    let alias = ctx.services.peri_config.as_ref().map(|c| c.config.active_alias.clone()).unwrap_or_default();
    let effort = panel.buf_thinking_effort.clone();
    let context_1m_val = panel.buf_context_1m.to_string();
    tokio::spawn(async move {
        let _ = acp.set_config_option("model", &alias).await;
        let _ = acp.set_config_option("thinking_effort", &effort).await;
        let _ = acp.set_config_option("context_1m", &context_1m_val).await;
    });
}
```

注意：`apply_confirm` 中的 `App::save_config` 调用保留（TUI 侧本地持久化），ACP 协议用于同步到 Server。

`apply_1m_context`（约第444行）中 `ctx.services.sync_peri_config_to_acp();` 替换为：

```rust
if let Some(ref acp_client) = ctx.app.acp_client {
    let acp = acp_client.clone();
    let val = panel.buf_context_1m.to_string();
    tokio::spawn(async move {
        let _ = acp.set_config_option("context_1m", &val).await;
    });
}
```

注意：`apply_1m_context` 中已有 `App::save_config`（第422行），保留不变。

- [ ] **Step 4: 替换 effort.rs 中的 /effort 命令**

`peri-tui/src/command/session/effort.rs`

替换 `app.services.sync_peri_config_to_acp();` 为：

```rust
if let Some(ref acp_client) = app.acp_client {
    let acp = acp_client.clone();
    let val = new_effort.to_string();
    tokio::spawn(async move {
        let _ = acp.set_config_option("thinking_effort", &val).await;
    });
}
```

- [ ] **Step 5: 替换 command/panel/model.rs 中的 /model 命令**

`peri-tui/src/command/panel/model.rs`

替换 `app.services.sync_peri_config_to_acp();` 为：

```rust
if let Some(ref acp_client) = app.acp_client {
    let acp = acp_client.clone();
    let alias = app.services.peri_config.as_ref().map(|c| c.config.active_alias.clone()).unwrap_or_default();
    tokio::spawn(async move {
        let _ = acp.set_config_option("model", &alias).await;
    });
}
```

- [ ] **Step 6: 替换 panel_login.rs 中的 Login 面板操作**

`peri-tui/src/app/panel_login.rs`

有两处 `sync_peri_config_to_acp()`（约第58行和第160行）。全部替换为：

```rust
if let Some(ref acp_client) = self.acp_client {
    let acp = acp_client.clone();
    let cfg = self.services.peri_config.as_ref().unwrap().clone();
    tokio::spawn(async move {
        let _ = acp.update_config(&cfg).await;
    });
}
```

- [ ] **Step 7: 替换 login_panel/component.rs 中的编辑/删除确认**

`peri-tui/src/app/login_panel/component.rs`

有三处 `ctx.services.sync_peri_config_to_acp();`（约第65行、第210行、第258行）。全部替换为：

```rust
if let Some(ref acp_client) = ctx.app.acp_client {
    let acp = acp_client.clone();
    let cfg = ctx.services.peri_config.as_ref().unwrap().clone();
    tokio::spawn(async move {
        let _ = acp.update_config(&cfg).await;
    });
}
```

注意：`PanelContext` 可能没有 `acp_client` 字段。需要检查 `PanelContext` 结构体定义，必要时添加 `acp_client` 引用或通过 `ctx.app` 访问。

- [ ] **Step 8: 编译验证**

Run: `cargo build -p peri-tui`
Expected: 编译通过，无 `sync_peri_config_to_acp` 引用残留

- [ ] **Step 9: 全局搜索确认无遗漏**

Run: `grep -r "sync_peri_config_to_acp" peri-tui/src/`
Expected: 无匹配

Run: `grep -r "acp_peri_config\|acp_provider" peri-tui/src/`
Expected: 无匹配（除注释外）

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "refactor: replace all sync_peri_config_to_acp with ACP protocol calls

All config changes now go through ACP protocol:
- Model/effort/1M changes use set_config_option
- Login panel CRUD uses update_config
- All fire-and-forget via tokio::spawn"
```

---

### Task 6: 端到端编译 + 测试验证

**Files:** 无新增修改

- [ ] **Step 1: 全量编译**

Run: `cargo build`
Expected: 全量编译通过

- [ ] **Step 2: 运行 peri-acp 测试**

Run: `cargo test -p peri-acp`
Expected: 所有测试通过（含迁移的 store 测试）

- [ ] **Step 3: 运行 peri-tui 测试**

Run: `cargo test -p peri-tui`
Expected: 所有测试通过

- [ ] **Step 4: 运行全量测试**

Run: `cargo test`
Expected: 所有测试通过

- [ ] **Step 5: 运行 clippy**

Run: `cargo clippy --workspace 2>&1 | head -50`
Expected: 无新增 warning

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: verify full build and tests after ACP config protocol migration"
```
