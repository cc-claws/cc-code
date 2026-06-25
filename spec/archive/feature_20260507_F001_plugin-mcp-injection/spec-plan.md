# 插件 MCP Env 展开时机修正 + pluginSource 旁路表 执行计划

**目标:** 将插件 MCP server 的环境变量展开移到合并之前（per-plugin 独立展开），在 McpClientPool 新增 plugin_sources 旁路表记录 plugin@marketplace 来源标识。

**技术栈:** Rust 2021 edition, rmcp 1.6.0, serde_json, parking_lot, tokio, tracing, thiserror

**设计文档:** spec/feature_20260507_F001_plugin-mcp-injection/spec-design.md

## 改动总览

- 涉及 3 个文件：`mcp/config.rs`（load_merged_config 内部重排 + plugin_sources 收集）、`mcp/client.rs`（McpClientPool 新增 plugin_sources 旁路表）、`plugin/config.rs`（ClaudeSettings extraKnownMarketplaces 支持对象格式反序列化）
- Task 1（config.rs 重排）是 Task 2（client.rs 旁路表）的前置依赖
- [TRAP] 预存 Bug 发现：`ClaudeSettings` 的 `extraKnownMarketplaces` 字段原有反序列化器只支持数组格式，但 Claude Code 的 `settings.json` 使用对象格式，导致整个 `ClaudeSettings` 解析失败、`load_enabled_plugins` 静默失败、插件 MCP 服务器全部丢失。已在 Task 1 中追加修复：新增 `deserialize_known_marketplaces` 自定义反序列化器同时支持对象和数组两种格式。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - 位置: 项目根目录
  - 运行 `cargo check -p peri-middlewares`
  - 预期: 编译检查通过，无错误
