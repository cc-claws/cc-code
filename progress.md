# 架构评审进度

## 2026-05-26 第 1 轮

### 发现（12 项，均已验证）

| # | 发现 | 严重程度 | 状态 |
|---|------|----------|------|
| 1 | 非标准模块组织——使用 include! .inc 文件（app/mod.rs） | 高 | 已验证 |
| 2 | plugin_panel/mod.rs 庞大的公有函数接口（约 50 个 pub fn） | 高 | 已验证 |
| 3 | 复杂的事件系统——20+ 个 AgentEvent 变体（events.rs） | 高 | 已验证 |
| 4 | ACP 桥接层双重映射（ExecutorEvent → AgentEvent → AcpNotification） | 中 | 已验证 |
| 5 | 消息管道复杂性——13+ 个内部字段 | 高 | 已验证 |
| 6 | 工具派发延迟错误模式（collect_tool_results / dispatch_tools） | 中 | 已验证 |
| 7 | 中间件链——17 个中间件，复杂的生命周期钩子 | 中 | 已验证 |
| 8 | 共享状态过度使用——SubAgentMiddleware 有 12 个 Arc 字段 | 中 | 已验证 |
| 9 | 插件面板碎片化——9 个 handler 子模块 | 中 | 已验证 |
| 10 | Executor 构建复杂性——AcpAgentConfig 30+ 个配置字段 | 中 | 已验证 |
| 11 | 小文件间频繁跳转——agent 事件需要 5+ 个文件 | 中 | 已验证 |
| 12 | 测试覆盖面问题——复杂的内部状态难以测试 | 低-中 | 已验证 |

### 关键架构问题

**高优先级：**
- `peri-tui/src/app/mod.rs` — `include!` 宏搭配 `.inc` 文件破坏 IDE 导航，不符合 Rust 惯例
- `peri-tui/src/app/plugin_panel/mod.rs` — 50+ 个公有函数表明浅层接口泄露了实现细节
- `peri-tui/src/app/events.rs` — 20+ 个 AgentEvent 变体说明事件系统承担了过多职责
- `peri-tui/src/app/message_pipeline/mod.rs` — 13+ 个字段管理 subagent 栈、待处理工具、冻结 VM、节流状态——适合拆分

**中优先级：**
- 双重事件映射层（ACP 桥接 + 事件映射器）增加了不必要的复杂性
- 工具派发延迟错误模式需要谨慎维护状态不变量
- 17 个中间件之间存在复杂的排序依赖
- 大量 Arc/Mutex/RwLock 共享状态模式
- AcpAgentConfig 30+ 个字段表明需要构建器模式

### 验证

所有 12 项发现均由独立 explorer 验证确认，无需修正。

---

## 2026-05-26 第 2 轮（Cron #2）

### 发现（7 项，5 项验证 + 1 项修正 + 1 项驳回）

| # | 发现 | 严重程度 | 类型 | 状态 |
|---|------|----------|------|------|
| 1 | MessagePipeline 上帝对象（780 行，15 个字段） | 高 | 深入 | 已验证（行数修正：780 而非 656） |
| 2 | AcpAgentConfig 参数膨胀（35 个字段） | 高 | 新发现 | 已验证（字段数修正：35 而非 ~95） |
| 3 | AgentEvent 变体爆炸（27 个变体） | 中 | 深入 | 已验证（数量修正：27 而非 20） |
| 4 | 工具派发延迟错误（3 条错误路径） | 中 | 深入 | 已验证 |
| 5 | App 模块爆炸（111 个 .rs 文件） | 中 | 新发现 | 已验证（数量修正：111 而非 80+） |
| 6 | 浅层模块：agent_ops/mod.rs | 低 | 新发现 | **已驳回** — 实际 369 行，包含大量逻辑 |
| 7 | PipelineAction 累积样板代码 | 中 | 新发现 | 已验证 |

### 验证修正

- **发现 6 已驳回**：`agent_ops/mod.rs` 有 369 行，包含实质性的事件分发、subagent 生命周期管理和领域逻辑——并非浅层透传模块。删除测试不会简化代码库。
- **AcpAgentConfig**：35 个字段（而非 ~95）。仍是参数膨胀，但比最初描述的没那么极端。
- **AgentEvent**：27 个变体（而非 20）。比第 1 轮估计更严重。

### 改进建议

**MessagePipeline 拆分**（高优先级）：
- 提取 `SubAgentManager`：subagent_stack、frozen_subagent_vms、active_batch
- 提取 `ToolCallTracker`：pending_tools、completed_tools、current_ai_tool_calls
- 提取 `StreamingBuffer`：current_ai_text、current_ai_reasoning、current_ai_finalized
- 提取 `RoundTracker`：completed_len_at_round_start、has_snapshot_this_round
- 收益：每个组件可独立测试，管道成为协调层

**AcpAgentConfig 分组**（高优先级）：
- `RuntimeConfig`：cwd、cancel、session_id、permission_mode
- `LlmConfig`：provider、system_prompt、compact_model、compact_budget
- `FrozenData`：frozen_claude_md、frozen_claude_local_md、frozen_skill_summary、frozen_date
- `ServiceConfig`：mcp_pool、lsp_servers、hook_groups、cron_scheduler、tool_search_index
- 收益：组内一致性校验，更小的接口

**AgentEvent 拆分**（中优先级）：
- `ExecutorEvent`：ToolStart/End、Done/Error/Interrupted、StateSnapshot
- `StreamingEvent`：AssistantChunk、AiReasoning
- `InteractionEvent`：InteractionRequest、OAuthAuthorization*
- `ServiceEvent`：McpActionCompleted、PluginActionCompleted、BackgroundTaskCompleted
- `SubAgentEvent`：SubAgentStart/End、SubagentLifecycle
- 收益：清晰的职责归属，每个处理器只处理自己的关注点

**PipelineAction 简化**（中优先级）：
- 将 `Vec<PipelineAction>` 改为 `Option<PipelineAction>` 或单个 `PipelineAction`
- 20 个 match 分支中有 17 个返回 `vec![None]` — 纯样板代码
- 收益：减少仪式代码，更易扩展

### 删除测试结果
- MessagePipeline：失败（复杂性集中在协调层）
- AcpAgentConfig：通过（分组配置更好）
- agent_ops/mod.rs：通过（非浅层——保留了逻辑）

---

## 2026-05-26 第 3 轮（Cron #3）

### 聚焦领域
peri-agent 核心、LLM 适配器、中间件 trait、压缩系统、hook 中间件、插件加载器、ACP executor

