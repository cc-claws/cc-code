# Multi-Agent Design (Fork + Prompt 优化) 执行计划

**目标:** 为 SubAgent 新增 Fork 路径（继承父消息历史），并优化 System Prompt 中的 Agent 工具指导段落

**技术栈:** Rust (tokio async + async-trait), parking_lot::RwLock 共享消息引用, include_str! 编译时嵌入 + 运行时占位符替换

**设计文档:** spec-design.md

## 改动总览

本次改动涉及 **7 个文件**（4 修改 + 1 重写 + 2 小改），分两条主线：
- **Fork 路径**（Task 1）：SubAgentMiddleware 新增 `parent_messages` 字段并在 `before_agent` 中快照消息 → SubAgentTool 新增 `invoke_fork()` 方法 → agent.rs 创建共享引用并注入。防递归通过 fork directive 规则约束（非硬编码排除 Agent），保持 tools-block cache 命中。
- **Prompt 优化**（Task 2）：11_subagent.md 从 21 行扩写为 ~85 行完整指南 → prompt.rs 新增 `format_available_agents` + `{{available_agents}}` 占位符替换 → mod.rs 移除 `before_agent` 中的 agent 摘要 prepend、`scan_agents` 改 pub、删除 `build_agents_summary` → lib.rs re-export。
- **依赖关系**：Task 2 依赖 Task 1（修改同一 `before_agent` 函数）。Task 2 的 `before_agent` 简化步骤需在 Task 1 的 `parent_messages` 快照逻辑之后执行。
- **关键决策**：使用 `Arc<parking_lot::RwLock<Vec<BaseMessage>>>` 共享消息快照（读多写少）；Fork 子 agent 注册全量父工具含 Agent 自身（cache 命中）；agent 列表注入从 middleware prepend 迁移到 system prompt 占位符（一次注入、结构统一）。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - `cargo build -p peri-middlewares -p peri-tui`
- [x] 验证测试工具可用
  - `cargo test -p peri-middlewares --lib -- subagent::tool::tests::test_tool_name`
  - `cargo test -p peri-tui --lib -- prompt::tests::test_no_overrides_contains_all_sections`

**检查步骤:**
- [x] 构建成功
  - `cargo build -p peri-middlewares -p peri-tui 2>&1 | tail -3`
  - 预期: `Finished`，无 error
- [x] 测试框架可用
  - `cargo test -p peri-middlewares --lib -- subagent 2>&1 | tail -5`
  - 预期: 现有测试全部通过，0 failed

---

### Task 1: Fork 路径端到端实现

**背景:**
实现 Fork 路径——子 agent 继承父 agent 的完整消息历史 + system prompt + 工具集，使 LLM 在子 agent 上下文中拥有与父等价的信息，同时保持 Anthropic Prompt Cache 命中。当前 SubAgentTool 只有 Normal 路径（`subagent_type` → agent 定义文件 → 独立上下文），无法复用父消息历史。Task 2（System Prompt 优化）和 Task 3（验收）依赖本 Task 的 `fork` 参数基础。

**涉及文件:**
- 修改: `peri-middlewares/src/subagent/mod.rs`
- 修改: `peri-middlewares/src/subagent/tool.rs`
- 修改: `peri-tui/src/app/agent.rs`

**执行步骤:**

- [x] 在 SubAgentMiddleware 中新增 `parent_messages` 字段和 `with_parent_messages()` builder
  - 位置: `peri-middlewares/src/subagent/mod.rs` `SubAgentMiddleware` 结构体定义（~L45）
  - 新增字段:
    ```rust
    /// 父 agent 消息快照的共享引用，在 before_agent 中写入，供 Fork 子 agent 读取
    parent_messages: Option<Arc<parking_lot::RwLock<Vec<BaseMessage>>>>,
    ```
  - 在 `new()` 构造器（~L61）中初始化为 `None`
  - 新增 builder 方法（在 `with_cancel()` 之后，~L92）:
    ```rust
    pub fn with_parent_messages(
        mut self,
        messages: Arc<parking_lot::RwLock<Vec<BaseMessage>>>,
    ) -> Self {
        self.parent_messages = Some(messages);
        self
    }
    ```
  - 在文件顶部 `use std::sync::Arc;` 之后追加 `use parking_lot::RwLock;`（已有 Arc，仅追加 RwLock）
  - 原因: 中间件持有共享引用，在 `before_agent` 中写入快照，在 `build_tool` 中传递给 SubAgentTool

