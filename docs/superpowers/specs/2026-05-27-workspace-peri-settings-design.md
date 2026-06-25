# 工作区 .peri/settings.json 读取与合并

**日期**: 2026-05-27  
**状态**: 设计完成

## 概述

支持从工作区目录 `{cwd}/.peri/settings.json` 加载配置，并与全局 `~/.peri/settings.json` 合并。workspace 中出现的字段覆盖全局对应字段，实现项目级配置定制。

## 动机

当前所有 Provider/Model/Thinking/Env/Skills 等配置仅来源于全局 `~/.peri/settings.json`。不同项目可能需要不同的模型、推理强度或环境变量，每次手动切换不便捷。工作区级配置允许在项目目录内放置 `.peri/settings.json`，进入该目录启动 peri 时自动应用项目特定配置。

## 设计

### 合并策略

- **局部覆盖**：workspace 中出现的顶层 key 替换全局对应 key，未出现的保留全局值
- **数组直接替换**（如 `providers`）：不深度合并
- **空数组不覆盖**：`"providers": []` 视为未填写，保留全局 providers
- **`diff_enabled` 直接覆盖**：bool 类型无法区分"未写"和"写了 false"，统一覆盖

### 加载生命周期

跟随现有全局 settings.json 的加载——TUI/stdio 启动时 `store::load()` 一次性加载并合并。`save()` 始终写回 `~/.peri/settings.json`，工作区配置只读。

### 初始化向导

`load()` 返回合并后的配置，`needs_setup()` 检查合并结果。工作区配置完整的（有 provider + apiKey），即使全局为空也跳过 setup wizard。

## 代码改动

### `peri-acp/src/provider/config.rs` — `AppConfig::merge_overrides()`

新增方法，按字段覆盖：

```rust
impl AppConfig {
    pub fn merge_overrides(&mut self, workspace: AppConfig) {
        if !workspace.providers.is_empty() {
            self.providers = workspace.providers;
        }
        if !workspace.active_alias.is_empty() {
            self.active_alias = workspace.active_alias;
        }
        if !workspace.active_provider_id.is_empty() {
            self.active_provider_id = workspace.active_provider_id;
        }
        if workspace.skills_dir.is_some() { self.skills_dir = workspace.skills_dir; }
        if workspace.thinking.is_some() { self.thinking = workspace.thinking; }
        if workspace.env.is_some() { self.env = workspace.env; }
        if workspace.compact.is_some() { self.compact = workspace.compact; }
        if workspace.language.is_some() { self.language = workspace.language; }
        if workspace.persona.is_some() { self.persona = workspace.persona; }
        if workspace.tone.is_some() { self.tone = workspace.tone; }
        if workspace.claude_md_excludes.is_some() { self.claude_md_excludes = workspace.claude_md_excludes; }
        if workspace.proactiveness.is_some() { self.proactiveness = workspace.proactiveness; }
        if workspace.context_1m.is_some() { self.context_1m = workspace.context_1m; }
        self.diff_enabled = workspace.diff_enabled;
        self.extra.extend(workspace.extra);
    }
}
```

### `peri-tui/src/config/store.rs` — 加载逻辑

新增 `workspace_config_path()`，修改 `load()`：

```rust
pub fn workspace_config_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let path = cwd.join(".peri").join("settings.json");
    if path.exists() { Some(path) } else { None }
}

pub fn load() -> Result<PeriConfig> {
    let mut merged = load_from(&config_path())?;
    if let Some(ws_path) = workspace_config_path() {
        let workspace = load_from(&ws_path)?;
        merged.config.merge_overrides(workspace.config);
    }
    Ok(merged)
}
```

- `load_from()` — 不变
- `save()` / `save_to()` — 不变，始终写 `~/.peri/settings.json`

## 不受影响的部分

- MCP 配置合并（`load_merged_config_full`）— 已有独立的三层合并
- 插件配置、Hooks、Skills 目录 — 各有独立加载路径
- setup wizard 保存 — 始终写 `~/.peri/settings.json`

## 测试

| 层级 | 测试点 | 位置 |
|------|--------|------|
| 单元 | workspace 完整覆盖空全局 | `config.rs` |
| 单元 | 全局有 provider，workspace 只覆盖 model | `config.rs` |
| 单元 | workspace 为 default → 全局不变 | `config.rs` |
| 单元 | providers 空数组 → 全局 providers 不变 | `config.rs` |
| 单元 | `diff_enabled` 覆盖 | `config.rs` |
| 集成 | 临时目录模拟全局 + 工作区双文件，`load()` 返回合并结果 | `store.rs` |

## 边界情况

- **cwd 无权限读取** — `current_dir()` 返回 `Err`，`workspace_config_path()` 返回 `None`，降级为纯全局
- **workspace 文件格式错误** — `load_from()` 返回 `Err`，`load()` 传播错误（不静默忽略）
- **工作区 .peri/ 目录不存在** — `workspace_config_path()` 返回 `None`
- **print mode (`-p`)** — `cli_print.rs` 通过 `config::load()` 自动获得合并行为

## 不做什么

- 不深层合并对象字段（如 `thinking` 只覆盖整个 block）
- 不向上递归查找 `.peri/` 目录
- 不写入工作区配置文件
- 不给 worktree 目录单独的配置路径
