# MCP 管理面板与后台初始化 人工验收清单

**生成时间:** 2026-05-02 14:00
**关联计划:** spec/feature_20260502_F002_mcp-management/spec-plan.md
**关联设计:** spec/feature_20260502_F002_mcp-management/spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 编译全 workspace: `cargo build 2>&1 | tail -5`
- [x] [AUTO] 运行全量测试基线: `cargo test 2>&1 | tail -20`

### 测试数据准备
- [x] [AUTO] 准备项目级 MCP 配置（用于运行时验收）: `echo '{"mcpServers":{"test-echo":{"command":"echo","args":["hello"]}}}' > .mcp.json`

---

## 验收项目

### 场景 1：编译与测试基线

#### - [x] 1.1 Workspace 编译通过
- **来源:** spec-plan.md Task 0 / spec-design.md 验收标准
- **目的:** 确认所有 crate 编译无错
- **操作步骤:**
  1. [A] `cargo build 2>&1 | tail -5` → 期望包含: `Finished`

#### - [x] 1.2 MCP 模块单元测试通过
- **来源:** spec-plan.md Task 1
- **目的:** 确认 McpInitStatus/Pool 扩展单元测试全部通过
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp:: 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 1.3 TUI 模块单元测试通过
- **来源:** spec-plan.md Task 3/4/5
- **目的:** 确认 TUI 层所有新增测试通过（含 mcp_panel、headless 渲染）
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [x] 1.4 全量测试无回归
- **来源:** spec-plan.md Task 6 / spec-design.md 验收标准
- **目的:** 确认整体功能无回归
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -20` → 期望包含: `test result: ok`，无 `FAILED`

---

### 场景 2：MCP 后台初始化架构（数据层）

#### - [x] 2.1 McpInitStatus 枚举定义完整
- **来源:** spec-plan.md Task 1 / spec-design.md McpInitStatus 状态机
- **目的:** 确认状态机四个变体存在
- **操作步骤:**
  1. [A] `grep -n "McpInitStatus" peri-middlewares/src/mcp/client.rs | head -10` → 期望包含: `Pending` 和 `Initializing` 和 `Ready` 和 `Failed`
  2. [A] `grep -n "McpInitStatus" peri-middlewares/src/mcp/mod.rs` → 期望包含: `McpInitStatus`

#### - [x] 2.2 McpClientPool 并发安全结构
- **来源:** spec-plan.md Task 1 / spec-design.md McpClientPool 扩展
- **目的:** 确认 clients 改为 RwLock、services 改为 Mutex
- **操作步骤:**
  1. [A] `grep -A 6 "pub struct McpClientPool" peri-middlewares/src/mcp/client.rs` → 期望包含: `RwLock` 和 `Mutex` 和 `configs`

#### - [x] 2.3 McpClientPool 新增方法完整
- **来源:** spec-plan.md Task 1 / spec-design.md McpClientPool 新增方法
- **目的:** 确认 6 个核心方法存在
- **操作步骤:**
  1. [A] `grep -n "pub async fn run_initialize\|pub async fn reconnect\|pub async fn remove_server\|pub fn server_infos\|pub fn get_tools\|pub fn get_resources\|pub fn new_pending" peri-middlewares/src/mcp/client.rs` → 期望精确: 每个方法名各出现一次

#### - [x] 2.4 ServerInfo 类型已导出
- **来源:** spec-plan.md Task 1
- **目的:** 确认 TUI 层可引用 ServerInfo
- **操作步骤:**
  1. [A] `grep -n "ServerInfo" peri-middlewares/src/mcp/mod.rs` → 期望包含: `ServerInfo`

---

### 场景 3：配置持久化删除

#### - [x] 3.1 remove_server_from_config 函数存在
- **来源:** spec-plan.md Task 2 / spec-design.md McpConfig 持久化扩展
- **目的:** 确认配置删除入口函数已实现
- **操作步骤:**
  1. [A] `grep -n "pub fn remove_server_from_config" peri-middlewares/src/mcp/config.rs` → 期望包含: `cwd: &Path, server_name: &str`

#### - [x] 3.2 McpConfigError 包含 WriteError
- **来源:** spec-plan.md Task 2
- **目的:** 确认写入错误类型已定义
- **操作步骤:**
  1. [A] `grep -n "WriteError" peri-middlewares/src/mcp/config.rs` → 期望包含: `WriteError`

#### - [x] 3.3 原子写入使用 tempfile + rename
- **来源:** spec-plan.md Task 2 / spec-design.md 配置文件修改的原子性
- **目的:** 确认配置写入不会因中断导致数据丢失
- **操作步骤:**
  1. [A] `grep -n "atomic_write_json\|tempfile\|rename" peri-middlewares/src/mcp/config.rs` → 期望包含: `tempfile` 和 `rename`

#### - [x] 3.4 配置删除测试通过
- **来源:** spec-plan.md Task 2
- **目的:** 确认项目级/全局/不存在的 server 删除场景均覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::config::tests::test_remove_server 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 4：TUI 后台初始化集成

#### - [x] 4.1 block_in_place 同步初始化已移除
- **来源:** spec-plan.md Task 3 / spec-design.md 验收标准（不阻塞 UI）
- **目的:** 确认旧的同步阻塞初始化逻辑已删除
- **操作步骤:**
  1. [A] `grep -n "block_in_place\|McpClientPool::initialize" peri-tui/src/app/agent_ops.rs` → 期望精确: 无 MCP 初始化相关的 `block_in_place` 匹配

#### - [x] 4.2 spawn_mcp_init 在 run_app 中调用
- **来源:** spec-plan.md Task 3 / spec-design.md MCP 提前初始化
- **目的:** 确认 MCP 在 App 创建后立即后台初始化
- **操作步骤:**
  1. [A] `grep -n "spawn_mcp_init" peri-tui/src/main.rs` → 期望包含: `spawn_mcp_init`

#### - [x] 4.3 App 包含 mcp_init_rx 字段
- **来源:** spec-plan.md Task 3
- **目的:** 确认 agent task 可通过 watch channel 等待 MCP 就绪
- **操作步骤:**
  1. [A] `grep -n "mcp_init_rx" peri-tui/src/app/mod.rs` → 期望包含: 字段定义和 `None` 初始化

#### - [x] 4.4 agent task 内异步等待 MCP 就绪
- **来源:** spec-plan.md Task 3 / spec-design.md Lazy Wait 策略
- **目的:** 确认首条消息如 MCP 未就绪则异步等待（最多 30s）
- **操作步骤:**
  1. [A] `grep -n "mcp_init_rx\|McpInitStatus::Ready" peri-tui/src/app/agent_ops.rs` → 期望包含: `mcp_init_rx` clone 和 `Ready` 匹配
  2. [A] `grep -n "from_secs(30)\|Duration::from_secs(30)" peri-tui/src/app/agent_ops.rs` → 期望包含: `30`

#### - [x] 4.5 App 包含 mcp_ready_shown_until 字段
- **来源:** spec-plan.md Task 5
- **目的:** 确认就绪提示 3 秒自动消失有字段支撑
- **操作步骤:**
  1. [A] `grep -n "mcp_ready_shown_until" peri-tui/src/app/mod.rs` → 期望包含: 字段定义和 `None` 初始化

---

### 场景 5：/mcp 命令与面板数据结构

#### - [x] 5.1 McpCommand 注册到 default_registry
- **来源:** spec-plan.md Task 4 / spec-design.md 命令注册
- **目的:** 确认 /mcp 命令可被 dispatch
- **操作步骤:**
  1. [A] `grep -n "mcp::McpCommand\|pub mod mcp" peri-tui/src/command/mod.rs` → 期望精确: 两行匹配（模块声明 + 注册调用）

#### - [x] 5.2 McpPanel / McpPanelView 定义完整
- **来源:** spec-plan.md Task 4 / spec-design.md 数据结构
- **目的:** 确认面板状态管理结构就绪
- **操作步骤:**
  1. [A] `grep -n "pub struct McpPanel\|pub enum McpPanelView" peri-tui/src/app/mcp_panel.rs` → 期望精确: 各匹配一次

#### - [x] 5.3 面板操作方法完整（10 个）
- **来源:** spec-plan.md Task 4 / spec-design.md 面板交互
- **目的:** 确认导航/详情/Tab/删除/重连/关闭操作全覆盖
- **操作步骤:**
  1. [A] `grep -n "pub fn mcp_panel_" peri-tui/src/app/mcp_panel.rs` → 期望包含: `mcp_panel_move_up` 和 `mcp_panel_move_down` 和 `mcp_panel_enter` 和 `mcp_panel_back` 和 `mcp_panel_tab` 和 `mcp_panel_request_delete` 和 `mcp_panel_confirm_delete` 和 `mcp_panel_cancel_delete` 和 `mcp_panel_reconnect` 和 `mcp_panel_close`

#### - [x] 5.4 App 包含 mcp_panel 字段
- **来源:** spec-plan.md Task 4
- **目的:** 确认 TUI 可持有面板状态
- **操作步骤:**
  1. [A] `grep -n "mcp_panel" peri-tui/src/app/mod.rs` → 期望包含: 模块声明和 re-export 和字段定义和 `None` 初始化

#### - [x] 5.5 McpPanel 单元测试通过
- **来源:** spec-plan.md Task 4
- **目的:** 确认面板数据结构和操作方法测试覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- app::mcp_panel::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 6：面板渲染与状态栏集成

#### - [x] 6.1 render_mcp_panel 函数存在
- **来源:** spec-plan.md Task 5 / spec-design.md 渲染
- **目的:** 确认 MCP 面板有渲染入口
- **操作步骤:**
  1. [A] `grep -n "pub(crate) fn render_mcp_panel" peri-tui/src/ui/main_ui/panels/mcp.rs` → 期望包含: `render_mcp_panel`

#### - [x] 6.2 MCP 模块注册到 panels/mod.rs
- **来源:** spec-plan.md Task 5
- **目的:** 确认面板模块可被发现
- **操作步骤:**
  1. [A] `grep -n "pub mod mcp" peri-tui/src/ui/main_ui/panels/mod.rs` → 期望包含: `pub mod mcp`

#### - [x] 6.3 main_ui.rs 包含 MCP 面板渲染分发和高度计算
- **来源:** spec-plan.md Task 5 / spec-design.md 渲染
- **目的:** 确认面板在主 UI 中被正确调度
- **操作步骤:**
  1. [A] `grep -n "mcp_panel\|panels::mcp" peri-tui/src/ui/main_ui.rs` → 期望包含: 渲染调用和高度计算分支

#### - [x] 6.4 状态栏显示 MCP 初始化进度
- **来源:** spec-plan.md Task 5 / spec-design.md 状态栏显示
- **目的:** 确认用户可感知 MCP 后台连接状态
- **操作步骤:**
  1. [A] `grep -n "McpInitStatus\|mcp_init_rx\|MCP" peri-tui/src/ui/main_ui/status_bar.rs` → 期望包含: `McpInitStatus` 和 `Initializing` 和 `Ready` 和 `Failed`

#### - [x] 6.5 状态栏显示 MCP 面板快捷键提示
- **来源:** spec-plan.md Task 5 / spec-design.md 面板交互
- **目的:** 确认面板操作有按键引导
- **操作步骤:**
  1. [A] `grep -n "mcp_panel\|McpPanelView" peri-tui/src/ui/main_ui/status_bar.rs` → 期望包含: `mcp_panel` 和 `McpPanelView`

#### - [x] 6.6 event.rs 包含 handle_mcp_panel 函数
- **来源:** spec-plan.md Task 5 / spec-design.md 面板交互
- **目的:** 确认面板键盘事件有处理入口
- **操作步骤:**
  1. [A] `grep -n "handle_mcp_panel\|mcp_panel.is_some()" peri-tui/src/event.rs` → 期望包含: 函数定义和分发调用

#### - [x] 6.7 Paste 事件拦截 MCP 面板
- **来源:** spec-plan.md Task 5 / CLAUDE.md 面板注意事项
- **目的:** 确认粘贴文本不会穿透到 textarea
- **操作步骤:**
  1. [A] `grep -n "mcp_panel.is_some()" peri-tui/src/event.rs` → 期望包含: 在 paste 拦截条件链中

#### - [x] 6.8 MCP 面板 headless 渲染测试通过
- **来源:** spec-plan.md Task 5
- **目的:** 确认面板渲染逻辑可测试
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::main_ui::panels::mcp::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 7：TUI 运行时交互验收

#### - [x] 7.1 TUI 启动后 MCP 后台初始化不阻塞 UI
- **来源:** spec-design.md 验收标准 / spec-plan.md Task 3
- **目的:** 确认用户可在 MCP 连接期间正常输入和交互
- **操作步骤:**
  1. [H] 运行 `cargo run -p peri-tui`，观察启动后是否可立即输入文字 → 是/否

#### - [x] 7.2 状态栏显示 MCP 连接进度
- **来源:** spec-design.md 状态栏显示
- **目的:** 确认初始化过程中状态栏有连接中提示
- **操作步骤:**
  1. [H] 启动 TUI（.mcp.json 已配置 test-echo 服务器），观察状态栏是否出现 `[i] MCP 连接中` → 是/否

#### - [x] 7.3 状态栏 MCP 就绪提示 3 秒后消失
- **来源:** spec-design.md 状态栏显示 / spec-plan.md Task 5
- **目的:** 确认就绪提示不会永久占据状态栏空间
- **操作步骤:**
  1. [H] 启动 TUI，观察状态栏 MCP 就绪提示是否约 3 秒后消失 → 是/否

#### - [x] 7.4 /mcp 命令打开管理面板
- **来源:** spec-design.md 验收标准 / spec-plan.md Task 4
- **目的:** 确认命令触发面板正常弹出
- **操作步骤:**
  1. [H] 输入 `/mcp` 按 Enter，观察是否弹出 "MCP 服务器" 面板 → 是/否

#### - [x] 7.5 服务器列表显示状态和工具/资源计数
- **来源:** spec-design.md 渲染（ServerList 行格式）
- **目的:** 确认面板行格式包含传输类型、状态、工具/资源计数
- **操作步骤:**
  1. [H] 在 /mcp 面板中，观察列表行是否包含 `[stdio]` 或 `[http]`、`Connected` 或 `Failed`、`N tools, M resources` → 是/否

#### - [x] 7.6 Enter 进入工具详情视图
- **来源:** spec-design.md 验收标准 / spec-plan.md Task 4
- **目的:** 确认可查看服务器工具列表
- **操作步骤:**
  1. [H] 选中 Connected 状态的服务器按 Enter，观察是否进入工具列表视图 → 是/否

#### - [x] 7.7 Tab 切换工具/资源视图
- **来源:** spec-design.md 验收标准 / spec-plan.md Task 4
- **目的:** 确认 Tab 在 ToolList 和 ResourceList 间切换
- **操作步骤:**
  1. [H] 在工具列表视图中按 Tab，观察标题是否切换为 "资源列表" → 是/否
  2. [H] 再按 Tab，观察标题是否切回 "工具列表" → 是/否

#### - [x] 7.8 Esc 返回服务器列表 / 关闭面板
- **来源:** spec-design.md 面板交互 / spec-plan.md Task 4
- **目的:** 确认 Esc 正确处理视图层级
- **操作步骤:**
  1. [H] 在工具列表按 Esc，观察是否返回服务器列表 → 是/否
  2. [H] 在服务器列表按 Esc，观察面板是否关闭 → 是/否

#### - [x] 7.9 Ctrl+D 触发删除确认弹窗
- **来源:** spec-design.md 删除流程 / spec-plan.md Task 4
- **目的:** 确认删除需二次确认
- **操作步骤:**
  1. [H] 在服务器列表中按 Ctrl+D，观察是否出现 "确定删除 ... 此操作将从配置文件中永久移除" 提示 → 是/否

#### - [x] 7.10 删除确认后配置文件条目被移除
- **来源:** spec-design.md 验收标准 / spec-plan.md Task 2/4
- **目的:** 确认删除是持久化的
- **操作步骤:**
  1. [H] 确认删除后，检查 `.mcp.json` 文件中对应 server 条目是否已移除: `cat .mcp.json` → 是/否

#### - [x] 7.11 MCP 初始化失败时状态栏显示错误（修复后通过）
- **来源:** spec-design.md 状态栏显示
- **目的:** 确认用户可感知初始化异常
- **操作步骤:**
  1. [H] 配置一个无效 MCP 服务器（如 `{"command":"nonexistent-binary"}`），启动 TUI，观察状态栏是否显示 `[i] MCP 初始化失败` → 是/否

#### - [x] 7.12 Headless 测试保持现有行为
- **来源:** spec-design.md 验收标准 / spec-plan.md Task 3
- **目的:** 确认后台初始化不影响 headless 测试
- **操作步骤:**
  1. [A] `cargo test -p peri-tui --lib -- ui::headless 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 8：边界与回归