- [x] 在 SubAgentMiddleware::before_agent 中写入父消息快照
  - 位置: `peri-middlewares/src/subagent/mod.rs` `before_agent()` 方法（~L205）
  - 在方法体开头（`let cwd = ...` 之前）插入快照逻辑:
    ```rust
    // 快照当前 state.messages 到共享引用，供 Fork 子 agent 继承
    // 在 prepend agent summary 之前执行，避免子 agent 继承这份 agent 列表摘要
    if let Some(ref pm) = self.parent_messages {
        *pm.write() = state.messages().to_vec();
    }
    ```
  - 原因: 快照时机在 prepend 之前，确保 Fork 子 agent 获得的是纯对话历史，不含本中间件注入的 agent 摘要

- [x] 在 SubAgentMiddleware::build_tool 中传递 parent_messages
  - 位置: `peri-middlewares/src/subagent/mod.rs` `build_tool()` 方法（~L95）
  - 在现有的 `if let Some(ref cancel)` 块之后（~L108）追加:
    ```rust
    if let Some(ref pm) = self.parent_messages {
        tool = tool.with_parent_messages(Arc::clone(pm));
    }
    ```
  - 原因: 将共享消息引用传递给工具实例，使 invoke_fork 可读取

- [x] 在 SubAgentTool 中新增 `parent_messages` 字段和相关方法
  - 位置: `peri-middlewares/src/subagent/tool.rs` `SubAgentTool` 结构体（~L43）
  - 新增字段（在 `cancel` 字段之后，~L59）:
    ```rust
    /// 父 agent 消息快照的共享引用（Fork 路径使用）
    /// RwLock.read() 获取深拷贝，RwLock.write() 由 SubAgentMiddleware::before_agent 更新
    parent_messages: Option<Arc<parking_lot::RwLock<Vec<BaseMessage>>>>,
    ```
  - 在 `new()` 构造器（~L63）的 Self 初始化中追加 `parent_messages: None`
  - 新增 builder 方法（在 `with_cancel()` 之后，~L92）:
    ```rust
    /// 设置父消息共享引用，Fork 路径通过 RwLock.read() 获取深拷贝
    pub fn with_parent_messages(
        mut self,
        messages: Arc<parking_lot::RwLock<Vec<BaseMessage>>>,
    ) -> Self {
        self.parent_messages = Some(messages);
        self
    }
    ```
  - 在文件顶部追加 `use parking_lot::RwLock;`（已有 `use std::sync::Arc;`）
  - 原因: SubAgentTool 需要访问父消息历史以构建 Fork 子 agent 的初始状态

- [x] 在 SubAgentTool::parameters() 中新增 `fork` 属性
  - 位置: `peri-middlewares/src/subagent/tool.rs` `parameters()` 方法（~L170）
  - 在 `properties` 对象中追加（在 `cwd` 属性之后，~L203）:
    ```rust
    "fork": {
        "type": "boolean",
        "description": "Set to true to fork the current agent with full conversation context. The forked agent inherits all messages, tools, and system prompt from the parent. Use when the task requires context from the ongoing conversation"
    }
    ```
  - 原因: LLM 通过 `fork: true` 参数触发 Fork 路径

- [x] 重构 SubAgentTool::invoke() 为 Normal/Fork 双分支
  - 位置: `peri-middlewares/src/subagent/tool.rs` `invoke()` 方法（~L207）
  - 在提取 `cwd` 之后（~L225）、`// 1. 查找 agent 定义文件` 之前（~L227），插入 Fork 检测分支:
    ```rust
    let is_fork = input.get("fork").and_then(|v| v.as_bool()).unwrap_or(false);
    if is_fork {
        return self.invoke_fork(&prompt, &cwd).await;
    }
    ```
  - 原因: Fork 路径跳过 agent 定义文件查找和工具过滤，直接进入独立执行逻辑

