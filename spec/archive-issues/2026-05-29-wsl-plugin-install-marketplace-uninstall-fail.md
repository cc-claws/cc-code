> 归档于 2026-05-29，原路径 spec/issues/2026-05-29-wsl-plugin-install-marketplace-uninstall-fail.md
# Peri 插件系统依赖 Claude Code 目录结构，未安装 CC 时安装/卸载不可用

**状态**：Fixed
**优先级**：高
**创建日期**：2026-05-29
**修复日期**：2026-05-29

## 问题描述

Perihelion 的插件系统依赖 Claude Code 的 `~/.claude/` 目录结构（`plugins/cache/`、`plugins/installed_plugins.json`、`settings.json` 的 `enabledPlugins` 字段等）。在未安装 Claude Code 的环境中（如 WSL），插件安装和市场源卸载不可用。安装 CC 后功能恢复正常。

这说明 Peri 的插件系统目前无法独立运行，隐式依赖 CC 的目录和配置文件已存在。

## 症状详情

| 功能 | 有 CC | 无 CC |
|------|-------|-------|
| 添加市场源 | 正常 | 正常 |
| 插件安装 | 正常 | 失败 |
| 市场源卸载 | 正常 | 失败 |

**环境信息**：
- OS：WSL（Windows Subsystem for Linux）
- Claude Code：未安装 → 安装后恢复正常

## 复现条件

- **复现频率**：必现（无 CC 环境）
- **触发步骤**：
  1. 在未安装 Claude Code 的环境中启动 Perihelion TUI
  2. 打开插件面板，添加市场源（成功）
  3. 尝试安装插件 → 失败
  4. 尝试卸载市场源 → 失败
- **环境**：任意 OS，`~/.claude/` 目录不存在或缺少 CC 的目录结构

## 修复方向

启动时检测 `~/.claude/` 目录是否为 CC 原生创建的完整结构，若缺少则自动补建插件所需的子目录和最小配置文件，使 Peri 能独立运行插件系统。

**根因**：`~/.claude/plugins/marketplaces/` 不存在 → `refresh_marketplace` 中 git clone 写入缓存失败 → 无 `marketplace.json` → `get_marketplace_manifest` 返回 `PluginNotFound` → 安装失败。所有路径操作本身有安全回退，但 `get_marketplace_manifest` 依赖缓存已存在，无回退。

### 排查结果（2026-05-29）

经代码审查，**所有文件系统路径操作在"无 CC 目录"时均有安全回退**：

| 操作 | 文件不存在时行为 |
|------|----------------|
| `load_installed_plugins` | 返回空列表 ✅ |
| `save_installed_plugins` | `atomic_write_json` 自动 `create_dir_all` ✅ |
| `load_known_marketplaces` | 返回空列表 ✅ |
| `save_known_marketplaces` | 自动创建 ✅ |
| `load_claude_settings` | 返回默认值 ✅ |
| `save_claude_settings_enabled_plugins` | 自动创建 ✅ |
| `update_enabled_plugins` | 自动创建 ✅ |
| `get_marketplace_manifest` | ❌ 返回 `PluginNotFound` 错误 |

**确认根因**：无 CC 环境 → `~/.claude/plugins/marketplaces/` 不存在 → `refresh_marketplace` (git clone) 写入缓存失败 → 无 `marketplace.json` → `get_marketplace_manifest` 返回 `PluginNotFound` → 安装失败。

1. **市场源 refresh 在 WSL 网络环境下失败**（git clone/fetch 超时、DNS 解析等），导致 `marketplace_cache_dir/{marketplace}/` 下没有 `marketplace.json`，后续安装必然 `PluginNotFound`
2. **市场源卸载的名称匹配问题**：`persist_marketplace_delete` 用 `MarketplaceSource` 解析出的 `km_name` 与 UI 中 `entry.name` 对比，若 refresh 失败导致 `known_marketplaces.json` 中 `source` 与 UI 显示名称不一致，则删不掉
3. **无法在 macOS 本地复现 WSL 环境差异**，需要在 WSL 上开启 `RUST_LOG=debug` 捕获实际错误信息

### 下一步

在 WSL 环境下运行 `RUST_LOG=debug cargo run -p peri-tui`，执行插件安装和市场源卸载操作，收集日志中的：
- `Marketplace '{name}' 刷新失败` (warn)
- `Plugin install failed` (PluginActionCompleted)
- `PluginAction failed` (PluginActionCompleted)

## 涉及文件

- `peri-middlewares/src/plugin/config.rs` —— `claude_home()`、路径配置、`atomic_write_json` 中目录创建
- `peri-middlewares/src/plugin/installer/install.rs` —— `install_plugin` 安装流程
- `peri-tui/src/app/plugin_panel/handlers/plugin_handlers/persistence.rs` —— 市场源持久化
