> 归档于 2026-05-24，原路径 spec/issues/2026-05-21-setup-wizard-settings-not-reloaded.md

# Setup 向导完成后 ACP Server 配置未刷新，导致 API key 未生效

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-21
**修复日期**：2026-05-21

## 问题描述

首次启动程序 → Setup 向导配置 Provider 和 API key → 保存成功后立即发送消息 → API 调用返回认证错误（401/403）。重启程序后正常工作。

核心原因：Setup 向导保存配置后，只更新了 TUI 侧的 `app.services.peri_config`，但 ACP Server（负责实际 Agent 构建和 LLM 调用的后端）持有的 `AcpServerConfig.peri_config` 和 `AcpServerConfig.provider` 仍然是启动时的空默认值。

## 症状详情

| 表现 | 说明 |
|------|------|
| 触发时机 | Setup 向导完成保存后，第一次提交消息 |
| API 响应 | 认证错误（无有效 API key） |
| 配置文件 | `~/.peri/settings.json` 已正确写入（验证方法：重启后正常工作） |
| 用户操作 | 必须重启程序才能正常使用 |

## 根因分析

### 数据流

```
main.rs 启动:
  App::new() → app.services.peri_config = load() (空 / 默认值)
  AcpServerConfig {
      provider: Arc<RwLock<LlmProvider>>,    ← 从 app.services 构造，生命周期独立
      peri_config: Arc<RwLock<PeriConfig>>,  ← 从 app.services 构造，生命周期独立
  }
  run_acp_server(config) ← server_config 被 move 进 ACP Server

Setup 向导保存:
  keyboard.rs: SaveAndClose
    → save_setup()         ← 写入 ~/.peri/settings.json ✓
    → refresh_after_setup(cfg)
        → app.services.peri_config = Some(cfg)  ← TUI 层更新 ✓
        → AcpServerConfig.provider / peri_config ← 没有更新 ✗
          ↑ 这两个 Arc 已被 move 进 ACP Server spawn task
          ↑ App 结构体不持有引用，无法回写

用户提交消息:
  agent_submit.rs: LlmProvider::from_config(app.services.peri_config) ← 正确，有 API key
    → 但仅用于本地 model_name / context_window 显示
  ACP prompt: 读取 AcpServerConfig.peri_config / provider ← 仍是空默认值 ✗
    → build_agent() 使用空 API key → API 调用 401
```

### 关键代码位置

1. **Setup 保存处理** — `peri-tui/src/event/keyboard.rs:91-98`：调用 `save_setup` + `refresh_after_setup`
2. **refresh_after_setup** — `peri-tui/src/app/mod.rs:535-542`：仅更新 `ServiceRegistry`，未更新 ACP Server 的 Arcs
3. **AcpServerConfig 创建** — `peri-tui/src/main.rs:601-617`：provider/peri_config Arcs 被 move 进 server，App 不持有引用
4. **ACP prompt 执行** — `peri-tui/src/acp_server/mod.rs:96-97`：读取 `cfg.provider.clone()` / `cfg.peri_config.clone()`

## 涉及文件

- `peri-tui/src/app/mod.rs:535-542` — `refresh_after_setup()` 未更新 ACP Server 配置
- `peri-tui/src/main.rs:601-617` — `AcpServerConfig` 创建，Arcs 被 move 后 App 失去引用
- `peri-tui/src/event/keyboard.rs:91-98` — Setup 保存后的事件处理链
- `peri-tui/src/acp_server/mod.rs:96-97` — ACP prompt 读取旧配置

## 建议修复方向

1. **方案 A（推荐）**：在 `App` / `ServiceRegistry` 中存储 `AcpServerConfig.provider` 和 `AcpServerConfig.peri_config` 的 Arc 克隆，`refresh_after_setup()` 中通过 `RwLock::write()` 更新
2. **方案 B**：`refresh_after_setup()` 通过 ACP 通知/请求机制告知 Server 重新加载配置
3. 同类问题：`login_panel/component.rs`、`panel_model.rs`、`model_panel.rs` 中的配置保存路径也需要检查是否同步更新了 ACP Server 的 Arcs

## 修复方案

在 `ServiceRegistry` 中新增 `acp_provider` 和 `acp_peri_config` 字段，持有与 ACP Server 共享的 `Arc<RwLock<>>` 引用。`main.rs` 中创建 `AcpServerConfig` 后克隆 Arcs 存入 `app.services`。新增 `ServiceRegistry::sync_peri_config_to_acp()` 方法，将当前 `peri_config` 和重构建的 `LlmProvider` 写入共享 Arc。

**覆盖的 8 条配置修改路径**（均在修改后调用 `sync_peri_config_to_acp()`）：

| 路径 | 文件 |
|------|------|
| Setup 向导 SaveAndClose | `refresh_after_setup()` → `app/mod.rs` |
| Login Browse Enter | `login_panel/component.rs` |
| Login Edit/New Enter | `login_panel/component.rs` |
| Login ConfirmDelete Enter | `login_panel/component.rs` |
| Login wrapper 方法（测试用） | `panel_login.rs` |
| Model Enter confirm | `panel_model.rs` → `model_panel.rs:apply_and_close` |
| Model 1M context toggle | `model_panel.rs:apply_1m_context` |
| Alt+M 模型循环 | `event/keyboard.rs` |
| `/model` 命令 | `command/panel/model.rs` |
| `/effort` 命令 | `command/session/effort.rs` |