#### - [x] 8.1 空 MCP 配置时 /mcp 显示引导消息
- **来源:** spec-plan.md Task 4 McpCommand execute（infos.is_empty 分支）
- **目的:** 确认无配置时有合理引导
- **操作步骤:**
  1. [H] 删除 .mcp.json，运行 TUI，输入 `/mcp`，观察消息区是否出现 ".mcp.json 或 settings.json" 引导文字 → 是/否

#### - [x] 8.2 确认删除模式下其他按键均取消
- **来源:** spec-plan.md Task 5 handle_mcp_panel（确认模式拦截）
- **目的:** 确认只有 Enter 确认、其他键取消
- **操作步骤:**
  1. [H] 触发删除确认后按字母键，观察是否取消确认（提示消失） → 是/否

#### - [x] 8.3 Ctrl+R 仅对 Failed 状态服务器生效
- **来源:** spec-plan.md Task 4 mcp_panel_reconnect
- **目的:** 确认 Connected 服务器不会被意外重连
- **操作步骤:**
  1. [H] 选中 Connected 服务器按 Ctrl+R，观察是否无反应（不触发重连） → 是/否

#### - [x] 8.4 删除最后一个服务器后面板自动关闭
- **来源:** spec-plan.md Task 4 mcp_panel_confirm_delete（列表为空关闭面板）
- **目的:** 确认空列表不残留空面板
- **操作步骤:**
  1. [H] 配置仅一个服务器，删除确认后观察面板是否自动关闭 → 是/否

