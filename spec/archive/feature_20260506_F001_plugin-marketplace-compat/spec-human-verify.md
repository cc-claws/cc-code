# 插件系统兼容 Claude Code Marketplace 人工验收清单

**生成时间:** 2026-05-06
**关联计划:** spec-plan-2.md
**关联设计:** spec-design.md

> 所有验收项均可自动化验证，无需人类参与。以下清单可直接由 `/sdd-start-human-verify` 自动执行。

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译全 workspace: `cargo build 2>&1 | tail -5`
- [ ] [AUTO] 编译 middlewares crate: `cargo build -p peri-middlewares 2>&1 | tail -3`
- [ ] [AUTO] 编译 TUI crate: `cargo build -p peri-tui 2>&1 | tail -3`

---

## 验收项目

### 场景 1：编译与模块注册

#### - [x] 1.1 Workspace 全量编译通过
- **来源:** spec-plan-2.md Task 7 / spec-design.md 验收标准
- **目的:** 确认全 workspace 无编译错误
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -5` → 期望包含: `Finished`

#### - [x] 1.2 plugin 模块已注册
- **来源:** spec-plan-2.md Task 0 检查步骤
- **目的:** 确认 plugin 模块在 lib.rs 中公开导出
- **操作步骤:**
  1. [A] `grep "pub mod plugin" peri-middlewares/src/lib.rs` → 期望包含: `pub mod plugin`

#### - [x] 1.3 PluginManifest 类型可导入
- **来源:** spec-plan-2.md Task 0 检查步骤
- **目的:** 确认 PluginManifest 类型已从 plugin 模块导出
- **操作步骤:**
  1. [A] `grep "PluginManifest" peri-middlewares/src/plugin/mod.rs` → 期望包含: `PluginManifest`

---

### 场景 2：插件清单类型兼容性

#### - [x] 2.1 plugin.json 解析 roundtrip 测试通过
- **来源:** spec-plan-2.md Task 7 步骤 2 / spec-design.md 验收标准 "能解析 Claude Code 格式的 plugin.json"
- **目的:** 确认 Claude Code schemas.ts 格式的 plugin.json 可正确反序列化
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- plugin::types::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [x] 2.2 PluginLoadResult 和 SinglePluginLoad 类型导出
- **来源:** spec-plan-2.md Task 5 步骤 1-2
- **目的:** 确认聚合加载结果类型已公开导出
- **操作步骤:**
  1. [A] `grep "PluginLoadResult" peri-middlewares/src/plugin/mod.rs` → 期望包含: `PluginLoadResult`

---

### 场景 3：Marketplace 发现链路

#### - [x] 3.1 marketplace 模块测试通过
- **来源:** spec-plan-2.md Task 7 步骤 3 / spec-design.md 验收标准 "能从 GitHub/URL/本地拉取"
- **目的:** 确认 GitHub/URL/local/NPM 拉取逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- plugin::marketplace 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 4：Skills 中间件集成

#### - [x] 4.1 SkillsMiddleware with_extra_dirs 方法存在
- **来源:** spec-plan-2.md Task 5 步骤 3 / spec-design.md 验收标准 "skills 追加到搜索路径"
- **目的:** 确认插件 skills 路径注入扩展点已实现
- **操作步骤:**
  1. [A] `grep "with_extra_dirs" peri-middlewares/src/skills/mod.rs` → 期望包含: `with_extra_dirs`

#### - [x] 4.2 extra_dirs 注入测试通过
- **来源:** spec-plan-2.md Task 5 步骤 13
- **目的:** 确认额外目录正确追加到搜索路径
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- skills::tests::test_extra_dirs_injected 2>&1 | tail -3` → 期望包含: `ok`

#### - [x] 4.3 extra_dirs 优先级测试通过
- **来源:** spec-plan-2.md Task 5 步骤 13
- **目的:** 确认插件目录在项目级目录之后（同名先到先得）
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- skills::tests::test_extra_dirs_priority_after_project 2>&1 | tail -3` → 期望包含: `ok`

#### - [x] 4.4 extra_dirs 不存在路径跳过测试通过
- **来源:** spec-plan-2.md Task 5 步骤 13
- **目的:** 确认不存在的目录不会出现在搜索路径中
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- skills::tests::test_extra_dirs_nonexistent_skipped 2>&1 | tail -3` → 期望包含: `ok`

---

### 场景 5：MCP 配置合并

#### - [x] 5.1 ConfigSource::Plugin 变体存在
- **来源:** spec-plan-2.md Task 5 步骤 4 / spec-design.md "插件 MCP 命名空间"
- **目的:** 确认插件配置来源标记已定义
- **操作步骤:**
  1. [A] `grep "Plugin" peri-middlewares/src/mcp/config.rs | grep -i "source\|enum"` → 期望包含: `Plugin`