- [x] 验证测试工具可用
  - 位置: 项目根目录
  - 运行 `cargo test -p peri-middlewares --lib -- mcp::config::tests 2>&1 | tail -3`
  - 预期: 测试框架可用，现有测试通过

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo check -p peri-middlewares 2>&1 | tail -1`
  - 预期: 输出包含 "Finished" 且无 "error"
- [x] 现有 MCP 测试通过
  - `cargo test -p peri-middlewares --lib -- mcp::config 2>&1 | grep "test result"`
  - 预期: "test result: ok" 且 0 failed

---

### Task 1: mcp/config.rs — load_merged_config 内部重排 + plugin_sources 收集

**背景:**
[业务语境] 插件 MCP server 的 env 变量展开当前发生在合并之后（Step 6 走到 Plugin 分支时），导致不同插件的同名 env var 相互污染。修正方案：将插件 env 展开移到合并之前（每插件独立上下文展开）。
[修改原因] load_merged_config 的 Step 2 收集插件时不展开（仅存原始 config + install_path + data_path 三元组），Step 5 合并未展开值，Step 6 才对 Plugin 来源做 context-expand。本 Task 将 Step 2 改为**立即展开并存入展开后的 config**，Step 5/6 简化（plugin 来源跳过二次展开），同时构建 plugin_sources 旁路表（namespaced_server_name → "name@marketplace"）。
[上下游影响] Task 2（client.rs）需要从 load_merged_config 获取 plugin_sources 旁路表，本 Task 的输出签名变更（新增 `load_merged_config_full` 返回 `(McpConfigFile, HashMap<String, String>)`）即为 Task 2 的输入依赖。无其他前置依赖。

**涉及文件:**
- 修改: `peri-middlewares/src/mcp/config.rs`
- 后续 Task 修改: `peri-middlewares/src/mcp/mod.rs`（新增 `pub(crate) use` 导出）

**执行步骤:**

- [x] 重构 `plugin_servers` 的数据结构：从 `HashMap<String, (McpServerConfig, PathBuf, PathBuf)>` 改为 `HashMap<String, McpServerConfig>`
  - 位置: `load_merged_config()` 函数内，Step 2 的 `plugin_servers` 声明处（~L297-L300）
  - 将 `let mut plugin_servers: HashMap<String, (McpServerConfig, std::path::PathBuf, std::path::PathBuf)> = HashMap::new();` 改为 `let mut plugin_servers: HashMap<String, McpServerConfig> = HashMap::new();`
  - 遍历 for 循环内（~L301-L311）：将 `plugin_servers.insert(namespaced, (cfg, plugin.install_path.clone(), plugin.data_path.clone()));` 改为 `let expanded_cfg = expand_server_config_with_context(&cfg, Some(&plugin.install_path), Some(&plugin.data_path), None); plugin_servers.insert(namespaced, expanded_cfg);`
  - 原因: 插件 env 展开移到合并之前，不需要再保存 PathBuf 延迟展开

- [x] 更新 Step 4 hash 去重逻辑，适配简化后的 `plugin_servers` 类型
  - 位置: `load_merged_config()` Step 4（~L334-L342）
  - 将 `plugin_servers.retain(|_, (cfg, _, _)| {` 改为 `plugin_servers.retain(|_, cfg| {`
  - 原因: plugin_servers 的 value 从三元组变为 McpServerConfig，不再需要解构

- [x] 更新 Step 5 合并逻辑，适配简化后的 `plugin_servers` 类型
  - 位置: `load_merged_config()` Step 5（~L344-L351）
  - 将 `for (name, (cfg, _, _)) in &plugin_servers {` 改为 `for (name, cfg) in &plugin_servers {`
  - 原因: 类型对齐

- [x] 更新 Step 6 变量展开逻辑：plugin 来源跳过二次展开
  - 位置: `load_merged_config()` Step 6（~L353-L374），`if matches!(server_config.source, Some(ConfigSource::Plugin))` 分支
  - 改为：对 Plugin 来源直接使用 `server_config.clone()` 作为 expanded（已在上一步展开），无需再次调用 `expand_server_config_with_context` 或查找 `plugin_servers`
  - 新代码:
    ```
    let expanded = if matches!(server_config.source, Some(ConfigSource::Plugin)) {
        server_config.clone()
    } else {
        expand_server_config(&server_config)
    };
    ```
  - 原因: 插件 config 在 Step 2 已做 per-plugin context-expand，此处无需重复展开

- [x] 提取 `load_merged_config` 核心逻辑到新函数 `pub(crate) fn load_merged_config_full`
  - 位置: `load_merged_config` 函数定义处（~L277），在这个函数之前新增 `load_merged_config_full`
  - `load_merged_config_full` 签名为 `pub(crate) fn load_merged_config_full(cwd: &Path, claude_home: &Path) -> (McpConfigFile, HashMap<String, String>)`
  - 将上述重构后的核心逻辑（Steps 1-6）全部移入 `load_merged_config_full`
  - `load_merged_config` 简化为：`load_merged_config_full(cwd, claude_home).0`

- [x] 在 `load_merged_config_full` 中构建 `plugin_sources: HashMap<String, String>`
  - 位置: Step 2（加载插件后），即 ~L296 之后、遍历 plugin 之前
  - 加载 installed_plugins 建立 plugin_name → marketplace 映射：调用 `crate::plugin::config::load_installed_plugins(Some(&installed_path))`，对其返回的 `InstalledPlugins.plugins` 迭代，构建 `HashMap<String, String>`（plugin_name → marketplace）
  - 代码：
    ```
    let marketplace_map: HashMap<String, String> = crate::plugin::config::load_installed_plugins(None)
        .map(|installed| installed.plugins.iter()
            .map(|p| (p.name.clone(), p.marketplace.clone()))
            .collect())
        .unwrap_or_default();
    ```
  - 在遍历 plugin 的 for 循环内部（~L301），构建 plugin_sources 条目：`let namespaced = format!("{}__{}", plugin.name, name);`（注意：原代码用 `plugin:{}:{}` 格式作为内部 key，但实际工具名使用的是 `mcp__{plugin_name}__{server_name}` 格式。这里关键是 plugin_sources 的 key 应与工具名的 server 部分一致，即直接使用 `namespaced` 变量作为 key）
  - 构建 value: 从 `marketplace_map` 获取 `plugin.name` 对应的 marketplace，格式为 `"{name}@{marketplace}"`
  - 代码：
    ```
    let source_id = format!("{}@{}", plugin.name, marketplace_map.get(&plugin.name).cloned().unwrap_or_default());
    plugin_sources.insert(namespaced, source_id);
    ```
  - 初始化 `let mut plugin_sources: HashMap<String, String> = HashMap::new();` 在函数开头
  - 返回: `(merged, plugin_sources)`

- [x] 更新 `mod.rs` 导出，使 `load_merged_config_full` 对同 crate 可见
  - 位置: `peri-middlewares/src/mcp/mod.rs`，~L17-L19 的 `pub use config::` 块
  - 新增：`pub(crate) use config::load_merged_config_full;`
  - `pub fn load_merged_config` 已存在于 pub use 中，不需改动

- [x] 为 `load_merged_config_full` 的 plugin_sources 构建逻辑编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/config.rs`（追加到 `#[cfg(test)] mod tests`）
  - 测试场景:
    - 无插件时返回空 plugin_sources: 构造一个没有插件目录的 claude_home，调用 `load_merged_config_full`，断言 plugin_sources 为 `{}`
    - 单插件单 MCP 来源正确: 在临时目录创建模拟 installed_plugins.json（含 `{"version":1,"plugins":[{"id":"p1@mkt","name":"p1","marketplace":"mkt","version":"1.0","install_path":"..."}]}`），设置环境变量让插件 loader 能解析（或 mock 方式），断言 plugin_sources 包含 `"p1__srv1" → "p1@mkt"`
    - 多插件多 marketplace: 验证不同 marketplace 的插件正确映射
  - 运行命令: `cargo test -p peri-middlewares --lib -- "mcp::config::tests::test_load_merged_config_full" 2>&1 | grep "test result"`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 签名验证 — load_merged_config_full 存在于模块内
  - `grep -n "pub(crate) fn load_merged_config_full" peri-middlewares/src/mcp/config.rs`
  - 预期: 输出一行，包含行号
- [x] 签名验证 — load_merged_config 保持 pub 不变
  - `grep -n "pub fn load_merged_config" peri-middlewares/src/mcp/config.rs`
  - 预期: 输出一行，包含行号，调用 `load_merged_config_full`
- [x] 插件 env 立即展开验证 — plugin_servers 不再包含三元组
  - `grep -c "PathBuf, PathBuf" peri-middlewares/src/mcp/config.rs`
  - 预期: 输出 0（函数体内不再有此模式）
- [x] Step 6 不再对 Plugin 来源重复展开
  - `grep -A5 "matches.*server_config.source.*Plugin" peri-middlewares/src/mcp/config.rs`
  - 预期: 展示的分支内容为 `server_config.clone()` 而非 `expand_server_config_with_context`
- [x] 构建无错误
  - `cargo check -p peri-middlewares 2>&1 | tail -1`
  - 预期: 输出包含 "Finished" 且无 "error"
- [x] 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- "mcp::config::tests::test_load_merged_config_full" 2>&1 | grep "test result"`
  - 预期: "test result: ok" 且 0 failed
- [x] 现有 MCP config 测试无回归
  - `cargo test -p peri-middlewares --lib -- mcp::config 2>&1 | grep "test result"`
  - 预期: "test result: ok" 且 0 failed

**认知变更:**
- [x] [CLAUDE.md] 插件 MCP server 的 env 展开必须在合并之前执行（per-plugin 独立上下文），避免不同插件的同名 env var 交叉污染。load_merged_config_full 返回的 plugin_sources map 的 key 格式为 `"plugin_name__server_name"`（双下划线分隔），与工具名 `mcp__{plugin_name}__{server_name}` 中的 server 部分一致。LoadedPlugin 当前没有 marketplace 字段，marketplace 信息只能从 InstalledPlugin（installed_plugins.json）中获取。

---

### Task 2: mcp/client.rs — McpClientPool 新增 plugin_sources 旁路表

**背景:**
McpClientPool 需要感知插件 MCP server 的来源标识（`plugin@marketplace`），以便 TUI 面板显示和诊断信息中区分普通 server 与插件 server。Task 1 已产出 `(McpConfigFile, HashMap<String, String>)` 元组，本 Task 在池中新增旁路表存储该映射。

**涉及文件:**
- 修改: `peri-middlewares/src/mcp/client.rs`

**执行步骤:**
- [x] 在 `McpClientPool` 结构体新增 `plugin_sources` 字段
  - 位置: `peri-middlewares/src/mcp/client.rs:93-97`，`configs` 字段下方
  - 新增: `plugin_sources: parking_lot::RwLock<HashMap<String, String>>,`
  - 类型选择原因: 与现有 `configs` 字段一致，使用 `parking_lot::RwLock` 保证并发安全
- [x] 在 `McpClientPool` impl 块中新增 `plugin_source_of` 查询方法
  - 位置: `peri-middlewares/src/mcp/client.rs`，在 `new_pending()` 方法之后（~L111）
  - 实现:
    ```rust
    pub fn plugin_source_of(&self, name: &str) -> Option<String> {
        self.plugin_sources.read().get(name).cloned()
    }
    ```
- [x] 修改 `new_pending()` 方法，初始化 `plugin_sources` 为空 HashMap
  - 位置: `peri-middlewares/src/mcp/client.rs:104-110`
  - 在结构体字面量中插入: `plugin_sources: parking_lot::RwLock::new(HashMap::new()),`
- [x] 修改 `run_initialize()` 方法，从 `load_merged_config_full` 获取 `plugin_sources` 并写入池
  - 位置: `peri-middlewares/src/mcp/client.rs:124`
  - 将 `let config = super::load_merged_config(cwd, claude_home);` 替换为 `let (config, plugin_sources) = super::load_merged_config_full(cwd, claude_home);`
  - 在解析完 config 后（~L131，`config.mcp_servers.is_empty()` 检查之前），插入: `*pool.plugin_sources.write() = plugin_sources;`
- [x] 修改 `initialize()` 方法，同样从 `load_merged_config_full` 获取 `plugin_sources`
  - 位置: `peri-middlewares/src/mcp/client.rs:848`
  - 将 `let config = super::load_merged_config(cwd, claude_home);` 替换为 `let (config, plugin_sources) = super::load_merged_config_full(cwd, claude_home);`
  - 在创建 pool 后（~L849），插入: `pool.plugin_sources.write().extend(plugin_sources);`
  - 在 `Arc::try_unwrap` 的 fallback 重建路径中（~L1012-1016），添加: `plugin_sources: parking_lot::RwLock::new(p.plugin_sources.read().clone()),`
- [x] 为本 Task 核心逻辑编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/client.rs` 的 `#[cfg(test)] mod tests` 模块内（~L1120 之后）
  - 测试场景:
    - `new_pending()` 创建的池 `plugin_sources` 为空: 调用 `plugin_source_of("any")` 返回 `None`
    - 写入 `plugin_sources` 后可查询: 模拟写入映射 `("p1" → "marketplace_a")`，验证 `plugin_source_of("p1")` 返回 `Some("marketplace_a".into())`
    - 不存在的 server 名返回 `None`: 验证 `plugin_source_of("nonexistent")` 返回 `None`
  - 运行命令: `cargo test -p peri-middlewares --lib mcp::client::tests -- --nocapture`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 `McpClientPool` 新增字段编译通过
  - `cargo check -p peri-middlewares 2>&1 | grep -c "error"`
  - 预期: 输出 0
- [x] 验证 `plugin_source_of` 方法存在且签名正确
  - `cargo doc -p peri-middlewares --no-deps 2>&1 | grep "error" | wc -l`
  - 预期: 0（无文档生成错误）
- [x] 验证新增单元测试全部通过
  - `cargo test -p peri-middlewares --lib mcp::client::tests -- --nocapture 2>&1 | grep "test result"`
  - 预期: "test result: ok" 且 0 failed
- [x] 验证全量 MCP 测试无回归
  - `cargo test -p peri-middlewares --lib -- mcp:: 2>&1 | grep "test result"`
  - 预期: "test result: ok" 且 0 failed

---

### Task 3: 插件 MCP Env 展开时机修正 + pluginSource 旁路表 验收

**前置条件:**
- 已完成 Task 0（环境准备）、Task 1（config.rs 重排）、Task 2（client.rs 旁路表）
- 构建环境：Rust toolchain 已安装，`cargo` 可用
- 项目根目录执行所有命令

**端到端验证:**

1. [x] 运行完整测试套件确保无回归
   - `cargo test -p peri-middlewares --lib 2>&1 | grep "test result"`
   - 预期: "test result: ok" 且 0 failed

2. [x] 验证 plugin env 展开在合并前生效（两插件同名 env 不冲突）
   - 运行新增的 `test_load_merged_config_full` 测试：`cargo test -p peri-middlewares --lib -- "test_load_merged_config_full" 2>&1 | grep "test result"`
   - 预期: 测试通过，各插件 MCP server 保留独立展开后的值

3. [x] 验证 plugin_sources 旁路表可查询
   - 运行 `plugin_source_of` 相关测试：`cargo test -p peri-middlewares --lib -- mcp::client 2>&1 | grep "test result"`
   - 预期: 测试通过，`plugin_source_of` 对插件 server 返回 `"name@marketplace"`，对非插件返回 `None`

4. [x] 验证 plugin_servers 数据结构已简化（不再携带 PathBuf 三元组）
   - `grep -c "PathBuf, PathBuf" peri-middlewares/src/mcp/config.rs`
   - 预期: 输出 0（原 `(McpServerConfig, PathBuf, PathBuf)` 三元组已移除）

5. [x] 验证构建无编译警告
   - `cargo check -p peri-middlewares 2>&1 | grep -E "warning\|error" | grep -v "generated" | wc -l`
   - 预期: 0（无新增 warning 或 error）
