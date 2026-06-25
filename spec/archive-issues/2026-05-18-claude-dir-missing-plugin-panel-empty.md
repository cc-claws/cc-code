> 归档于 2026-05-18，原路径 spec/issues/2026-05-18-claude-dir-missing-plugin-panel-empty.md

# ~/.claude 目录不存在时插件面板 Discover/Marketplaces 视图无法使用

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-18

## 问题描述

当用户首次运行 peri，`~/.claude` 目录不存在时，打开 `/plugin` 面板：
- **Installed 视图**：正常，显示 "No plugins installed"
- **Discover 视图**：显示 "No plugins available"，没有任何可浏览/安装的插件
- **Marketplaces 视图**：仅显示 official marketplace 条目，但状态为 Stale，缓存为空

用户无法从"目录不存在"状态过渡到可用的插件系统——必须在面板外手动触发 marketplace 刷新才能创建目录并获取数据。

## 症状详情

| 视图 | ~/.claude 不存在时 | ~/.claude 存在且刷新后 |
|------|---------------------|------------------------|
| Installed | "No plugins installed"（正确） | 已安装插件列表 |
| Discover | "No plugins available"（**误导**） | 所有 marketplace 的可浏览插件列表 |
| Marketplaces | official 条目存在但 Stale，无缓存数据 | 各 marketplace 的插件数/已安装数/状态 |

### 为什么会这样

- `open_plugin_panel()` 从 `marketplaces_cache_dir()`（指向 `~/.claude/plugins/marketplaces/`）加载已缓存的 marketplace manifest
- `~/.claude/` 不存在 → 缓存目录不存在 → `try_load_cache()` 返回 `None` → Discover 列表为空
- 虽然 official marketplace 在代码中被硬编码加入 `all_known`，但无缓存数据支撑

### 已有容错

所有读取路径在文件不存在时都返回默认值（不会 panic 或崩溃），写入路径通过 `atomic_write_json` 的 `create_dir_all` 自动创建父目录。问题**不是**崩溃或错误，而是**终端用户体验**——首次使用时无法自然进入插件发现流程。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 确保 `~/.claude/` 目录不存在（`rm -rf ~/.claude`）
  2. 启动 peri
  3. 打开 `/plugin` 面板
  4. 切换到 Discover 视图 → 显示 "No plugins available"
- **环境**：首次安装 peri 或删除了 `~/.claude/` 的用户

## 期望行为

打开插件面板时，应自动创建 `~/.claude/plugins/` 必要子目录，并为 official marketplace 触发首次刷新，使用户看到可安装的插件列表。

## 涉及文件

- `peri-middlewares/src/plugin/config.rs` —— 所有 `~/.claude` 路径函数定义（`claude_home`, `plugins_dir`, `marketplaces_cache_dir` 等）
- `peri-tui/src/app/panel_ops.rs:373-700` —— `open_plugin_panel()` 加载插件/Discover/Marketplace 数据的主逻辑
- `peri-tui/src/ui/main_ui/panels/plugin.rs:883-893` —— Discover 视图空状态渲染（"No plugins available" 消息）
- `peri-middlewares/src/plugin/marketplace/manager.rs` —— `MarketplaceManager::init()` 和 `try_load_cache()`
