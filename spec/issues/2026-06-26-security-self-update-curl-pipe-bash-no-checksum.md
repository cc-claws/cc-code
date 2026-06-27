# 自更新机制 curl|bash 无 checksum/签名校验

**状态**：Open
**优先级**：低
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding L1，置信度 8/10）

## 问题描述

`run_update_unix` 执行 `bash -c "curl -fsSL <SCRIPT_URL_SH> | bash"`，URL 硬编码 raw.githubusercontent.com HTTPS，TLS 正常。但脚本输出直接喂给 bash，**无 checksum/签名**，`-L` 跟随重定向。属 `docs/superpowers/plans/2026-05-16-self-update-simplify-to-curl-pipe-bash.md` 文档化的有意设计，本 issue 仅作加固记录。

## 当前行为

```rust
// peri-tui/src/update.rs:34-47
pub fn run_update_unix() -> Result<(), UpdateError> {
    let script_url = SCRIPT_URL_SH; // 硬编码 raw.githubusercontent.com HTTPS
    let cmd = format!("curl -fsSL {} | bash", script_url);
    // spawn bash -c $cmd
}

// peri-tui/src/update.rs:49-69
// run_update_windows 类似，使用 PowerShell iex
```

- ✅ TLS 正常（curl 默认校验）
- ✅ URL 硬编码，非用户可控
- ❌ 无 sha256 checksum
- ❌ 无 GPG/cosign 签名
- ❌ `-L` 跟随重定向（GitHub 301 到 codeload 时仍 https，但若仓库被改名/迁移到攻击者域名可被利用）
- ❌ 仓库主分支被攻陷后任意修改 install.sh 即可立即让所有 `peri update` 用户中招

## 预期行为

| 项 | 当前 | 预期 |
|----|------|------|
| 完整性校验 | 无 | sha256 强制校验 |
| 签名 | 无 | cosign / GPG 签名（可选） |
| 重定向策略 | `-L` 跟随 | 限定 hostname 白名单 |
| 失败回退 | 直接报错 | 校验失败时保留旧版本 |

## 利用场景

1. 攻击者通过社工 / 凭据泄露 / 内部威胁拿到 `cc-claws/cc-code` main 分支写权限。
2. 修改 `install.sh` 注入 payload（不修改 Rust 代码，仅 install 脚本）。
3. 等待用户运行 `peri update`。
4. payload 以用户权限执行。
5. 因 install 脚本可独立修改，绕过 Rust 编译/CI 的所有审计。

注：此场景在"上游仓库可信"的前提下属于残余风险。GitHub 已经在仓库层面提供了 branch protection / signed commits 等机制，但 peri 客户端不强制校验。

## 修复方案

任选其一，按推荐度：

1. **sha256 校验**（最小加固）：
   - 发布时附带 `install.sh.sha256`（同样 raw.githubusercontent.com URL）
   - update 流程先下载 install.sh，校验 sha256，再喂给 bash
   - sha256 文件在 release 时随二进制一起发布并写入 CHANGELOG

2. **签名校验**（长期）：
   - 引入 cosign 签名，keyless 模式绑定 GitHub OIDC
   - 客户端内置 cosign 公钥，update 时验证签名

3. **GitHub Release 二进制**：
   - 直接下载 GitHub Release 的 tarball（含 sha256），跳过 install.sh
   - tarball 解包到 `~/.peri/versions/<version>/`，软链到 `~/.peri/current`

4. **保持现状**：接受 `curl|bash` 模式的残余风险，但在 README 显著位置提示用户 "运行 `peri update` 等同于执行仓库 maintainer 的任意脚本"。

## 涉及文件

- `peri-tui/src/update.rs:34-47` — `run_update_unix`
- `peri-tui/src/update.rs:49-69` — `run_update_windows`
- `docs/superpowers/plans/2026-05-16-self-update-simplify-to-curl-pipe-bash.md` — 当前设计的原始决策文档

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 L1，文档化有意设计的加固建议） |
