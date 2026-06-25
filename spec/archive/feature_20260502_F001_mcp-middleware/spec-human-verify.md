# MCP Middleware 人工验收清单

**生成时间:** 2026-05-02 22:00
**关联计划:** spec-plan-1.md / spec-plan-2.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust toolchain: `rustc --version`
- [x] [AUTO] 编译项目: `cargo build 2>&1 | tail -5`
- [x] [AUTO] 运行全量测试基线: `cargo test 2>&1 | tail -20`

---

## 验收项目

### 场景 1：MCP 模块结构与编译

#### - [x] 1.1 MCP 模块文件完整
- **来源:** spec-plan-1.md Task 1-4 / spec-design.md §模块结构
- **目的:** 确认 MCP 模块包含 7 个子文件
- **操作步骤:**
  1. [A] `ls peri-middlewares/src/mcp/ | sort` → 期望包含: `client.rs config.rs middleware.rs mod.rs resource_tool.rs tool_bridge.rs transport.rs`

#### - [x] 1.2 peri-middlewares 编译通过
- **来源:** spec-plan-1.md Task 0
- **目的:** 确认 MCP 模块无编译错误
- **操作步骤:**
  1. [A] `cargo build -p peri-middlewares 2>&1 | tail -5` → 期望包含: `Finished`

#### - [x] 1.3 peri-tui 编译通过
- **来源:** spec-plan-2.md Task 8
- **目的:** 确认 TUI 集成无编译错误
- **操作步骤:**
  1. [A] `cargo build -p peri-tui 2>&1 | tail -5` → 期望包含: `Finished`

---

### 场景 2：配置加载与合并

#### - [x] 2.1 McpConfig 单元测试通过
- **来源:** spec-plan-1.md Task 1 检查步骤
- **目的:** 确认配置加载/合并/env 展开正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::config::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [x] 2.2 McpServerConfig 解析含 type 字段的 JSON
- **来源:** spec-design.md §配置格式（实际 `.mcp.json` 含 `"type": "http"` 字段）
- **目的:** 确认未知字段被 serde 忽略不报错
- **操作步骤:**
  1. [A] `grep -c 'deny_unknown_fields' peri-middlewares/src/mcp/config.rs` → 期望精确: `0`

#### - [x] 2.3 环境变量展开覆盖所有字符串字段
- **来源:** spec-plan-1.md Task 1 / spec-design.md §合并规则
- **目的:** 确认 ${VAR} 在 command/args/env/url/headers 中均被展开
- **操作步骤:**
  1. [A] `grep -n 'expand_env_vars\|expand_server_config' peri-middlewares/src/mcp/config.rs | head -10` → 期望包含: `expand_server_config`

---

### 场景 3：传输层与连接池

#### - [x] 3.1 Transport 层单元测试通过
- **来源:** spec-plan-1.md Task 2 检查步骤
- **目的:** 确认 stdio/HTTP 传输配置正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::transport::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [x] 3.2 HTTP transport 传递自定义 headers
- **来源:** spec-design.md §Streamable HTTP 传输 + 实际修复
- **目的:** 确认 Authorization 等 headers 被正确注入 transport config
- **操作步骤:**
  1. [A] `grep -n 'custom_headers' peri-middlewares/src/mcp/client.rs | head -5` → 期望包含: `config.custom_headers(custom_headers)`

#### - [x] 3.3 McpClientPool 单元测试通过
- **来源:** spec-plan-1.md Task 3 检查步骤
- **目的:** 确认连接池管理逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::client::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 3.4 rmcp patch 生效
- **来源:** 实际修复（rmcp 上游 bug）
- **目的:** 确认本地 patch 覆盖了 crates-io 的 rmcp
- **操作步骤:**
  1. [A] `grep -A2 '\[patch.crates-io\]' Cargo.toml` → 期望包含: `rmcp`
  2. [A] `grep -n 'content_length.*0.*Accepted' rust-mcp-patch/src/transport/common/reqwest/streamable_http_client.rs` → 期望包含: `Accepted`

---

### 场景 4：工具桥接

