# before_model / after_model 钩子新增 - 执行计划

**目标:** 在 Middleware trait 中新增两个生命周期钩子：

| 钩子 | 触发点 | 用途 |
|------|--------|------|
| `before_model` | 每轮 `call_llm` 之前 | 上下文压缩、token 预算预处理 |
| `after_model` | `call_llm` 返回后、工具分发/最终答案前 | 响应后处理、token 累积校验、日志 |

不影响现有 19 个中间件实现。

**范围:** 仅 `peri-agent` crate（`middleware/trait.rs` + `middleware/chain.rs` + `chain_test.rs`）+ ReAct 循环调用点（`executor/mod.rs`）。

**设计决策:** 默认空实现——所有现有 `impl Middleware<S> for Xxx` 自动继承，无需逐个修改。

---

## 改动文件

| 文件 | 改动 | 行数 |
|------|------|------|
| `peri-agent/src/middleware/trait.rs` | 新增 `before_model` + `after_model` 方法定义 | +17 |
| `peri-agent/src/middleware/chain.rs` | 新增 `run_before_model` + `run_after_model` 方法 | +16 |
| `peri-agent/src/agent/executor/mod.rs` | ReAct 循环中插入两个调用点 | +4 |
| `peri-agent/src/middleware/chain_test.rs` | 新增 11 个测试 | +220 |

---

## Step 1: Middleware trait 新增两个钩子

**文件:** `peri-agent/src/middleware/trait.rs`

### 1.1 文档注释更新（现有第 8-15 行）

```rust
/// 生命周期钩子执行顺序：
/// ── Agent 生命周期级 ──
/// 1. before_agent  - Agent 开始执行前
///
/// ── 每轮 ReAct 迭代 ──
/// 2. before_model  - 每轮 LLM 调用前（在 call_llm 之前）
/// 3. after_model   - 每轮 LLM 调用后（call_llm 返回后、工具分发/最终答案前）
/// 4. before_tool   - 每次工具调用前（可修改工具调用参数）
/// 5. after_tool    - 每次工具调用后
/// ── 每轮 ReAct 迭代 ──
///
/// 6. after_agent   - Agent 完成后（可修改最终输出）
/// 7. on_error      - 发生错误时
```

### 1.2 新增 `before_model`（在 `after_tool` 后插入，第 72 行后）

```rust
    /// LLM 调用前调用（在每轮 ReAct 循环的 call_llm 之前）
    ///
    /// 可用于上下文压缩、token 预算检查等预处理操作。
    /// 默认空实现。
    async fn before_model(&self, state: &mut S) -> AgentResult<()> {
        let _ = state;
        Ok(())
    }
```

### 1.3 新增 `after_model`（在 `before_model` 后、`after_agent` 前）

```rust
    /// LLM 调用后调用（call_llm 返回后、工具分发或最终答案处理前）
    ///
    /// `reasoning` 包含模型的完整响应（思考文本、工具调用列表、最终答案）。
    /// 可用于响应后处理、token 累积校验、日志记录等。
    /// 默认空实现。
    async fn after_model(
        &self,
        state: &mut S,
        reasoning: &crate::agent::react::Reasoning,
    ) -> AgentResult<()> {
        let _ = (state, reasoning);
        Ok(())
    }
```

**注意:** `Reasoning` 类型路径需为 `crate::agent::react::Reasoning`（trait.rs 顶层已有 `use crate::agent::react::AgentOutput`，可补 `Reasoning`）。建议在文件头部 import 中追加：

```rust
use crate::agent::react::{AgentOutput, Reasoning, ToolCall, ToolResult};
```

---

## Step 2: MiddlewareChain 新增两个方法

**文件:** `peri-agent/src/middleware/chain.rs`

### 2.1 `run_before_model`（在 `run_after_tool` 后插入，第 118 行后）

```rust
    /// 顺序执行 before_model 钩子
    ///
    /// 在每个 ReAct step 的 LLM 调用前执行。
    /// 遇错即停——后续中间件不执行，错误向上传播。
    pub async fn run_before_model(&self, state: &mut S) -> AgentResult<()> {
        for middleware in &self.middlewares {
            middleware.before_model(state).await?;
        }
        Ok(())
    }
```

### 2.2 `run_after_model`（在 `run_before_model` 后）

**Chain.rs 头部 import 更新：**
```rust
use crate::agent::react::{AgentOutput, Reasoning, ToolCall, ToolResult};
```