- [x] 实现 SubAgentTool::invoke_fork() 方法
  - 位置: `peri-middlewares/src/subagent/tool.rs` `impl SubAgentTool` 块中（在 `filter_tools()` 之后，`#[async_trait] impl BaseTool` 之前，~L158）
  - 方法签名和实现:
    ```rust
    /// Fork 路径：子 agent 继承父的完整消息历史 + system prompt + 工具集
    async fn invoke_fork(
        &self,
        prompt: &str,
        cwd: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // 1. 获取父消息深拷贝
        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Ok("错误：Fork 路径需要父消息历史，但 parent_messages 未设置".to_string()),
        };

        // 2. 构建 fork directive Human 消息
        let fork_directive = format!(
            "<fork_directive>\n\
             You are a forked agent continuing from the parent conversation.\n\
             You have full access to the conversation history above.\n\
             \n\
             RULES:\n\
             1. Do NOT spawn sub-agents — execute directly using your tools\n\
             2. Do NOT ask questions — act on the directive below\n\
             3. Stay strictly within your assigned scope\n\
             4. Report structured facts, then stop\n\
             5. Keep your response under 500 words unless specified otherwise\n\
             \n\
             Output format:\n\
               Scope: <your assigned scope in one sentence>\n\
               Result: <the answer or key findings>\n\
               Key files: <relevant file paths>\n\
               Files changed: <list if you modified files>\n\
             </fork_directive>\n\n\
             {prompt}"
        );

        // 3. 用深拷贝的父消息构建子 AgentState
        let mut fork_state = AgentState::with_messages(cwd.to_string(), parent_msgs);

        // 4. 组装子 ReActAgent（与 Normal 路径相同的中间件链）
        let llm = (self.llm_factory)(None);
        let mut agent_builder = ReActAgent::new(llm).max_iterations(200);

        // 中间件链：AgentsMd → Skills → SkillPreload → Todo
        agent_builder = agent_builder
            .add_middleware(Box::new(AgentsMdMiddleware::new()))
            .add_middleware(Box::new(SkillsMiddleware::new().with_global_config()))
            .add_middleware(Box::new(TodoMiddleware::new({
                let (tx, _rx) = mpsc::channel(8);
                tx
            })));

        // 5. 注入 system prompt（通过 system_builder 获取，与 Normal 路径一致）
        if let Some(ref builder) = self.system_builder {
            let system_content = builder(None, cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // 6. 注册全量父工具（无过滤，包含 Agent 自身以保持 cache 命中）
        for tool in self.parent_tools.iter() {
            agent_builder = agent_builder.register_tool(
                Box::new(crate::tools::ArcToolWrapper(Arc::clone(tool)))
                    as Box<dyn BaseTool>
            );
        }

        // 7. 透传父事件处理器
        if let Some(handler) = &self.event_handler {
            agent_builder = agent_builder.with_event_handler(Arc::clone(handler));
        }

        // 8. 执行（input = fork_directive，会被 execute() 追加为 Human 消息）
        match agent_builder
            .execute(
                AgentInput::text(fork_directive),
                &mut fork_state,
                self.cancel.clone(),
            )
            .await
        {
            Ok(output) => Ok(format_subagent_result(&output)),
            Err(peri_agent::error::AgentError::Interrupted) => {
                Ok("Fork 子 agent 执行被中断".to_string())
            }
            Err(e) => {
                let msg = format!("Fork 子 agent 执行失败：{}", e);
                Err(msg.into())
            }
        }
    }
    ```
  - 原因: Fork 路径的核心实现——深拷贝父消息、构建 directive、组装与 Normal 路径一致的中间件链、注册全量工具、执行子 agent

- [x] 在 agent.rs 中创建共享消息引用并传入 SubAgentMiddleware
  - 位置: `peri-tui/src/app/agent.rs` SubAgent 组装处（~L216）
  - 在 `let subagent = SubAgentMiddleware::new(...)` 之前（~L215）新增:
    ```rust
    // 父消息快照共享引用：SubAgentMiddleware::before_agent 写入，Fork 子 agent 读取
    let parent_messages: Arc<parking_lot::RwLock<Vec<BaseMessage>>> =
        Arc::new(parking_lot::RwLock::new(Vec::new()));
    ```
  - 修改 SubAgentMiddleware 组装链（~L216-225），在 `.with_cancel(cancel.clone())` 之后追加:
    ```rust
    .with_parent_messages(parent_messages)
    ```
  - 原因: TUI 层创建共享引用并注入中间件，在每轮 ReAct 开始时自动快照消息

