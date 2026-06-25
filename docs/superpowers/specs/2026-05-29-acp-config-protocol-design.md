# ACP 配置协议纯净化设计

> 日期：2026-05-29
> 状态：Draft

## 目标

废除 TUI↔ACP Server 的 Arc 共享内存，所有配置变更走 ACP 协议。消除 TUI 层与 ACP Server 的隐式耦合，为 Stdio/IDE 外部客户端铺平道路。

## 决策记录

通过 grill-me 确认以下决策：

- 模型切换、thought_effort、context_1m → `session/set_config_option`
- Login 面板 Provider CRUD → 新增 `session/update_config`（传输完整 PeriConfig）
- thought_effort 和 context_1m 持久化到磁盘，model 仅内存
- TUI 侧乐观更新 + `tokio::spawn` fire-and-forget ACP 请求
- ACP Server 自己 `store::load()` 初始化，不依赖 TUI 传入
- `store.rs` 从 `peri-tui` 迁移到 `peri-acp`
- configId 统一为 `thought_effort`（废弃 `thought_level`）
- context_1m 注册为自定义 configOption（category `_context`）

## configId 清单

| configId | 处理 | 持久化 |
|----------|------|--------|
| `mode` | 写 permission_mode | 否 |
| `model` | 解析 alias → 写 provider Arc | 否 |
| `thought_effort` | 写 PeriConfig.thinking.effort + save | 是 |
| `context_1m` | 写 PeriConfig.context_1m + save | 是 |

## 架构

### 改造前

```
TUI 快捷键/面板 → 修改本地 PeriConfig
                 → sync_peri_config_to_acp() → 写 Arc<RwLock<PeriConfig>>
                 → ACP Server 读 Arc（共享内存）
```

### 改造后

```
TUI 快捷键/面板 → 乐观更新本地 PeriConfig（立即刷新 UI）
                 → tokio::spawn(acp.set_config_option / acp.update_config)
                 → ACP Server 修改自己的 Arc + 按需持久化
                 → 返回 configOptions（TUI 可选择性消费）
```

## Section 1：store.rs 迁移

将 `peri-tui/src/config/store.rs` 移到 `peri-acp/src/provider/store.rs`。

**改动**：
1. `peri-acp/src/provider/store.rs`：从 `peri-tui` 搬入 `load`/`save`/`save_to`/`config_path`/`workspace_config_path`/`load_from`
2. `peri-acp/src/provider/mod.rs`：新增 `pub mod store;`
3. `peri-acp/Cargo.toml`：新增 `dirs-next` 依赖
4. `peri-tui/src/config/mod.rs`：改为 re-export `peri_acp::provider::store::{load, save, save_to}`
5. `peri-tui/src/config/store.rs`：删除
6. 测试文件 `store_test.rs` 随 store.rs 迁移

## Section 2：ACP Server 侧改动

### 2.1 AcpServerConfig 新增字段

```rust
pub struct AcpServerConfig {
    // ...existing fields...
    pub config_path: std::path::PathBuf,
}
```

TUI 构造时传入 `store::config_path()`。ACP Server 构造时自行调用 `store::load()` 初始化 `peri_config` 和 `provider`。

### 2.2 set_config_option handler 扩展

提取持久化辅助函数：

```rust
fn persist_config(cfg: &AcpServerConfig) {
    let c = cfg.peri_config.read();
    if let Err(e) = store::save_to(&c, &cfg.config_path) {
        warn!(error = %e, "Failed to persist config");
    }
}
```

各 configId 处理：

- `thought_effort`：写 `PeriConfig.thinking.effort` + `persist_config()`
- `context_1m`：写 `PeriConfig.context_1m` + `persist_config()`
- `model`：不变（仅内存）
- `mode`：不变（仅内存）

### 2.3 新增 session/update_config handler

```rust
"session/update_config" => {
    let new_cfg: PeriConfig = serde_json::from_value(params)?;
    *cfg.peri_config.write() = new_cfg.clone();
    if let Some(p) = LlmProvider::from_config(&new_cfg) {
        *cfg.provider.write() = p;
    }
    persist_config(cfg);
    let resp = build_config_options_response(cfg);
    serde_json::to_value(resp).map_err(...)
}
```

