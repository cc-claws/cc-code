# ADR-001: 采用 Kameo Actor 框架重构 SubAgent 并发模型

**状态**: Proposed
**日期**: 2026-05-26
**决策者**: 项目维护者
**审查**: 并发正确性 / 迁移可行性 / 领域模型适配 / Kameo 框架适配（4 视角）

---

## 背景

Perihelion 当前的并发模型基于手工管理的 `tokio::spawn` + 共享状态（`Arc<Mutex<T>>`）。以下结构性问题集中在 **SubAgent 并发**场景：

1. **SubAgent 事件路由靠字符串匹配**：`source_agent_id` 字符串在并发场景下不可靠，已导致 ID 冲突、事件溢出等多个 issue（#0516, #0519, #0524）。
2. **SubAgent 4 条路径的样板代码重复**：Normal/Fork/Background/ForkBackground 各自硬编码工具注册、system prompt 构建、event handler 绑定、cancel token 管理（约 100 行/路径）。
3. **Background Agent 生命周期管理复杂**：`BackgroundTaskRegistry` + `bg_event_sender` + `deregister_runtime` + `DeregisterGuard` 的手动编排容易出错。
4. **跨 async 边界的状态安全**：`frozen_subagent_vms` 等轮次作用域状态依赖手动清理，background agent 跨轮次存活时容易累积或遗漏。

**不在范围内的问题**：

- `tool_dispatch` 的延迟写入是**语义约束**（AI 消息 + 所有 tool_result 必须原子写入），不是并发缺陷，Actor mailbox 串行化不能替代它。
- ReAct 循环（`LLM → tools → emit`）是同步顺序循环，Actor 化零并发收益。
- Middleware chain 的 `before_tool`/`after_tool` 需要即时决策（如 HookMiddleware 的 Block/Allow），不适合消息传递。
- Compact 需要直接操作 state 替换整个 messages 数组，不适合跨 Actor 消息传递。
- Cancel 的原子性依赖 `CancellationToken` 即时传播，Actor 消息链有 mailbox 延迟，不能替代。

这些问题本质上是 **SubAgent 并发单元缺少结构化隔离**——即 Actor 模型要解决的核心问题。但 Actor 模型不适用于 ReAct 循环、延迟写入、compact 等同步语义场景。

## 决策

**采用 Kameo（0.20+）作为 Actor 框架，将 SubAgent 并发模型迁移到 Actor 架构。范围限定在 SubAgent + 共享资源层，不涉及 ReAct 循环主体、Middleware Chain、Compact 等同步语义组件。**

## 候选方案评估

| 维度 | Kameo | Ractor | 自建轻量抽象 |
|------|-------|--------|-------------|
| tokio 原生 | 是（`tokio::spawn`） | 是 | 是 |
| API 复杂度 | 低（derive 宏 + trait impl） | 中（Actor + ActorRef 生命周期） | 低（自定义） |
| Supervision | 内置（link_died 回调） | 最完整（Erlang OTP 风格） | 需自行实现 |
| 分布式支持 | 早期阶段 | 成熟（`ractor_cluster`） | 无 |
| 对现有代码侵入性 | 低 | 中 | 最低 |
| 社区活跃度 | 高（0.20, 2025 持续更新） | 高 | N/A |
| 成熟度 | 年轻（2024 发布） | 较成熟 | 取决于自身投入 |

### 选择 Kameo 的理由

1. **迁移成本最低**：SubAgent 的 `tokio::spawn` 可直接替换为 `Actor::spawn()`，EventSink 继续作为外部接口。
2. **per-actor mailbox 解决事件路由**：每个 SubAgentActor 拥有独立 channel，`ActorRef<SubAgentActor>` 天然类型安全路由，替代字符串 `source_agent_id` 匹配。
3. **supervision 足够**：Kameo 的 `link_died` 回调覆盖退出监控，满足 SubAgent 生命周期管理需求。
4. **无需分布式**：项目是单机多 Agent 并发，Ractor 的 `ractor_cluster` 无收益。
5. **可替换性兜底**：Kameo 底层是 `tokio::spawn` + `mpsc`，最坏情况可 fork 自行维护。

### 关键映射关系（仅限 SubAgent 范围）