#### - [x] 5.2 merge_plugin_servers 命名空间测试通过
- **来源:** spec-plan-2.md Task 5 步骤 14 / spec-design.md 验收标准 "mcpServers 合并到连接池"
- **目的:** 确认合并后 key 格式为 `{plugin_name}__{server_name}`
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_merge_plugin_servers_namespaced 2>&1 | tail -3` → 期望包含: `ok`

#### - [x] 5.3 merge_plugin_servers 保留已有配置测试通过
- **来源:** spec-plan-2.md Task 5 步骤 14
- **目的:** 确认合并后原有服务器不被覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_merge_plugin_servers_preserves_existing 2>&1 | tail -3` → 期望包含: `ok`

#### - [x] 5.4 merge_plugin_servers 来源标记测试通过
- **来源:** spec-plan-2.md Task 5 步骤 14
- **目的:** 确认合并后的服务器 source 为 ConfigSource::Plugin
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_merge_plugin_servers_source_tag 2>&1 | tail -3` → 期望包含: `ok`

---

### 场景 6：SubAgent 集成

#### - [x] 6.1 scan_agents_with_extra_dirs 导出正确
- **来源:** spec-plan-2.md Task 5 步骤 7-8 / spec-design.md 验收标准 "SubAgent 搜索路径追加"
- **目的:** 确认扩展 agent 扫描函数已公开导出
- **操作步骤:**
  1. [A] `grep "scan_agents_with_extra_dirs" peri-middlewares/src/lib.rs` → 期望包含: `scan_agents_with_extra_dirs`

#### - [x] 6.2 scan_agents_with_extra_dirs 测试通过
- **来源:** spec-plan-2.md Task 5 步骤 15
- **目的:** 确认插件 agent 路径追加和去重逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- subagent::tests::test_scan_agents_with_extra 2>&1 | tail -5` → 期望包含: `ok`

---

### 场景 7：插件加载器

#### - [x] 7.1 load_enabled_plugins 导出正确
- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认聚合加载函数已从 plugin 模块导出
- **操作步骤:**
  1. [A] `grep "load_enabled_plugins" peri-middlewares/src/plugin/mod.rs` → 期望包含: `load_enabled_plugins`

#### - [x] 7.2 load_enabled_plugins 测试通过
- **来源:** spec-plan-2.md Task 5 步骤 16 / spec-design.md 验收标准 "写入 installed_plugins.json"
- **目的:** 确认只加载启用插件，目录不存在时安全返回空结果
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- plugin::loader::tests::test_load_enabled 2>&1 | tail -5` → 期望包含: `ok`

---

### 场景 8：TUI 插件命令系统

#### - [x] 8.1 /plugin 命令已注册
- **来源:** spec-plan-2.md Task 6 步骤 5-6 / spec-design.md 验收标准 "/plugin TUI 面板"
- **目的:** 确认 PluginCommand 在 default_registry 中注册
- **操作步骤:**
  1. [A] `grep -r "PluginCommand" peri-tui/src/command/` → 期望包含: `PluginCommand`

#### - [x] 8.2 PluginCommandAdapter 测试通过
- **来源:** spec-plan-2.md Task 5 步骤 17 / spec-design.md 验收标准 "commands 在 / 浮层可见"
- **目的:** 确认适配器正确桥接 CommandEntry 到 Command trait
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- command::plugin_command::tests 2>&1 | tail -5` → 期望包含: `ok`

---

### 场景 9：TUI 插件面板

#### - [x] 9.1 PluginPanel 渲染模块存在
- **来源:** spec-plan-2.md Task 6 步骤 7-9
- **目的:** 确认面板渲染模块已创建并注册
- **操作步骤:**
  1. [A] `grep "render_plugin_panel" peri-tui/src/ui/main_ui/panels/plugin.rs` → 期望包含: `render_plugin_panel`

#### - [x] 9.2 main_ui 集成 plugin_panel 渲染与高度计算
- **来源:** spec-plan-2.md Task 6 步骤 10 / spec-design.md "TUI 集成面板操作"
- **目的:** 确认面板在 main_ui 中正确集成（渲染 + 高度计算）
- **操作步骤:**
  1. [A] `grep "plugin_panel" peri-tui/src/ui/main_ui.rs` → 期望包含: `plugin_panel`

#### - [x] 9.3 状态栏快捷键分支存在
- **来源:** spec-plan-2.md Task 6 步骤 11 / spec-design.md 验收标准 "状态栏显示快捷键"
- **目的:** 确认 render_second_row 包含 plugin_panel 快捷键提示
- **操作步骤:**
  1. [A] `grep "plugin_panel" peri-tui/src/ui/main_ui/status_bar.rs` → 期望包含: `plugin_panel`

#### - [x] 9.4 event.rs 按键处理函数存在
- **来源:** spec-plan-2.md Task 6 步骤 12
- **目的:** 确认 handle_plugin_panel 函数已定义并调用
- **操作步骤:**
  1. [A] `grep "handle_plugin_panel" peri-tui/src/event.rs` → 期望包含: `handle_plugin_panel`

