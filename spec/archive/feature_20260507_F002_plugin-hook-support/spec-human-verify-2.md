# Plugin Hook Support 上层 验收清单

**生成时间:** 2026-05-07 20:04
**关联计划:** spec/feature_20260507_F002_plugin-hook-support/spec-plan-2.md
**关联设计:** spec/feature_20260507_F002_plugin-hook-support/spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链版本: `rustc --version && cargo --version` → 期望包含: rustc 1.x / cargo 1.x
- [x] [AUTO] 编译 workspace: `cargo build 2>&1 | tail -5` → 期望包含: Finished 且无 error
- [x] [AUTO] 验证 ipnet 依赖: `grep "ipnet" peri-middlewares/Cargo.toml` → 期望包含: ipnet = "2.10"

### 测试数据准备
- [x] [AUTO] 验证 hooks 模块完整文件列表: `ls peri-middlewares/src/hooks/` → 期望包含: ssrf_guard.rs, executor.rs, middleware.rs, loader.rs

---

## 验收项目

### 场景 1：Task 5 — SSRF 防护实现

#### - [x] 1.1 SSRF 防护公开函数签名
- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认 check_url API 签名正确
- **操作步骤:**
  1. [A] `grep -A 3 "pub fn check_url" peri-middlewares/src/hooks/ssrf_guard.rs` → 期望包含: pub fn check_url(url: &str) -> Result<(), String>

#### - [x] 1.2 SSRF 防护模块编译
- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认 ssrf_guard 模块编译通过
- **操作步骤:**
  1. [A] `cargo build -p peri-middlewares 2>&1 | grep -E "(Compiling|Finished|error)" | tail -5` → 期望包含: Finished 且无 error

#### - [x] 1.3 SSRF 防护单元测试通过
- **来源:** spec-plan-2.md Task 5 检查步骤
- **目的:** 确认公网/loopback/私有 IPv4/IPv6/mapped IPv6/无效 URL 场景全覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib ssrf_guard 2>&1 | grep -E "(test result:|running)" | tail -3` → 期望包含: test result: ok 且无 failed

#### - [x] 1.4 ssrf_guard 模块声明
- **来源:** spec-plan-2.md Task 5 执行步骤
- **目的:** 确认模块在 mod.rs 中声明
- **操作步骤:**
  1. [A] `grep -n "pub mod ssrf_guard" peri-middlewares/src/hooks/mod.rs` → 期望包含: pub mod ssrf_guard

---

### 场景 2：Task 6 — Hook 执行器（4 种）

#### - [x] 2.1 executor.rs 文件编译通过
- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认执行器模块编译无错误
- **操作步骤:**
  1. [A] `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|warning:.*executor)"` → 期望精确: (无输出)

#### - [x] 2.2 执行器函数导出正确
- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认 4 种执行器公共 API 导出
- **操作步骤:**
  1. [A] `grep -n "pub use executor" peri-middlewares/src/hooks/mod.rs` → 期望包含: pub use executor::{execute_command_hook, execute_prompt_hook, execute_http_hook, execute_agent_hook}

#### - [x] 2.3 Command 执行器测试通过
- **来源:** spec-plan-2.md Task 6
- **目的:** 确认 stdin JSON 协议、退出码语义、超时、环境变量注入
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::executor::test_execute_command 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.4 Prompt 执行器测试通过
- **来源:** spec-plan-2.md Task 6
- **目的:** 确认 LLM 调用、JSON/纯文本/失败/超时/$ARGUMENTS 替换
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::executor::test_execute_prompt 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.5 HTTP 执行器测试通过
- **来源:** spec-plan-2.md Task 6
- **目的:** 确认成功/空 body/非 JSON/错误/SSRF 阻断/CRLF 防护/env 白名单
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::executor::test_execute_http 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.6 Agent 执行器测试通过
- **来源:** spec-plan-2.md Task 6
- **目的:** 确认 SyntheticOutputTool/超时/防递归/防嵌套
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::executor::test_execute_agent 2>&1 | tail -3` → 期望包含: test result: ok

#### - [x] 2.7 HTTP SSRF 防护集成
- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认 HTTP 执行器调用 ssrf_guard
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::executor::test_execute_http::test_ssrf_blocking 2>&1 | tail -5` → 期望包含: test result: ok