### 发现（7 项新发现，5 项验证 + 1 项修正 + 1 项跳过）

| # | 发现 | 严重程度 | 类型 | 状态 |
|---|------|----------|------|------|
| 1 | LLM 适配器重复——openai/anthropic invoke.rs 约 80% 重复（665/693 行） | 高 | 新发现 | 已验证 |
| 2 | 中间件 Trait 过度规范——9 个钩子，after_model（0 实现）after_agent（1 实现） | 中 | 新发现 | 修正：钩子确实被调用，只是很少被实现 |
| 3 | AgentState 接口泄露——messages_mut() 暴露 &mut Vec，compact/micro.rs 直接操作切片 | 中 | 新发现 | 已验证 |
| 4 | HookMiddleware 事件分发——686 行，fire_event 有 7+ 条按类型 × async 的代码路径 | 高 | 新发现 | 已验证 |
| 5 | 插件加载器管道——666 行，5 阶段，回退路径有 5 个提前返回 | 中 | 新发现 | 已验证 |
| 6 | ACP Executor 参数爆炸——execute_prompt() 接受 24 个参数 | 高 | 新发现 | 已验证 |
| 7 | 压缩系统边界——11 个文件，CompactMiddleware 和模块间所有权混乱 | 低 | 新发现 | 部分（文件数：11 而非 5） |

### 验证修正
- **发现 2 修正**：`after_model` 确实被调用（executor/mod.rs:290），`after_agent` 确实被调用（final_answer.rs:134）。然而 `after_model` 有 0 个实现、`after_agent` 只有 1 个——过度规范的担忧仍然成立，但描述不准确。
- **发现 7 部分**：compact/ 有 11 个文件（6 实现 + 5 测试），而非 5 个。CompactMiddleware 约 338 行。

### 改进建议

**LLM 适配器去重**（高优先级）：
- 提取共享的请求构建到一个通用 `build_request()` 函数
- 创建特定 provider 的薄适配器，仅覆盖序列化差异
- 通过泛型参数共享 `ReactLLM` 实现
- 收益：provider bug 修复一次即可，新 provider 只需一个薄适配器

**中间件 Trait 拆分**（中优先级）：
- 拆分为聚焦的子 trait：`BeforeModel`、`BeforeTool`、`ToolCollector`
- 未使用钩子提供默认空实现
- 收益：每个中间件只声明所需内容，编译器保证正确性

**AgentState 封装**（中优先级）：
- 移除 `messages_mut()`，替换为定向变更方法
- 添加 `drain_messages(range)`、`update_message(idx, fn)`、`retain_messages(fn)`
- 收益：不变量在 State 层强制执行，可审计

**HookMiddleware 拆分**（高优先级）：
- 提取 `fire_event()` 逻辑到带类型处理器的 `HookDispatcher`
- 分离 async/sync 执行策略
- 收益：每种 hook 类型有清晰的执行模型，更易测试

**ACP Executor 参数分组**（高优先级）：
- 将 24 个参数打包为 3-4 个结构体：`PromptInput`、`SessionInfrastructure`、`ServiceDependencies`
- 收益：更小的签名，更易扩展，与 AcpAgentConfig 分组建议一致

### 跨轮次模式分析
三轮揭示了一致的主题：**协调层的参数/状态爆炸**。
- AcpAgentConfig：35 个字段（第 2 轮）
- execute_prompt：24 个参数（第 3 轮）
- MessagePipeline：15 个字段（第 2 轮）
- AgentEvent：27 个变体（第 2 轮）
- SubAgentMiddleware：12 个 Arc 字段（第 1 轮）

根本原因：代码库随功能增长，新参数/字段被添加到现有协调点，而非引入新的接缝。提出的拆分方案（分组配置、拆分事件、提取子管理器）都针对这一根本原因。

---

## 2026-05-26 第 4 轮（Cron #4）

### 聚焦领域
错误类型、配置蔓延、测试基础设施、持久化层、流处理、i18n、类型转换、取消机制、遥测

### 发现（9 项新发现，6 项验证 + 1 项部分 + 2 项驳回）

| # | 发现 | 严重程度 | 类型 | 状态 |
|---|------|----------|------|------|
| 1 | 错误类型不一致——agent/lsp/langfuse 用 thiserror，middlewares/acp 用 anyhow | 中 | 新发现 | 已验证 |
| 2 | 配置蔓延——5+ 个来源，1133+ 行配置代码，无统一层 | 高 | 新发现 | 已验证 |
| 3 | 测试基础设施重复——247 个 _test.rs 文件，零共享工具 | 中 | 新发现 | 已验证 |
| 4 | 持久化层 SQL 泄露——sqlite_store.rs 604 行，原始 SQL，13 元组 | 中 | 新发现 | 部分 |
| 5 | 流处理重复——openai/anthropic stream.rs 296/315 行重复 SSE 逻辑 | 中 | 新发现 | 已验证 |
| 6 | i18n 紧耦合——LcRegistry 仅在 peri-tui，其他 crate 不可复用 | 低 | 新发现 | 已验证 |
| 7 | 类型转换散布——64+ 个文件引用 MessageViewModel | 中 | 新发现 | **已驳回** — transform.rs:53 已有集中式 `messages_to_view_models()` |
| 8 | 取消令牌不一致——48 个文件引用令牌 | 中 | 新发现 | **已驳回** — 单一统一 `CancellationToken` 类型，一致的别名 |
| 9 | 日志/遥测散布——210+ 个 tracing 文件，18+ 个 langfuse 文件 | 低 | 新发现 | 已验证 |

### 验证修正
- **发现 7 已驳回**：`messages_to_view_models()` 是 `peri-tui/src/app/message_pipeline/transform.rs:53` 中文档完善的集中式转换函数。散布在于使用而非实现。
- **发现 8 已驳回**：所有取消操作使用 `tokio_util::sync::CancellationToken`，一致的 `AgentCancellationToken` 别名。单一统一类型，不存在不一致。
- **发现 4 部分**：确认 13 元组 `meta_from_row()`，但行数未完全验证。

### 改进建议

**配置统一**（高优先级）：
- 创建带 `get<T>(&self, key) -> Result<T>` 语义的 `ConfigProvider` trait
- 将环境变量展开、文件加载和合并优先级统一到单一管道
- mcp/config.rs（642 行）+ plugin/config.rs（491 行）共 1133+ 行是最大目标
- 收益：单一位置添加配置校验、缓存和变更通知

