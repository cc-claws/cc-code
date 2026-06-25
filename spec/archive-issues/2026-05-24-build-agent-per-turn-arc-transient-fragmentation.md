> 归档于 2026-05-24，原路径 spec/issues/2026-05-24-build-agent-per-turn-arc-transient-fragmentation.md

# build_agent 每轮重建大对象产生瞬态分配碎片

**状态**：Fixed
**优先级**：中
**类型**：性能
**创建日期**：2026-05-24
**修复日期**：2026-05-24

## 问题描述

`build_agent()` 在每次 `session/prompt` 时被调用，创建完整的 ReActAgent + 16 个 middleware + 大量 Arc 包装的对象（LLM 实例、reqwest Client、工具集、子 Agent 工厂等）。这些对象在 prompt 结束后被正确 drop（heapdump 确认 allocated 降至 ~9 MB），但 drop 过程中产生的 ~68 万次瞬态 malloc/free 造成 jemalloc arena slabs 碎片化，内存无法归还 OS。

heapdump 显示 `/clear` 后 `active - allocated = 17.5 MB`（arena 碎片化空闲页），加上非 jemalloc 的 43.4 MB，总计 ~60 MB RSS 不释放。

**关联 issue**：`spec/issues/2026-05-22-memory-linear-growth-no-compact.md`（现象 6 确认了双重问题：arena 碎片化 + 非 jemalloc 运行时持有）

## 症状详情

### 每轮重建的大对象清单

| 对象 | 文件:行号 | 大小估算 | 说明 |
|------|----------|---------|------|
| `compact_model` | `executor.rs:278` | ~1-2 MB | `provider.clone().into_model().into()` 每轮创建新 LLM 实例（含 reqwest Client + TLS） |
| `LlmAutoClassifier.model` | `builder.rs:154` | ~1-2 MB | `Arc::new(Mutex::new(provider.clone().into_model()))` 每轮创建第二个 LLM 实例 |
| `parent_tools` (Vec<Box<dyn BaseTool>>) | `builder.rs:168-181` | ~0.5-1 MB | FilesystemMiddleware::build_tools + TerminalMiddleware + MCP tool bridges 每轮重建 |
| MCP tool bridges | `builder.rs:172-180` | ~0.5 MB | `build_tool_bridges(pool)` 每轮重新生成，每个 bridge clone Arc<McpClientPool> |
| `llm_factory` (Arc<dyn Fn>) | `builder.rs:188-214` | ~0.3 MB | 闭包捕获 provider clone + peri_config clone |
| `system_builder` (Arc<dyn Fn>) | `builder.rs:218-227` | ~0.1 MB | 闭包捕获 frozen_date clone |
| `background_registry` | `builder.rs:235-237` | ~0.1 MB | Arc + UnboundedChannel |
| 16 个 `Box<dyn Middleware>` | `builder.rs:288-410` | ~1-2 MB | 每个 middleware 自身 + 内部 state |
| `hitl.auto_classifier` cache | `auto_classifier.rs:47` | 累积 | HashMap<(String, u64), CacheEntry> 无过期驱逐 |

### Arc 引用计数审计结果

所有 prompt 级 Arc 在 `tokio::spawn` task 结束后引用计数回到 baseline（已审计完整链路 `prompt.rs` → `executor` → `builder` → `ReActAgent`）。**引用计数无泄漏**，问题是 drop 时产生的瞬态分配碎片。

### 数据佐证（来自 heapdump）

- 每轮 ~68 万次 transient malloc，97.3% 在 prompt 结束前已 free
- `allocated` 降至 9.3 MB（Rust 堆对象极少），但 `active - allocated = 17.5 MB`（arena 碎片化）
- `mapped = 116.2 MB`（jemalloc 虚拟地址空间膨胀）

## 性能数据

| 指标 | 值 |
|------|-----|
| 每轮 malloc 次数 | ~680,000 |
| free/malloc 比 | 97.3% |
| 每轮 RSS 增长 | ~40 MB |
| `/clear` 后 arena 碎片 | ~17.5 MB |
| 每轮新建 reqwest Client | 至少 2 个（compact_model + auto_classifier） |

## 出现场景

- **必现**：每次 `session/prompt` 调用
- **影响**：长时间运行的 TUI 会话（50-100 轮）后 RSS 可达数 GB
- **与分配器无关**：系统分配器对照实验确认同样行为

## 改进方向（记录用户期望）

1. **跨 prompt 复用 LLM 实例**：`compact_model` 和 `LlmAutoClassifier.model` 不需要每轮重建，可以 session 级缓存（reqwest Client 本身就是线程安全的）
2. **跨 prompt 复用 parent_tools**：工具集在 cwd 不变时无需重建
3. **Middleware 实例池化**：16 个 middleware 中大部分是无状态的（FilesystemMiddleware、TerminalMiddleware、WebMiddleware），可以跨 prompt 复用
4. **减少 Arc clone 深度**：`llm_factory`、`system_builder` 等闭包捕获了大量 clone，可改为引用共享