#### - [x] 2.8 Agent 执行器防递归
- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认子 agent 不包含 HookMiddleware
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::executor::test_execute_agent::test_no_recursion 2>&1 | tail -5` → 期望包含: test result: ok

#### - [x] 2.9 执行器测试覆盖率
- **来源:** spec-plan-2.md Task 6 检查步骤
- **目的:** 确认测试场景全覆盖
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::executor 2>&1 | grep -E "running \d+ test" | head -1` → 期望包含: running 16 test
  2. [A] `cargo test -p peri-middlewares --lib hooks::executor 2>&1 | grep "test result:"` → 期望包含: test result: ok

---

### 场景 3：Task 7 — HookMiddleware 实现

#### - [x] 3.1 HookMiddleware 构造函数编译
- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认结构体和构造函数编译通过
- **操作步骤:**
  1. [A] `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|not implemented|missing method)" | head -5` → 期望精确: (无输出)

#### - [x] 3.2 Middleware trait 实现完整
- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认 trait 方法全部实现
- **操作步骤:**
  1. [A] `cargo check -p peri-middlewares --lib 2>&1 | grep -E "not implemented|missing method"` → 期望精确: (无输出)

#### - [x] 3.3 HookMiddleware 模块导出
- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认 HookMiddleware 类型可从 hooks 模块访问
- **操作步骤:**
  1. [A] `grep -n "pub use middleware::HookMiddleware" peri-middlewares/src/hooks/mod.rs` → 期望包含: pub use middleware::HookMiddleware

#### - [x] 3.4 fire_event 核心逻辑编译
- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认匹配和执行调用链编译通过
- **操作步骤:**
  1. [A] `cargo check -p peri-middlewares --lib 2>&1 | grep -E "fire_event|matches_matcher|matches_if_condition" | grep -i error` → 期望精确: (无输出)

#### - [x] 3.5 HookMiddleware 单元测试通过
- **来源:** spec-plan-2.md Task 7 检查步骤
- **目的:** 确认 no_hooks/once/matcher/condition/block/modify_input/before_tool 场景
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::middleware 2>&1 | tail -5` → 期望包含: test result: ok

---

### 场景 4：Task 8 — 插件 Hook 加载与注册

#### - [x] 4.1 PluginManifest.hooks 类型变更编译
- **来源:** spec-plan-2.md Task 8 检查步骤
- **目的:** 确认 Option<serde_json::Value> → Option<HooksConfig> 类型变更无破坏
- **操作步骤:**
  1. [A] `cargo check -p peri-middlewares --lib 2>&1 | grep -E "hooks|HooksConfig" | grep error` → 期望精确: (无输出)

#### - [x] 4.2 LoadedPlugin.hooks_config 字段
- **来源:** spec-plan-2.md Task 8 检查步骤
- **目的:** 确认 hooks_config 字段存在
- **操作步骤:**
  1. [A] `grep -n "pub hooks_config" peri-middlewares/src/plugin/loader.rs` → 期望包含: pub hooks_config

#### - [x] 4.3 PluginLoadResult.all_hooks 字段
- **来源:** spec-plan-2.md Task 8 检查步骤
- **目的:** 确认 hooks 聚合字段存在
- **操作步骤:**
  1. [A] `grep -n "pub all_hooks" peri-middlewares/src/plugin/loader.rs` → 期望包含: pub all_hooks

#### - [x] 4.4 extract_hooks 单元测试通过
- **来源:** spec-plan-2.md Task 8 检查步骤
- **目的:** 确认 hooks.json 优先/plugin.json 回退/空/解析失败回退
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks::loader 2>&1 | tail -5` → 期望包含: test result: ok

