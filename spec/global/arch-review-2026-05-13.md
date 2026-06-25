# 架构深度审查报告

**日期**: 2026-05-13
**方法**: 3 个并行 Explorer Agent + 手动交叉验证
**范围**: 全 workspace 6 crate（排除 `rust-mcp-patch/` 和 `peri-cli/`）
**上一次审查**: tech-review-2025-05-08.md（综合 7.6/10 B+）

---

## 总体评估

| 维度 | 上次 | 本次 | 变化 | 说明 |
|------|------|------|------|------|
| 核心框架深度 | 7.5/10 | 7.5/10 | — | trait 设计优秀，但缺少 TokenEstimator / StatePersistence 等内层 seam |
| 中间件层耦合 | 6.5/10 | **6.8/10** | ↑0.3 | HITL→ToolSearch 字符串耦合仍在，但整体方向正确 |
| TUI 层架构 | 7.8/10 | **8.0/10** | ↑0.2 | God Object 已消除，但 ServiceRegistry 混入 UI 状态 |
| 跨 crate 边界 | 8.0/10 | 8.0/10 | — | TUI 仍直接调用 compact 内部函数（无 facade） |
| 测试覆盖 | — | **6.0/10** | 新维度 | compact 子系统零测试、SubAgent 集成测试不足 |
| **综合** | **7.6/10 B+** | **7.7/10 B+** | **↑0.1** | Phase 2 收益持续兑现，Phase 3 遗留项明确 |

---

## 深化机会（按优先级排序）

### 候选 1：ServiceRegistry 职责扩散 ⚠️ 中优先级

**文件**: `peri-tui/src/app/service_registry.rs`（82 行，21 个 pub 字段）

**问题**: ServiceRegistry 混合了 3 种不相关职责——

| 类别 | 字段 | 数量 |
|------|------|------|
| 全局服务 | peri_config, cwd, provider_name, model_name, thread_store, mcp_pool, ... | 10 |
| 全局 UI 状态 | setup_wizard, oauth_prompt, mode_highlight_until, model_highlight_until, mcp_ready_shown_until, quit_pending_since, mouse_available | 7 |
| 进程级设施 | bg_event_tx/rx, config_path_override, claude_settings_override, resource_monitor, permission_mode | 4 |

**删除测试**: 删除 ServiceRegistry，10 个服务字段的重构复杂度重新出现在调用方——**服务字段在 earning its keep**。但 7 个 UI 状态字段（setup_wizard, oauth_prompt, highlight timers, mouse_available）是 **pass-through**，删除后复杂度不会重新出现（它们是 UI 层的临时状态，应属于 UiState 或独立结构体）。

**方案**: 将 UI 状态字段移入 `UiState` 或新建 `GlobalUiState`（setup_wizard, oauth_prompt, highlight timers）。ServiceRegistry 仅保留服务字段（10→10，但语义更纯粹）。

**收益**:

- Locality：setup_wizard/oauth_prompt 逻辑集中在 UI 层而非散落在 ServiceRegistry
- Testability：测试 ServiceRegistry 不需要构造 UI 状态

---

### 候选 2：SubAgent 硬编码中间件链（4×3 重复） ⚠️ 中优先级

**文件**: `peri-middlewares/src/subagent/tool.rs`（979 行）

**问题**: 4 条执行路径（Normal/Fork/Background/Fork+Background）各自硬编码 5 个中间件构造。每次新增中间件需同步修改 4 处（已确认行号：299-301, 455-465, 606-608, 848-859）。

**删除测试**: 删除 `build_subagent_middlewares()` 提取函数，复杂度重新出现在 4 个调用点——**当前实现 NOT earning its keep**，是重复而非深度。

**方案**: 提取 `fn build_subagent_middlewares(config: SubAgentMiddlewareConfig) -> Vec<Box<dyn Middleware<AgentState>>>`，4 条路径调用同一构造函数。`SubAgentMiddlewareConfig` 参数化差异（是否含 SkillPreload、Todo channel 等）。

**收益**:

- Locality：中间件构造逻辑集中在一处
- Leverage：新增中间件只需修改 1 处
- 测试改进：可单独测试构造函数

---

### 候选 3：compact 子系统零测试 🔴 高优先级

**文件**: `peri-agent/src/agent/compact/`（4 个文件，~2100 行非测试代码）

| 文件 | 行数 | 测试行数 |
|------|------|----------|
| micro.rs | 510 | 0 |
| full.rs | 685 | 0 |
| re_inject.rs | 650 | 0 |
| invariant.rs | 420 | 0 |

**问题**: compact 是最危险的系统操作——它直接修改消息历史（原地删除、摘要替换）。错误的 compact 实现可能导致对话上下文损坏、工具配对断裂、用户数据丢失。但这个子系统**完全没有测试**。

