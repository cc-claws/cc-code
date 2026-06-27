# Sync KDF 重用 pair_code 作为 password 和 salt（预计算攻击）

**状态**：Open
**优先级**：中
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding M2，置信度 8/10）

## 问题描述

`derive_key(pair_code)` 把 pair_code 同时用作 PBKDF2 的 password 和 salt（`crypto.rs:29-30` 已亲验）。pair_code 是短人类可读串（4-8 位数字常见），既是 password 又是 salt，salt 的"每用户熵"完全失效。被动观察者拿到首块密文后可对候选 pair_code 暴力破解，AES-GCM tag 作为正确性 oracle。

## 当前行为

```rust
// peri-tui/src/sync/crypto.rs:24-34
pub fn derive_key(pair_code: &str) -> [u8; AES_KEY_LEN] {
    let mut key = [0u8; AES_KEY_LEN];
    pbkdf2::derive(
        PBKDF2_HMAC_SHA256,
        NonZeroU32::new(PBKDF2_ITERATIONS).expect("100000 > 0"),
        pair_code.as_bytes(),  // salt
        pair_code.as_bytes(),  // password
        &mut key,
    );
    key
}
```

PBKDF2 迭代 100,000 次（合理），AES-256-GCM nonce 用 `OsRng`（正确），但 salt 完全失效。

## 预期行为

| 项 | 当前 | 预期 |
|----|------|------|
| salt 来源 | pair_code 本身 | 每次同步会话独立的随机 salt（≥16 字节） |
| KDF | PBKDF2-SHA256 100k | 改用 Argon2id（GPU/ASIC 抗性更强）或保留 PBKDF2 但配随机 salt |
| 密钥派生输入 | 仅 pair_code | pair_code + ECDH 共享密钥（高熵） |
| pair_code 熵 | 短人类串 | ≥128 bit base32，OsRng 生成 |

## 利用场景

1. 攻击者被动观察网络流量（公共 WiFi、ISP 日志、relay 被攻陷）。
2. 截获首块 `DataChunk`：`IV(12B) + ciphertext + auth_tag(16B)`。
3. 对常见 pair_code 字典（"123456"、"0000"、"abcd" 等）逐个派生 AES 密钥，尝试 AES-GCM 解密。
4. tag 匹配即正确密钥，解密整条会话。
5. 与 [[2026-06-26-security-sync-relay-mitm-can-decrypt-credentials]]（H1）叠加：relay 持有 pair_code 即可离线解密，无需 MITM。

## 修复方案

任选其一：

1. **引入 ECDH**：见 H1 修复方案 #2，从根本上消除 pair_code 作为密钥源。
2. **保留 PBKDF2 但用随机 salt**：
   - 发送方在握手阶段生成 16 字节 `OsRng` salt
   - 通过首条消息（明文或带外）发送给 receiver
   - `derive_key(pair_code, salt)` 用 receiver 提供的 salt
3. **升级 KDF**：改用 Argon2id，参数 `m=64MiB, t=3, p=4`。
4. **增加 pair_code 熵**：UI 强制使用 6+ 位 base32 代码（~30 bit 熵）。

## 涉及文件

- `peri-tui/src/sync/crypto.rs:24-34` — `derive_key` 实现
- `peri-tui/src/sync/sender.rs` — 调用 `derive_key` 的发送方
- `peri-tui/src/sync/receiver.rs` — 调用 `derive_key` 的接收方

## 关联

- 配套修复见 [[2026-06-26-security-sync-relay-mitm-can-decrypt-credentials]]（H1），两者需一起改

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 M2） |