| 当前概念 | Actor 化后 |
|---------|-----------|
| `SubAgentTool` 4 条路径 | `SubAgentActor`（参数化配置） |
| `tokio::spawn` + event_handler 透传 | `Actor::spawn()` + `ActorRef` 类型安全路由 |
| `source_agent_id` 字符串路由 | `ActorRef<SubAgentActor>` |
| `BackgroundTaskRegistry` | `SessionActor.background_agents` |
| `DeregisterGuard` RAII | Actor `on_stop` 回调 |
| `frozen_subagent_vms` HashMap | Supervision children + 轮次清理消息 |

### 明确不变的部分

| 组件 | 保持现状的原因 |
|------|---------------|
| ReAct 循环 | 同步顺序循环，零并发收益 |
| `tool_dispatch` 延迟写入 | 原子性语义约束，mailbox 串行化不能替代 |
| Middleware Chain | `before_tool`/`after_tool` 需要即时决策 |
| Compact | 全局 state 替换需直接操作 |
| `CancellationToken` | 即时传播，Actor 消息有延迟 |
| `EventSink` trait | 保持为 ACP/TUI 的稳定接口 |

## 目标架构设计

### Actor 层级与 Supervision Tree

```
SessionActor (per session, supervisor)
├── SubAgentActor #1 (per invocation)
│   └── 内部: 直接调用 ReActAgent.execute()（非 Actor）
│       └── [递归] SubAgentTool → spawn SubAgentActor...
├── SubAgentActor #2
├── SubAgentActor #3 (background)
└── ...
```

**关键设计决策**：SubAgentActor 内部**不嵌套 AgentActor**——而是直接调用现有的 `ReActAgent.execute()`。Actor 只负责生命周期管理和事件路由，ReAct 循环保持当前的直接调用模式。

**Supervision 策略**：

- `SessionActor` 监控 `SubAgentActor`：SubAgent 退出（正常/异常）→ 清理 `background_agents`、通知 TUI
- Background SubAgent 跨轮次存活时，轮次结束通过显式消息 `CleanupRound` 清理 `frozen_vm`

### Actor 定义

> **注意**：以下为设计意图伪代码，非可编译代码。Kameo 0.20 的实际 API 使用 `Actor` trait + `on_start(args) -> Result<Self>` 工厂模式，`Message` 为泛型 trait，无 `#[derive(Message)]`。实施前需基于 0.20 写原型验证。

#### SessionActor — 会话级 SubAgent 管理

**职责**：管理当前 session 的所有 SubAgentActor 的生命周期。不替代 `SessionState`，仅封装 SubAgent 相关状态。

```rust
// 伪代码 — 表达设计意图
struct SessionActor {
    session_id: String,
    // SubAgent 管理
    active_subagents: HashMap<String, ActorRef<SubAgentActor>>,
    background_agents: HashMap<String, ActorRef<SubAgentActor>>,
    // 共享资源引用（非 Actor）
    event_sink: Arc<dyn EventSink>,       // Phase 1: 保持现有接口
    cancel_token: AgentCancellationToken, // 保持现有 CancellationToken
    // 其他共享资源（Arc 引用，非 Actor）
    mcp_pool: Option<Arc<McpClientPool>>,
    agent_pool: Arc<Mutex<AgentPool>>,
}

// 消息
struct SpawnSubAgent { config: SubAgentConfig }  // → ActorRef<SubAgentActor>
struct SubAgentCompleted { agent_id: String, result: SubAgentResult }
struct CancelSubAgent { agent_id: String }
struct CleanupRound {}  // 轮次结束时清理 frozen_vm
struct Shutdown {}       // session 结束，cancel 所有子 Agent
```

**消除的痛点**：
- `BackgroundTaskRegistry` + `bg_event_sender` + `deregister_runtime` → `background_agents` HashMap + supervision
- `DeregisterGuard` RAII → Actor `on_stop` 自动清理
- `source_agent_id` 事件路由 → `ActorRef` 类型安全

#### SubAgentActor — 单次 SubAgent 执行

**职责**：封装单次 SubAgent 调用的生命周期。内部直接调用 `ReActAgent.execute()`，不嵌套 AgentActor。