#### - [x] 4.1 McpToolBridge 单元测试通过
- **来源:** spec-plan-1.md Task 4 检查步骤
- **目的:** 确认 MCP→BaseTool 桥接正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::tool_bridge::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [x] 4.2 工具命名格式为 mcp__{server}__{tool}
- **来源:** spec-design.md §McpToolBridge
- **目的:** 确认桥接工具使用正确命名空间
- **操作步骤:**
  1. [A] `grep -n 'mcp__' peri-middlewares/src/mcp/tool_bridge.rs | head -5` → 期望包含: `format!("mcp__{}__{}"`

#### - [x] 4.3 工具调用超时 120s
- **来源:** spec-design.md §连接管理策略
- **目的:** 确认超时与 Bash 工具对齐
- **操作步骤:**
  1. [A] `grep -n 'from_secs(120)' peri-middlewares/src/mcp/tool_bridge.rs` → 期望包含: `120`

---

### 场景 5：资源读取工具

#### - [x] 5.1 McpResourceTool 单元测试通过
- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认资源读取逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::resource_tool::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [x] 5.2 资源读取工具名和超时
- **来源:** spec-plan-2.md Task 5 / spec-design.md §McpResourceTool
- **目的:** 确认工具名固定为 mcp_read_resource，超时 120s
- **操作步骤:**
  1. [A] `grep -n 'TOOL_NAME\|from_secs(120)' peri-middlewares/src/mcp/resource_tool.rs` → 期望包含: `mcp_read_resource`

---

### 场景 6：McpMiddleware 中间件集成

#### - [x] 6.1 McpMiddleware 单元测试通过
- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认中间件 collect_tools 正确注入工具
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp::middleware::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 6.2 McpMiddleware 注册在 SubAgentMiddleware 之后
- **来源:** spec-design.md §中间件注册位置
- **目的:** 确认注册顺序正确
- **操作步骤:**
  1. [A] `grep -n 'McpMiddleware\|SubAgentMiddleware\|add_middleware' peri-tui/src/app/agent.rs | grep -A1 'subagent'` → 期望包含: `McpMiddleware`

#### - [x] 6.3 parent_tools 包含 MCP 工具
- **来源:** spec-plan-2.md Task 8 / spec-design.md §SubAgent 继承
- **目的:** 确认子 Agent 可继承 MCP 工具
- **操作步骤:**
  1. [A] `grep -n 'build_tool_bridges\|McpResourceTool' peri-tui/src/app/agent.rs` → 期望包含: `build_tool_bridges`

---

### 场景 7：HITL 审批扩展

#### - [x] 7.1 HITL 审批规则包含 mcp__ 前缀匹配
- **来源:** spec-plan-2.md Task 7 / spec-design.md §HITL 审批集成
- **目的:** 确认非 YOLO 模式下 MCP 工具需审批
- **操作步骤:**
  1. [A] `grep -n 'starts_with("mcp__")' peri-middlewares/src/hitl/mod.rs` → 期望包含: `mcp__`

#### - [x] 7.2 mcp_read_resource 不被拦截
- **来源:** spec-design.md §HITL 审批集成（只读不拦截）
- **目的:** 确认资源读取工具免审批
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- hitl::tests 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [x] 7.3 HITL MCP 相关测试通过
- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认 MCP 工具审批的正向/反向断言
- **操作步骤:**
  1. [A] `grep -c 'test_mcp' peri-middlewares/src/hitl/mod.rs` → 期望包含: `3`

---

### 场景 8：TUI 集成

#### - [x] 8.1 App 结构体包含 mcp_pool 字段
- **来源:** spec-plan-2.md Task 8 / spec-design.md §TUI 集成点
- **目的:** 确认 pool 持久化在 App 层
- **操作步骤:**
  1. [A] `grep -n 'mcp_pool' peri-tui/src/app/mod.rs | head -5` → 期望包含: `Option<Arc<peri_middlewares::mcp::McpClientPool>>`

#### - [x] 8.2 AgentRunConfig 传递 mcp_pool
- **来源:** spec-plan-2.md Task 8
- **目的:** 确认 pool 从 App 传入 agent 函数
- **操作步骤:**
  1. [A] `grep -c 'mcp_pool' peri-tui/src/app/agent.rs` → 期望包含: `5`

#### - [x] 8.3 pool 惰性初始化
- **来源:** spec-plan-2.md Task 8 / spec-design.md §TUI 集成点
- **目的:** 确认首次 agent 启动时初始化 pool
- **操作步骤:**
  1. [A] `grep -n 'McpClientPool::initialize' peri-tui/src/app/agent_ops.rs` → 期望包含: `McpClientPool::initialize`