```rust
    /// 顺序执行 after_model 钩子
    ///
    /// 在 LLM 调用返回后、工具分发或最终答案处理前执行。
    /// 传入完整的 `Reasoning`（思考文本、工具调用、最终答案）供中间件检查。
    /// 遇错即停。
    pub async fn run_after_model(
        &self,
        state: &mut S,
        reasoning: &Reasoning,
    ) -> AgentResult<()> {
        for middleware in &self.middlewares {
            middleware.after_model(state, reasoning).await?;
        }
        Ok(())
    }
```

**设计要点:**
- 两者均按序执行，遇错即停
- `before_model`: 纯副作用钩子，仅接收 `&mut S`
- `after_model`: 额外接收 `&Reasoning`（只读引用），中间件可检查模型响应但不可修改

**为何 `after_model` 传入 `&Reasoning` 而非 owned?**
- `Reasoning` 后续还要用于 `dispatch_tools` / `handle_final_answer`——不能消费
- `AgentResult<()>` 而非 `AgentResult<Reasoning>`：避免 19 个中间件的默认实现路径产生 clone 开销（`source_message: Option<BaseMessage>` 较重）。若改为修改链模式，默认实现需 `Ok(reasoning.clone())`，与 `after_agent` 的 `Ok(output.clone())` 一致但代价更大。观察链模式零克隆开销

---

## Step 3: ReAct 循环插入调用点

**文件:** `peri-agent/src/agent/executor/mod.rs`，第 243-281 行的 `for step` 循环。

**修改后:**

```rust
        for step in 0..self.max_iterations {
            state.set_current_step(step);

            // 钩子: before_model — LLM 调用前（compact 检查点）
            self.chain.run_before_model(state).await?;

            // LLM 推理
            let reasoning =
                self::llm_step::call_llm(self, state, &tool_refs, step, &cancel).await?;

            // 钩子: after_model — LLM 调用后（响应后处理）
            self.chain.run_after_model(state, &reasoning).await?;

            if reasoning.needs_tool_call() {
                // 工具分发
                // ... 后续不变
```

**两个钩子退出的影响:**

| 钩子 | `?` 传播场景 | 行为 |
|------|-------------|------|
| `before_model` 报错 | state 已修改（如 compact 失败半途） | 错误向上传播，executor 停止。state 通过 `&mut` 引用仍可被调用者访问（与所有现有钩子一致） |
| `after_model` 报错 | `reasoning` 完整，state 可能被前序中间件修改 | 同上——错误停止循环，不会进入工具分发。`call_llm` 错误路径走 `on_error`，不触发 `after_model` |

---

## Step 4: 测试新增

**文件:** `peri-agent/src/middleware/chain_test.rs`，在现有测试末尾追加。

**需追加的 import:**
```rust
use crate::agent::react::Reasoning;  // 测试中构造 Reasoning 需要
```

### 4.1 测试 `before_model` 顺序执行

```rust
    #[tokio::test]
    async fn test_before_model_sequential_order() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();

        struct BeforeModelRecorder {
            name: String,
            log: Arc<Mutex<Vec<String>>>,
        }
        #[async_trait]
        impl Middleware<AgentState> for BeforeModelRecorder {
            fn name(&self) -> &str { &self.name }
            async fn before_model(&self, _state: &mut AgentState) -> AgentResult<()> {
                self.log.lock().unwrap().push(format!("{}.before_model", self.name));
                Ok(())
            }
        }

        chain.add(Box::new(BeforeModelRecorder { name: "A".into(), log: Arc::clone(&log) }));
        chain.add(Box::new(BeforeModelRecorder { name: "B".into(), log: Arc::clone(&log) }));
        chain.add(Box::new(BeforeModelRecorder { name: "C".into(), log: Arc::clone(&log) }));

        let mut state = AgentState::new("/tmp");
        chain.run_before_model(&mut state).await.unwrap();

        assert_eq!(
            log.lock().unwrap().clone(),
            vec!["A.before_model", "B.before_model", "C.before_model"]
        );
    }
```

### 4.2 测试 `before_model` 错误短路