```rust
// 伪代码 — 表达设计意图
struct SubAgentActor {
    agent_id: String,
    mode: SubAgentMode,  // Normal | Fork | Background | ForkBackground
    parent_ref: ActorRef<SessionActor>,

    // 执行状态
    inner_agent: Option<ReActAgent>,  // 直接持有，非 Actor
    state: Option<AgentState>,
    frozen_vm: Option<MessageViewModel>,

    // 共享资源引用
    event_sink: Arc<dyn EventSink>,       // 透传给内部 agent
    cancel_token: AgentCancellationToken, // 保持 CancellationToken
}

// 核心消息
struct Execute { prompt: String, agent_type: String, fork: bool, history: Vec<BaseMessage> }
struct Cancel {}  // 触发 cancel_token.cancel()

impl Handler<Execute> for SubAgentActor {
    async fn handle(&mut self, msg: Execute) {
        // 统一的中间件构造（消除 4 条路径重复）
        let config = SubAgentMiddlewareConfig::from_mode(msg.fork, self.mode.is_background());
        let middlewares = build_subagent_middlewares(config);

        // 构建 ReActAgent（复用现有逻辑）
        let agent = build_subagent_agent(config, self.cancel_token.clone(), self.event_sink.clone());
        self.inner_agent = Some(agent);

        // 直接调用 execute（同步循环，非 Actor 消息传递）
        let result = self.inner_agent.as_mut().unwrap().execute(input, &mut state, Some(cancel));

        // 完成 → 通知 SessionActor
        self.parent_ref.tell(SubAgentCompleted { agent_id, result });
    }
}

// Supervision: Actor 退出时自动清理
impl Actor for SubAgentActor {
    async fn on_stop(&mut self) {
        // 清理 frozen_vm
        // 通知 parent（如果尚未通知）
    }
}
```

**消除的痛点**：
- **4 条路径样板代码**：统一为 `SubAgentMiddlewareConfig::from_mode()` + 单个 Handler
- **source_agent_id 字符串路由**：`parent_ref: ActorRef<SessionActor>` 类型安全
- **frozen_vm 手动清理**：`on_stop` 自动清理

**明确不变的部分**：
- **ReAct 循环保持直接调用**：`agent.execute()` 是同步循环，不走 Actor 消息传递
- **CancellationToken 保持现有语义**：级联/独立 cancel 通过 token clone 传播，不走 Actor 消息
- **EventSink 保持现有接口**：SubAgentActor 持有 `Arc<dyn EventSink>`，事件通过现有路径发送
- **tool_dispatch 延迟写入不变**：内部 ReActAgent 的工具执行仍走 `collect_tool_results → dispatch_tools`

### 共享资源层

以下跨 session 共享组件**不 Actor 化**，保持 `Arc` 引用由 SessionActor 持有：

| 组件 | 类型 | 原因 |
|------|------|------|
| `McpClientPool` | `Option<Arc<McpClientPool>>` | 跨 session 连接池，生命周期超出单个 session |
| `AgentPool` | `Arc<Mutex<AgentPool>>` | Session 级 LLM 实例缓存，避免 arena 碎片化 |
| `CronScheduler` | `Option<Arc<Mutex<CronScheduler>>>` | 跨 session 定时任务 |
| `ThreadStore` | `Option<Arc<dyn ThreadStore>>` | 跨 session 持久化 |
| `LspMiddleware` | 通过 MiddlewareChain 管理 | `after_tool` 文件同步是同步钩子 |
| `HookMiddleware` | 通过 MiddlewareChain 管理 | Block/Allow 需要即时决策 |
| `ToolSearchIndex` | `Arc<ToolSearchIndex>` | 只读索引，无需 Actor |

### 数据流（Actor 化后）

```
TUI 输入
  → AcpTuiClient.prompt()
  → MpscTransport.send_request("session/prompt")
  → executor::execute_prompt()                    ← 不变
    → build_agent() → ReActAgent.execute()        ← 不变
      → ReAct 循环: LLM → tool_calls             ← 不变
        → collect_tool_results → dispatch_tools   ← 延迟写入不变
        → SubAgentTool.invoke():
            → SessionActor.tell(SpawnSubAgent)     ← 新：Actor spawn
              → SubAgentActor.tell(Execute)
                → 内部 ReActAgent.execute()        ← 直接调用，不变
                → EventSink.push_event()           ← 不变
                → SubAgentCompleted 回传 SessionActor
        → 其他工具: Read/Grep/Bash...              ← 不变
      → ExecutorEvent → EventSink → Transport → TUI ← 不变
```

**与当前数据流的差异（仅 SubAgent 路径）**：
1. `tokio::spawn` → `SubAgentActor::spawn()`（Kameo 管理）
2. `source_agent_id` → `ActorRef` 类型安全路由
3. `BackgroundTaskRegistry` → `SessionActor.background_agents`
4. 其余路径完全不变

### 迁移策略

**Phase 0 — 验证原型（1-2 天）**

