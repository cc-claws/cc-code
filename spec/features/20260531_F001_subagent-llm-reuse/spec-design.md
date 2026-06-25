# Feature: 20260531_F001 - SubAgent LLM Instance Reuse

## 需求背景

mimalloc 统计数据显示 23 分钟会话中：

- **总 heap 139 个**（每个 tokio task 一个 mimalloc thread heap）
- **废弃页 17.2K**（线程退出后遗留的分配页）
- **累计 purge 490 MiB**（内存暴涨→释放的循环）

根因分析：每次 SubAgent 调用都通过 `llm_factory` 新建 `Box<dyn ReactLLM>`，内部持有独立的 `reqwest::Client`（含连接池 + TLS session，~1-2 MB/实例）。23 分钟内 140 个 SubAgent 调用（含 fork/background/sync）就产生 140 次 `reqwest::Client` 创建/销毁。这与 `AgentPool` 已经解决的 Main Agent 问题是同类问题——`AgentPool` 缓存 `compact_model` 和 `auto_classifier_model`，跨 prompt 复用 `reqwest::Client`，消除了 Main Agent 路径的 2-4 MB/轮瞬态分配。

但 SubAgent 路径没有任何缓存机制，每个 SubAgent 都是一次性 LLM 实例。

## 目标

- 在 SubAgent 构建路径中引入 LLM 实例缓存，跨 SubAgent 调用复用 `reqwest::Client`
- 复用 `AgentPool` 基础设施，避免引入新的池化机制
- 缓存命中时 SubAgent 创建开销降低 ~60%（跳过 `reqwest::Client` 构建 + TLS handshake 复用）
- 后续效果：减少 mimalloc heap 数量、减少废弃页、降低累计 purge 量

## 方案设计

### 1. 当前数据流

```
每轮 prompt → executor::execute_prompt()
  → agent_pool.lock() → 取/建 cached_llm (compact_model + auto_classifier_model)
  → build_agent(cfg, cached_llm) → 构建 17 个 middleware + SubAgentTool
    └─ llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM>>
       每次调用: provider.into_model() → 新建 reqwest::Client → RetryableLLM 包装
  → agent.execute()
    └─ LLM 调用 SubAgentTool.invoke()
      → build_agent_from_def()
        → (self.llm_factory)(model_alias)  ← 每 SubAgent 一次新建 LLM
      → subagent.execute()  ← 可能在工具并发 (join_all) 中 2-3 个同时执行
      → LLM 实例被 drop            ← jemalloc arena 碎片化
  → pool.lock().store_llm(new_cache)  ← 只缓存 Main Agent 的 compact_model/auto_classifier
```

**核心约束**：`llm_factory` 是 `Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM>>`，是一个工厂闭包。要引入缓存，需要让工厂闭包能访问共享缓存。

### 2. 方案：SubAgent LLM 缓存池

#### 2.1 架构

```
AgentPool 新增字段：
  subagent_llm_cache: HashMap<String, Arc<dyn BaseModel>>
    key = "provider_name:model_name"  (fingerprint)
    value = 已构建的 BaseModel (含 reqwest::Client)
```

#### 2.2 数据流

```
每轮 prompt → executor::execute_prompt()
  → pool.lock() → 取/建 cached_llm (compact_model + auto_classifier_model)
  → build_agent(cfg, cached_llm, &pool)  ← 传入 pool 引用
    └─ llm_factory 内部逻辑：
       1. 用 provider fingerprint 查 pool.subagent_llm_cache
       2. 命中 → BaseModelReactLLM::from_cached(model) + RetryableLLM 包装（无新 Client 分配）
       3. 未命中 → provider.into_model() 新建 → 插入缓存 → 同上
  → agent.execute()
    └─ SubAgentTool.invoke() → build_agent_from_def() → llm_factory(model_alias)
      → 缓存命中 → 无新 reqwest::Client 分配
  → pool.lock().store_llm(new_cache)  ← 不变，SubAgent 缓存已在 pool 内
```

#### 2.3 关键设计决策

| 维度 | 决策 | 理由 |
|------|------|------|
| **缓存位置** | `AgentPool` 内部（与 compact/classifier 缓存同结构） | 复用已有 infrastructure，统一 invalidation |
| **缓存 key** | `"provider_name:model_name"` fingerprint | 与 AgentPool fingerprint 机制一致 |
| **缓存生命周期** | Session 级（与 AgentPool 同生命） | 跨 SubAgent 调用复用，session 关闭时清理 |
| **缓存 invalidation** | 模型切换（`session/set_model`）时全体清空 | 与 AgentPool.invalidate() 同步 |
| **并发安全** | `parking_lot::Mutex<AgentPool>` 已有锁，缓存操作在持锁内 | 无新增锁 |
| **内存上限** | 无硬上限（实际值 ≤ 模型别名数量，5-8 个） | SubAgent 模型种类有限（sonnet/opus/haiku/gpt-4o 等别名） |
| **BaseModel vs ReactLLM** | 缓存 `Arc<dyn BaseModel>`，不缓存 RetryableLLM 包装 | `BaseModel` 无状态、可共享；`RetryableLLM` 有重试计数器（per-invocation） |
| **model_alias=None 路径** | 使用 `provider_name:model_name` 的 "default" 指纹 | 与父 agent 同模型的 SubAgent 走同一条缓存 |