校验：providers 非空、active_provider_id 指向存在的 provider、active_alias 合法。

### 2.4 初始化

`AcpServerConfig` 构造时：
- 调用 `store::load()` 获取 PeriConfig
- 从 PeriConfig 计算 `LlmProvider`
- TUI 不再 clone Arc 到 ServiceRegistry

## Section 3：TUI 侧改动

### 3.1 删除 Arc 共享

| 删除项 | 文件 |
|--------|------|
| `acp_peri_config` 字段 | `service_registry.rs` |
| `acp_provider` 字段 | `service_registry.rs` |
| `sync_peri_config_to_acp()` 方法 | `service_registry.rs` |
| Arc clone 赋值 | `main.rs:655-656` |
| 测试中的 None 赋值 | `panel_ops.rs:132-133` |

### 3.2 调用点替换

所有 `sync_peri_config_to_acp()` 调用替换为 ACP 协议调用：

| 调用点 | 替换为 |
|--------|--------|
| `shortcuts.rs` Ctrl+T 切模型 | `spawn(acp.set_config_option("model", alias))` |
| `shortcuts.rs` Ctrl+Shift+T 切 Provider | `spawn(acp.set_config_option("model", alias))` |
| `panel_model.rs` 面板确认 | `spawn(acp.set_config_option("model", ...))` + `spawn(acp.set_config_option("thought_effort", ...))` |
| `effort.rs` /effort 命令 | `spawn(acp.set_config_option("thought_effort", value))` |
| `model_panel.rs` 1M 开关 | `spawn(acp.set_config_option("context_1m", value))` |
| `panel_login.rs` 保存/删除 | `spawn(acp.update_config(full_config))` |
| `login_panel/component.rs` 编辑/新建/删除 | `spawn(acp.update_config(full_config))` |
| `command/panel/model.rs` /model 命令 | `spawn(acp.set_config_option("model", alias))` |
| `app/mod.rs` refresh_after_setup（Setup 向导保存） | `spawn(acp.update_config(full_config))` |

### 3.3 AcpTuiClient 新增方法

```rust
pub async fn update_config(&self, config: &PeriConfig) -> Result<Vec<ConfigOption>, String> {
    let session_id = self.current_session_id.lock().unwrap().clone().ok_or("no active session")?;
    let params = serde_json::to_value(config)?;
    let resp = self.transport.send_request("session/update_config", params).await?;
    // 解析 configOptions 从响应
}
```

### 3.4 初始化流程变更

`main.rs` 不再 clone Arc 到 ServiceRegistry。`AcpServerConfig` 构造时自行 load + 计算 provider。

## Section 4：错误处理

### 4.1 fire-and-forget 失败

TUI 和 ACP Server 同进程，channel 断开意味着进程快崩了。不额外处理失败回滚。

### 4.2 update_config 校验

- providers 非空
- active_provider_id 指向存在的 provider
- active_alias 是合法别名

校验失败返回 ACP error。TUI 侧可回滚乐观更新或弹 system_note。

### 4.3 并发安全

ACP Server 串行处理请求，后到的覆盖先到的，与当前行为一致。

## Section 5：测试策略

### 5.1 单元测试

| 测试 | 位置 | 验证 |
|------|------|------|
| store.rs 迁移后 load/save | `peri-acp/src/provider/store_test.rs` | 迁移不破坏读写 |
| set_config_option context_1m | `peri-tui/src/acp_server/requests_test.rs` | 内存值正确 + 磁盘文件更新 |
| set_config_option thought_effort | 同上 | 内存值正确 + 磁盘文件更新 |
| session/update_config | 同上 | 完整 PeriConfig 应用 + 持久化 + provider 重算 |

### 5.2 手动验证清单

1. Ctrl+T 切模型 → 面板显示正确 → 发起对话用新模型
2. Ctrl+Shift+T 切 Provider → 同上
3. `/effort high` → 重启后 effort 保持
4. 1M 开关 → 重启后保持
5. Login 面板新建 Provider → 可选为新 Provider → 对话正常
6. Login 面板删除 Provider → 自动回退 → 对话正常