#### - [x] 9.5 PluginPanel 单元测试通过
- **来源:** spec-plan-2.md Task 6 步骤 13
- **目的:** 确认面板状态管理（构造、光标、Tab切换、删除确认）正确
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- plugin_panel 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 10：端到端回归

#### - [x] 10.1 middlewares crate 全量测试通过
- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认 middlewares 无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [x] 10.2 TUI crate 全量测试通过
- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认 TUI 无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [x] 10.3 Workspace 全量测试通过
- **来源:** spec-plan-2.md Task 7 步骤 1
- **目的:** 确认全 workspace 无回归
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -20` → 期望包含: `test result: ok`

---

### 场景 11：边界与回归

#### - [!] 11.1 Headless 测试隔离验证
- **来源:** spec-design.md 验收标准 "Headless 测试不写入真实 ~/.claude/"
- **目的:** 确认插件面板配置写入使用 override 路径，不影响用户真实配置
- **操作步骤:**
  1. [A] `grep "config_path_override" peri-tui/src/app/plugin_panel.rs` → 期望包含: `config_path_override`

#### - [x] 11.2 插件 MCP 命名空间约定在 CLAUDE.md 中记录
- **来源:** spec-plan-2.md Task 5 认知变更 / spec-design.md "插件 MCP 命名空间"
- **目的:** 确认命名空间约定已写入项目知识库
- **操作步骤:**
  1. [A] `grep "plugin_name.*server_name" CLAUDE.md` → 期望包含: `plugin_name`

#### - [x] 11.3 SkillsMiddleware with_extra_dirs 扩展点在 CLAUDE.md 中记录
- **来源:** spec-plan-2.md Task 5 认知变更
- **目的:** 确认扩展点文档化，后续修改时不会误删
- **操作步骤:**
  1. [A] `grep "with_extra_dirs" CLAUDE.md` → 期望包含: `with_extra_dirs`

---

## 验收后清理

本清单无后台服务需清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | Workspace 全量编译通过 | 1 | 0 | ⬜ |
| 场景 1 | 1.2 | plugin 模块已注册 | 1 | 0 | ⬜ |
| 场景 1 | 1.3 | PluginManifest 类型可导入 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | plugin.json roundtrip 测试 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | PluginLoadResult 类型导出 | 1 | 0 | ⬜ |
| 场景 3 | 3.1 | marketplace 模块测试 | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | with_extra_dirs 方法存在 | 1 | 0 | ⬜ |
| 场景 4 | 4.2 | extra_dirs 注入测试 | 1 | 0 | ⬜ |
| 场景 4 | 4.3 | extra_dirs 优先级测试 | 1 | 0 | ⬜ |
| 场景 4 | 4.4 | extra_dirs 不存在路径跳过 | 1 | 0 | ⬜ |
| 场景 5 | 5.1 | ConfigSource::Plugin 变体 | 1 | 0 | ⬜ |
| 场景 5 | 5.2 | MCP 命名空间测试 | 1 | 0 | ⬜ |
| 场景 5 | 5.3 | MCP 保留已有配置测试 | 1 | 0 | ⬜ |
| 场景 5 | 5.4 | MCP 来源标记测试 | 1 | 0 | ⬜ |
| 场景 6 | 6.1 | scan_agents_with_extra_dirs 导出 | 1 | 0 | ⬜ |
| 场景 6 | 6.2 | scan_agents_with_extra_dirs 测试 | 1 | 0 | ⬜ |
| 场景 7 | 7.1 | load_enabled_plugins 导出 | 1 | 0 | ⬜ |
| 场景 7 | 7.2 | load_enabled_plugins 测试 | 1 | 0 | ⬜ |
| 场景 8 | 8.1 | /plugin 命令已注册 | 1 | 0 | ⬜ |
| 场景 8 | 8.2 | PluginCommandAdapter 测试 | 1 | 0 | ⬜ |
| 场景 9 | 9.1 | PluginPanel 渲染模块存在 | 1 | 0 | ⬜ |
| 场景 9 | 9.2 | main_ui 集成验证 | 1 | 0 | ⬜ |
| 场景 9 | 9.3 | 状态栏快捷键分支 | 1 | 0 | ⬜ |
| 场景 9 | 9.4 | event.rs 按键处理 | 1 | 0 | ⬜ |
| 场景 9 | 9.5 | PluginPanel 单元测试 | 1 | 0 | ⬜ |
| 场景 10 | 10.1 | middlewares 全量测试 | 1 | 0 | ⬜ |
| 场景 10 | 10.2 | TUI 全量测试 | 1 | 0 | ⬜ |
| 场景 10 | 10.3 | Workspace 全量测试 | 1 | 0 | ⬜ |
| 场景 11 | 11.1 | Headless 测试隔离 | 1 | 0 | ⬜ |
| 场景 11 | 11.2 | MCP 命名空间 CLAUDE.md 记录 | 1 | 0 | ⬜ |
| 场景 11 | 11.3 | with_extra_dirs CLAUDE.md 记录 | 1 | 0 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