#### 2.4 已知限制

| 限制 | 说明 | 影响 |
|------|------|------|
| **API Key 变更不触发 invalidation** | fingerprint 只含 `provider:model` 不含 `api_key`。运行时换 Key 后缓存返回旧凭证的 BaseModel | 低——运行时换 Key 场景罕见。此限制在现有 Main Agent 缓存中同样存在，不在本 feature 范围内修复 |
| **缓存无 LRU 上限** | HashMap 无容量限制。最坏情况 ~20 entries × 1.5 MB = 30 MB | 低——SubAgent 模型别名有限（5-8 个），模型切换产生的旧 entry 通过 `invalidate()` 清除 |
| **缓存不跨 Session** | AgentPool 生命周期 = Session 生命周期。新 session 重新初始化 | 设计意图，非限制 |

#### 2.5 不改变的行为

- `llm_factory` 签名不变：`Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync>>`
- SubAgent 每次调用仍然创建 `RetryableLLM` 包装（轻量，无 `reqwest::Client`）
- Model alias 查找逻辑不变：`LlmProvider::from_config_for_alias()` 仍独立执行
- 不同 model_alias 的 SubAgent 使用不同缓存 key，不会错配

### 3. 实现要点

**变更文件**：

| 文件 | 变更 |
|------|------|
| `peri-acp/src/session/agent_pool.rs` | 新增 `subagent_llm_cache: HashMap<String, Arc<dyn BaseModel>>`，`get_or_create_subagent_llm()` 方法，`invalidate()` 增加 `subagent_llm_cache.clear()` |
| `peri-acp/src/agent/builder.rs` | `build_agent()` 新增 `pool: &Arc<Mutex<AgentPool>>` 参数；`llm_factory` 闭包捕获 `pool` 引用，内部调用 `pool.lock().get_or_create_subagent_llm()` |
| `peri-acp/src/session/executor.rs` | `build_agent()` 调用处传入 `&pool` |
| `peri-tui/src/acp_server/requests.rs` | `session/set_model` 和 `session/set_config_option("model")` 新增 `pool.lock().invalidate()` 调用（**当前缺失**，本 feature 需补充） |
| `peri-acp/src/transport/acp_stdio.rs` | `session/set_model` 和 `session/set_config_option("model")` 新增 `pool.lock().invalidate()` 调用（Stdio 路径同样缺失） |

**`get_or_create_subagent_llm()` 伪代码**：

```rust
impl AgentPool {
    /// 获取或创建 SubAgent LLM 实例（线程安全，双检锁优化）。
    ///
    /// 快速路径（cache hit）仅持锁 ~1μs 查 HashMap。
    /// 慢路径（cache miss）在锁外创建 reqwest::Client（~10-100ms），
    /// 然后持锁写缓存，避免阻塞其他 SubAgent 的快速路径。
    fn get_or_create_subagent_llm(
        pool: &Arc<parking_lot::Mutex<AgentPool>>,
        fingerprint: &str,
        provider: &LlmProvider,
    ) -> Arc<dyn BaseModel> {
        // 快速路径：持锁查缓存
        {
            let pool = pool.lock();
            if let Some(cached) = pool.subagent_llm_cache.get(fingerprint) {
                return Arc::clone(cached);
            }
        }
        // 慢路径：锁外创建 reqwest::Client（TLS 初始化 10-100ms）
        let new_model: Arc<dyn BaseModel> = Arc::new(provider.clone().into_model());
        // 再次持锁写缓存（可能被其他线程抢先写入，or_insert 保证只存一个）
        pool.lock()
            .subagent_llm_cache
            .entry(fingerprint.to_string())
            .or_insert(new_model)
            .clone()
    }

    /// 清除所有缓存（模型切换/API Key 变更时调用）。
    fn invalidate(&mut self) {
        self.cached_llm = None;
        self.fingerprint.clear();
        self.subagent_llm_cache.clear();  // 新增
    }
}
```

**`llm_factory` 修改**：