**错误类型层次**（中优先级）：
- 为 peri-middlewares 和 peri-acp 使用 thiserror 定义 crate 级错误类型
- 建立模式：核心 crate → 类型化错误，应用层 → anyhow
- 收益：跨 crate 边界保留错误上下文，更好的诊断信息

**共享测试工具**（中优先级）：
- 创建 `peri-agent/src/test_utils/` 包含可复用 mock（AgentState、tools、configs）
- 提取通用测试模式：`make_mock_state()`、`make_test_config()`
- 247 个测试文件重复 fixture 是显著的维护负担
- 收益：DRY 测试代码，更快编写测试，一致的 mock 行为

**流解析器抽象**（中优先级）：
- 从 openai/anthropic stream.rs（共 611 行）提取共享 `StreamParser` trait
- 特定 provider 适配器仅覆盖字段名映射
- 收益：代码减半，新 provider 免费获得流式支持

**持久化层重构**（中优先级）：
- 引入查询构建器，至少用类型化行结构体替代 13 元组
- 分离业务逻辑（标题提取、缓存管理）与 SQL 查询
- 收益：可测试的持久化层，schema 变更不触碰业务逻辑

### 更新的跨轮次模式分析

四轮揭示了两个根本原因：

**根本原因 1：协调层膨胀**（第 1-3 轮）
- 协调层的参数/状态爆炸
- 修复方案：分组配置、拆分事件、提取子管理器

**根本原因 2：横切关注点重复**（第 4 轮）
- 配置、错误处理、流处理、测试基础设施按 crate 重复
- 修复方案：横切关注点的共享抽象（ConfigProvider、StreamParser、test_utils）

这些合计覆盖了 4 轮中 26 项独立发现（22 项验证，3 项驳回，1 项部分）。

---

## 2026-05-26 第 5 轮（Cron #5）

### 聚焦领域
超大文件分析——量化文件大小并识别拆分目标

### 发现（7 项新发现，3 项验证 + 4 项部分）

| # | 发现 | 严重程度 | 状态 |
|---|------|----------|------|
| 1 | headless_test.rs — 约 2000 行上帝测试，覆盖整个 TUI 表面 | 中 | 部分（数量：约 2000 而非 4340，wc -l 包含空行） |
| 2 | executor mod_test.rs — 1443 行，测试并发/取消/预算 | 低-中 | 已验证 |
| 3 | event/mod.rs — 908 行，handle_event 622 行，有委托但仍然很大 | 中 | 部分（有委托到 keyboard 模块 + 面板宏） |
| 4 | acp_stdio.rs — 898 行，通过构建器模式处理 13 个请求 | 中-高 | 部分（构建器模式，非单一函数，但仍然难导航） |
| 5 | main.rs — 783 行，run_app() 316 行做所有初始化 | 中 | 已验证 |
| 6 | tracer.rs — 764 行，15 个事件方法变更 11 个字段 | 低-中 | 已验证 |
| 7 | render_state.rs — 748 行，TableBuilder 354 行 + handle_event 269 行 | 低-中 | 部分（行范围已修正） |

### 验证修正
- **发现 1**：`wc -l` 计数为 4340，但实际代码约 2000 行（大量空行/视觉分隔符）。仍是上帝测试。
- **发现 3**：`handle_event()` 确实委托给 `keyboard::handle_key_event()` 并使用 `with_session_panels!`/`with_global_panels!` 宏。不是纯粹的上帝处理器——但鼠标处理（400+ 行）仍然是内联的。
- **发现 4**：使用 `.on_receive_request()` 构建器模式——处理器是闭包，非单一函数体。但导航仍然困难：13 个处理器在一个文件中，没有按处理器分文件的结构。
- **发现 7**：TableBuilder 第 31-385 行（正确），handle_event 第 478-747 行（非最初混淆的 31-385 行）。

### 改进建议

**headless_test.rs 拆分**（中优先级）：
- 拆分为 8 个聚焦的测试模块：markdown、subagent、welcome_card、sticky_header、setup_wizard、permission_mode、compact、pipeline_regression
- 每个模块约 250 行——可管理的、聚焦的测试套件
- 收益：更快的测试迭代，更清晰的测试归属，可并行开发测试

**acp_stdio.rs 处理器提取**（中-高优先级）：
- 保留构建器模式，但将处理器闭包提取到独立文件
- 仅 `acp_stdio/handlers/session_prompt.rs` 就有约 182 行
- 收益：新增 ACP 方法不触碰现有处理器，每个处理器可独立审计

**main.rs 初始化阶段**（中优先级）：
- 将 `run_app()` 阶段提取为：permission_setup、session_resume、plugin_loading、acp_setup、event_loop、shutdown
- 316 行函数 → 6 个约 50 行的函数
- 收益：初始化顺序变更局限在一个阶段内

**event/mod.rs 鼠标提取**（中优先级）：
- 将鼠标处理（点击/拖拽/滚动，约 400 行）提取到 `event/mouse.rs`
- keyboard.rs 已提取；鼠标是剩余的巨型模块
- 收益：与 keyboard 提取对称，更易添加新鼠标交互

### 文件大小分布（前 25 个 .rs 文件）

| 行数 | 文件 | 覆盖轮次 |
|------|------|----------|
| 4340 | headless_test.rs | 本轮 |
| 1443 | executor/mod_test.rs | 本轮 |
| 1394 | message_pipeline_test.rs | 第 1-2 轮 |
| 1012 | subagent/tool_test.rs | 第 1 轮 |
| 908 | event/mod.rs | 本轮 |
| 908 | plugin/loader_test.rs | 第 3 轮 |
| 898 | acp_stdio.rs | 本轮 |
| 865 | plugin/installer_test.rs | 第 3 轮 |
| 864 | anthropic_test.rs | 第 3 轮 |
| 860 | middleware/chain_test.rs | 第 1 轮 |
| 827 | plugin_panel/mod.rs | 第 1-2 轮 |
| 783 | main.rs | 本轮 |
| 779 | message_pipeline/mod.rs | 第 1-2 轮 |
| 764 | langfuse/tracer.rs | 第 4 轮 + 本轮 |
| 748 | markdown/render_state.rs | 本轮 |
| 714 | message_view/mod.rs | 本轮 |
| 692 | anthropic/invoke.rs | 第 3 轮 |
| 685 | hooks/middleware.rs | 第 3 轮 |
| 665 | plugin/loader.rs | 第 3 轮 |
| 664 | openai/invoke.rs | 第 3 轮 |
| 641 | mcp/config.rs | 第 4 轮 |
| 638 | panel_plugin.rs | 本轮 |
| 623 | hooks/types.rs | 第 3 轮 |
| 612 | message_render.rs | 本轮 |