- [x] 更新 AGENT_DESCRIPTION 常量，新增 fork 参数说明
  - 位置: `peri-middlewares/src/subagent/tool.rs` `AGENT_DESCRIPTION` 常量（~L25）
  - 在 "When to use:" 段落之前（~L34）插入:
    ```
    Fork mode (fork: true):
    - Inherits the parent agent's full conversation history, system prompt, and tool set
    - The prompt is treated as a directive within the existing context, not a standalone briefing
    - Do NOT re-explain background that is already in the conversation history
    - Use for tasks that require context from the ongoing conversation (e.g., continuing a multi-file refactor)
    - The forked agent follows a structured output format: Scope, Result, Key files, Files changed
    ```
  - 原因: LLM 根据 description 决定是否使用 fork 模式，需要清晰的用法指导

- [x] 为 Fork 路径核心逻辑编写单元测试
  - 测试文件: `peri-middlewares/src/subagent/tool.rs` 内 `#[cfg(test)] mod tests`（在文件末尾 `}` 之前追加）
  - 测试场景:
    - **test_fork_inherits_parent_messages**: 构造包含 2 条历史消息的 parent_messages → 调用 `fork: true` → 验证 MockLLM 收到的 messages 长度为 2(历史) + 1(system) + 1(fork directive) = 4
    - **test_fork_registers_all_tools_including_agent**: 构造 parent_tools 含 Read/Agent → 调用 `fork: true` → 验证 MockLLM 收到的 tools 中包含 "Agent"（不排除）
    - **test_fork_without_parent_messages_returns_error**: 不设置 parent_messages → 调用 `fork: true` → 验证返回错误消息
    - **test_fork_system_prompt_consistent**: 设置 system_builder → 调用 `fork: true` → 验证 MockLLM 收到的 system 消息内容与 builder 返回一致
    - **test_fork_directive_includes_rules**: 调用 `fork: true` + prompt → 验证 MockLLM 收到的最后一条消息包含 `<fork_directive>` 和 `RULES`
  - 运行命令: `cargo test -p peri-middlewares --lib -- subagent::tool::tests::test_fork`
  - 预期: 5 个 fork 相关测试全部通过

- [x] 为 SubAgentMiddleware 的 parent_messages 传递编写单元测试
  - 测试文件: `peri-middlewares/src/subagent/mod.rs` 内 `#[cfg(test)] mod tests`
  - 测试场景:
    - **test_before_agent_snapshots_messages**: 创建含 2 条消息的 AgentState → 调用 before_agent → 验证 parent_messages RwLock 中包含 2 条消息
    - **test_build_tool_receives_parent_messages**: 设置 with_parent_messages → 调用 build_tool → 验证返回的 SubAgentTool 包含 parent_messages
  - 运行命令: `cargo test -p peri-middlewares --lib -- subagent::tests::test_parent_messages`
  - 预期: 2 个传递测试通过

**检查步骤:**
- [x] 编译通过
  - `cargo build -p peri-middlewares -p peri-tui 2>&1 | tail -5`
  - 预期: 输出 `Compiling ...` 后无 error，最终显示 `Finished`
- [x] Fork 路径测试全部通过
  - `cargo test -p peri-middlewares --lib -- test_fork 2>&1 | tail -10`
  - 预期: 5 个 test_fork 测试全部 `ok`
- [x] 中间件传递测试通过
  - `cargo test -p peri-middlewares --lib -- test_parent_messages 2>&1 | tail -5`
  - 预期: 2 个 test_parent_messages 测试全部 `ok`
- [x] 现有 Normal 路径测试无回归
  - `cargo test -p peri-middlewares --lib -- subagent 2>&1 | tail -15`
  - 预期: 所有现有测试 + 新增测试全部 `ok`，0 failed

