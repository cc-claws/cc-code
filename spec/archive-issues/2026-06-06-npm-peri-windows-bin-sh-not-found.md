> 归档于 2026-06-24，原路径 spec/issues/2026-06-06-npm-peri-windows-bin-sh-not-found.md

# npm 全局安装 peri 后 Windows 上执行报 /bin/sh.exe not found

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-06

## 问题描述

在 Windows 上通过 `npm install -g @cc-claw/peri` 全局安装 peri 后，执行 `peri` 命令报错 `/bin/sh.exe` 找不到。npm 自动生成的 PowerShell shim 尝试用 `/bin/sh` 执行 shell 脚本，但 Windows 没有这个路径。

## 症状详情

执行 `peri --version`（或其他 peri 子命令）时，PowerShell 输出：

```
无法将"/bin/sh.exe"项识别为 cmdlet、函数、脚本文件或可运行程序的名称。
所在位置 C:\Users\wismy\AppData\Roaming\npm\peri.ps1:24 字符: 7
+     & "/bin/sh$exe"  "$basedir/node_modules/@cc-claw/peri/bin/peri" $ ...
+       ~~~~~~~~~~~~~
    + CategoryInfo          : ObjectNotFound: (/bin/sh.exe:String) [], CommandNotFoundException
    + FullyQualifiedErrorId : CommandNotFoundException
```

npm 全局 shim 文件 `~\AppData\Roaming\npm\peri.ps1` 的关键内容：

```powershell
# 第 11-24 行：尝试用 /bin/sh 执行 bin/peri shell 脚本
if (Test-Path "$basedir//bin/sh$exe") {
    & "$basedir//bin/sh$exe"  "$basedir/node_modules/@cc-claw/peri/bin/peri" $args
} else {
    & "/bin/sh$exe"  "$basedir/node_modules/@cc-claw/peri/bin/peri" $args  # ← 此行报错
}
```

而包内 `bin/` 目录已有正确的 Windows 包装脚本：

- `bin/peri.cmd` → `@echo off\r\n"%~dp0peri.exe" %*\r\n`
- `bin/peri.ps1` → 直接调 `peri.exe`

但 npm 全局 shim 绕过了这些，直接包装 `bin/peri`（shell 脚本）。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. Windows 11 + PowerShell
  2. `npm install -g @cc-claw/peri`
  3. 执行 `peri --version`
- **环境**：Windows（所有版本），npm 全局安装方式

## 涉及文件

- `package.json` —— `bin` 字段指向 `./bin/peri`（shell 脚本），npm 据此生成全局 shim
- `bin/peri` —— Linux shell 脚本入口，npm 在 Windows 上用 `/bin/sh` 包装它
- `bin/peri.cmd` / `bin/peri.ps1` —— `install.js` 生成的正确 Windows 包装脚本（被 npm 全局 shim 绕过）
- `install.js` —— postinstall 脚本，下载平台二进制并生成 `peri.cmd`/`peri.ps1`

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-06 | — | Open | agent | 创建 |
| 2026-06-06 | Open | Fixed | agent | 修复：bin/peri 从 shell 改为 Node.js 脚本 |

## 修复记录

**修复方案**：将 `npm/bin/peri` 从 shell 脚本（`#!/bin/sh`）改为 Node.js 脚本（`#!/usr/bin/env node`）

**根因**：npm 的 `bin` 字段指向 shell 脚本时，npm 在 Windows 上生成的全局 shim 会尝试用 `/bin/sh` 执行，但 Windows 没有这个路径。

**解决方式**：改为 Node.js 脚本后，npm 生成的 shim 使用 `node` 执行，跨平台兼容。

**提交**：`hotfix#npm-windows-bin-sh` 分支，commit b33864a