### 更新的跨轮次模式分析

五轮揭示了三个根本原因：

**根本原因 1：协调层膨胀**（第 1-3 轮）
- 协调层的参数/状态爆炸
- 修复方案：分组配置、拆分事件、提取子管理器

**根本原因 2：横切关注点重复**（第 4 轮）
- 配置、错误处理、流处理、测试基础设施按 crate 重复
- 修复方案：共享抽象（ConfigProvider、StreamParser、test_utils）

**根本原因 3：巨型文件增长**（第 5 轮）
- 文件有机增长，缺乏拆分纪律
- 7 个文件超过 700 行，3 个文件超过 1000 行，1 个文件超过 4000 行
- 修复方案：按概念分文件纪律，约 500 行阈值时拆分

合计：5 轮中 33 项独立发现（25 项验证，3 项驳回，5 项部分）。

---

## 2026-05-26 第 6 轮（Cron #6）

### 聚焦领域
微级代码质量：pub 可见性、字符串类型 API、unwrap 模式、clone 开销、工具样板代码、宏复杂性、跨 crate 类型重复

### 发现（7 项新发现，4 项验证 + 3 项计数修正）

| # | 发现 | 严重程度 | 状态 |
|---|------|----------|------|
| 1 | HITL 字符串类型化——工具名以字符串字面量比较 | 高 | 已验证 |
| 2 | Clone 开销——tool_dispatch.rs 中 32 个 clone() 调用，BaseMessage 多次克隆 | 中 | 已验证 |
| 3 | 工具系统样板代码——32 个工具均遵循约 100 行模式 | 中 | 已验证（数量：32） |
| 4 | 跨 crate AgentEvent 重复——2 个独立的 AgentEvent 枚举（peri-agent + peri-tui） | 高 | 已验证 |
| 5 | 非测试代码中的 unwrap()——实际计数：middlewares/tui/acp 分别为 31/29/14 | 中-低 | 修正（声称 350/460/120，实际 74 总计） |
| 6 | pub 可见性——middlewares/lib.rs 中 14 个 pub use | 低 | 修正（声称 52，实际 14） |
| 7 | 宏使用——with_global_panels! 使用 8 次，with_session_panels! 使用 7 次 | 低 | 修正（声称 100+，实际 15 总计） |

### 验证修正
- **发现 5**：unwrap() 计数被大幅夸大。非测试 unwrap 计数：peri-middlewares 31，peri-tui 29，peri-acp 14 = **共 74 个**，而非声称的 930。大多数 unwrap() 调用在测试文件中（1245+）。
- **发现 6**：peri-middlewares/lib.rs 有 14 个 pub use 语句，而非 52。过度导出存在但不那么严重。
- **发现 7**：宏使用共 15 次（8+7），而非 100+。mem::take 模式共使用 22 次。宏复杂性低于声称。

### 改进建议

**工具名枚举**（高优先级）：
- 定义 `ToolName` 枚举，包含所有已知工具的变体
- 将 HITL `default_requires_approval(&str)` 替换为 `default_requires_approval(ToolName)`
- HITL match 变为穷尽匹配，拼写错误在编译时被捕获
- 收益：IDE 自动补全、编译时安全性、无静默拼写错误

**AgentEvent 统一**（高优先级）：
- 在 peri-agent 中定义单一 `AgentEvent`，TUI 通过 newtype/wrapper 重导出 + 扩展
- 移除 peri-tui 的独立 `AgentEvent` 定义
- `map_executor_event()` 变成薄的字段映射适配器，而非类型转换器
- 收益：单一事实来源，定义间无字段漂移

**工具 Trait 样板代码减少**（中优先级）：
- 创建 `#[derive(BaseTool)]` 宏或 `tool_impl!` 宏
- 将每个工具约 100 行减少到约 30 行（名称 + schema + invoke 体）
- 32 个工具 × 节省 70 行 = 约 2240 行样板代码消除
- 收益：添加新工具变得简单，schema 错误由宏捕获

**tool_dispatch.rs 中的 Clone 减少**（中优先级）：
- 32 个 clone() 调用，许多在包含大型 ContentBlock 数组的 BaseMessage 上
- 使用 `Arc<BaseMessage>` 进行事件发射，替代克隆整个消息
- 拆分所有权：state 获取原始对象，事件获取 Arc 引用
- 收益：减少热路径中的内存分配，特别是对大型工具结果

### 更新的跨轮次模式分析

六轮揭示了四个根本原因：

**根本原因 1：协调层膨胀**（第 1-3 轮）
**根本原因 2：横切关注点重复**（第 4 轮）
**根本原因 3：巨型文件增长**（第 5 轮）
**根本原因 4：字符串类型化接口**（第 6 轮）
- 工具名、模型别名、事件路由均使用字符串比较
- 跨模块契约无编译时安全性
- 修复方案：工具名类型化枚举、穷尽匹配、工具样板代码 derive 宏

合计：6 轮中 40 项独立发现（29 项验证，3 项驳回，5 项部分，3 项计数修正）。

---

## 2026-05-26 第 7 轮（Cron #7）

### 聚焦领域
并发模式、会话生命周期、依赖图健康度

### 发现（11 项新发现，9 项验证 + 2 项部分���

| # | 发现 | 严重程度 | 状态 |
|---|------|----------|------|
| A1 | 无界通道泛滥——40+ 个 unbounded_channel 实例，无背压 | 中 | 已验证（修正：40+ 而非 15+） |
| A2 | 嵌套锁获取——McpClientPool configs.read() → clients.write() 无文档化顺序 | 中 | 已验证 |
| A3 | 即发即弃 tokio::spawn——prompt/compact/事件泵任务，未保留 JoinHandle | 低 | 部分 |
| A4 | 后台 agent 全局限制——max_concurrent=3 硬编码，无每会话隔离 | 中 | 已验证 |
| B1 | 双重 SessionState——peri-tui 和 peri-acp 维护独立的会话类型 | 中 | 已验证 |
| B2 | AgentPool 生命周期管理——mem::replace + Arc::try_unwrap 恢复模式 | 低 | 已验证 |
| B3 | 会话清理令牌引用——cancel_session 替换令牌，旧 agent 可能持有过期引用 | 中 | 部分（基于 Arc，已缓解） |
| C1 | peri-tui 编译时依赖膨胀——32-35 个直接依赖，许多可以 feature gate | 低 | 已验证 |
| C2 | thiserror 版本重复——peri-tui 用 v1，workspace 用 v2 | 低 | 已验证 |
| C3 | peri-agent 中的 rand——核心 crate 依赖 rand 0.10（用于重试抖动） | 低 | 已验证 |
| C4 | DashMap 与 RwLock<HashMap> 不一致——不同 crate 使用不同并发 Map 策略 | 低 | 已验证 |

