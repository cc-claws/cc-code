# Feature: 20260507_F001 — plugin-mcp-injection

## 需求背景

当前 `load_merged_config()` 的 env 展开步骤顺序有误：对插件 MCP server 的环境变量（`${CLAUDE_PLUGIN_ROOT}`、`${CLAUDE_PLUGIN_DATA}`、`${user_config.KEY}`）展开发生在**三层合并之后**。此时已无法区分"这个 server 来自哪个插件"，导致：

1. 两插件 A 和 B 都引用 `${user_config.API_KEY}`（各自不同的 key），合并后只有一个值胜出
2. 插件 server 配置在整个合并池中二次展开，丢失 per-plugin 上下文

Claude Code 的做法是**先展开后合并**——每个插件的 MCP server 在加入全局配置池之前，由插件专属的 `resolvePluginMcpEnvironment()` 独立处理。Peri 缺少这个 per-plugin 展开时机。

同时，Peri 缺少 `pluginSource`（`plugin@marketplace` 标识）。在 Claude Code 中，`ScopedMcpServerConfig.pluginSource` 驱动 channel 白名单 gating。Peri 当前的 `ConfigSource::Plugin` 仅是一个无数据的枚举标签，无法区分具体插件来源。

## 目标

- 将插件 MCP server 的 env 展开移到合并之前（per-plugin 独立展开）
- 在 `McpClientPool` 上记录每个插件 MCP server 的 `plugin@marketplace` 来源标识
- 不改任何 pub struct 的字段——零 breaking change
- 不改 MCPB bundle、channels、options 存储——这些在后续 feature 中独立实现

## 方案设计

### 选定方案：内部重排 + 旁路存储

只改两个文件，不改任何 pub struct：

| 文件 | 改动 |
|------|------|
| `mcp/config.rs:295-374` | `load_merged_config()` 内部重排：step 2 中每个 plugin server 先独立展开 env，再入合并池；step 6 删除插件分支 |
| `mcp/client.rs` | `McpClientPool` 新增 `plugin_sources: HashMap<String, String>` 旁路表，`run_initialize()` 中填充，暴露 getter |

### 不改的文件

`LoadedPlugin`、`McpServerConfig`、`ConfigSource`、`mcp_panel.rs`——全部零变更。

### 详细设计

#### 3.1 `load_merged_config()` 重排（config.rs）

**现状** step 2-6 的关键代码路径：

```rust
// step 2: 插件 MCP → 收集原始配置（含未展开占位符）
let mut plugin_servers: HashMap<String, (McpServerConfig, PathBuf, PathBuf)> = ...;
for plugin in &plugin_load_result.plugins {
    for (name, config) in &plugin.mcp_servers {
        let namespaced = format!("plugin:{}:{}", plugin.name, name);
        let mut cfg = config.clone();  // ← 原始配置，${CLAUDE_PLUGIN_ROOT} 等未展开
        cfg.source = Some(ConfigSource::Plugin);
        plugin_servers.insert(namespaced, (cfg, install_path, data_path));
    }
}

// step 3: project config
// step 4: 去重（基于未展开值的 hash）
// step 5: 合并 global → plugin → project

// step 6: 展开 env ← 此时已丢失 per-plugin 上下文
for name in names {
    if matches!(server_config.source, Some(ConfigSource::Plugin)) {
        // 从 plugin_servers 找回 install_path/data_path，但 user_config 已丢失归属
        expand_server_config_with_context(&server_config, install_path, data_path, None)
    } else {
        expand_server_config(&server_config)
    }
}
```

**改为**：step 2 中每个 plugin server 先独立展开，step 6 仅处理 project/global。

```rust
// step 2: 插件 MCP → 每个 server 先展开 env → 入池
let mut plugin_servers: HashMap<String, McpServerConfig> = HashMap::new();
for plugin in &plugin_load_result.plugins {
    for (name, config) in &plugin.mcp_servers {
        let namespaced = format!("plugin:{}:{}", plugin.name, name);
        // ★ 先独立展开 env（per-plugin 上下文独立，避免合并后同名 key 冲突）
        let expanded = expand_server_config_with_context(
            config,
            Some(&plugin.install_path),
            Some(&plugin.data_path),
            None,  // user_config 本次不涉及，后续 options 存储 feature 接入
        );
        let mut cfg = expanded;
        cfg.source = Some(ConfigSource::Plugin);
        plugin_servers.insert(namespaced, cfg);
    }
}

// step 3: project config (不变)
// step 4: 去重 — hash 现在基于展开后的实际值（如 command 已解析路径），更准确
// step 5: 合并 global → plugin → project (不变)

// step 6: 变量展开 — 仅 project/global
for name in names {
    if let Some(cfg) = merged.mcp_servers.get(&name) {
        if !matches!(cfg.source, Some(ConfigSource::Plugin)) {
            // 仅 project/global 的 ${VAR} 在此展开
            merged.mcp_servers.insert(name, expand_server_config(cfg));
        }
        // 插件 server 已在 step 2 展开，跳过
    }
}
```