- `cargo add kameo@0.20` 到 `peri-middlewares`
- 写最小 SubAgentActor 原型：spawn + 执行 + on_stop 通知
- 验证 Kameo 0.20 API 可行性（`Actor` trait、`on_start`、`link_died`、`ActorRef` Clone+Send+Sync）
- **不改任何现有代码**，纯实验分支

**Phase 1 — SubAgentActor**

- 将 `SubAgentTool` 的 4 条路径统一为 `SubAgentActor`
- 引入 `SessionActor` 仅管理 SubAgent 生命周期
- `EventSink`、`CancellationToken`、`tool_dispatch` 保持不变
- **不改 ACP/TUI 层**，Transport 协议不变
- 全量回归测试

**Phase 2 — 清理**

- 移除 `BackgroundTaskRegistry`
- 移除 `source_agent_id` 字符串路由相关代码
- 统一中间件构造（消除 4 条路径样板代码）
- 全量回归测试

**不再推进的部分（除非有新的明确收益）**：

- ~~AgentActor~~：ReAct 循环是同步顺序循环，零并发收益
- ~~ToolPoolActor~~：延迟写入是语义约束，mailbox 串行化不能替代
- ~~EventBridgeActor~~：EventSink trait 作为稳定接口继续存在
- ~~SessionActor 替代 SessionState~~：Phase 3 的 TUI 改动范围远超文档预期

### 过渡期约束

- Phase 0-1 期间，Actor（SubAgent）和现有 `tokio::spawn`（其他并发）共存
- SubAgentActor 内部仍使用 `CancellationToken` 管理 cancel，不依赖 Actor 消息传递
- SubAgentActor 仍通过 `Arc<dyn EventSink>` 发送事件，不引入 EventBridgeActor
- 所有 Actor 内部的 panic 通过 `on_stop`/`on_link_died` 统一转发到 `tracing::error`
- 新增并发逻辑必须选择一种模型（Actor 或 tokio::spawn），禁止在同一功能中混用
- 每个 Phase 完成后全量回归测试

## 后果

### 正向

- SubAgent 并发事件路由从字符串匹配升级为类型安全，消除 ID 冲突和事件溢出类 bug
- SubAgent 4 条路径样板代码统一为单个 Actor Handler
- Background Agent 生命周期从手动 registry + RAII guard 简化为 supervision + HashMap
- frozen_vm 清理从手动 `retain()` 变为 Actor `on_stop` 自动处理

### 负向

- 引入新的外部依赖（kameo crate），需持续跟踪上游变更（当前 0.20，API 仍在演化）
- 团队需要学习 Actor 模型思维方式
- Phase 0-1 期间 SubAgent 用 Actor、其他并发用 tokio::spawn，两套模型共存
- kameo-macros 编译时间增量（预计 <10%，因 syn/quote 已在依赖树中）

### 风险缓解

- **Phase 0 原型验证**：在修改任何生产代码前，用最小原型确认 Kameo API 可行性
- **范围限定**：只 Actor 化 SubAgent + 共享资源管理，不动 ReAct 循环、延迟写入、compact 等同步语义组件
- **EventSink 保持不变**：Actor 化不改变 ACP/TUI 的消费接口
- **CancellationToken 保持不变**：Cancel 仍走现有 token 机制
- **可替换性**：Kameo 底层是 tokio::spawn + mpsc，最坏情况下可 fork 并自行维护

## 审查记录

本 ADR 经 4 视角审查，主要修改：

| 审查意见 | 处理 |
|---------|------|
| ReAct 循环不适合 Actor 化 | ✅ 接受，移除 AgentActor，SubAgent 内部直接调用 ReActAgent.execute() |
| 延迟写入是语义约束不是并发缺陷 | ✅ 接受，不再声称 mailbox 替代延迟写入 |
| Compact 不适合跨 Actor 消息传递 | ✅ 接受，compact 保留为 Agent 内部操作 |
| Cancel 需要保持 CancellationToken | ✅ 接受，SubAgentActor 仍使用 CancellationToken |
| Middleware chain 不适合消息传递 | ✅ 接受，middleware 保持同步方法调用 |
| Kameo 0.14 API 已过时 | ✅ 接受，更新为 0.20+，标注代码为伪代码 |
| Phase 依赖有隐性循环 | ✅ 接受，EventBridgeActor 移除，SubAgent 直接使用 EventSink |
| 遗漏跨 session 共享组件 | ✅ 接受，补充"共享资源层"表格 |
| EventBridgeActor 性能瓶颈 | ✅ 接受，移除 EventBridgeActor，保持现有 EventSink |