**删除测试**: 删除 compact 子系统，所有复杂度重新出现在调用方（需要重新实现压缩逻辑）——**compact 在 earning its keep**，但接口没有测试保护。

**方案**: 优先为 `invariant.rs` 添加单元测试（compact 前后不变量校验），然后为 `micro.rs` 添加结构化测试（已知输入→预期输出）。`full.rs` 和 `re_inject.rs` 需要 MockLLM 集成测试。

**收益**:

- 安全网：防止 compact 引入数据损坏回归
- 文档价值：测试用例本身就是 compact 行为的规范文档

---

### 候选 4：event.rs `std::mem::take` 借用检查器 workaround ⚠️ 中优先级

**文件**: `peri-tui/src/event.rs`（1408 行，12 处 `std::mem::take`）

**问题**: PanelManager dispatch 需要 `&mut self`（面板）+ `&mut App 其余字段`（PanelContext），Rust 借用检查器无法证明两者不重叠。当前用 `std::mem::take` + 归还模式绕过（349, 394, 761, 978, 999, 1027, 1042, 1067, 1082, 1111, 1127 行）。

**方案**: 重构 `dispatch_key` 签名为独立 `&mut` 参数，让 Rust 借用检查器在编译时验证不重叠：

```rust
// Before: &self 方法 + PanelContext 包含 App 引用
pm.dispatch_key(input, &mut ctx)

// After: 自由函数，两个独立 &mut 参数
fn dispatch_panel_key(
    pm: &mut PanelManager,
    services: &mut ServiceRegistry,
    session_mgr: &mut SessionManager,
    input: Input,
) -> EventResult
```

**收益**:

- 消除 12 处 `std::mem::take` workaround（take 后忘记归还 = 运行时 panic）
- 编译时安全：如果签名有重叠，编译器会直接报错

---

### 候选 5：TUI 直接访问核心框架内部类型 ⚠️ 低优先级

**文件**: `peri-tui/src/app/agent.rs:633`, `agent_compact.rs:252`

**问题**: TUI 层直接调用 `peri-agent` 的内部函数：

- `peri_agent::agent::compact::micro_compact_enhanced`
- `peri_agent::agent::compact::full_compact`
- `peri_agent::agent::compact::re_inject`

这些函数不在 `peri-agent` 的公共 API 中（通过 `pub(crate)` 或模块可见性暴露）。跨 crate 访问内部类型违反了分层架构的意图。

**方案**: 在 `peri-agent` 中创建 facade 层，将 compact 操作封装为公共 API：

```rust
// peri-agent/src/lib.rs
pub fn compact_messages(state: &mut AgentState, config: CompactConfig) -> AgentResult<()>;
```

**收益**:

- 真正的 crate 边界隔离
- 未来可替换 compact 实现而不影响 TUI

---

### 候选 6：AgentState 持久化耦合 ⚠️ 低优先级

**文件**: `peri-agent/src/agent/state.rs`（245 行）

**问题**: `AgentState` 将持久化逻辑直接嵌入状态结构体——`store`、`thread_id`、`persist_tx` 三个字段（43-51 行）与 `with_persistence()` 构造方法（95-113 行）。`add_message()` 方法（142-148 行）混合了状态追加和持久化写入两个职责。

**删除测试**: 删除 `with_persistence()`，持久化逻辑重新出现在调用方——**持久化在 earning its keep**，但应通过 seam 而非直接耦合。

**方案**: 提取 `StatePersistence` trait：

```rust
trait StatePersistence: Send + Sync {
    fn persist_message(&self, message: &BaseMessage) -> AgentResult<()>;
}
```

`AgentState` 持有 `Option<Arc<dyn StatePersistence>>` 而非直接持有 channel。

**收益**:

- 可测试性：测试 AgentState 不需要 channel
- 灵活性：可替换为 batch persistence、no-op persistence 等

---

### 候选 7：HITL→ToolSearch 字符串协议耦合 ⚠️ 低优先级

**文件**: `peri-middlewares/src/hitl/mod.rs:67-77`

**问题**: HITL 中间件通过字符串匹配 `"ExecuteExtraTool"` 来解析实际工具名。这是跨中间件的隐式协议——HITL 需要知道 ToolSearch 的内部工具命名约定。

**方案**: 将工具名解析逻辑移入 ToolSearch 中间件（作为 `before_tool` 钩子的一部分），或定义共享常量。

**收益**:

- HITL 中间件可独立于 ToolSearch
- 命名变更只需修改一处

---

## 已验证的良好 Seam

以下 seam 经 3 个 Agent 独立验证，确认具有**真实的深度**（高 leverage behind small interface）：