**认知变更:**
- [x] [CLAUDE.md] SubAgentMiddleware 新增 `parent_messages: Arc<parking_lot::RwLock<Vec<BaseMessage>>>` 字段，在 `before_agent` 中于 prepend agent summary 之前快照 state.messages。Fork 子 agent 通过 `RwLock.read().clone()` 获取深拷贝，不修改原始历史。
- [x] [CLAUDE.md] Fork 子 agent 注册全量父工具（包含 Agent 自身），防递归通过 fork directive 规则约束而非硬编码排除。这是为了保持工具列表与父完全一致，最大化 Anthropic Prompt Cache 的 tools-block 命中。

---

### Task 2: System Prompt 指导优化 + 动态 Agent 列表注入

**背景:**
优化 System Prompt 中 Agent 工具指导段落，从 21 行扩写为完整的委派指南（使用时机/反模式/prompt 写作技巧/Fork 模式/示例），并将 agent 列表注入从 `before_agent` 的 System 消息 prepend 迁移到 system prompt 占位符替换，实现一次注入、结构统一。当前 `11_subagent.md` 指导过于简略，LLM 委派决策和 prompt 质量不稳定；`before_agent` 中的 `scan_agents + build_agents_summary + prepend_message` 逻辑将移除，agent 列表改为通过 `{{available_agents}}` 占位符在 system prompt 构建时注入。本 Task 依赖 Task 1 完成后 `before_agent` 中新增的 `parent_messages` 快照逻辑（保留快照、移除摘要注入）。

**涉及文件:**
- 修改: `peri-tui/prompts/sections/11_subagent.md`
- 修改: `peri-tui/src/prompt.rs`
- 修改: `peri-middlewares/src/subagent/mod.rs`
- 修改: `peri-middlewares/src/lib.rs`

**执行步骤:**

- [x] 重写 `11_subagent.md` 为完整委派指南
  - 位置: `peri-tui/prompts/sections/11_subagent.md`（全文替换）
  - 将现有 21 行内容替换为以下 8 段结构（参考 spec-design.md §3.2，使用英文匹配项目 prompt 风格）：
    ```markdown
    # SubAgent Delegation

    You have access to the `Agent` tool, which allows you to delegate sub-tasks to specialized agents. Agents are defined in `.claude/agents/{subagent_type}.md` or `.claude/agents/{subagent_type}/agent.md`.

    ## Available agent types

    {{available_agents}}

    ## When to use sub-agents

    - For tasks that benefit from independent context isolation (e.g., code review while working on a different feature)
    - For tasks requiring specialized persona or behavior defined in agent configuration files
    - For parallelizable sub-tasks that do not depend on each other's results
    - When you need to break a complex task into smaller, independently executable pieces

    ## When NOT to use sub-agents

    - To read a specific file → use the `Read` tool directly
    - To search for class or function definitions → use the `Grep` tool directly
    - To find files by name pattern → use the `Glob` tool directly
    - For tasks that only require searching through 2-3 files → use the `Read` tool
    - For unrelated tasks that don't benefit from specialized agent behavior

    ## Writing the prompt

    When delegating to a sub-agent, write the prompt as if briefing a smart colleague who just joined the project:

    - Explain the **goal** and **why** — don't just list tasks
    - Include relevant **constraints** and **decisions already made** to avoid repeated exploration
    - Specify whether the sub-agent should **write code** or **only research**
    - If you need a brief answer, say so explicitly (e.g., "keep your response under 200 words")
    - Never delegate understanding — if you need to understand something, read it yourself first

    The sub-agent has **no access** to the parent conversation history. The `prompt` parameter must contain **all necessary context** for the sub-agent to complete its work independently.

    ## Fork mode (fork: true)

    When `fork` is set to `true`, the sub-agent inherits the full conversation history, system prompt, and tool set from the parent:

    - The `prompt` is treated as a **directive** within the existing context, not a standalone briefing
    - Do **not** re-explain background that is already in the conversation history
    - Use for tasks that require context from the ongoing conversation (e.g., continuing a multi-file refactor)
    - The forked agent follows a structured output format: **Scope**, **Result**, **Key files**, **Files changed**
    - Fork mode is mutually exclusive with `subagent_type` — when `fork: true`, the `subagent_type` parameter is ignored

    ## Usage notes

    - Always include a short `description` (3-5 words) when calling the Agent tool — this helps with UI display and logging
    - Sub-agent results are **not directly visible to the user** — you must summarize and present the findings yourself
    - You can launch **multiple sub-agents in parallel** by including multiple `tool_use` blocks in a single message
    - Clearly tell the sub-agent whether it should **write code** or **only perform research**

    ## Examples

    **Example 1: Code review**

    <tool_call name="Agent">
    {"subagent_type": "code-reviewer", "description": "Review auth module", "prompt": "Review the authentication module in src/auth/ for security vulnerabilities. Focus on: 1) SQL injection risks, 2) Token handling, 3) Input validation. The module uses JWT with RS256 signing. Report findings with severity levels."}
    </tool_call

    **Example 2: Fork for multi-file refactor**

    <tool_call name="Agent">
    {"fork": true, "description": "Rename UserId type", "prompt": "Rename the `UserId` type to `AccountId` across all files in src/domain/. Update all type annotations, function signatures, and imports. Do NOT modify test files."}
    </tool_call

    **Example 3: Parallel research**

    <tool_call name="Agent">
    {"subagent_type": "researcher", "description": "Analyze error patterns", "prompt": "Analyze error handling patterns in src/services/. List all places where errors are silently swallowed (no logging, no propagation). Focus on the payment and order modules."}
    </tool_call
    ```
  - 原因: 从 21 行扩写为完整指南，覆盖 When NOT to use / Writing the prompt / Fork mode / Usage notes / Examples 段落，稳定 LLM 委派决策和 prompt 质量