#### - [x] 4.5 hooks 聚合集成测试通过
- **来源:** spec-plan-2.md Task 8 检查步骤
- **目的:** 确认单插件/多插件/matcher 优先级/plugin_options 转换
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib plugin::loader::test_hooks 2>&1 | tail -5` → 期望包含: test result: ok

#### - [x] 4.6 TUI AgentRunConfig.plugin_hooks 字段
- **来源:** spec-plan-2.md Task 8 检查步骤
- **目的:** 确认 TUI 层接收 hooks 配置
- **操作步骤:**
  1. [A] `grep -n "pub plugin_hooks" peri-tui/src/app/agent.rs` → 期望包含: pub plugin_hooks

#### - [x] 4.7 TUI HookMiddleware 集成编译
- **来源:** spec-plan-2.md Task 8 检查步骤
- **目的:** 确认 TUI 层 HookMiddleware 创建代码编译通过
- **操作步骤:**
  1. [A] `cargo check -p peri-tui --bin peri-tui 2>&1 | grep -E "(error|HookMiddleware)" | grep error` → 期望精确: (无输出)

#### - [x] 4.8 PluginManifest.hooks 向后兼容
- **来源:** spec-plan-2.md Task 10 验证步骤 4
- **目的:** 确认 hooks 类型变更不破坏现有测试
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib plugin::types::tests 2>&1 | grep "test result:"` → 期望包含: test result: ok

#### - [x] 4.9 HookMiddleware 中间件链位置
- **来源:** spec-plan-2.md Task 10 验证步骤 5
- **目的:** 确认 HookMiddleware 在 HITL 之后、SubAgent 之前
- **操作步骤:**
  1. [A] `grep -n "add_middleware" peri-tui/src/app/agent.rs` → 期望包含: HookMiddleware 出现在 hitl 和 subagent 的 add_middleware 之间

---

### 场景 5：Task 9 — AgentEvent 扩展与 SubAgent 事件转发

#### - [x] 5.1 AgentEvent 新变体编译
- **来源:** spec-plan-2.md Task 9 检查步骤
- **目的:** 确认 5 个新变体编译通过
- **操作步骤:**
  1. [A] `cargo check -p peri-agent --lib 2>&1 | grep error` → 期望精确: (无输出)

#### - [x] 5.2 SubAgent 事件转发编译
- **来源:** spec-plan-2.md Task 9 检查步骤
- **目的:** 确认 SubAgentMiddleware 事件发出代码编译通过
- **操作步骤:**
  1. [A] `cargo check -p peri-middlewares --lib 2>&1 | grep error` → 期望精确: (无输出)

#### - [x] 5.3 AgentEvent 序列化测试通过
- **来源:** spec-plan-2.md Task 9 检查步骤 / Task 10 验证步骤 6
- **目的:** 确认 5 个新变体序列化/反序列化 roundtrip
- **操作步骤:**
  1. [A] `cargo test -p peri-agent --lib agent::events 2>&1 | grep "test result:"` → 期望包含: test result: ok

#### - [x] 5.4 SubAgent 事件转发测试通过
- **来源:** spec-plan-2.md Task 9 检查步骤 / Task 10 验证步骤 7
- **目的:** 确认事件发出验证和无 handler 不 panic
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib subagent::tool 2>&1 | grep "test result:"` → 期望包含: test result: ok

---

### 场景 6：Task 10 — 完整验收

#### - [x] 6.1 全量测试套件无回归
- **来源:** spec-plan-2.md Task 10 验证步骤 1
- **目的:** 确认所有 crate 全部测试通过
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -15` → 期望包含: test result: ok 且无 FAILED

