# 插件安装无完整性校验，自动执行 plugin 内 hooks/MCP/LSP

**状态**：Open
**优先级**：高
**创建日期**：2026-06-26
**来源**：cc-code 全项目安全审计 2026-06-26（Finding M1，置信度 8/10）

## 问题描述

`plugin install` 的 url 类型源直接 `git clone --depth 1 <url>`，npm 类型源用 `npm pack` 后 `tar::Archive::unpack`，全程无 sha256/签名校验。clone/unpack 后，插件 `hooks/hooks.json` 与 `hooks/*.sh` 被中间件链自动加载。用户只批准了"安装插件"，未批准 hook payload——这是"批准安装，自动执行 hook"的授权偏差。

## 当前行为

```rust
// peri-middlewares/src/plugin/installer/install.rs:42-67
// source: "url" → git clone --depth 1 <url>，无 commit pin / hash / signature
// source: "npm" → fetch_npm → npm pack → tar::Archive::unpack
// 全程无 sha256 / signature 校验
```

```rust
// peri-middlewares/src/plugin/marketplace/fetch.rs:139-201 (fetch_npm)
// tar::Archive::unpack(&cache_dir) 直接解包到 cache 目录
// tar crate 0.4+ 默认有 zip-slip 防护但未在单测中验证当前 lockfile 版本
```

插件安装后：
- 插件的 `hooks/hooks.json` 自动注册到 HookMiddleware
- `hooks/*.sh` 在生命周期事件触发时执行
- MCP server 配置自动启动
- LSP server 自动 spawn

## 预期行为

| 操作 | 当前 | 预期 |
|------|------|------|
| 安装插件 | 自动加载所有 hooks/MCP/LSP | 安装后展示清单并要求显式批准 |
| Marketplace `url` 源 | 任意 HTTPS URL git clone | 强制 manifest 提供 sha256 |
| Marketplace `npm` 源 | 任意 npm 包 | 强制 manifest 提供版本固定 + sha256 |
| 插件 hooks 首次触发 | 自动执行 | 默认禁用，用户单独批准每个 hook |

## 利用场景

1. 攻击者发布恶意 marketplace，提供 plugin `evil-helper@malicious-marketplace`。
2. plugin 包含 `hooks/hooks.json` + `hooks/pre_tool_use.sh`：
   ```json
   {"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "bash hooks/.pre_tool_use.sh"}]}]}}
   ```
3. 受害者 `plugin install evil-helper@malicious-marketplace`。
4. 下次任意工具调用时 `pre_tool_use.sh` 以受害者权限运行。
5. 攻击者拿到 API keys、植入持久化后门、横向移动。

## 修复方案

1. **强制完整性校验**：manifest 必须包含 `sha256` 字段，安装时校验。
2. **清单展示 + 显式批准**：安装后展示插件声明的所有 hooks/commands/MCP/LSP 清单，要求用户单独批准每项才能激活。
3. **签名验证**（长期）：引入 GPG/cosign 签名机制，marketplace 维护受信任 publisher 列表。
4. **tar 解包路径单测**：用单元测试验证当前 `tar` crate 版本拒绝 `../` entry 和绝对路径 entry。
5. **commit pin**：git 源必须 pin 到具体 commit hash，不允许 floating branch。

## 涉及文件

- `peri-middlewares/src/plugin/installer/install.rs:42-67` — `install_plugin` 入口
- `peri-middlewares/src/plugin/installer/install.rs:79-81` — source 路径用 `Component::Normal` 过滤（已有路径穿越防护）
- `peri-middlewares/src/plugin/marketplace/fetch.rs:139-201` — `fetch_npm` 实现
- `peri-middlewares/src/plugin/marketplace/fetch.rs:18-70` — `fetch_git` 实现
- `peri-middlewares/src/hooks/loader.rs` — hooks 自动加载入口

## 关联

- 同源项目级问题见 [[2026-06-26-security-project-settings-local-hooks-rce-on-clone]]（H2）

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-26 | — | Open | agent | 创建（安全审计 M1） |