- [x] 在 `mod.rs` 中将 `scan_agents` 改为 `pub`
  - 位置: `peri-middlewares/src/subagent/mod.rs` `scan_agents()` 函数签名（~L113）
  - 将 `fn scan_agents(cwd: &str)` 改为 `pub fn scan_agents(cwd: &str)`
  - 原因: `prompt.rs`（在 `peri-tui` crate 中）需要调用此函数扫描 agent 目录

- [x] 简化 `SubAgentMiddleware::before_agent`，移除 agent 摘要注入逻辑（先于 build_agents_summary 删除执行）
  - 位置: `peri-middlewares/src/subagent/mod.rs` `before_agent()` 方法（~L205-222）
  - Task 1 已在此方法开头添加 `parent_messages` 快照逻辑。本步骤将整个方法体替换为仅保留快照逻辑：
    ```rust
    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 快照当前 state.messages 到共享引用，供 Fork 子 agent 继承（Task 1）
        if let Some(ref pm) = self.parent_messages {
            *pm.write() = state.messages().to_vec();
        }
        Ok(())
    }
    ```
  - 原因: agent 列表注入已迁移到 system prompt 的 `{{available_agents}}` 占位符，`before_agent` 不再需要扫描 agent 目录和 prepend 摘要消息。必须先执行此步骤使 `build_agents_summary` 成为死代码，再删除该函数

- [x] 在 `mod.rs` 中移除 `build_agents_summary` 函数（在 before_agent 简化之后执行）
  - 位置: `peri-middlewares/src/subagent/mod.rs`（~L177-193）
  - 删除整个 `build_agents_summary` 函数体
  - 原因: before_agent 已简化，此函数成为死代码

- [x] 在 `lib.rs` 中 re-export `scan_agents`
  - 位置: `peri-middlewares/src/lib.rs`（~L46）
  - 将现有行:
    ```rust
    pub use subagent::{SkillPreloadMiddleware, SubAgentMiddleware, SubAgentTool};
    ```
    改为:
    ```rust
    pub use subagent::{scan_agents, SkillPreloadMiddleware, SubAgentMiddleware, SubAgentTool};
    ```
  - 原因: 使 `peri-tui` 的 `prompt.rs` 可通过 `peri_middlewares::scan_agents` 调用

- [x] 在 `prompt.rs` 中新增 `format_available_agents` 函数
  - 位置: `peri-tui/src/prompt.rs`（在 `build_system_prompt` 函数之前，~L57）
  - 新增函数:
    ```rust
    /// 扫描 `.claude/agents/` 目录，格式化为 agent 列表字符串。
    ///
    /// 格式：`- **{name}** (`{agent_id}`): {description}`
    /// 无 agent 时返回提示信息。
    fn format_available_agents(cwd: &str) -> String {
        let agents = peri_middlewares::scan_agents(cwd);
        if agents.is_empty() {
            return "No agents currently configured. You can add agent definitions in `.claude/agents/`.".to_string();
        }
        agents
            .iter()
            .map(|(agent_id, name, description)| {
                format!("- **{}** (`{}`): {}", name, agent_id, description)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    ```
  - 原因: 将 `scan_agents` 返回的元组列表格式化为 Markdown 列表，替换 `{{available_agents}}` 占位符