```rust
    #[tokio::test]
    async fn test_before_model_error_short_circuits() {
        struct FailBeforeModel;
        #[async_trait]
        impl Middleware<AgentState> for FailBeforeModel {
            fn name(&self) -> &str { "FailBeforeModel" }
            async fn before_model(&self, _state: &mut AgentState) -> AgentResult<()> {
                Err(AgentError::MiddlewareError {
                    middleware: "FailBeforeModel".to_string(),
                    reason: "intentional failure".to_string(),
                })
            }
        }

        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();

        struct Recorder { name: String, log: Arc<Mutex<Vec<String>>> }
        #[async_trait]
        impl Middleware<AgentState> for Recorder {
            fn name(&self) -> &str { &self.name }
            async fn before_model(&self, _state: &mut AgentState) -> AgentResult<()> {
                self.log.lock().unwrap().push(format!("{}.before_model", self.name));
                Ok(())
            }
        }

        chain.add(Box::new(Recorder { name: "A".into(), log: Arc::clone(&log) }));
        chain.add(Box::new(FailBeforeModel));
        chain.add(Box::new(Recorder { name: "B".into(), log: Arc::clone(&log) }));

        let mut state = AgentState::new("/tmp");
        let result = chain.run_before_model(&mut state).await;

        assert!(result.is_err());
        assert_eq!(log.lock().unwrap().clone(), vec!["A.before_model"]);
    }
```

### 4.3 测试 `after_model` 顺序执行

```rust
    #[tokio::test]
    async fn test_after_model_sequential_order() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();

        struct AfterModelRecorder {
            name: String,
            log: Arc<Mutex<Vec<String>>>,
        }
        #[async_trait]
        impl Middleware<AgentState> for AfterModelRecorder {
            fn name(&self) -> &str { &self.name }
            async fn after_model(&self, _state: &mut AgentState, _reasoning: &Reasoning) -> AgentResult<()> {
                self.log.lock().unwrap().push(format!("{}.after_model", self.name));
                Ok(())
            }
        }

        chain.add(Box::new(AfterModelRecorder { name: "A".into(), log: Arc::clone(&log) }));
        chain.add(Box::new(AfterModelRecorder { name: "B".into(), log: Arc::clone(&log) }));

        let mut state = AgentState::new("/tmp");
        let reasoning = Reasoning {
            thought: String::new(),
            final_answer: None,
            tool_calls: vec![],
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
        };
        chain.run_after_model(&mut state, &reasoning).await.unwrap();

        assert_eq!(
            log.lock().unwrap().clone(),
            vec!["A.after_model", "B.after_model"]
        );
    }
```

### 4.4 测试 `after_model` 错误短路

```rust
    #[tokio::test]
    async fn test_after_model_error_short_circuits() {
        struct FailAfterModel;
        #[async_trait]
        impl Middleware<AgentState> for FailAfterModel {
            fn name(&self) -> &str { "FailAfterModel" }
            async fn after_model(&self, _state: &mut AgentState, _reasoning: &Reasoning) -> AgentResult<()> {
                Err(AgentError::MiddlewareError {
                    middleware: "FailAfterModel".to_string(),
                    reason: "intentional failure".to_string(),
                })
            }
        }

        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();

        struct Recorder { name: String, log: Arc<Mutex<Vec<String>>> }
        #[async_trait]
        impl Middleware<AgentState> for Recorder {
            fn name(&self) -> &str { &self.name }
            async fn after_model(&self, _state: &mut AgentState, _reasoning: &Reasoning) -> AgentResult<()> {
                self.log.lock().unwrap().push(format!("{}.after_model", self.name));
                Ok(())
            }
        }

        chain.add(Box::new(Recorder { name: "A".into(), log: Arc::clone(&log) }));
        chain.add(Box::new(FailAfterModel));
        chain.add(Box::new(Recorder { name: "B".into(), log: Arc::clone(&log) }));

        let mut state = AgentState::new("/tmp");
        let reasoning = Reasoning {
            thought: String::new(),
            final_answer: None,
            tool_calls: vec![],
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
        };
        let result = chain.run_after_model(&mut state, &reasoning).await;

        assert!(result.is_err());
        assert_eq!(log.lock().unwrap().clone(), vec!["A.after_model"]);
    }
```

### 4.5 测试空链 + 默认 no-op

```rust
    #[tokio::test]
    async fn test_before_model_empty_chain_ok() {
        let chain = MiddlewareChain::<AgentState>::new();
        let mut state = AgentState::new("/tmp");
        assert!(chain.run_before_model(&mut state).await.is_ok());
    }

    #[tokio::test]
    async fn test_after_model_empty_chain_ok() {
        let chain = MiddlewareChain::<AgentState>::new();
        let mut state = AgentState::new("/tmp");
        let reasoning = Reasoning {
            thought: String::new(),
            final_answer: None,
            tool_calls: vec![],
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
        };
        assert!(chain.run_after_model(&mut state, &reasoning).await.is_ok());
    }

    #[tokio::test]
    async fn test_new_hooks_default_noop() {
        // NoopMiddleware 的 before_model/after_model 默认实现不应报错
        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(NoopMiddleware::new("noop")));
        let mut state = AgentState::new("/tmp");

        chain.run_before_model(&mut state).await.unwrap();

        let reasoning = Reasoning {
            thought: String::new(),
            final_answer: None,
            tool_calls: vec![],
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
        };
        chain.run_after_model(&mut state, &reasoning).await.unwrap();
    }
```