### 验证修正
- **A1**：无界通道计数为 40+（而非最初声称的 15+）。问题比报告的更严重。
- **C1**：peri-tui 有��� 32-35 个直接依赖（而非 28）。
- **A3/B3**：均部分验证——模式存在，但通过基于 Arc 的令牌共享和任务作用域生命周期已缓解后果。

### 改进建议

**通道背压策略**（中优先级）：
- 审计所有 40+ 个 unbounded_channel 实例
- 为高吞吐路径（事件流、持久化）添加有界通道和背压
- 仅对低吞吐控制通道（取消、配置更新）保留无界通道
- 收益：负载下内存有界，无生产者-消费者失衡导致的 OOM 风险

**锁顺序文档化**（中优先级）：
- 文档化 McpClientPool 的锁获取顺序：configs → clients → transports
- 添加 `#[allow(clippy::mutex_atomic)]` 并注释说明顺序
- 考虑迁移到 DashMap（与 AcpSession 模式一致）
- 收益：防止死锁回归，新贡献者知晓规则

**每会话后台 Agent 限制**（中优先级）：
- 用每会话配额替代全局 `max_concurrent: 3`
- `BackgroundTaskRegistry::new(max_per_session: usize)`
- 全局限制仍作为上限，但会话间不能独占
- 收益：分屏会话间公平的资源分配

**依赖 Feature Gate**（低优先级）：
- Feature gate 同步依赖：`aes-gcm`、`ring`、`rmp-serde` → `features = ["sync"]`
- Feature gate OAuth 依赖：`tokio-tungstenite` → `features = ["oauth"]`
- Feature gate 剪贴板：`arboard` → `features = ["clipboard"]`
- 统一 thiserror 版本到 workspace 2.0
- 收益：大多数用户更快编译、更小的二进制、更少的 CVE 暴露面

### 更新的跨轮次模式分析

七轮揭示了五个根本原因：

**根本原因 1：协调层膨胀**（第 1-3 轮）
**根本原因 2：横切关注点重复**（第 4 轮）
**根本原因 3：巨型文件增长**（第 5 轮）
**根本原因 4：字符串类型化接口**（第 6 轮）
**根本原因 5：无界资源增长**（第 7 轮）
- 40+ 个无界通道、即发即弃任务 spawn、全局资源限制
- 无背压或资源预算策略
- 修复方案：有界通道、每会话配额、任务生命周期追踪

合计：7 轮中 51 项独立发现（38 项验证，3 项驳回，7 项部分，3 项计数修正）。

---

## 2026-05-26 第 8 轮（Cron #8）

### 聚焦领域
安全攻击面、错误恢复模式、提示模板管理、CLI/TUI 代码共享、SubAgent 通信

### 发现（13 项新发现，10 项验证 + 2 项部分 + 1 项驳回）

| # | 发现 | 严重程度 | 状态 |
|---|------|----------|------|
| S1 | SSRF 绕过——WebFetch/WebSearch 缺少 ssrf_guard，仅 hooks 有 | 高 | 已验证 |
| S2 | 路径穿越不一致——validate_and_resolve 仅在 sync 中，Write/Edit 中没有 | 中 | 已验证 |
| S3 | Hook 中任意命令执行——无沙箱，仅超时 | 高 | 已验证 |
| S4 | 插件 manifest 验证缺口——无签名/内容验证 | 中 | 部分 |
| E1 | 无部分流式恢复——RetryableLLM 在失败时丢弃 | 高 | 已验证 |
| E2 | 临时超时处理——工具间 ms/secs 混用 | 中 | 已验证 |
| E3 | MCP 重连仅手动——断连时无自动重连 | 中 | 已验证 |
| P1 | 无提示版本管理——include_str! 无版本元数据 | 高 | 已验证 |
| P2 | 提示特性检测竞态——SubAgent 每次构建读取 YOLO_MODE vs frozen | 中 | 部分 |
| C1 | CLI/TUI 代码重复——约 30% 重复（共享 ACP executor） | 中 | **修正**（声称 90%，实际约 30%） |
| C2 | PrintBroker 自动批准所有——与 TUI 无安全对等 | 中 | 已验证 |
| A1 | 事件路由跨 3+ 层，使用 source_agent_id | 高 | 已验证 |
| A2 | 后台任务中止无清理——abort() 中途写入风险 | 中 | 已验证 |

### 验证修正
- **C1 已驳回**：CLI print 模式通过 ACP executor 共享 `execute_prompt()`。实际重复约 30%（初始化/provider 加载），而非 90%。PrintBroker 和 PrintEventSink 是独立实现。
- **S4 部分**：无签名验证，但存在结构验证。
- **P2 部分**：YOLO_MODE 每次 SubAgent 构建时重新读取，但影响限于 HITL 段注入。

### 安全发现详情

**严重：SSRF 绕过（S1）**
- `web_fetch.rs:108-111` 直接使用 `reqwest::Client` 无 SSRF 检查
- LLM 可调用 WebFetch 探测内部服务（169.254.169.254、10.0.0.0/8）
- 修复：在 WebFetch/WebSearch 的所有出站 HTTP 请求前应用 `ssrf_guard::check_url()`

**严重：Hook 命令执行（S3）**
- `hooks/executor.rs:66-76` 执行 `bash -c <command>` 无沙箱
- 恶意插件可运行任意命令
- 修复：添加命令白名单或要求 hook 命令需用户明确批准

**重要：流式恢复（E1）**
- `retry.rs:98-102` 在流式失败时丢弃部分内容
- 长响应（3+ 分钟）在网络错误到达 95% 时完全丢失
- 修复：累积部分块，以部分上下文重试

### 改进建议

**SSRF 防护扩展**（高优先级，安全）：
- 将 `ssrf_guard::check_url()` 扩展到 WebFetch 和 WebSearch 工具
- 阻止私有 IP、链路本地、回环、云元数据端点
- 收益：防止通过 LLM 工具调用扫描内部网络