- [x] 在 `prompt.rs` 的 `build_system_prompt` 中新增 `{{available_agents}}` 占位符替换
  - 位置: `peri-tui/src/prompt.rs` `build_system_prompt()` 函数的 `.replace()` 链末尾（~L127）
  - 在现有 `.replace("{{date}}", &env.date)` 之后追加:
    ```rust
    .replace("{{available_agents}}", &format_available_agents(&env.cwd))
    ```
  - 原因: 运行时扫描 agent 目录并将列表注入到 system prompt 中，替代 `before_agent` 的 prepend 逻辑

- [x] 更新 `mod.rs` 测试中的 `test_before_agent_injects_summary`
  - 位置: `peri-middlewares/src/subagent/mod.rs` 测试 `test_before_agent_injects_summary`（~L328-354）
  - 将测试改名为 `test_before_agent_no_longer_injects_summary` 并修改断言：
    ```rust
    #[tokio::test]
    async fn test_before_agent_no_longer_injects_summary() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("tester.md"),
            "---\nname: tester\ndescription: Runs tests\n---\n\nYou run tests.\n",
        )
        .unwrap();

        let m = SubAgentMiddleware::new(
            vec![],
            None,
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        );
        let mut state = AgentState::new(dir.path().to_str().unwrap());
        <SubAgentMiddleware as Middleware<AgentState>>::before_agent(&m, &mut state)
            .await
            .unwrap();

        // agent 列表已迁移到 system prompt 占位符注入，before_agent 不再 prepend 消息
        assert_eq!(
            state.messages().len(), 0,
            "before_agent 不应注入 agent 摘要消息"
        );
    }
    ```
  - 同时将 `test_before_agent_no_agents_no_op`（~L357-368）的断言保持不变（已正确验证无 agent 时 messages 为 0）
  - 原因: agent 列表注入逻辑已移除，测试需验证新行为

- [x] 为 `format_available_agents` 和 `{{available_agents}}` 替换编写单元测试
  - 测试文件: `peri-tui/src/prompt.rs` 内 `#[cfg(test)] mod tests`（在现有测试之后追加）
  - 测试场景:
    - **test_available_agents_placeholder_replaced**: 创建含 `.claude/agents/tester.md` 的临时目录 → 调用 `build_system_prompt(None, dir, features{subagent_enabled:true})` → 验证结果包含 `- **tester** (`tester`): A test agent` 且不含 `{{available_agents}}`
    - **test_available_agents_placeholder_empty_dir**: 创建无 agent 文件的临时目录 → 调用 `build_system_prompt` → 验证结果包含 "No agents currently configured"
    - **test_available_agents_not_replaced_when_subagent_disabled**: `features{subagent_enabled:false}` → 调用 `build_system_prompt` → 验证结果不含 "SubAgent Delegation"（整个段落未注入）
    - **test_format_available_agents_with_agents**: 调用 `format_available_agents` 传入含 agent 定义的临时目录 → 验证返回格式为 `- **{name}** (`{id}`): {desc}` 且每行一个 agent
    - **test_format_available_agents_empty_dir**: 调用 `format_available_agents` 传入不存在的路径 → 验证返回 "No agents currently configured"
  - 运行命令: `cargo test -p peri-tui --lib -- prompt::tests::test_available_agents`
  - 预期: 5 个 available_agents 相关测试全部通过

**检查步骤:**
- [x] 编译通过
  - `cargo build -p peri-middlewares -p peri-tui 2>&1 | tail -5`
  - 预期: 输出 `Compiling ...` 后无 error，最终显示 `Finished`
- [x] 中间件测试通过（含更新的 before_agent 测试）
  - `cargo test -p peri-middlewares --lib -- subagent::tests 2>&1 | tail -15`
  - 预期: 所有测试 `ok`，包括 `test_before_agent_no_longer_injects_summary`、`test_before_agent_no_agents_no_op`、`test_scan_agents_*`