**关键变化**：

- `plugin_servers` 的类型从 `HashMap<String, (McpServerConfig, PathBuf, PathBuf)>` 变为 `HashMap<String, McpServerConfig>`——不再需要携带 `install_path` / `data_path`，因为展开已经在入池前完成
- step 4 去重 hash 现在基于展开后的 command/url/headers 值——更准确地去除重复
- step 6 只需对 project/global 做 `${VAR}` 展开，不再需要区分来源

#### 3.2 `McpClientPool` 加 pluginSource 旁路表（client.rs）

```rust
pub struct McpClientPool {
    // ... 现有字段（handles, configs, initialized, init_rx 等） ...
    /// 插件 MCP server 来源标识：server_name → "plugin@{marketplace}"
    /// 例："plugin:slack:slack-server" → "slack@claude-plugins-official"
    /// 供后续 channel 白名单 gating 使用
    plugin_sources: HashMap<String, String>,
}

impl McpClientPool {
    /// 查询某个 server 的插件来源标识
    pub fn plugin_source_of(&self, name: &str) -> Option<&str> {
        self.plugin_sources.get(name).map(|s| s.as_str())
    }
}
```

在 `run_initialize()` 中填充——遍历 `load_merged_config()` 返回的 servers，对 `source == Some(ConfigSource::Plugin)` 的条目，收集其来源。来源标识从 `plugin_load_result.plugins` 中提取（已有 `name` 和 `marketplace` 字段，拼接为 `"{name}@{marketplace}"`）。

### 不变的内容

| 组件 | 状态 | 原因 |
|------|------|------|
| `LoadedPlugin` struct | 不改 | 内部已有 `name`、`mcp_servers` 等字段，够用 |
| `McpServerConfig` struct | 不改 | `source` 字段（`ConfigSource::Plugin`）已标记插件来源 |
| `ConfigSource` enum | 不改 | `Plugin` 变体已足够标记；具体来源走 `plugin_sources` 旁路 |
| `McpServerEntry` enum | 不改 | MCPB/array 变体后续 feature 补充 |
| mcp panel (`mcp_panel.rs`) | 不改 | 仍通过 `ConfigSource::Plugin` 判断归类即可 |
| `PluginMiddleware` | 不改 | 当前 noop 保持 noop，后续 channels 集成时再激活 |

## 影响与风险

| 维度 | 评估 |
|------|------|
| 向后兼容 | 完全兼容——合并后的最终结果不变，仅展开顺序调整 |
| 去重准确性 | 提升——hash 基于展开后的实际值而非原始模板 |
| 性能 | 无影响——展开总次数不变，仅时机调整 |
| 测试 | 需更新去重相关测试（hash 值变化）；新增 env 展开时机测试；新增 plugin_sources 填充测试 |

## 测试策略

### 单元测试（config.rs）

1. **env 展开在合并前生效**：构造两个插件 A/B，各有同名 `${user_config.KEY}` 但值不同的 env 字段，验证合并后各自保留独立值
2. **`${CLAUDE_PLUGIN_ROOT}` 展开**：验证展开后的 command 路径含实际插件目录
3. **去重仍正常**：构造 global 和 plugin 有相同展开后内容的 server，验证 plugin 被去重
4. **project 覆盖插件**：project 级同名 server 正确覆盖插件 server

### 单元测试（client.rs）

1. **`plugin_source_of()` 返回正确值**：对插件 server 返回 `"name@marketplace"`，对非插件 server 返回 `None`

### 集成验证

- Headless 测试：发消息触发 agent，检查 mcp pool 中 plugin server 的状态为 Ready
- 手动验证：安装含 MCP server 的插件，确认工具在 `/mcp` 面板中正常显示

## 设计决策记录

| 决策 | 理由 |
|------|------|
| pluginSource 用旁路表而非 ConfigSource 扩展 | 零 breaking change，`.mcp.json` 对 ConfigSource::Plugin 的 match 不必修改 |
| user_config 暂不接入 | options 存储层尚未实现，expand_server_config_with_context(user_config=None) 在当前路径下与现状行为一致（`${user_config.X}` 展开为空字符串） |
| MCPB/Channels 后续独立 feature | 每次 feature 范围可控，减少回归风险 |

---
*创建日期: 2026-05-07*