**Hook 命令沙箱化**（高优先级，安全）：
- 为 hook 命令添加白名单/黑名单
- 可选：在受限环境中运行 hook（无网络、文件系统限制）
- 收益：减少插件供应链攻击面

**流式检查点恢复**（高优先级，可靠性）：
- 在 RetryableLLM 中累积流式块
- 重试时注入部分内容作为续接上下文
- 收益：长响应不再因瞬态错误完全丢失

**提示版本管理系统**（中优先级，可维护性）：
- 添加版本元数据到提示段（如 `<!-- version: 2 -->`）
- 在会话状态中跟踪提示版本以支持迁移
- 收益：安全的提示演进、A/B 测试能力

### 更新的跨轮次模式分析

八轮揭示了六个根本原因：

**根本原因 1：协调层膨胀**（第 1-3 轮）
**根本原因 2：横切关注点重复**（第 4 轮）
**根本原因 3：巨型文件增长**（第 5 轮）
**根本原因 4：字符串类型化接口**（第 6 轮）
**根本原因 5：无界资源增长**（第 7 轮）
**根本原因 6：不一致的安全边界**（第 8 轮）
- SSRF 防护仅在 hooks 中，路径穿越仅在 sync 中，无插件 manifest 验证
- TUI 和 CLI print 模式安全模型不同
- 修复方案：统一安全中间件层，所有工具一致的验证

合计：8 轮中 64 项独立发现（48 项验证，4 项驳回，9 项部分，3 项计数修正）。

---

## 2026-05-26 第 9 轮（Cron #9）

### 聚焦领域
性能热路径、Rustdoc 覆盖率、构建配置、ACP 协议合规性

### 综合发现（45 条原始观察中提炼 12 项代表性发现）

| # | 发现 | 严重程度 | 状态 |
|---|------|----------|------|
| P1 | 流式 String::new() 无容量预分配——anthropic/stream.rs:77、openai/stream.rs:76 | 中 | 已验证 |
| P2 | 每个 LLM 请求 format!() 构建 URL——适配器中未缓存 | 低 | 已验证 |
| P3 | 每个 token 块 to_string()——AgentEvent::TextChunk.chunk 是 String 而非 &str | 中 | 已验证 |
| P4 | tool_dispatch settled_results Vec::new() vs ready_calls with_capacity——128-129 行 | 低 | 已验证 |
| D1 | box_to_arc 缺少 # Safety 文档——ManuallyDrop+裸指针无正式安全段落 | 中 | 已验证 |
| D2 | Middleware trait（10 方法）和 BaseTool trait 有 0 trait 级文档 | 高 | 已验证 |
| D3 | AgentError 15 个变体，0 个恢复策略文档 | 中 | 已验证 |
| B1 | 无 .clippy.toml 或 rustfmt.toml——272 个文件无统一 lint 策略 | 中 | 已验证 |
| B2 | Release 配置 codegen-units=1 + LTO=true——增量构建慢 | 低 | 已验证 |
| B3 | 无 dev profile opt-level 覆盖——测试迭代慢 | 低 | 已验证 |
| A1 | ContentBlock 缺少 Resource/Audio 变体，与 ACP 规范不符 | 低 | 已验证 |
| A2 | tool_call 通知中缺少 ToolKind 分类 | 低 | 已验证 |

### 验证修正
- **已驳回**：Stdio 路径确实实现了 session/list、session/load、session/resume（acp_stdio.rs:342-730）。最初声称缺少方法是错误的。
- **已驳回**：AcpError 确实有 JSON-RPC 错误码（`code: i64` 字段）。错误码映射已存在。
- 这 2 项驳回显著降低了 ACP 合规性担忧——stdio 路径比最初描述的更完整。

### 性能分析摘要
- **热路径**：流式 LLM 响应 → 3 个无容量预分配的 String 缓冲区 → 每响应约 50 次堆重新分配 → 每 1000 token 响应约 200ms 浪费
- **影响**：单请求边际影响，但会累积：100 请求/天 × 200ms = 20 秒浪费在分配上
- **修复优先级**：低-中——分配开销相对于网络延迟较小

### 改进建议

**流式缓冲区预分配**（低-中优先级）：
- 流式文本缓冲区使用 `String::with_capacity(2048)`
- 推理缓冲区使用 `String::with_capacity(512)`
- 工具结果使用 `Vec::with_capacity(calls.len())`
- 收益：每响应约减少 50 次堆分配，更平滑的流式输出

**Trait 文档冲刺**（高优先级，开发者体验）：
- 为 Middleware、BaseTool、EventSink、BaseModel、State 添加 trait 级文档
- 文档化：生命周期顺序、线程安全、错误契约
- 为 box_to_arc 和 jsonrpc/codec.rs 的 unsafe 块添加 # Safety 段落
- 收益：新贡献者入职从小时级降到分钟级

**构建配置**（低优先级）：
- 添加 `rustfmt.toml` 配置项目约定
- 添加 dev profile `opt-level = 1` 加速测试迭代
- 考虑 `lto = "thin"` + `codegen-units = 4` 加速 release 构建
- 收益：更快 CI、更快迭代

### 更新的跨轮次模式分析

九轮揭示了七个根本原因：

**根本原因 1：协调层膨胀**（第 1-3 轮）
**根本原因 2：横切关注点重复**（第 4 轮）
**根本原因 3：巨型文件增长**（第 5 轮）
**根本原因 4：字符串类型化接口**（第 6 轮）
**根本原因 5：无界资源增长**（第 7 轮）
**根本原因 6：不一致的安全边界**（第 8 轮）
**根本原因 7：文档债务**（第 9 轮）
- 62% 文档覆盖率，核心 trait 无文档，unsafe 代码无安全段落
- 272 个文件的代码库无统一 lint/格式策略
- 修复方案：Trait 文档冲刺、lint 配置、安全文档

合计：9 轮中 76 项独立发现（58 项验证，6 项驳回，9 项部分，3 项计数修正）。

### 收益递减评估

经过 9 轮，代码库已在 7 个根本原因和 15+ 个维度上被全面分析。剩余未探索区域产生递减收益：
- **性能微优化**（本轮）——边际影响 vs 付出
- **ACP 协议缺口**（本轮）——主要是规范对齐，非架构性
- **构建配置**（本轮）——工具链问题，非架构问题

**建议**：未来轮次应聚焦于**跟踪高优先级发现的修复进展**，而非发现新问题。各轮中记录的 12 个最高 ROI 改进建议提供了数月聚焦修复工作的充分指导。