#### - [x] 6.2 hooks 模块完整编译和导出
- **来源:** spec-plan-2.md Task 10 验证步骤 2
- **目的:** 确认 hooks 模块所有子模块测试通过
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib hooks 2>&1 | grep "test result:"` → 期望包含: test result: ok

#### - [x] 6.3 TUI 层集成编译通过
- **来源:** spec-plan-2.md Task 10 验证步骤 3
- **目的:** 确认 TUI 应用编译通过
- **操作步骤:**
  1. [A] `cargo build -p peri-tui 2>&1 | tail -5` → 期望包含: Finished 且无 error

---

## 验收后清理

无后台服务需要清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | SSRF 防护公开函数签名 | A | | ✅ |
| 场景 1 | 1.2 | SSRF 防护模块编译 | A | | ✅ |
| 场景 1 | 1.3 | SSRF 防护单元测试通过 | A | | ✅ |
| 场景 1 | 1.4 | ssrf_guard 模块声明 | A | | ✅ |
| 场景 2 | 2.1 | executor.rs 文件编译通过 | A | | ✅ |
| 场景 2 | 2.2 | 执行器函数导出正确 | A | | ✅ |
| 场景 2 | 2.3 | Command 执行器测试通过 | A | | ✅ |
| 场景 2 | 2.4 | Prompt 执行器测试通过 | A | | ✅ |
| 场景 2 | 2.5 | HTTP 执行器测试通过 | A | | ✅ |
| 场景 2 | 2.6 | Agent 执行器测试通过 | A | | ✅ |
| 场景 2 | 2.7 | HTTP SSRF 防护集成 | A | | ✅ |
| 场景 2 | 2.8 | Agent 执行器防递归 | A | | ✅ |
| 场景 2 | 2.9 | 执行器测试覆盖率 | A | | ✅ |
| 场景 3 | 3.1 | HookMiddleware 构造函数编译 | A | | ✅ |
| 场景 3 | 3.2 | Middleware trait 实现完整 | A | | ✅ |
| 场景 3 | 3.3 | HookMiddleware 模块导出 | A | | ✅ |
| 场景 3 | 3.4 | fire_event 核心逻辑编译 | A | | ✅ |
| 场景 3 | 3.5 | HookMiddleware 单元测试通过 | A | | ✅ |
| 场景 4 | 4.1 | PluginManifest.hooks 类型变更编译 | A | | ✅ |
| 场景 4 | 4.2 | LoadedPlugin.hooks_config 字段 | A | | ✅ |
| 场景 4 | 4.3 | PluginLoadResult.all_hooks 字段 | A | | ✅ |
| 场景 4 | 4.4 | extract_hooks 单元测试通过 | A | | ✅ |
| 场景 4 | 4.5 | hooks 聚合集成测试通过 | A | | ✅ |
| 场景 4 | 4.6 | TUI AgentRunConfig.plugin_hooks 字段 | A | | ✅ |
| 场景 4 | 4.7 | TUI HookMiddleware 集成编译 | A | | ✅ |
| 场景 4 | 4.8 | PluginManifest.hooks 向后兼容 | A | | ✅ |
| 场景 4 | 4.9 | HookMiddleware 中间件链位置 | A | | ✅ |
| 场景 5 | 5.1 | AgentEvent 新变体编译 | A | | ✅ |
| 场景 5 | 5.2 | SubAgent 事件转发编译 | A | | ✅ |
| 场景 5 | 5.3 | AgentEvent 序列化测试通过 | A | | ✅ |
| 场景 5 | 5.4 | SubAgent 事件转发测试通过 | A | | ✅ |
| 场景 6 | 6.1 | 全量测试套件无回归 | A | | ✅ |
| 场景 6 | 6.2 | hooks 模块完整编译和导出 | A | | ✅ |
| 场景 6 | 6.3 | TUI 层集成编译通过 | A | | ✅ |

**验收结论:** ✅ 全部通过