### 4.6 测试混合 Hook 链（不同中间件覆盖不同钩子）

```rust
    /// 验证 before_model 和 after_model 在同一链中独立执行：
    /// A 覆盖两个钩子，B 只覆盖 before_model，C 只覆盖 after_model。
    /// run_before_model 应触发 A+B 但跳过 C；
    /// run_after_model 应触发 A+C 但跳过 B。
    #[tokio::test]
    async fn test_mixed_before_and_after_model_in_same_chain() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));

        // A 覆盖两个钩子
        struct BothHooks { log: Arc<Mutex<Vec<String>>> }
        #[async_trait]
        impl Middleware<AgentState> for BothHooks {
            fn name(&self) -> &str { "A" }
            async fn before_model(&self, _state: &mut AgentState) -> AgentResult<()> {
                self.log.lock().unwrap().push("A.before_model".into());
                Ok(())
            }
            async fn after_model(&self, _state: &mut AgentState, _r: &Reasoning) -> AgentResult<()> {
                self.log.lock().unwrap().push("A.after_model".into());
                Ok(())
            }
        }

        // B 只覆盖 before_model
        struct BeforeOnly { log: Arc<Mutex<Vec<String>>> }
        #[async_trait]
        impl Middleware<AgentState> for BeforeOnly {
            fn name(&self) -> &str { "B" }
            async fn before_model(&self, _state: &mut AgentState) -> AgentResult<()> {
                self.log.lock().unwrap().push("B.before_model".into());
                Ok(())
            }
        }

        // C 只覆盖 after_model
        struct AfterOnly { log: Arc<Mutex<Vec<String>>> }
        #[async_trait]
        impl Middleware<AgentState> for AfterOnly {
            fn name(&self) -> &str { "C" }
            async fn after_model(&self, _state: &mut AgentState, _r: &Reasoning) -> AgentResult<()> {
                self.log.lock().unwrap().push("C.after_model".into());
                Ok(())
            }
        }

        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(BothHooks { log: Arc::clone(&log) }));
        chain.add(Box::new(BeforeOnly { log: Arc::clone(&log) }));
        chain.add(Box::new(AfterOnly { log: Arc::clone(&log) }));

        let mut state = AgentState::new("/tmp");

        // run_before_model: A + B 执行，C 不执行
        log.lock().unwrap().clear();
        chain.run_before_model(&mut state).await.unwrap();
        assert_eq!(
            log.lock().unwrap().clone(),
            vec!["A.before_model", "B.before_model"]
        );

        // run_after_model: A + C 执行，B 不执行
        log.lock().unwrap().clear();
        let reasoning = Reasoning {
            thought: "test".into(),
            final_answer: None,
            tool_calls: vec![],
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
        };
        chain.run_after_model(&mut state, &reasoning).await.unwrap();
        assert_eq!(
            log.lock().unwrap().clone(),
            vec!["A.after_model", "C.after_model"]
        );
    }
```

### 4.7 测试 state 变更在钩子间可见

```rust
    /// before_model 修改 state（如添加消息），随后 after_model 应能读取该修改。
    #[tokio::test]
    async fn test_state_mutation_visible_across_hooks() {
        let marker_id = Arc::new(Mutex::new(None::<MessageId>));

        struct Writer { marker_id: Arc<Mutex<Option<MessageId>>> }
        #[async_trait]
        impl Middleware<AgentState> for Writer {
            fn name(&self) -> &str { "Writer" }
            async fn before_model(&self, state: &mut AgentState) -> AgentResult<()> {
                let msg = BaseMessage::system(
                    ContentBlock::text("marker written by before_model"),
                );
                let id = msg.id();
                state.add_message(msg);
                *self.marker_id.lock().unwrap() = Some(id);
                Ok(())
            }
        }

        struct Reader { marker_id: Arc<Mutex<Option<MessageId>>> }
        #[async_trait]
        impl Middleware<AgentState> for Reader {
            fn name(&self) -> &str { "Reader" }
            async fn after_model(&self, state: &mut AgentState, _r: &Reasoning) -> AgentResult<()> {
                let expected_id = self.marker_id.lock().unwrap().unwrap();
                let found = state.messages().iter().any(|m| m.id() == Some(expected_id));
                assert!(found, "after_model 应能看到 before_model 写入的消息");
                Ok(())
            }
        }

        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(Writer { marker_id: Arc::clone(&marker_id) }));
        chain.add(Box::new(Reader { marker_id: Arc::clone(&marker_id) }));

        let mut state = AgentState::new("/tmp");
        chain.run_before_model(&mut state).await.unwrap();

        let reasoning = Reasoning {
            thought: String::new(),
            final_answer: None,
            tool_calls: vec![],
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
        };
        chain.run_after_model(&mut state, &reasoning).await.unwrap();
    }
```

