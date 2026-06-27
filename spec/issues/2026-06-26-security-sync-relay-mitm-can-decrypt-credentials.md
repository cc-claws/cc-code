# Sync relay 是加密 MITM，可解密全部同步内容（含 API keys）

**状态**：Open
**优先级**：紧急
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding H1，置信度 9/10）

## 问题描述

`peri sync` 通过公网 WebSocket relay 中继配置同步。relay 服务器生成 pair_code 并下发，客户端用它派生 AES-256-GCM 密钥加密同步内容。**relay 持有自己生成的 pair_code 即可解密全部 payload**，包括 `~/.peri/settings.json` 里的 `ProviderConfig.api_key`、`~/.claude/settings.json`、`~/.mcp.json` 的 OAuth client secrets、plugin cache。

## 当前行为

`crypto.rs:24-34` 把 `pair_code` 同时用作 PBKDF2 的 password 和 salt：

```rust
pub fn derive_key(pair_code: &str) -> [u8; AES_KEY_LEN] {
    let mut key = [0u8; AES_KEY_LEN];
    pbkdf2::derive(
        PBKDF2_HMAC_SHA256,
        NonZeroU32::new(PBKDF2_ITERATIONS).expect("100000 > 0"),
        pair_code.as_bytes(),  // salt = pair_code
        pair_code.as_bytes(),  // password = pair_code
        &mut key,
    );
    key
}
```

`sender.rs:23-46` 中 pair_code 来自 relay 下发的 `WsMessage::PairCreated`。
`receiver.rs:29` 又把 pair_code 通过 `?code={pair_code}` URL 参数回传给 relay。
`scanner.rs:47-64` 扫描 `~/.peri/settings.json` 等含 `ProviderConfig.api_key` 字段的明文配置，打包后加密。

## 预期行为

- relay 不能解密任何客户端负载。
- pair_code 高熵且不由 relay 控制。

## 利用场景

1. 用户运行 `peri sync sender` 连接公网 relay（默认或第三方）。
2. relay 生成 pair_code（短人类可读串，4-8 位数字常见）并下发给 sender。
3. receiver 用同一 pair_code 派生密钥，relay 全程中继 `DataChunk`。
4. relay 离线留存所有 `DataChunk` + pair_code，任意时刻派生 AES 密钥解密拿到受害者全部 LLM API key 和 OAuth 凭据。
5. 用户 API key 被滥用产生账单，或 OAuth token 被用于横向渗透 MCP 服务器。

## 修复方案

任选其一，按推荐度排序：

1. **客户端生成 pair_code**：发送方用 `OsRng` 生成 ≥128-bit base32 pair_code，发送给 relay 仅做匹配；receiver 通过带外（屏幕输入）获取。relay 始终不知道 pair_code。
2. **X25519 ECDH 一次性密钥**：两客户端在 relay 之上协商临时密钥，relay 只看到密文。
3. **最低限度**：UI 显著提示"通过不受信任的 relay 同步会暴露 API key"，建议用户自建 relay。

## 涉及文件

- `peri-tui/src/sync/crypto.rs:24-34` — `derive_key` 实现（KDF password=salt=pair_code）
- `peri-tui/src/sync/sender.rs:23-46` — pair_code 来源（relay 下发）
- `peri-tui/src/sync/receiver.rs:29` — pair_code 通过 URL 回传 relay
- `peri-tui/src/sync/scanner.rs:47-64` — 明文扫描范围（含 `ProviderConfig.api_key`）
- `peri-tui/src/sync/packer.rs:32-43` — 打包含密钥的 settings.json

## 关联

- 同源 KDF 弱点见 [[2026-06-26-security-sync-kdf-reuses-paircode-as-salt-and-password]]（M2）
- `~/.peri/settings.json` 含明文 API key 的存储问题不在此 issue 范围（按"secrets on disk"分类排除）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 H1） |