```rust
// 当前
let llm_factory = Arc::new(move |model_alias: Option<&str>| {
    let p = /* resolve provider */;
    let llm = BaseModelReactLLM::new(p.into_model()); // 每次新建 Client
    Box::new(RetryableLLM::new(llm, RetryConfig::default()))
});

// 修改后
let pool_for_factory = Arc::clone(&pool);
let llm_factory = Arc::new(move |model_alias: Option<&str>| {
    let p = /* resolve provider (不变) */;
    let fp = format!("{}:{}", p.display_name(), p.model_name());
    let cached_model = pool_for_factory.lock().get_or_create_subagent_llm(&fp, &p);
    let llm = BaseModelReactLLM::new(cached_model); // 复用 Client
    Box::new(RetryableLLM::new(llm, RetryConfig::default()))
});
```

### 4. `BaseModelReactLLM` 约束检查

当前 `BaseModelReactLLM::new()` 接受 `Box<dyn BaseModel>`：

```rust
// peri-agent/src/llm/mod.rs (预期)
pub struct BaseModelReactLLM {
    model: Box<dyn BaseModel>,
    // ...
}

impl BaseModelReactLLM {
    pub fn new(model: Box<dyn BaseModel>) -> Self { ... }
}
```

需求：支持从 `Arc<dyn BaseModel>` 构造。方案：

- `BaseModel` trait 已经是 `Send + Sync`，Arc 共享是安全的
- 新增 `BaseModelReactLLM::from_arc(model: Arc<dyn BaseModel>)` 构造器
- 内部需要 `Box<dyn BaseModel>` 可改为 `Arc<dyn BaseModel>`？否——`BaseModelReactLLM` 是 owned 结构，改为 Arc 可能影响 drop 语义。更安全的方案是用 `Arc::try_unwrap()`... 不行。
- **最佳方案**：`BaseModelReactLLM` 内部存储从 `Arc` 改为 `Either<Box, Arc>` 或简单的 `Arc<dyn BaseModel>`。检查现有使用——所有当前构造都是 `Box`（owned），没有需要 `Box` 独占所有权的场景（`BaseModel` trait 方法都是 `&self`）。

**确认**：`BaseModel` trait 方法签名：

```rust
#[async_trait]
pub trait BaseModel: Send + Sync {
    async fn generate(&self, ...) -> ...;
    fn model_name(&self) -> &str;
    fn provider_name(&self) -> &str;
    fn context_window(&self) -> usize;
    fn thinking_config(&self) -> Option<&ThinkingConfig>;
    // 全部 &self，无 &mut self
}
```

全部方法是 `&self`，无 `&mut self`。`Arc<dyn BaseModel>` 完全安全。

**决定**：`BaseModelReactLLM.model` 字段从 `Box<dyn BaseModel>` 改为 `Arc<dyn BaseModel>`。工厂函数 `BaseModelReactLLM::new(Box)` 内部 `Box → Arc`。`from_arc` 直接存储 Arc。

### 5. Mimalloc 预期效果

| 指标 | 优化前（23min 会话） | 优化后（预期） |
|------|---------------------|---------------|
| 总 heap 数 | 139 | 减少到 ~100（SubAgent 每调用少一次 heap 分配） |
| 废弃页 | 17.2K | 减少 30-40%（减少大量瞬态 `reqwest::Client` 分配/释放） |
| 累计 purge | 490 MiB | 减少 20-30%（减少 Client 分配的内存暴涨） |
| Peak RSS | 221 MiB | 持平（`reqwest::Client` 分配量不大，主要是碎片改善） |
| `build_agent_from_def()` 延迟 | ~5-10ms（新建 Client + TLS） | ~1ms（仅 RetryableLLM 包装） |

> 注意：mimalloc heap = thread heap。tokio task 不会创建 OS 线程（不创建新 heap）。139 个 heap 对应 139 个**实际 OS 线程**（tokio worker + spawn_blocking pool + 子进程管理）。SubAgent 缓存主要减少 `reqwest::Client` 的瞬态分配量（purge 和废弃页），对 heap 数量的直接影响有限——heap 数量主要来自 tokio runtime 线程池和 `spawn_blocking` 调用。

### 6. MCP / Bash 进程模型讨论

**当前状态**：MCP 和 Bash 已使用进程模型（每个 MCP server 独立 `tokio::process::Command`，Bash 每次 `cmd.output()` 新进程）。这符合"外部数据不可控"的安全隔离原则。

**不需要改为 in-process 模型**。理由：
1. 安全隔离：MCP server / Bash 命令可能 crash、OOM、被恶意利用，进程隔离是最小信任边界
2. 资源生命周期：子进程的 stdout/stderr 内存由 OS 管理，自动回收，不进入 peri 的 jemalloc arena
3. 并发性：进程模型天然并行，不竞争 peri 的 tokio runtime 线程池

唯一的优化空间：MCP server 的 `tokio::spawn` 用于 stderr 日志读取——每 MCP server 一个长期 task。当前数量有限（3-8 个 server），不是主要瓶颈。

## 约束一致性