- [x] prompt 测试通过（含新增 available_agents 测试）
  - `cargo test -p peri-tui --lib -- prompt::tests::test_available_agents 2>&1 | tail -10`
  - 预期: 5 个 available_agents 测试全部 `ok`
- [x] 现有 prompt 测试无回归
  - `cargo test -p peri-tui --lib -- prompt::tests 2>&1 | tail -15`
  - 预期: 所有测试 `ok`，0 failed
- [x] `scan_agents` 公开可见性验证
  - `grep -n 'pub fn scan_agents' peri-middlewares/src/subagent/mod.rs`
  - 预期: 匹配到一行，签名为 `pub fn scan_agents`
- [x] `build_agents_summary` 已移除验证
  - `grep -c 'build_agents_summary' peri-middlewares/src/subagent/mod.rs`
  - 预期: 0（函数已删除，无残留引用）
- [x] `{{available_agents}}` 占位符替换验证
  - `grep -n 'available_agents' peri-tui/src/prompt.rs`
  - 预期: 至少 3 行匹配（format_available_agents 函数定义 + .replace 调用 + 占位符字符串）

---

### Task 3: Multi-Agent Design 验收

**前置条件:**
- 构建命令: `cargo build -p peri-middlewares -p peri-tui`
- Task 1（Fork 路径）和 Task 2（Prompt 优化）已完成

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p peri-middlewares -p peri-tui 2>&1 | tail -20`
   - 预期: 全部测试通过，0 failed
   - 失败排查: 根据失败的测试名定位到 Task 1（`test_fork*`/`test_parent_messages*`）或 Task 2（`test_available_agents*`/`test_before_agent_*`）

2. Fork 路径功能验证——消息继承
   - `cargo test -p peri-middlewares --lib -- test_fork_inherits_parent_messages 2>&1 | tail -5`
   - 预期: 测试通过，Fork 子 agent 收到父消息历史
   - 失败排查: 检查 Task 1 的 `invoke_fork()` 实现（tool.rs）

3. Fork 路径功能验证——工具集一致性
   - `cargo test -p peri-middlewares --lib -- test_fork_registers_all_tools_including_agent 2>&1 | tail -5`
   - 预期: 测试通过，Fork 子 agent 工具集包含 Agent（未硬编码排除）
   - 失败排查: 检查 Task 1 的 `invoke_fork()` 工具注册逻辑

4. Fork 路径功能验证——防递归
   - `cargo test -p peri-middlewares --lib -- test_fork_directive_includes_rules 2>&1 | tail -5`
   - 预期: 测试通过，fork directive 包含 `<fork_directive>` 和 `RULES` 段落
   - 失败排查: 检查 Task 1 的 `invoke_fork()` 中 fork directive 模板

5. System Prompt 指导内容验证
   - `cargo test -p peri-tui --lib -- test_available_agents_placeholder_replaced 2>&1 | tail -5`
   - 预期: 测试通过，`{{available_agents}}` 占位符被替换为 agent 列表
   - 失败排查: 检查 Task 2 的 `format_available_agents` 函数和 `build_system_prompt` 中的 `.replace()` 调用

6. `before_agent` 不再注入 agent 摘要
   - `cargo test -p peri-middlewares --lib -- test_before_agent_no_longer_injects_summary 2>&1 | tail -5`
   - 预期: 测试通过，`before_agent` 执行后 state.messages 为空
   - 失败排查: 检查 Task 2 对 `before_agent` 的简化是否完整

7. Normal 路径（subagent_type 指定）行为不变
   - `cargo test -p peri-middlewares --lib -- subagent::tool::tests::test_tool_executes_with_valid_agent_file 2>&1 | tail -5`
   - 预期: 测试通过，Normal 路径仍然正常工作
   - 失败排查: 检查 Task 1 对 `invoke()` 的重构是否影响了 Normal 路径分支

8. 编译最终确认——无 warning
   - `cargo build -p peri-middlewares -p peri-tui 2>&1 | grep -i warning`
   - 预期: 无新增 warning（特别是 `dead_code`、`unused_import`）
   - 失败排查: 检查 Task 2 是否完整删除了 `build_agents_summary` 及其引用