### 4.8 测试含 tool_calls 和 final_answer 的 Reasoning 传递

```rust
    /// 验证 after_model 可接收含工具调用的 Reasoning（非空 vec![]）。
    #[tokio::test]
    async fn test_after_model_with_tool_calls() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));

        struct Inspector { log: Arc<Mutex<Vec<String>>> }
        #[async_trait]
        impl Middleware<AgentState> for Inspector {
            fn name(&self) -> &str { "Inspector" }
            async fn after_model(&self, _state: &mut AgentState, r: &Reasoning) -> AgentResult<()> {
                self.log.lock().unwrap().push(format!("tool_count={}", r.tool_calls.len()));
                self.log.lock().unwrap().push(format!("has_answer={}", r.final_answer.is_some()));
                Ok(())
            }
        }

        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(Inspector { log: Arc::clone(&log) }));

        let mut state = AgentState::new("/tmp");
        let reasoning = Reasoning {
            thought: "need to search".into(),
            final_answer: Some("final answer".into()),
            tool_calls: vec![
                ToolCall::new("tc1", "test_read".into(), serde_json::json!({})),
                ToolCall::new("tc2", "test_write".into(), serde_json::json!({})),
            ],
            source_message: None,
            usage: None,
            model: "test-model".into(),
            streamed: false,
        };
        chain.run_after_model(&mut state, &reasoning).await.unwrap();

        let captured = log.lock().unwrap().clone();
        assert!(captured.contains(&"tool_count=2".to_string()));
        assert!(captured.contains(&"has_answer=true".to_string()));
    }
```

### 4.9 测试未覆盖新钩子的中间件也能正常工作

```rust
    /// 验证仅覆盖旧钩子（before_tool、after_tool 等）的中间件
    /// 在新钩子被调用时不报错（默认空实现）。
    #[tokio::test]
    async fn test_unrelated_middleware_ignores_new_hooks() {
        // OrderRecorder 仅覆盖 name()、before_tool()、after_tool()
        // 其 before_model/after_model 使用默认空实现
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(OrderRecorder { name: "A".into(), log: Arc::clone(&log) }));
        chain.add(Box::new(OrderRecorder { name: "B".into(), log: Arc::clone(&log) }));

        let mut state = AgentState::new("/tmp");
        // 不应报错
        chain.run_before_model(&mut state).await.unwrap();

        let reasoning = Reasoning {
            thought: String::new(),
            final_answer: None,
            tool_calls: vec![],
            source_message: None,
            usage: None,
            model: String::new(),
            streamed: false,
        };
        chain.run_after_model(&mut state, &reasoning).await.unwrap();

        // 确认没有日志写入（OrderRecorder 未覆盖新钩子）
        assert!(log.lock().unwrap().is_empty());
    }
```

---

## 全量验证

```bash
cargo build -p peri-agent                  # 编译通过
cargo test -p peri-agent --lib             # peri-agent 单元测试
cargo test -p peri-middlewares --lib       # 中间件 crate 测试
cargo test --workspace                     # 全量（确保无回归）
```

**预期:** 所有现有测试通过，新增 11 个测试通过（2 before_model + 2 after_model + 2 空链 + 1 noop + 1 混合钩子 + 1 state 可见性 + 1 含工具调用 + 1 旧中间件兼容）。

---

## 风险

| 风险 | 实际风险等级 |
|------|-------------|
| 19 个现有中间件实现因新增方法编译失败 | **零风险** — 默认空实现，编译器不强制覆盖 |
| `async_trait` vtable 膨胀 | **低** — 新增 2 个空方法不影响 trait object 大小 |
| `Reasoning` 类型需引入 import | **低** — 在 `trait.rs` 已有 `use crate::agent::react::AgentOutput`，追加 `Reasoning` 即可 |
| `after_model` 报错导致未进入工具分发 | **设计意图** — 中间件显式拒绝时不应继续；若需非致命警告，中间件自行 `tracing::warn!` 后返回 `Ok(())` |