## 涉及文件

- `peri-acp/src/session/executor.rs` —— `execute_prompt()` 入口，`compact_model` 每轮创建（:278）
- `peri-acp/src/agent/builder.rs` —— `build_agent()` 全量重建（:94-417）
- `peri-tui/src/acp_server/prompt.rs` —— TUI 侧 prompt 执行入口
- `peri-agent/src/llm/openai/mod.rs` —— reqwest Client 每次构造
- `peri-agent/src/llm/anthropic/mod.rs` —— reqwest Client 每次构造
- `peri-middlewares/src/hitl/auto_classifier.rs` —— LlmAutoClassifier 每轮新建

## 修复记录

### 方案：Session 级 AgentPool 缓存 LLM 实例

在 `peri-acp/src/session/agent_pool.rs` 新增 `AgentPool` 结构体，持有 session 级可复用的 `CachedLlmInstances`（含 `compact_model` + `auto_classifier_model` 的 Arc 引用）。`execute_prompt()` 首次调用时全量构建并存入 pool，后续调用通过 `has_valid_cache()` 检查 provider fingerprint 后复用缓存实例。

### 已实现的改进

| 改进方向 | 状态 | 说明 |
|---------|------|------|
| 跨 prompt 复用 LLM 实例 | ✅ 已实现 | `compact_model` + `auto_classifier_model` 含 reqwest Client 缓存到 session 级，减少 ~2-4 MB/turn |
| 跨 prompt 复用 parent_tools | ⏭️ 跳过 | `BaseTool` 不实现 `Clone`，`SubAgentMiddleware` 取得所有权使缓存复杂化，ROI 低 |
| Middleware 实例池化 | ✅ 部分实现 | 为 `GitAttributionMiddleware` 和 `CompactMiddleware` 添加 `reset()` 方法，为全量复用做准备 |
| 减少 Arc clone 深度 | ✅ 隐式实现 | `CachedLlmInstances` 通过 Arc 共享替代每轮 `into_model()` 重建 |
| ReActAgent 跨 turn 更新 | ✅ 已实现 | 添加 `set_event_handler/set_system_prompt/set_notification_rx` 方法，支持 `&mut self` 原地更新 |

### 修复 Commits

| Commit | 内容 |
|--------|------|
| `ededd7f` | `feat(peri-acp): add AgentPool for session-scoped LLM instance reuse` |
| `8e71a0f` | `feat(peri-middlewares): add reset() to stateful middleware for cross-turn reuse` |
| `26e3df7` | `feat(peri-acp): integrate AgentPool for cross-prompt LLM instance reuse` |

注：中间还有 `feat(peri-agent): add per-turn update methods to ReActAgent` 被 amend 合并到主集成 commit 中。

### 变更文件

- `peri-acp/src/session/agent_pool.rs` — **新建**：AgentPool + CachedLlmInstances
- `peri-acp/src/session/agent_pool_test.rs` — **新建**：7 个单元测试
- `peri-acp/src/session/executor.rs` — pool 参数 + cache 检查/存储逻辑
- `peri-acp/src/agent/builder.rs` — 接受 `Option<&CachedLlmInstances>`，复用 LLM 实例
- `peri-agent/src/agent/executor/mod.rs` — 3 个 `set_*` per-turn 更新方法 + 3 个测试
- `peri-middlewares/src/attribution/mod.rs` — `reset()` 方法 + 测试
- `peri-middlewares/src/compact_middleware.rs` — `reset()` 方法 + 测试
- `peri-tui/src/acp_server/mod.rs` — SessionState 持有 AgentPool
- `peri-tui/src/acp_server/prompt.rs` — 传递 pool 参数
- `peri-tui/src/acp_server/requests.rs` — pool 提取/恢复生命周期管理
- `peri-tui/src/acp_stdio.rs` — stdio 路径 pool 生命周期
- `peri-tui/src/cli_print.rs` — `-p` 模式一次性 pool

### 模型切换 Invalidation

采用惰性 invalidation：`has_valid_cache()` 通过 `"provider_name:model_name"` fingerprint 检测 provider 变化。`session/set_model` 和 `session/set_config_option(model)` 不需要显式调用 `invalidate()`——下次 prompt 时 fingerprint 不匹配自动触发全量重建。

### 未覆盖项（后续可优化）

- **工具集缓存**：`BaseTool` 不实现 `Clone` 且 `SubAgentMiddleware` 取得所有权，需要重构为 `Arc<dyn BaseTool>` 共享模式
- **Middleware 全量复用**：当前仅添加了 `reset()` 方法准备，未实现跨 turn middleware 实例池化（需解决 `MiddlewareChain` 所有权问题）
- **llm_factory / system_builder 闭包缓存**：闭包捕获的数据 session 内不变，可作为 pool 一部分缓存