| Seam | Adapter 数量 | 验证方 | 评价 |
|------|-------------|--------|------|
| `BaseModel` trait | 3（Anthropic/OpenAI/Mock） | Agent 1 | ✅ 真实 seam，新增 provider 无需改现有代码 |
| `ThreadStore` trait | 2（SQLite/Filesystem） | Agent 1 | ✅ 真实 seam，可 mock 测试 |
| `BaseTool` trait | 30+（所有工具实现） | Agent 1+2 | ✅ 真实 seam，核心扩展点 |
| `Middleware<S>` trait | 15（所有中间件） | Agent 1+2 | ✅ 真实 seam，链式组合 |
| `PanelComponent` trait | 10（所有面板） | 手动 | ✅ 真实 seam，Phase 2 成果 |
| `PipelineAction` enum | 3 变体 | 手动 | ✅ 真实 seam，统一 UI 变更描述 |
| `ToolProvider` trait | 2（SearchExtraTools/MCP） | Agent 2 | ✅ 真实 seam，动态工具发现 |

---

## 量化指标对比

| 指标 | 上次（2025-05-08） | 本次（2026-05-13） | 目标 | 状态 |
|------|-------------------|-------------------|------|------|
| App 顶层字段 | 34→**3** | **3** | < 20 | 🟢 达标 |
| AppCore 字段 | 39→**0** | **0**（已消除） | 0 | 🟢 达标 |
| event.rs 行数 | 2486→1026 | **1408** | < 1000 | 🟡 回升* |
| 非测试 unwrap/expect | ~140 | **~80** | < 30 | 🟡 改善中 |
| 超长函数 >150 行 | 29→~5-8 | **~8-10** | < 10 | 🟢 达标 |
| 面板互斥逻辑重复 | 10→0 | **0** | 0 | 🟢 达标 |
| std::mem::take workaround | 4 | **12** | 0 | 🔴 新增 |
| compact 测试覆盖 | — | **0** | > 0 | 🔴 零覆盖 |
| SubAgent 中间件重复 | 5×3 | **5×4** | 1 处定义 | 🔴 未变 |
| 最大非测试文件 | — | **2017**（plugin_panel.rs） | < 1500 | 🟡 |
| ServiceRegistry 字段 | — | **21** | < 15 | 🟡 |
| 零循环依赖 | 0 | **0** | 0 | 🟢 健康 |
| unsafe 块 | 0 | **0** | 0 | 🟢 健康 |

> *event.rs 行数从 1026 回升至 1408：Phase 2 重构后新增了 ACP（Agent Client Protocol）事件分发逻辑（~380 行）。面板组件化成果保持不变。

---

## 推荐路线图

### Phase 3A — 安全网（1-2 天）

1. **compact 子系统测试**：优先 `invariant.rs`，然后 `micro.rs`
2. **SubAgent `build_subagent_middlewares()` 提取**：消除 4×3 重复

### Phase 3B — 借用检查器清理（2-3 天）

1. **event.rs dispatch 签名重构**：消除 12 处 `std::mem::take`
2. **ServiceRegistry 职责分离**：UI 状态字段移出

### Phase 3C — crate 边界加固（2-3 天）

1. **compact facade**：TUI 不再直接调用内部函数
2. **HITL→ToolSearch 解耦**：消除字符串协议

### Phase 3D — 内层 seam（可选，3-5 天）

1. **AgentState 持久化提取**：`StatePersistence` trait
2. **TokenEstimator trait**：可插拔 token 估算策略

---

## 方法说明

本报告由 3 个并行 Explorer Agent 独立分析 + 手动交叉验证生成：

| Agent | 范围 | 工具调用 | 耗时 |
|-------|------|---------|------|
| Agent 1 | peri-agent 深度 & seam 分析 | 46 | 140s |
| Agent 2 | peri-middlewares 耦合 & seam 分析 | 44 | 155s |
| Agent 3 | 域模型 & 架构决策探索 | 38 | 297s |

**交叉验证**：

- Agent 1 指出 "compact 零测试" → 手动确认 4 个文件均无 `#[cfg(test)]`
- Agent 2 指出 "SubAgent 4×3 重复" → 手动确认 4 处行号（299/455/606/848）
- Agent 1 指出 "State 持久化耦合" → 手动确认 state.rs:43-51 三字段
- Agent 2 指出 "HITL→ToolSearch 字符串耦合" → 手动确认 hitl/mod.rs:67-77
- 三个 Agent 一致确认：BaseModel/ThreadStore/BaseTool/Middleware 为真实 seam

**未覆盖**：

- `langfuse-client`（独立 crate，上次审查已确认隔离良好）
- `peri-lsp`（独立 crate，上次审查已确认接口清晰）
- `peri-widgets`（11 组件，零内部依赖，上次审查已确认深度适当）
- `rust-mcp-patch/`（临时补丁，上游修复后删除）

---

*报告由 3 位并行 Explorer Agent 分析 + 手动交叉验证生成。2026-05-13。*