- **Prompt Cache 稳定性**：`BaseModel` 是纯 HTTP client，不包含 system prompt / tools / messages。复用不影响 Anthropic cache 前缀
- **Middleware 链不变**：SubAgent 的 middleware 链（AgentsMd → Skills → SkillPreload → Todo）完全不变
- **工具过滤不变**：缓存只影响 LLM 实例创建，不影响 `filter_tools()` 逻辑
- **事件路由不变**：`child_handler_factory` / `SourceAgentIdHandler` / `child_thread_id` 均不变
- **取消传播不变**：`CancelPolicy` / `AgentCancellationToken` 树不变
- **编码规范**：tracing 日志、async-trait、parking_lot RwLock，全都遵守项目规范

## 验收标准

- [ ] `AgentPool` 支持 `get_or_create_subagent_llm()` 缓存查询
- [ ] `invalidate()` 同步清空 SubAgent 缓存
- [ ] `BaseModelReactLLM` 支持从 `Arc<dyn BaseModel>` 构造
- [ ] 第一次 SubAgent 调用（cache miss）正常创建 LLM 实例
- [ ] 第二次同模型 SubAgent 调用（cache hit）复用 LLM 实例，无新增 `reqwest::Client`
- [ ] 模型切换后（`session/set_model`）缓存 invalidation 正确，下次调用重建
- [ ] 不同 model_alias 的 SubAgent 使用不同缓存 key，互不干扰
- [ ] 现有所有 SubAgent 测试通过（normal / fork / background / concurrent）
- [ ] 新增 AgentPool 单元测试（cache hit/miss/invalidation/subagent 隔离）
  - `test_subagent_cache_miss_creates_new`：首次调用 → cache miss → 创建新 `Arc`
  - `test_subagent_cache_hit_returns_same`：相同 fingerprint 再次调用 → 返回相同 `Arc`（`Arc::ptr_eq`）
  - `test_subagent_cache_different_fingerprint_isolation`：不同 fingerprint → 不同 entry
  - `test_invalidate_clears_subagent_cache`：`invalidate()` 后 `subagent_llm_cache` 为空
- [ ] Mimalloc stats 对比：长时间会话后总 heap、废弃页、累计 purge 均减少

---

## 审查附录（2026-05-31 ultra-batch 三人审查）

三个 subagent 分别从**架构正确性**、**设计完整性**、**实现风险**三个视角独立审查，发现高度收敛。

### 审查结论：PARTIAL（修复 2 个 P0 项后升 PASS）

### P0 问题（已修复到本文档）

| # | 问题 | 状态 |
|---|------|------|
| 1 | `session/set_model` 和 `session/set_config_option("model")` 缺少 `pool.invalidate()` 调用（设计文档原文声称"已有"，实际不存在） | ✅ 变更文件表已更新，新增 `requests.rs` + `acp_stdio.rs` |
| 2 | `invalidate()` 未清空 `subagent_llm_cache`（只清 `cached_llm` + `fingerprint`） | ✅ 伪代码中 `invalidate()` 已新增 `.clear()` |

### P1 优化（已采纳到本文档）

| # | 问题 | 状态 |
|---|------|------|
| 3 | `into_model()` 在持锁下创建 `reqwest::Client`（10-100ms），并发 SubAgent 串行化 | ✅ 伪代码改为双检锁（锁外创建 + 锁内写入） |
| 4 | API Key 变更不触发缓存 invalidation（fingerprint 不含 api_key） | ✅ 已记录到"已知限制" |
| 5 | Stdio 路径的 `set_model` 未列入变更文件 | ✅ 变更文件表已补充 `acp_stdio.rs` |

### 已验证（低风险/无风险）

| 检查项 | 结论 |
|--------|------|
| `BaseModelReactLLM` Box→Arc 连锁影响 | 全 `&self` 方法，无 `Box` 独占所有权假设 ✅ |
| `reqwest::Client` 线程安全共享 | 内部 `Arc<Inner>`，所有方法 `&self` ✅ |
| `RetryableLLM` 重试行为 | 重试状态在栈上，不共享 ✅ |
| Anthropic Prompt Cache 污染 | 服务端按 body hash 缓存，与 client 无关 ✅ |
| Normal/Fork/BG 三路径 | 共享同一 `llm_factory`（Arc 传递） ✅ |
| TLS Session 复用 | `reqwest 0.13` + `pool_max_idle_per_host(1)` 自动复用 ✅ |
| Cancel Token 交叉 | 与 `BaseModel` 完全正交 ✅ |
| Fork system prompt 差异 | 通过 `with_system_prompt()` 设置，不经过 LLM 实例 ✅ |
| CompactCommand 缓存复用 | 已通过 `CommandContext.compact_model` 复用 Main Agent 缓存，无需修改 ✅ |