#### - [x] 8.5 面板中 Ctrl+C 不退出应用
- **来源:** spec-plan.md Task 5 handle_mcp_panel（Ctrl+C 忽略）
- **目的:** 确认面板内 Ctrl+C 被拦截
- **操作步骤:**
  1. [H] 打开 /mcp 面板后按 Ctrl+C，观察应用是否仍在运行 → 是/否

---

## 验收后清理

- [x] [AUTO] 恢复项目级 MCP 配置: `rm -f .mcp.json`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | Workspace 编译通过 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | MCP 模块单元测试通过 | 1 | 0 | ✅ |
| 场景 1 | 1.3 | TUI 模块单元测试通过 | 1 | 0 | ✅ |
| 场景 1 | 1.4 | 全量测试无回归 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | McpInitStatus 枚举定义完整 | 2 | 0 | ✅ |
| 场景 2 | 2.2 | McpClientPool 并发安全结构 | 1 | 0 | ✅ |
| 场景 2 | 2.3 | McpClientPool 新增方法完整 | 1 | 0 | ✅ |
| 场景 2 | 2.4 | ServerInfo 类型已导出 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | remove_server_from_config 函数存在 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | McpConfigError 包含 WriteError | 1 | 0 | ✅ |
| 场景 3 | 3.3 | 原子写入使用 tempfile + rename | 1 | 0 | ✅ |
| 场景 3 | 3.4 | 配置删除测试通过 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | block_in_place 同步初始化已移除 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | spawn_mcp_init 在 run_app 中调用 | 1 | 0 | ✅ |
| 场景 4 | 4.3 | App 包含 mcp_init_rx 字段 | 1 | 0 | ✅ |
| 场景 4 | 4.4 | agent task 内异步等待 MCP 就绪 | 2 | 0 | ✅ |
| 场景 4 | 4.5 | App 包含 mcp_ready_shown_until 字段 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | McpCommand 注册到 default_registry | 1 | 0 | ✅ |
| 场景 5 | 5.2 | McpPanel / McpPanelView 定义完整 | 1 | 0 | ✅ |
| 场景 5 | 5.3 | 面板操作方法完整（10 个） | 1 | 0 | ✅ |
| 场景 5 | 5.4 | App 包含 mcp_panel 字段 | 1 | 0 | ✅ |
| 场景 5 | 5.5 | McpPanel 单元测试通过 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | render_mcp_panel 函数存在 | 1 | 0 | ✅ |
| 场景 6 | 6.2 | MCP 模块注册到 panels/mod.rs | 1 | 0 | ✅ |
| 场景 6 | 6.3 | main_ui.rs 包含渲染分发和高度计算 | 1 | 0 | ✅ |
| 场景 6 | 6.4 | 状态栏显示 MCP 初始化进度 | 1 | 0 | ✅ |
| 场景 6 | 6.5 | 状态栏显示 MCP 面板快捷键提示 | 1 | 0 | ✅ |
| 场景 6 | 6.6 | event.rs 包含 handle_mcp_panel | 1 | 0 | ✅ |
| 场景 6 | 6.7 | Paste 事件拦截 MCP 面板 | 1 | 0 | ✅ |
| 场景 6 | 6.8 | MCP 面板 headless 渲染测试通过 | 1 | 0 | ✅ |
| 场景 7 | 7.1 | MCP 后台初始化不阻塞 UI | 0 | 1 | ✅ |
| 场景 7 | 7.2 | 状态栏显示连接进度 | 0 | 1 | ✅ |
| 场景 7 | 7.3 | 就绪提示 3 秒后消失 | 0 | 1 | ✅ |
| 场景 7 | 7.4 | /mcp 命令打开管理面板 | 0 | 1 | ✅ |
| 场景 7 | 7.5 | 服务器列表显示完整 | 0 | 1 | ✅ |
| 场景 7 | 7.6 | Enter 进入工具详情 | 0 | 1 | ✅ |
| 场景 7 | 7.7 | Tab 切换工具/资源视图 | 0 | 2 | ✅ |
| 场景 7 | 7.8 | Esc 返回/关闭面板 | 0 | 2 | ✅ |
| 场景 7 | 7.9 | Ctrl+D 删除确认弹窗 | 0 | 1 | ✅ |
| 场景 7 | 7.10 | 删除后配置文件条目移除 | 0 | 1 | ✅ |
| 场景 7 | 7.11 | 初始化失败时状态栏显示错误 | 0 | 1 | ✅ |
| 场景 7 | 7.12 | Headless 测试保持现有行为 | 1 | 0 | ✅ |
| 场景 8 | 8.1 | 空 MCP 配置时 /mcp 显示引导 | 0 | 1 | ✅ |
| 场景 8 | 8.2 | 确认删除模式其他按键取消 | 0 | 1 | ✅ |
| 场景 8 | 8.3 | Ctrl+R 仅对 Failed 生效 | 0 | 1 | ✅ |
| 场景 8 | 8.4 | 删除最后服务器后面板关闭 | 0 | 1 | ✅ |
| 场景 8 | 8.5 | 面板中 Ctrl+C 不退出应用 | 0 | 1 | ✅ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
