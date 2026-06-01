# new_thread() 死锁风险 + session/update_config 状态不一致

**状态**：Open
**优先级**：中
**创建日期**：2026-05-29

## 问题描述

两个独立但相关的健壮性问题：

1. `new_thread()` 使用 `block_in_place + block_on` 在 tokio runtime 上同步等待 ACP `new_session()`。如果 transport 的响应处理与当前 runtime 共享线程池，所有 worker 线程可能阻塞在 `block_on` 上等待响应，但无人处理 incoming response → 死锁。ACP 大重构（bb388ca）将 `new_thread` 从 fire-and-forget 的 `tokio::spawn` 改为同步等待。

2. `session/update_config` 先写入 `peri_config`（`requests.rs:444`），再尝试创建 `LlmProvider`。如果 `from_config` 返回 `None`（配置无效），`peri_config` 已更新但 `provider` 未更新 → 配置与 provider 状态不一致。下一轮 agent 构建使用的 provider 可能与新 config 不匹配。

## 症状详情

### 症状 1：new_thread 偶发死锁

- TUI 界面完全冻结（无响应）
- 高概率在高并发场景下触发（多个 session 操作同时进行）
- 当前实际触发概率较低（runtime 线程数通常足够），但属于定时炸弹

### 症状 2：update_config 状态不一致

- `peri_config` 中记录的 active_provider_id 指向 A，但实际 `provider` 仍使用旧的 B
- 用户切换模型后，config 文件写入成功但 agent 仍使用旧模型
- 后续 set_config_option 或 set_model 可能产生更多不一致

## 复现条件

### 症状 1

- **复现频率**：偶发（依赖线程池压力）
- **触发步骤**：快速连续执行 `/clear` 或 session 切换操作

### 症状 2

- **复现频率**：特定条件下必现（config 中的 provider 配置无效时）
- **触发步骤**：
  1. 通过 `session/update_config` 提交一个 providers 中 API key 格式错误的配置
  2. `from_config` 返回 None，provider 未更新
  3. 观察：peri_config 已更新（持久化到文件），但 provider 仍为旧值

## 涉及文件

- `peri-tui/src/app/thread_ops.rs:374-381` — `block_in_place + block_on` 同步等待
- `peri-tui/src/acp_server/requests.rs:444-448` — 先写 config 后验证 provider