#### - [x] 8.4 App 退出时 shutdown pool
- **来源:** spec-plan-2.md Task 8 / spec-design.md §生命周期集成
- **目的:** 确认退出清理连接和子进程
- **操作步骤:**
  1. [A] `grep -n 'shutdown\|mcp_pool' peri-tui/src/main.rs | head -10` → 期望包含: `shutdown`

#### - [x] 8.5 headless 测试不受 MCP 影响
- **来源:** spec-plan-2.md Task 8
- **目的:** 确认 mcp_pool: None 在测试中正常
- **操作步骤:**
  1. [A] `grep -n 'mcp_pool' peri-tui/src/app/panel_ops.rs` → 期望包含: `mcp_pool: None`

---

### 场景 9：全量测试与回归

#### - [x] 9.1 MCP 模块全部测试通过
- **来源:** spec-plan-1.md + spec-plan-2.md Acceptance
- **目的:** 确认所有 MCP 单元测试无回归
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- mcp 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [x] 9.2 全 workspace 测试通过
- **来源:** spec-plan-2.md Acceptance
- **目的:** 确认 MCP 集成未引入回归
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [x] 9.3 CLAUDE.md 包含 MCP 中间件说明
- **来源:** 项目文档规范
- **目的:** 确认文档同步更新
- **操作步骤:**
  1. [A] `grep -c 'McpMiddleware\|mcp__' CLAUDE.md` → 期望包含: `5`

---

## 验收后清理

无需清理（无后台服务）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | MCP 模块文件完整 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | peri-middlewares 编译通过 | 1 | 0 | ✅ |
| 场景 1 | 1.3 | peri-tui 编译通过 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | McpConfig 单元测试通过 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | McpServerConfig 解析含 type 字段的 JSON | 1 | 0 | ✅ |
| 场景 2 | 2.3 | 环境变量展开覆盖所有字符串字段 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | Transport 层单元测试通过 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | HTTP transport 传递自定义 headers | 1 | 0 | ✅ |
| 场景 3 | 3.3 | McpClientPool 单元测试通过 | 1 | 0 | ✅ |
| 场景 3 | 3.4 | rmcp patch 生效 | 2 | 0 | ✅ |
| 场景 4 | 4.1 | McpToolBridge 单元测试通过 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | 工具命名格式 mcp__{server}__{tool} | 1 | 0 | ✅ |
| 场景 4 | 4.3 | 工具调用超时 120s | 1 | 0 | ✅ |
| 场景 5 | 5.1 | McpResourceTool 单元测试通过 | 1 | 0 | ✅ |
| 场景 5 | 5.2 | 资源读取工具名和超时 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | McpMiddleware 单元测试通过 | 1 | 0 | ✅ |
| 场景 6 | 6.2 | McpMiddleware 注册顺序正确 | 1 | 0 | ✅ |
| 场景 6 | 6.3 | parent_tools 包含 MCP 工具 | 1 | 0 | ✅ |
| 场景 7 | 7.1 | HITL 审批含 mcp__ 前缀匹配 | 1 | 0 | ✅ |
| 场景 7 | 7.2 | mcp_read_resource 不被拦截 | 1 | 0 | ✅ |
| 场景 7 | 7.3 | HITL MCP 相关测试通过 | 1 | 0 | ✅ |
| 场景 8 | 8.1 | App 结构体包含 mcp_pool | 1 | 0 | ✅ |
| 场景 8 | 8.2 | AgentRunConfig 传递 mcp_pool | 1 | 0 | ✅ |
| 场景 8 | 8.3 | pool 惰性初始化 | 1 | 0 | ✅ |
| 场景 8 | 8.4 | App 退出时 shutdown pool | 1 | 0 | ✅ |
| 场景 8 | 8.5 | headless 测试不受 MCP 影响 | 1 | 0 | ✅ |
| 场景 9 | 9.1 | MCP 模块全部测试通过 | 1 | 0 | ✅ |
| 场景 9 | 9.2 | 全 workspace 测试通过 | 1 | 0 | ✅ |
| 场景 9 | 9.3 | CLAUDE.md 包含 MCP 说明 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
