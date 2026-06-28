# MCP OAuth 回调服务器 state 参数实际未校验（永远为空字符串）

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding M3，置信度 8/10）
**修复日期**：2026-06-27（PR #82，commit 713ddcb1 / 56a1a07f）

## 问题描述

`OAuthCallbackServer::bind()` 初始化 `state_param: String::new()`（空字符串），`wait_inner()` 调用 `parse_callback_url(url_path, &self.state_param)` 时传入的 `expected_state` 永远是空。`parse_callback_url` 第 137 行的判断 `if !expected_state.is_empty() && state != expected_state` —— 由于 `expected_state` 永远为空，state 校验永远跳过。本地回调服务器本身不防御 CSRF。

state 实际由 `rmcp::OAuthState::handle_callback` 在 `oauth_flow.rs:181` 二次校验，所以登录流程本身仍受 rmcp 保护。但本地回调服务器（绑定 127.0.0.1 随机端口）接受任意 state 值并传给 `handle_callback`，若 rmcp 校验存在缺陷则无纵深防御。

## 当前行为

```rust
// peri-middlewares/src/mcp/callback_server.rs:24-46
// OAuthCallbackServer::bind() 初始化 state_param: String::new()
```

```rust
// peri-middlewares/src/mcp/callback_server.rs:88
// wait_inner() 调用 parse_callback_url(url_path, &self.state_param)
// self.state_param 始终是空字符串
```

```rust
// peri-middlewares/src/mcp/callback_server.rs:137
// parse_callback_url 中：
// if !expected_state.is_empty() && state != expected_state {
//     // 校验 state
// }
// 因 expected_state 永远为空，校验被完全跳过
```

## 预期行为

```rust
// OAuthCallbackServer 应当持有 OAuthState 生成的 state 值
struct OAuthCallbackServer {
    expected_state: String,  // 非空，来自 OAuthState::start_authorization
    // ...
}

// wait_inner() 应当在 parse_callback_url 中严格校验 state 一致
// 不一致时拒绝回调，不进入 handle_callback
```

## 利用场景

1. 受害者发起 MCP OAuth 授权，浏览器跳到 provider 完成登录。
2. provider 回调 `http://127.0.0.1:<port>/callback?code=...&state=...`。
3. 同机攻击者通过 `/proc/net/tcp`（Linux）或 `netstat`（macOS/Windows）枚举本地端口，找到受害者 peri 进程监听的随机端口。
4. 攻击者抢在浏览器回调之前连接该端口，提交攻击者自己 OAuth 流程拿到的 code。
5. 因 callback_server 不校验 state，code 被直接传给 `handle_callback`，若 rmcp 校验存在缺陷（state 由同一 OAuthState 处理但跨进程未隔离），可能导致 code 混淆。
6. 即使 rmcp 校验严格，本地回调服务器本身缺乏纵深防御。

## 修复方案

1. **传入真实 state**：在 `OAuthCallbackServer::bind()` 时传入 `OAuthState` 生成的 state 值。
2. **先校验后处理**：`wait_inner()` 在 `parse_callback_url` 中**先于** `handle_callback` 校验 state 一致性，不一致直接拒绝。
3. **绑定后立即设置 state**：缩小竞态窗口，state 在端口绑定成功后立即设置（防止攻击者抢端口）。
4. **加 token 校验**（可选）：state 与 PKCE code_verifier 绑定，code 交换阶段二次校验。

## 涉及文件

- `peri-middlewares/src/mcp/callback_server.rs:24-46` — `OAuthCallbackServer::bind()` state 初始化
- `peri-middlewares/src/mcp/callback_server.rs:88` — `wait_inner()` 调用
- `peri-middlewares/src/mcp/callback_server.rs:137` — `parse_callback_url` 校验逻辑
- `peri-middlewares/src/mcp/oauth_flow.rs:181` — `OAuthState::handle_callback` 真正校验位置

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 M3） |
| 2026-06-27 | Open | Fixed | agent | PR #82 合入：`OAuthCallbackServer` 接收 rmcp 生成的 state，`parse_callback_url` 严格校验 state 一致性，绑定后立即 `set_state` 缩小竞态窗口 |