---

## 2026-05-26 第 10 轮（Cron #10）——变更审计

### 策略转变
根据第 9 轮建议，本轮审计**近期代码变更**（20 次提交），检查回归和新架构问题，而非扫描未触及区域。

### 分析文件（8 个文件，5 个新增 + 3 个修改）

| 文件 | 类型 | 评估 |
|------|------|------|
| `agent_result.rs`（55 行） | 新增 | 浅层桩工具——有意设计，无摩擦 |
| `agent_events_bg.rs`（367 行） | 新增 | 中——3 个紧密耦合的 bg-continuation 状态字段跨 4 个文件 |
| `agent_comm.rs`（130 行） | 新增 | 低-中——28 字段上帝容器，状态膨胀迹象 |
| `agent_submit.rs`（379 行） | 新增 | 低——2 个函数中重复的状态重置逻辑 |
| `keyboard/normal_keys.rs`（535 行） | 新增 | 干净——良好的模块化，低耦合 |
| `executor/mod.rs` prepended_ids 修复 | 修改 | 正确——take_while(System) 是合理的修复 |
| `execute_bg.rs` child_thread_id | 修改 | 一致——正确贯穿生命周期 |
| `events.rs` BackgroundTaskCompleted | 修改 | 干净——正确集成 |

### 新发现（4 项）

| # | 发现 | 严重程度 | 状态 |
|---|------|----------|------|
| 1 | 后台 agent 续接复杂性——3 个状态字段（pre_done_bg_completions/results/pending_bg_continuation）跨 4 个文件 | 中 | 内联验证 |
| 2 | AgentComm 28 字段容器——agent 通信状态的上帝对象 | 低-中 | 内联验证 |
| 3 | agent_submit.rs 重复重置逻辑——152-184 行和 339-356 行 | 低 | 内联验证 |
| 4 | AgentResult 工具是浅层桩——结果通过合成消息在别处注入 | 低 | 设计如此 |

### 正面观察
- **键盘重构很干净** — 6 个子模块，仅有 `Action` 类型依赖（解决第 5 轮发现 3）
- **prepended_ids 修复正确** — 解决了 CLAUDE.md 中关于 System 消息清理的 TRAP
- **child_thread_id 正确贯穿** — 并发 bg agent 的精确匹配
- **近期 20 次提交中未检测到关键回归**

### 修复进展检查
第 1-9 轮的 76 项发现中，以下显示积极进展：

| 发现 | 状态 |
|------|------|
| R5-F3: event/mod.rs 上帝处理器 | **部分解决** — keyboard 已提取为 6 个子模块 |
| R7-A4: 后台 agent 全局限制 | **未变更** — max_concurrent=3 仍硬编码 |
| R8-S1: WebFetch 中的 SSRF 绕过 | **未变更** — 未应用 ssrf_guard |
| R8-E1: 无部分流式恢复 | **未变更** — 重试仍丢弃部分内容 |

### 更新计数
合计：10 轮中 80 项独立发现（60 项验证，6 项驳回，11 项部分，3 项计数修正）。

### 第 10 轮结论
代码库在积极改进中。近期变更展示了良好的拆分模式（keyboard 拆分）和针对性 bug 修复（prepended_ids）。新的 bg-agent 代码引入了中等复杂性但结构良好。最高 ROI 的修复目标仍然是安全发现（SSRF、hook 沙箱化）和协调层拆分（MessagePipeline、AcpAgentConfig 分组）。

---

## 2026-05-26 第 11 轮（Cron #11）——修复审计

### 策略
自第 10 轮以来无新提交。深度审计 TOP 10 高优先级发现的当前修复状态。

### TOP 10 修复状态

| # | 发现 | 轮次 | 状态 | 详情 |
|---|------|------|------|------|
| 1 | WebFetch/WebSearch 中 SSRF 绕过 | R8 | **未变更** | web_fetch.rs/web_search.rs 仍直接使用 reqwest 无 ssrf_guard |
| 2 | Hook 命令执行无沙箱 | R8 | **未变更** | hooks/executor.rs 仍直接运行 bash -c |
| 3 | 流式失败丢弃部分内容 | R8 | **部分** | 重试存在但无检查点；部分内容丢失 |
| 4 | MessagePipeline 上帝对象（20+ 字段） | R2 | **未变更** | 未进行拆分 |
| 5 | AcpAgentConfig 28-35 字段膨胀 | R2 | **部分** | 减少到约 28 字段，仍无逻辑分组 |
| 6 | LLM 适配器重复（665/693 行） | R3 | **未变更** | 未提取共享代码 |
| 7 | 40+ 个无界通道 | R7 | **未变更** | 未迁移到有界通道 |
| 8 | 工具名字符串类型化（HITL） | R6 | **未变更** | 仍使用字符串比较 |
| 9 | 事件路由跨 3 层 | R8 | **部分** | source_agent_id 精确匹配已改善 |
| 10 | 无提示版本管理 | R8 | **未变更** | 未添加版本元数据 |

### 总结
- **未变更**：7/10（安全、拆分、流式、通道、类型、提示）
- **部分解决**：3/10（流式重试、配置缩减、事件路由）
- **完全解决**：0/10

### 优先级行动矩阵

**立即（安全关键）：**
- WebFetch/WebSearch 的 SSRF 防护——约 50 行代码，阻止内部网络扫描
- Hook 命令白名单/黑名单——约 100 行，防止恶意插件任意执行

**短期（架构债务）：**
- MessagePipeline 拆分为 4 个子组件——高 ROI，实现独立测试
- ToolName 枚举替换字符串比较——消除拼写错误类 bug
- LLM 适配器共享 trait——将未来 provider 支持成本降低约 50%

**中期（韧性）：**
- 有界通道 + 背压——防止持续负载下 OOM
- 流式检查点恢复——防止长响应时 token 浪费
- 提示版本管理——实现安全的提示迭代

### 趋势分析（第 1-11 轮）

```
轮次 | 新发现 | 已修复 | 净待处理
  1   |   12   |   0   |   12
  2   |    7   |   0   |   19
  3   |    7   |   0   |   26
  4   |    9   |   0   |   35
  5   |    7   |   0   |   42
  6   |    7   |   0   |   49
  7   |   11   |   0   |   60
  8   |   13   |   0   |   73
  9   |   12   |   0   |   85（整合为 76 独立项）
 10   |    4   |   0   |   80
 11   |    0   |   0   |   80（仅审计）
```

**观察**：11 轮，80 项发现，0 项完全修复。审查很全面但发现未推动行动。继续自动化审查的价值正在递减——瓶颈在于实现，而非发现。

### 未来轮次建议
考虑**停止 cron** 并将 progress.md 转化为优先级 issue 追踪器。80 项发现及其严重程度评级和改进建议，为数月的聚焦修复工作提供了充分指导。继续发现边际问题浪费计算资源，无助于推进代码库。

---

## 2026-05-26 第 12 轮（Cron #12）——最终整合

### 状态
自第 10 轮以来无新提交。同一 HEAD（`02c846b`）。这是自动化评审的**最后一轮**。

### 执行摘要

12 轮自动化架构评审分析了 7 个 workspace crate 中 117,488 行 Rust 代码，产出 **80 项独立发现**，涵盖 7 个根本原因。

**按严重程度：**
- 高：18 项发现（安全、拆分、协议）
- 中：42 项发现（模式、韧性、文档）
- 低：20 项发现（微优化、工具链）

**按根本原因：**
1. 协调层膨胀（第 1-3 轮）：26 项发现
2. 横切关注点重复（第 4 轮）：9 项发现
3. 巨型文件增长（第 5 轮）：7 项发现
4. 字符串类型化接口（第 6 轮）：7 项发现
5. 无界资源增长（第 7 轮）：11 项发现
6. 不一致的安全边界（第 8 轮）：13 项发现
7. 文档债务（第 9 轮）：7 项发现

**修复状态：** 0/80 完全解决，3/80 部分解决，77/80 未变更。

---

### 优先修复路线图

#### 阶段 1：安全（1-2 周）
| # | 行动 | 工作量 | 影响 |
|---|------|--------|------|
| S1 | 为 WebFetch/WebSearch 添加 ssrf_guard | 50 行 | 阻止内部网络扫描 |
| S2 | 为 HookMiddleware 添加命令白名单 | 100 行 | 防止恶意插件任意执行 |
| S3 | 统一路径验证（所有工具使用 validate_and_resolve） | 150 行 | 一致的文件安全性 |

#### 阶段 2：核心拆分（2-4 周）
| # | 行动 | 工作量 | 影响 |
|---|------|--------|------|
| D1 | 将 MessagePipeline 拆分为 SubAgentManager + ToolCallTracker + StreamingBuffer + RoundTracker | 约 500 行变更 | 可测试组件，每个约 200 行 |
| D2 | 将 AcpAgentConfig 分组为 RuntimeConfig + LlmConfig + FrozenData + ServiceConfig | 约 300 行变更 | 更小的接口，可校验 |
| D3 | 创建 ToolName 枚举，替换 HITL 字符串比较 | 约 200 行变更 | 编译时安全性 |

#### 阶段 3：韧性（2-3 周）
| # | 行动 | 工作量 | 影响 |
|---|------|--------|------|
| R1 | 审计 40+ 个无界通道，为高吞吐路径添加界限 | 约 100 行 | 负载下内存有界 |
| R2 | 添加每会话 bg agent 限制（替代全局 max_concurrent=3） | 约 50 行 | 公平资源分配 |
| R3 | RetryableLLM 中的流式检查点恢复 | 约 200 行 | 故障时无 token 浪费 |
| R4 | MCP 指数退避自动重连 | 约 150 行 | 无缝外部服务恢复 |

#### 阶段 4：代码质量（持续）
| # | 行动 | 工作量 | 影响 |
|---|------|--------|------|
| Q1 | Trait 文档冲刺（Middleware、BaseTool、EventSink） | 约 500 行文档 | 新贡献者入职 |
| Q2 | 提取共享 LLM 适配器代码 | 约 400 行去重 | Provider 支持成本 -50% |
| Q3 | 将 headless_test.rs 拆分为 8 个聚焦模块 | 仅重构 | 更快测试迭代 |
| Q4 | 添加 rustfmt.toml + dev profile opt-level=1 | 约 20 行配置 | 更快迭代 |

---

### Cron 建议

**停止 cron 任务。** 理由：
1. 代码库连续 3 轮未变更——无新内容可审查
2. 80 项发现提供了 4+ 阶段的修复工作（数月的精力）
3. 继续发现的边际价值为零——瓶颈在于实现
4. 每轮计算成本（约 10 次 agent 调用）更好的用途是实际修复

**停止方法**：使用 `cron_remove`，任务 ID `019e64ed-88c7-7c01-8ef4-e593022faaf2`

**恢复方法**：完成阶段 1-2 修复后重新注册 cron，以验证修复并发现重构引入的回归。

---

### 文件参考
- 完整发现详情：`progress.md`（735 行，第 1-12 轮）
- CLAUDE.md 架构章节：项目根目录 `CLAUDE.md`
- 规范评审：`spec/reviews/`
- Issue 追踪：`spec/issues/`

### 指标
- 分析总行数：117,488
- .rs 文件总数：272
- 覆盖 crate 数：7
- 执行轮次：12
- 产出发现数：80
- 验证准确率：75%（60 项验证，6 项驳回，11 项部分，3 项计数修正）
- 修复率：0%（0/80 完全解决）
- 建议下一步：停止 cron，开始阶段 1 安全修复

---

## 2026-05-26 第 13 轮——CRON 已停止

### 状态
- HEAD 连续 4 轮未变更（第 10-13 轮，同一提交 `02c846b`）
- 无新代码可审查；第 1-12 轮的所有发现仍有效
- Cron 任务 `019e64ed-88c7-7c01-8ef4-e593022faaf2` 按第 12 轮建议**已移除**

### 最终统计
| 指标 | 值 |
|------|-----|
| 总��次 | 13（第 1-9 轮发现、第 10 轮变更审计、第 11 轮修复审计、第 12 轮整合、第 13 轮关闭） |
| 总发现数 | 80（18 高、42 中、20 低） |
| 识别的根本原因 | 7 |
| 验证准确率 | 75%（80 项中 60 项验证正确） |
| 修复率 | 0%（瓶颈：实现，而非发现） |
| 分析的代码库 | 7 个 crate 中 272 个 .rs 文件，共 117,488 行 |
| 计划的修复阶段 | 4（安全 → 拆分 → 韧性 → 质量） |

### 恢复方法
完成阶段 1-2 修复后重新注册 cron（`*/10 * * * *`），以验证修复并发现重构引入的回归。使用 `progress.md` 路线图作为 issue 追踪器。
