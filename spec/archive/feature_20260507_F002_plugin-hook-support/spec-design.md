# Feature: 20260507_F002 — plugin-hook-support

## 需求背景

F001（plugin-marketplace-compat）完成了插件的发现、安装、加载链路，F001（plugin-mcp-injection）完成了 MCP 服务器的 per-plugin env 展开。当前 Peri 已兼容 Claude Code 插件的 commands、agents、skills、MCP servers 四类资产。

最后一个缺失拼图是 **hooks**。Claude Code 的 hooks 系统是纯配置驱动的（JSON，非 JS 代码），在 agent 生命周期关键节点触发外部动作。插件通过 `hooks/` 目录或 `plugin.json` 内的 `hooks` 字段声明 hook 规则。

**关键事实**：Claude Code hooks 的 4 种执行类型（command/prompt/http/agent）全部是声明式配置，不需要 JS 运行时。Peri 已具备 shell 执行（TerminalMiddleware）、LLM 调用（BaseModelReactLLM）、HTTP 客户端（reqwest）、子 Agent（SubAgentMiddleware）四种能力，技术上完全可行。

当前 `PluginManifest.hooks` 字段为 `Option<serde_json::Value>`（`types.rs:104`），仅预留未实现。`LoadedPlugin`（`loader.rs:66`）不持有 hooks 数据。

## 目标

- 实现完整的 hook 执行引擎，支持 4 种 hook 类型（command/prompt/http/agent）
- 支持 Phase 1 的 13 个生命周期事件
- 新增 `HookMiddleware`，插入中间件链（HITL 之后、SubAgent 之前）
- 从插件的 `hooks.json` / `plugin.json` 内 `hooks` 字段加载 hook 配置
- 兼容 Claude Code 的 hooks.json 格式和 stdin/stdout JSON 协议
- 支持 `once`、`async`、`asyncRewake`、`matcher`、`if` 条件匹配语义
- Agent 类型 hook 使用完整 agent 循环（最多 50 轮），带防递归保护
- HTTP hook 内置 SSRF 防护 + CRLF 注入防护

**不做**：Phase 2 事件（Setup/Task/Elicitation/ConfigChange/Worktree/Instructions/Cwd/FileChanged/TeammateIdle/PermissionDenied）、output-styles、LSP、channels、userConfig、依赖解析、asyncRewake。

## 方案设计

### 选定方案：HookMiddleware 拦截模式

新增 `HookMiddleware` 实现 `Middleware` trait，插入中间件链位置 10（HITL 之后、SubAgent 之前）。在 `before_agent`、`before_tool`、`after_tool`、`after_agent` 回调中匹配已注册的 hook 规则并执行。

```
1. AgentDefineMiddleware
2. AgentsMdMiddleware
3. SkillsMiddleware
4. SkillPreloadMiddleware
5. FilesystemMiddleware
6. TerminalMiddleware
7. TodoMiddleware
8. CronMiddleware
9. HumanInTheLoopMiddleware
10. HookMiddleware              ← 新增
11. SubAgentMiddleware
12. McpMiddleware
[ReActAgent.with_system_prompt()]
```

### 生命周期事件映射

#### Phase 1：13 个事件（直接映射 Middleware 回调）

| Middleware 回调 | HookEvent | 输入数据 |
|----------------|-----------|---------|
| 用户输入提交时 | UserPromptSubmit | prompt 文本 |
| `before_agent()` | SessionStart | source + model（startup/resume/clear/compact） |
| `before_tool()` | PreToolUse | tool_name + tool_input + tool_use_id |
| `before_tool()` | PermissionRequest | tool_name + tool_input + tool_use_id |
| `after_tool()` (is_error=false) | PostToolUse | tool_name + tool_input + tool_output |
| `after_tool()` (is_error=true) | PostToolUseFailure | tool_name + tool_input + tool_output |
| `after_agent()` (正常) | Stop | agent output |
| `after_agent()` (异常) | StopFailure | error |
| `before_agent()` | PreCompact | 无（压缩在 before_agent 之前触发） |
| `after_agent()` | PostCompact | 无（压缩在 after_agent 之后触发） |
| SubAgentMiddleware 转发 | SubagentStart | 子 agent 名称 |
| SubAgentMiddleware 转发 | SubagentStop | 子 agent 名称 + 结果 |
| 外部触发 | SessionEnd | 无 |

#### Phase 2：10 个事件（留后续）

Setup、TaskCreated、TaskCompleted、TeammateIdle、PermissionDenied、Elicitation、ElicitationResult、ConfigChange、WorktreeCreate、WorktreeRemove、InstructionsLoaded、CwdChanged、FileChanged、Notification

### 数据模型

#### HookType（4 种执行类型）

```rust
/// 4 种 hook 执行类型，对齐 Claude Code schemas/hooks.ts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookType {
    /// Shell 命令执行 (bash/powershell)
    Command {
        command: String,
        #[serde(default)]
        shell: Option<String>,
        #[serde(default)]
        timeout: Option<u64>,            // 秒
        #[serde(default)]
        status_message: Option<String>,
        #[serde(default)]
        once: bool,
        #[serde(rename = "async", default)]
        async_run: bool,
        #[serde(rename = "asyncRewake", default)]
        async_rewake: bool,
        /// 粗粒度匹配器（字符串/正则），见"matcher vs if"章节
        #[serde(default)]
        matcher: Option<String>,
        /// 细粒度条件匹配（permission rule 语法），见"matcher vs if"章节
        #[serde(rename = "if", default)]
        condition: Option<String>,
    },
    /// LLM 提示词评估
    Prompt {
        prompt: String,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        status_message: Option<String>,
        #[serde(default)]
        once: bool,
        #[serde(default)]
        matcher: Option<String>,
        #[serde(rename = "if", default)]
        condition: Option<String>,
    },
    /// HTTP POST
    Http {
        url: String,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        allowed_env_vars: Vec<String>,
        #[serde(default)]
        status_message: Option<String>,
        #[serde(default)]
        once: bool,
        #[serde(default)]
        matcher: Option<String>,
        #[serde(rename = "if", default)]
        condition: Option<String>,
    },
    /// 子 Agent 执行（完整 agent 循环，最多 50 轮）
    Agent {
        prompt: String,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        status_message: Option<String>,
        #[serde(default)]
        once: bool,
        #[serde(default)]
        matcher: Option<String>,
        #[serde(rename = "if", default)]
        condition: Option<String>,
    },
}
```

#### HookEvent（13 个 Phase 1 事件）

```rust
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PermissionRequest,
    UserPromptSubmit,
    SessionStart,
    SessionEnd,
    Stop,
    StopFailure,
    SubagentStart,
    SubagentStop,
    PreCompact,
    PostCompact,
}
```

#### HookInput（对齐 Claude Code BaseHookInputSchema + 事件特定字段）

```rust
/// Hook 执行输入——通过 stdin JSON 传递给 command hook，或作为 HTTP body
///
/// 对齐 Claude Code coreSchemas.ts:
/// - BaseHookInputSchema: session_id, transcript_path, cwd, permission_mode, agent_id, agent_type
/// - 每个事件通过 hook_event_name 判别字段区分
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookInput {
    // === BaseHookInputSchema 基础字段 ===
    /// 会话 ID
    pub session_id: String,
    /// 会话 transcript 文件路径
    pub transcript_path: String,
    /// 当前工作目录
    pub cwd: String,
    /// 当前权限模式（"yolo" / "hitl" 等）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// 子 agent ID（仅子 agent 内触发时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Agent 类型（如 "general-purpose" / "code-reviewer"）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,

    // === 事件判别字段 ===
    /// 事件名称（如 "PreToolUse"、"SessionStart"）
    pub hook_event_name: HookEvent,

    // === 工具事件字段（PreToolUse / PostToolUse / PostToolUseFailure / PermissionRequest）===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<serde_json::Value>,

    // === UserPromptSubmit 事件字段 ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    // === SessionStart 事件字段 ===
    /// 来源：startup / resume / clear / compact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 当前模型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    // === Subagent 事件字段 ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_result: Option<String>,
}
```

#### HookResult（对齐 Claude Code syncHookResponseSchema）

```rust
/// Hook 执行结果——对齐 Claude Code src/types/hooks.ts syncHookResponseSchema
///
/// Claude Code 的 hook 输出是扁平 JSON（非 enum），包含多个可选字段。
/// Peri 解析为结构体后转换为内部 Action 枚举。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncHookResponse {
    /// 是否继续（默认 true）。false 时阻止 agent 继续执行
    #[serde(default)]
    pub continue_run: Option<bool>,
    /// 是否在 transcript 中隐藏 stdout（默认 false）
    #[serde(default)]
    pub suppress_output: Option<bool>,
    /// continue=false 时显示的停止原因
    #[serde(default)]
    pub stop_reason: Option<String>,
    /// 权限决策：approve=允许, block=阻止
    #[serde(default)]
    pub decision: Option<HookDecision>,
    /// 决策原因说明
    #[serde(default)]
    pub reason: Option<String>,
    /// 系统警告消息（展示给用户）
    #[serde(default)]
    pub system_message: Option<String>,
    /// 事件特定输出
    #[serde(default)]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HookDecision {
    Approve,
    Block,
}

/// 事件特定的 hook 输出——对齐 Claude Code hookSpecificOutput discriminated union
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "hookEventName", rename_all = "PascalCase")]
pub enum HookSpecificOutput {
    PreToolUse {
        /// 权限决策：ask / deny / allow / passthrough
        #[serde(default)]
        permission_decision: Option<PermissionDecision>,
        #[serde(default)]
        permission_decision_reason: Option<String>,
        /// 修改后的工具输入（PreToolUse hook 改写参数）
        #[serde(default)]
        updated_input: Option<serde_json::Value>,
        /// 附加上下文信息
        #[serde(default)]
        additional_context: Option<String>,
    },
    UserPromptSubmit {
        #[serde(default)]
        additional_context: Option<String>,
    },
    SessionStart {
        #[serde(default)]
        additional_context: Option<String>,
        /// 追加的初始用户消息
        #[serde(default)]
        initial_user_message: Option<String>,
        /// 监视路径列表（用于 FileChanged 事件，Phase 2）
        #[serde(default)]
        watch_paths: Option<Vec<String>>,
    },
    #[serde(other)]
    Other(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Ask,
    Deny,
    Allow,
    Passthrough,
}

/// 内部处理后的 hook 动作
#[derive(Debug, Clone)]
pub enum HookAction {
    /// 允许继续（默认行为）
    Allow,
    /// 阻止操作（decision=block / exit code 2 / continue=false）
    Block { reason: String },
    /// 修改工具输入（PreToolUse hook 的 updatedInput）
    ModifyInput { new_input: serde_json::Value },
    /// 修改权限行为（permissionDecision）
    PermissionOverride { decision: PermissionDecision, reason: Option<String> },
    /// 阻止 agent 继续执行（continue=false + stopReason）
    PreventContinuation { stop_reason: Option<String> },
    /// 向 agent 注入系统消息（systemMessage）
    SystemMessage { message: String },
    /// 追加上下文（additionalContext）
    AdditionalContext { context: String },
    /// SessionStart 追加初始消息
    InitialUserMessage { message: String },
}
```

#### HookMatchRule / HooksConfig

```rust
/// hooks.json 中单个 hook 规则组
///
/// 对齐 Claude Code hooks schema：
/// - matcher: 粗粒度匹配器（工具名/正则），在进程启动前过滤
/// - hooks: 该规则组下的所有 hook 定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookMatchRule {
    /// 粗粒度匹配器（见"matcher vs if"章节）
    #[serde(default)]
    pub matcher: Option<String>,
    pub hooks: Vec<HookType>,
}

/// 插件的完整 hooks 配置
pub type HooksConfig = HashMap<HookEvent, Vec<HookMatchRule>>;
```

#### RegisteredHook（运行时注册）

```rust
/// 已注册到 HookMiddleware 的 hook（带插件上下文）
#[derive(Debug, Clone)]
pub struct RegisteredHook {
    pub hook: HookType,
    pub event: HookEvent,
    /// 粗粒度匹配器（来自 HookMatchRule.matcher 或 HookType 内 matcher 字段）
    pub matcher: Option<String>,
    pub plugin_name: String,
    pub plugin_id: String,
    pub plugin_root: PathBuf,
    pub plugin_data_dir: PathBuf,
    /// 插件选项（userConfig 值，用于 CLAUDE_PLUGIN_OPTION_* 环境变量）
    pub plugin_options: HashMap<String, serde_json::Value>,
}
```

### matcher vs if —— 双层匹配机制

Claude Code 的 hook 配置有两层匹配机制，职责不同，**不可混淆**：

#### `matcher`：粗粒度字符串/正则匹配

**用途**：在进程启动前做快速过滤，避免为不匹配的事件启动 hook 进程。

**语法**：

- `"Write"` — 精确匹配工具名 `Write`
- `"Write|Edit"` — 管道分隔的精确匹配列表
- `"^Bash.*"` — 正则表达式
- `"*"` 或缺省 — 匹配所有

**匹配对象**：工具名（`tool_name`）或通知类型等单一字段。

**示例**：

```json
{
  "matcher": "Bash",
  "hooks": [{ "type": "command", "command": "bash -c 'echo checking bash'" }]
}
```

#### `if`：细粒度 permission rule 语法

**用途**：基于工具输入内容的精细匹配，使用和 HITL 权限规则相同的语法。

**语法**：`"{ToolName}({pattern})"`

**仅适用于**：PreToolUse / PostToolUse / PostToolUseFailure / PermissionRequest 四种工具事件。其他事件类型 `if` 字段无效。

**匹配对象**：工具名 + 工具输入内容（需要 tool 的 `preparePermissionMatcher` 实现）。

**示例**：

```json
{
  "matcher": "Bash",
  "hooks": [{
    "type": "command",
    "command": "bash -c 'echo checking git'",
    "if": "Bash(git commit)"
  }]
}
```

#### 匹配逻辑实现

```rust
/// 粗粒度匹配：matcher 字段
fn matches_matcher(matcher: &str, tool_name: &str) -> bool {
    if matcher == "*" || matcher.is_empty() {
        return true;
    }
    // 管道分隔的精确匹配
    if matcher.contains('|') {
        return matcher.split('|').any(|p| p.trim() == tool_name);
    }
    // 纯字母数字+下划线 → 精确匹配
    if matcher.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return matcher == tool_name;
    }
    // 否则按正则匹配
    Regex::new(matcher).map(|re| re.is_match(tool_name)).unwrap_or(false)
}

/// 细粒度匹配：if 条件字段（permission rule 语法）
/// 仅适用于工具事件，复用 HITL classify_tool 逻辑
fn matches_if_condition(condition: &str, tool_name: &str, tool_input: &serde_json::Value) -> bool {
    // 解析 "Bash(git commit)" → tool_name="Bash", rule="git commit"
    let (cond_tool, cond_rule) = parse_permission_rule(condition)?;
    if cond_tool != tool_name {
        return false;
    }
    if cond_rule.is_empty() {
        return true;
    }
    // 使用 tool 的 preparePermissionMatcher 实现（复用 HITL 逻辑）
    match_tool_rule(tool_name, tool_input, cond_rule)
}
```

### HookMiddleware 设计

#### 结构

```rust
pub struct HookMiddleware {
    /// 所有已注册的 hook，按事件分组
    hooks: Arc<RwLock<HashMap<HookEvent, Vec<RegisteredHook>>>>,
    /// LLM 工厂（用于 prompt/agent 类型 hook）
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// 当前 cwd（用于 shell hook）
    cwd: String,
    /// session_id（HookInput 基础字段）
    session_id: String,
    /// transcript 路径
    transcript_path: String,
    /// 当前权限模式
    permission_mode: String,
    /// 当前模型名
    current_model: String,
    /// once hook 追踪：已执行过的 hook key
    once_fired: Arc<Mutex<HashSet<String>>>,
}
```

#### Middleware trait 实现

```rust
#[async_trait]
impl<S: State> Middleware<S> for HookMiddleware {
    fn name(&self) -> &str { "HookMiddleware" }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        self.fire_event(HookEvent::SessionStart, HookInput::session_start()).await;
        Ok(())
    }

    async fn before_tool(&self, state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
        let input = HookInput::tool_call(&tool_call);

        // 1. 触发 PreToolUse hooks
        let result = self.fire_event(HookEvent::PreToolUse, input.clone()).await;

        // 2. 触发 PermissionRequest hooks（仅敏感工具）
        let perm_result = self.fire_event(HookEvent::PermissionRequest, input.clone()).await;

        // 3. 合并结果
        let mut modified_call = tool_call.clone();
        for action in [result, perm_result] {
            match action {
                HookAction::Block { reason } => {
                    return Err(AgentError::ToolRejected {
                        tool: tool_call.name.clone(),
                        reason,
                    });
                }
                HookAction::ModifyInput { new_input } => {
                    modified_call.input = new_input;
                }
                HookAction::PermissionOverride { decision, reason } => {
                    // 覆盖 HITL 权限决策
                    // TODO: 通过 state 注入权限覆盖
                }
                HookAction::PreventContinuation { stop_reason } => {
                    return Err(AgentError::ToolRejected {
                        tool: tool_call.name.clone(),
                        reason: stop_reason.unwrap_or_else(|| "Hook prevented continuation".into()),
                    });
                }
                _ => {}
            }
        }
        Ok(modified_call)
    }

    async fn after_tool(&self, state: &mut S, tool_call: &ToolCall, result: &ToolResult) -> AgentResult<()> {
        let event = if result.is_error {
            HookEvent::PostToolUseFailure
        } else {
            HookEvent::PostToolUse
        };
        let input = HookInput::tool_result(tool_call, result);
        self.fire_event(event, input).await;
        Ok(())
    }

    async fn after_agent(&self, state: &mut S, output: &AgentOutput) -> AgentResult<AgentOutput> {
        let result = self.fire_event(HookEvent::Stop, HookInput::agent_output(output)).await;
        // 处理 Stop hook 返回的 additionalContext / initialUserMessage 等
        // 通过 state 追加到消息历史
        Ok(output.clone())
    }
}
```

#### 事件触发核心

```rust
impl HookMiddleware {
    /// 触发一个生命周期事件，匹配并执行所有注册的 hook
    /// 返回合并后的 HookAction 列表（按优先级处理）
    async fn fire_event(&self, event: HookEvent, input: HookInput) -> HookAction {
        let hooks = self.hooks.read().await;
        let matchers = match hooks.get(&event) {
            Some(m) => m.clone(),
            None => return HookAction::Allow,
        };
        drop(hooks);

        let mut final_action = HookAction::Allow;

        for registered in &matchers {
            // 1. once 检查
            if self.is_once_hook(registered) && self.was_once_fired(registered) {
                continue;
            }

            // 2. matcher 粗粒度匹配（工具名/正则）
            if let Some(ref matcher) = registered.matcher {
                if !self.matches_matcher(matcher, &input) {
                    continue;
                }
            }

            // 3. if 细粒度条件匹配（permission rule 语法，仅工具事件）
            if let Some(condition) = self.get_condition(&registered.hook) {
                if !self.matches_if_condition(&condition, &input) {
                    continue;
                }
            }

            // 4. 变量替换（${CLAUDE_PLUGIN_ROOT}, ${CLAUDE_PLUGIN_DATA}）
            let resolved = self.resolve_variables(&registered.hook, &registered.plugin_root, &registered.plugin_data_dir, &input);

            // 5. 执行 hook
            let action = self.execute_hook(&resolved, &input, &registered).await;

            // 6. once 标记
            if self.is_once_hook(&registered) {
                self.mark_once_fired(&registered);
            }

            // 7. Block / PreventContinuation 短路
            if matches!(action, HookAction::Block { .. } | HookAction::PreventContinuation { .. }) {
                return action;
            }

            // 8. 合并 ModifyInput（后执行覆盖先执行）
            if matches!(action, HookAction::ModifyInput { .. }) {
                final_action = action;
            }
        }

        final_action
    }
}
```

### Hook 执行器

4 种执行器分别处理不同 hook 类型：

#### CommandHook（Shell 执行）

**stdin/stdout JSON 协议**（对齐 Claude Code `src/utils/hooks.ts`）：

- **stdin**：写入完整 `HookInput` JSON（包含 `hook_event_name`、`session_id`、`cwd` 等所有基础字段 + 事件特定字段）
- **stdout**：退出码 0 时，若以 `{` 开头则解析为 `SyncHookResponse` JSON；否则视为纯文本输出
- **退出码语义**：
  - `0` → 成功，解析 stdout JSON 获取 decision/continue 等字段
  - `1` → 非阻塞错误，记录 warn 日志，不阻断 agent
  - `2` → 阻塞错误（`outcome: "blocking_error"`），阻止 agent 继续执行

```rust
async fn execute_command(
    hook: &HookType::Command,
    input: &HookInput,
    cwd: &str,
    plugin_root: Option<&Path>,
    plugin_data_dir: Option<&Path>,
    plugin_options: &HashMap<String, serde_json::Value>,
) -> HookAction {
    let shell = hook.shell.as_deref().unwrap_or("bash");
    // 默认超时 600 秒（对齐 Claude Code TOOL_HOOK_EXECUTION_TIMEOUT_MS = 10min）
    let timeout = hook.timeout.unwrap_or(600);

    let mut cmd = tokio::process::Command::new(shell);
    cmd.arg("-c").arg(&hook.command);
    cmd.current_dir(cwd);

    // === 环境变量注入（对齐 Claude Code src/utils/hooks.ts:965-1010）===
    cmd.env("CLAUDE_PROJECT_DIR", cwd);

    // 插件上下文
    if let Some(root) = plugin_root {
        cmd.env("CLAUDE_PLUGIN_ROOT", root);
    }
    if let Some(data_dir) = plugin_data_dir {
        cmd.env("CLAUDE_PLUGIN_DATA", data_dir);
    }

    // 插件选项 → CLAUDE_PLUGIN_OPTION_* 环境变量
    for (key, value) in plugin_options {
        let env_key = format!("CLAUDE_PLUGIN_OPTION_{}",
            key.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>().to_uppercase());
        cmd.env(env_key, value.to_string());
    }

    // SessionStart/Setup/CwdChanged/FileChanged 事件支持 CLAUDE_ENV_FILE
    // Phase 2 实现

    // === stdin：写入完整 HookInput JSON ===
    let input_json = match serde_json::to_string(input) {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!("Hook input serialization failed: {}", e);
            return HookAction::Allow;
        }
    };

    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = tokio::time::timeout(
        Duration::from_secs(timeout),
        cmd.spawn().and_then(|mut child| {
            async move {
                use tokio::io::AsyncWriteExt;
                if let Some(ref mut stdin) = child.stdin {
                    stdin.write_all(input_json.as_bytes()).await?;
                    stdin.flush().await?;
                }
                child.wait_with_output().await
            }
        })
    ).await;

    match output {
        Ok(Ok(out)) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let exit_code = out.status.code().unwrap_or(-1);

            match exit_code {
                0 => {
                    // 解析 stdout JSON
                    parse_command_hook_output(&stdout)
                }
                1 => {
                    // 非阻塞错误，记录日志但不阻断
                    tracing::warn!("Hook command non-blocking error (exit 1): {}", stderr);
                    HookAction::Allow
                }
                2 => {
                    // 阻塞错误
                    let reason = if stdout.trim().is_empty() {
                        stderr.trim().to_string()
                    } else {
                        stdout.trim().to_string()
                    };
                    HookAction::Block { reason }
                }
                _ => {
                    // 其他非零退出码，记录日志但不阻断
                    tracing::warn!("Hook command exit code {}: {}", exit_code, stderr);
                    HookAction::Allow
                }
            }
        }
        Ok(Err(e)) => {
            tracing::warn!("Hook command spawn failed: {}", e);
            HookAction::Allow
        }
        Err(_) => {
            tracing::warn!("Hook command timed out ({}s)", timeout);
            HookAction::Allow
        }
    }
}

/// 解析 command hook stdout 输出
/// 对齐 Claude Code parseHookOutput + processHookJSONOutput
fn parse_command_hook_output(stdout: &str) -> HookAction {
    let trimmed = stdout.trim();

    // 不以 { 开头 → 纯文本输出，视为 Allow
    if !trimmed.starts_with('{') {
        return HookAction::Allow;
    }

    // 尝试解析为 SyncHookResponse JSON
    match serde_json::from_str::<SyncHookResponse>(trimmed) {
        Ok(response) => sync_response_to_action(&response),
        Err(e) => {
            // JSON 解析失败 → 纯文本，视为 Allow（记录日志）
            tracing::warn!("Hook stdout JSON parse failed: {}", e);
            HookAction::Allow
        }
    }
}

/// 将 SyncHookResponse 转换为内部 HookAction
fn sync_response_to_action(response: &SyncHookResponse) -> HookAction {
    // continue=false → 阻止继续
    if response.continue_run == Some(false) {
        return HookAction::PreventContinuation {
            stop_reason: response.stop_reason.clone(),
        };
    }

    // decision=block → 阻止操作
    if response.decision == Some(HookDecision::Block) {
        return HookAction::Block {
            reason: response.reason.clone().unwrap_or_else(|| "Blocked by hook".into()),
        };
    }

    // systemMessage → 注入系统消息
    if let Some(ref msg) = response.system_message {
        return HookAction::SystemMessage { message: msg.clone() };
    }

    // hookSpecificOutput → 事件特定处理
    if let Some(ref specific) = response.hook_specific_output {
        return hook_specific_to_action(specific);
    }

    HookAction::Allow
}
```

**环境变量协议**（对齐 Claude Code `src/utils/hooks.ts:965-1010`）：

| 变量 | 说明 |
|------|------|
| `CLAUDE_PROJECT_DIR` | 项目根目录（POSIX 路径，Windows 上也用 `/`） |
| `CLAUDE_PLUGIN_ROOT` | 插件安装路径（仅插件 hook） |
| `CLAUDE_PLUGIN_DATA` | 插件数据路径（仅插件 hook） |
| `CLAUDE_PLUGIN_OPTION_{KEY}` | 插件 userConfig 选项值（仅插件 hook） |
| `CLAUDE_ENV_FILE` | env 注入脚本路径（仅 SessionStart/Setup/CwdChanged/FileChanged，Phase 2） |

> **[TRAP]** 不存在 `HOOK_EVENT` / `HOOK_TOOL_NAME` / `HOOK_TOOL_INPUT` / `HOOK_TOOL_OUTPUT` 等环境变量。所有事件数据通过 **stdin JSON** 传递。

#### PromptHook（LLM 评估）

```rust
async fn execute_prompt(
    hook: &HookType::Prompt,
    input: &HookInput,
    llm_factory: &Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
) -> HookAction {
    let mut llm = llm_factory();
    // 默认超时 30 秒（对齐 Claude Code execPromptHook.ts）
    let timeout = Duration::from_secs(hook.timeout.unwrap_or(30));

    let prompt = hook.prompt.replace("$ARGUMENTS", &serde_json::to_string(input).unwrap_or_default());

    let result = tokio::time::timeout(timeout, async {
        llm.generate_reasoning(&prompt).await
    }).await;

    match result {
        Ok(Ok(response)) => parse_llm_hook_response(&response),
        Ok(Err(e)) => {
            tracing::warn!("Hook prompt failed: {}", e);
            HookAction::Allow
        }
        Err(_) => {
            tracing::warn!("Hook prompt timed out");
            HookAction::Allow
        }
    }
}
```

#### HttpHook（HTTP POST）

```rust
async fn execute_http(hook: &HookType::Http, input: &HookInput) -> HookAction {
    // SSRF 防护：检查目标地址
    if let Err(blocked) = ssrf_guard::check_url(&hook.url) {
        tracing::warn!("HTTP hook SSRF blocked: {} - {}", hook.url, blocked);
        return HookAction::Allow;
    }

    // URL 白名单检查
    if let Some(ref allowed) = get_allowed_http_hook_urls() {
        if !allowed.iter().any(|pattern| url_matches_pattern(&hook.url, pattern)) {
            tracing::warn!("HTTP hook URL not in allowlist: {}", hook.url);
            return HookAction::Allow;
        }
    }

    // 默认超时 600 秒（对齐 Claude Code DEFAULT_HTTP_HOOK_TIMEOUT_MS = 10min）
    let timeout = Duration::from_secs(hook.timeout.unwrap_or(600));

    let client = reqwest::Client::new();
    let mut request = client
        .post(&hook.url)
        .timeout(timeout)
        .json(&input);

    // 注入 headers（CRLF 注入防护 + env 白名单替换）
    let allowed_set: HashSet<String> = hook.allowed_env_vars.iter().cloned().collect();
    for (key, value) in &hook.headers {
        // CRLF 注入防护：移除 \r \n \x00
        let sanitized = sanitize_header_value(value, &allowed_set);
        request = request.header(key.as_str(), sanitized);
    }

    let response = request.send().await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();

            if status.is_success() {
                parse_http_hook_response(&body)
            } else {
                tracing::warn!("Hook HTTP {} from {}", status, hook.url);
                HookAction::Allow
            }
        }
        Err(e) => {
            tracing::warn!("Hook HTTP failed: {}", e);
            HookAction::Allow
        }
    }
}

/// CRLF 注入防护（对齐 Claude Code execHttpHook.ts:76-109）
fn sanitize_header_value(value: &str, allowed_env_vars: &HashSet<String>) -> String {
    // 1. 替换 ${VAR} 和 $VAR（仅白名单内）
    let interpolated = replace_env_vars(value, allowed_env_vars);
    // 2. 移除 \r \n \x00
    interpolated.chars().filter(|c| *c != '\r' && *c != '\n' && *c != '\0').collect()
}

/// 解析 HTTP hook 响应
/// 对齐 Claude Code parseHttpHookOutput：空 body 视为 {}，非 JSON 视为错误
fn parse_http_hook_response(body: &str) -> HookAction {
    let trimmed = body.trim();

    // 空 body → 视为 {}（有效 JSON）
    if trimmed.is_empty() {
        return HookAction::Allow;
    }

    // 不以 { 开头 → 非法（HTTP hook 必须返回 JSON）
    if !trimmed.starts_with('{') {
        tracing::warn!("HTTP hook must return JSON, got non-JSON body: {}",
            if trimmed.len() > 200 { format!("{}...", &trimmed[..200]) } else { trimmed.to_string() });
        return HookAction::Allow;
    }

    match serde_json::from_str::<SyncHookResponse>(trimmed) {
        Ok(response) => sync_response_to_action(&response),
        Err(e) => {
            tracing::warn!("HTTP hook JSON parse failed: {}", e);
            HookAction::Allow
        }
    }
}
```

**SSRF 防护**（对齐 Claude Code `src/utils/hooks/ssrfGuard.ts`）：

```rust
/// SSRF 防护：阻止对私有/内部网络的 HTTP 请求
///
/// 阻止范围（IPv4）：
///   0.0.0.0/8        "this" network
///   10.0.0.0/8       private
///   100.64.0.0/10    CGNAT / shared address space（部分云 metadata）
///   169.254.0.0/16   link-local（云 metadata）
///   172.16.0.0/12    private
///   192.168.0.0/16   private
///
/// 阻止范围（IPv6）：
///   ::               unspecified
///   fc00::/7         unique local
///   fe80::/10        link-local
///   ::ffff:<v4>      mapped IPv4 in blocked range
///
/// 允许（不阻止）：
///   127.0.0.0/8      loopback（本地开发 hook）
///   ::1              loopback
///   其他所有公网地址
pub mod ssrf_guard {
    pub fn check_url(url: &str) -> Result<(), String> {
        // 解析 URL → 提取 host → DNS 解析 → 检查 IP 范围
        // 使用 reqwest 或 trust-dns 解析，避免 DNS rebinding
        todo!("使用 ipnet crate 实现 IP 范围检查")
    }
}
```

#### AgentHook（完整 Agent 循环）

**对齐 Claude Code `execAgentHook.ts`**：Agent hook 使用完整的多轮 agent 循环（最多 50 轮），而非单次 LLM 调用。子 agent 通过 `SyntheticOutputTool` 返回结构化结果。

```rust
/// Agent hook 执行——完整 agent 循环
///
/// 对齐 Claude Code execAgentHook.ts:
/// - 使用完整 query() 多轮执行，最多 50 轮
/// - 默认使用 haiku（快速模型），可通过 hook.model 覆盖
/// - 默认超时 60 秒
/// - 通过 SyntheticOutputTool 返回结构化结果
/// - 不注册 HookMiddleware（防递归）
/// - 不允许 SubAgent 工具（防嵌套）
async fn execute_agent_hook(
    hook: &HookType::Agent,
    input: &HookInput,
    llm_factory: &Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    cwd: &str,
) -> HookAction {
    // 默认超时 60 秒（对齐 Claude Code execAgentHook.ts）
    let timeout = Duration::from_secs(hook.timeout.unwrap_or(60));
    let max_turns: usize = 50;

    let prompt = hook.prompt.replace("$ARGUMENTS", &serde_json::to_string(input).unwrap_or_default());

    let result = tokio::time::timeout(timeout, async {
        // 创建子 agent（不注册 HookMiddleware，防止递归）
        let llm = llm_factory();
        let mut agent = ReActAgent::new(BaseModelReactLLM::new(llm))
            .max_iterations(max_turns as u32);
        // 注册除 Agent 以外的父工具（防递归 + 防 SubAgent 嵌套）
        // 注册 SyntheticOutputTool 用于返回结构化结果

        let agent_state = AgentState::new(cwd);
        agent.execute(AgentInput::text(&prompt), &mut agent_state).await
    }).await;

    match result {
        Ok(output) => {
            // 从 agent 输出中提取 SyntheticOutputTool 调用结果
            extract_structured_output(&output)
        }
        Err(_) => {
            tracing::warn!("Hook agent timed out ({}s)", timeout.as_secs());
            HookAction::Allow
        }
    }
}

/// 从 agent 输出中提取 SyntheticOutputTool 返回的结构化结果
fn extract_structured_output(output: &AgentOutput) -> HookAction {
    // 查找 tool_result 中 SyntheticOutputTool 的返回值
    // 解析为 SyncHookResponse → sync_response_to_action()
    todo!("实现 SyntheticOutputTool 结果提取")
}
```

**防递归策略**：

- 子 agent **不注册 HookMiddleware**，从根本上防止递归触发
- 子 agent **不注册 Agent 工具**，防止 hook agent 再创建子 agent
- 通过 `SyntheticOutputTool` 返回结构化结果，而非依赖自然语言输出

### Hook 加载

#### 从插件加载

修改 `LoadedPlugin`（`loader.rs:66`），新增 `hooks_config` 字段：

```rust
pub struct LoadedPlugin {
    // ... 现有字段 ...
    /// 解析后的 hooks 配置
    pub hooks_config: Option<HooksConfig>,
}
```

加载流程（`loader.rs` 新增 `extract_hooks` 函数）：

```rust
pub(crate) fn extract_hooks(
    manifest: &PluginManifest,
    install_path: &Path,
) -> Option<HooksConfig> {
    // 优先级：hooks.json 文件 > plugin.json 内 hooks 字段

    // 1. 检查 hooks/hooks.json
    let hooks_file = install_path.join("hooks").join("hooks.json");
    if hooks_file.exists() {
        if let Ok(config) = fs::read_to_string(&hooks_file) {
            if let Ok(parsed) = serde_json::from_str::<HooksConfig>(&config) {
                return Some(parsed);
            }
        }
    }

    // 2. 检查 plugin.json 内的 hooks 字段
    if let Some(hooks) = &manifest.hooks {
        if let Ok(parsed) = serde_json::from_value::<HooksConfig>(hooks.clone()) {
            return Some(parsed);
        }
    }

    None
}
```

#### 注册到 HookMiddleware

在 `agent_ops.rs` 的 `run_universal_agent` 中：

```rust
// 从 plugin_data 提取 hooks
let registered_hooks: Vec<RegisteredHook> = plugin_data.plugins.iter()
    .filter_map(|plugin| {
        let config = plugin.hooks_config.as_ref()?;
        let mut hooks = Vec::new();
        for (event, matchers) in config {
            for rule in matchers {
                // rule.matcher 作为粗粒度匹配器
                for hook_def in &rule.hooks {
                    hooks.push(RegisteredHook {
                        hook: hook_def.clone(),
                        event: event.clone(),
                        // matcher 优先级：HookMatchRule.matcher > HookType 内 matcher
                        matcher: rule.matcher.clone().or_else(|| get_hook_matcher(hook_def)),
                        plugin_name: plugin.name.clone(),
                        plugin_id: plugin.id.clone(),
                        plugin_root: plugin.install_path.clone(),
                        plugin_data_dir: plugin.data_path.clone(),
                        plugin_options: plugin.user_config.clone().unwrap_or_default(),
                    });
                }
            }
        }
        Some(hooks)
    })
    .flatten()
    .collect();

// 创建 HookMiddleware
let hook_middleware = HookMiddleware::new(
    registered_hooks,
    llm_factory.clone(),
    cwd.to_string(),
    session_id.clone(),
    transcript_path.clone(),
    permission_mode.clone(),
    current_model.clone(),
);

// 插入中间件链（位置 10，HITL 之后）
agent.add_middleware(Box::new(hook_middleware));
```

### 变量替换

Hook 配置中的变量替换在执行前统一处理：

| 变量 | 替换值 |
|------|-------|
| `${CLAUDE_PLUGIN_ROOT}` | 插件安装路径 `install_path` |
| `${CLAUDE_PLUGIN_DATA}` | 插件数据路径 `data_path` |
| `${ARGUMENTS}` | `HookInput` 的 JSON 序列化 |
| `${user_config.KEY}` | 用户配置值（Phase 2，本次不实现） |
| `${VAR}` / `$VAR` | 环境变量（仅 http hook headers 的 `allowed_env_vars` 白名单内） |

### once / async / asyncRewake 语义

| 标志 | 行为 |
|------|------|
| `once: true` | 执行一次后自动移除，通过 `once_fired: HashSet<String>` 追踪 |
| `async: true` | 在后台 tokio::spawn 执行，不阻塞 agent 流程 |
| `asyncRewake: true` | 后台执行，退出码 2 时唤醒 agent（阻塞错误），隐含 async=true |

**async 实现细节**：

- `async: true` 的 hook 在 `tokio::spawn` 中执行
- async hook 的 stdout JSON 输出仍会被解析，但不阻塞 agent 主流程
- async hook 的 `systemMessage` 通过事件通知机制异步注入
- async hook 的 `decision=block` 结果会排队为通知，不阻塞当前操作

**asyncRewake 实现**：HookMiddleware 持有 `Arc<Notify>`，async hook 退出码 2 时 `notify_one()`。ReAct 循环需要在迭代间隙 check 此 notify（通过 `tokio::select!`）。Phase 1 仅实现 async，asyncRewake 留作后续优化。

### 错误处理

**原则：hook 失败不阻断 agent**（和 Claude Code 一致）。

- shell hook 超时/崩溃 → 日志 warn → Allow
- LLM hook 调用失败 → 日志 warn → Allow
- HTTP hook 网络错误 → 日志 warn → Allow
- hook JSON 解析失败 → 日志 warn → 视为纯文本（Allow）
- shell hook 退出码 1 → 非阻塞错误，日志 warn → Allow
- 唯一阻断情况：hook 显式返回 `decision: "block"` / `continue: false` / 退出码 2

### AgentEvent 扩展

新增事件用于 SubAgent 通知和 SessionEnd：

```rust
// 在 AgentEvent 枚举中新增
SubagentStarted { agent_name: String },
SubagentStopped { agent_name: String, result: String },
SessionEnded,
CompactStarted,
CompactCompleted,
UserPromptSubmitted { prompt: String },
```

HookMiddleware 监听这些事件（通过 event_handler），触发对应的 hook。

### PluginLoadResult 扩展

```rust
pub struct PluginLoadResult {
    // ... 现有字段 ...
    /// 所有插件的已注册 hook
    pub all_hooks: Vec<RegisteredHook>,
}
```

### 文件布局

```
peri-middlewares/src/
├── hooks/
│   ├── mod.rs              # 模块入口，公共 API 导出
│   ├── types.rs            # HookType, HookEvent, HookInput, SyncHookResponse, HookAction, HookSpecificOutput, HookMatchRule, HooksConfig, RegisteredHook
│   ├── middleware.rs        # HookMiddleware 实现（Middleware trait）
│   ├── executor.rs         # 4 种执行器：CommandHook, PromptHook, HttpHook, AgentHook
│   ├── matcher.rs          # 双层匹配：matches_matcher（粗粒度）+ matches_if_condition（细粒度 permission rule）
│   ├── ssrf_guard.rs       # SSRF 防护（IP 范围检查）
│   ├── variables.rs        # 变量替换（${CLAUDE_PLUGIN_ROOT} 等）
│   ├── output_parser.rs    # stdout/HTTP 响应解析：SyncHookResponse → HookAction
│   └── loader.rs           # 从插件提取 hooks 配置
└── plugin/
    ├── loader.rs           # 修改：extract_hooks() + LoadedPlugin 新增 hooks_config
    └── types.rs            # 修改：PluginManifest.hooks 类型从 serde_json::Value → Option<HooksConfig>
```

### 改动清单

| 文件 | 改动类型 | 说明 |
|------|---------|------|
| `hooks/mod.rs` | 新增 | 模块入口 |
| `hooks/types.rs` | 新增 | 全部数据类型定义 |
| `hooks/middleware.rs` | 新增 | HookMiddleware 实现 |
| `hooks/executor.rs` | 新增 | 4 种 hook 执行器 |
| `hooks/matcher.rs` | 新增 | 双层匹配逻辑 |
| `hooks/ssrf_guard.rs` | 新增 | SSRF 防护 |
| `hooks/variables.rs` | 新增 | 变量替换 |
| `hooks/output_parser.rs` | 新增 | stdout/HTTP 响应解析 |
| `hooks/loader.rs` | 新增 | 插件 hooks 配置提取 |
| `plugin/loader.rs` | 修改 | 新增 `extract_hooks()`，`LoadedPlugin` 加 `hooks_config` |
| `plugin/types.rs` | 修改 | `PluginManifest.hooks` 改为 `Option<HooksConfig>` |
| `plugin/mod.rs` | 修改 | 导出 hooks 相关类型 |
| `peri-tui/src/app/agent_ops.rs` | 修改 | 构建 HookMiddleware 并注入中间件链 |
| `peri-agent/src/agent/react.rs` | 修改 | AgentEvent 新增 Subagent/Session/Compact/UserPrompt 事件变体 |

### 测试策略

| 测试层 | 覆盖内容 |
|-------|---------|
| 单元测试（`hooks/executor.rs`） | 4 种执行器独立测试：stdin JSON 写入、stdout JSON 解析、退出码 0/1/2/其他、LLM 超时、HTTP SSRF 阻断、agent hook 50 轮限制 |
| 单元测试（`hooks/matcher.rs`） | matcher 粗粒度匹配（精确/管道/正则）、if 细粒度匹配（permission rule 语法）、两者组合 |
| 单元测试（`hooks/ssrf_guard.rs`） | IPv4/IPv6 私有地址阻止、loopback 允许、DNS 解析 |
| 单元测试（`hooks/output_parser.rs`） | SyncHookResponse 解析：decision=block/approve、continue=false、systemMessage、hookSpecificOutput.updatedInput、空 body、非 JSON body |
| 单元测试（`hooks/variables.rs`） | 变量替换：CLAUDE_PLUGIN_ROOT、ARGUMENTS、环境变量白名单 |
| 单元测试（`hooks/loader.rs`） | hooks.json 解析、plugin.json hooks 字段解析、优先级 |
| 集成测试（`hooks/middleware.rs`） | HookMiddleware 在 ReAct 循环中的行为：PreToolUse deny 阻断、ModifyInput 修改、once 只触发一次、matcher 过滤 |
| 集成测试（`plugin/loader.rs`） | extract_hooks 端到端：从模拟插件目录加载 hooks 配置 |

### Claude Code 兼容性对照

| Claude Code 特性 | Peri 实现 | 兼容性 |
|-----------------|----------------|-------|
| hooks.json 格式 | `HooksConfig = HashMap<HookEvent, Vec<HookMatchRule>>` | ✅ 完全兼容 |
| command hook stdin JSON | `HookInput` 写入 stdin（含 BaseHookInputSchema 全部字段） | ✅ 完全兼容 |
| command hook stdout JSON | `SyncHookResponse` 解析（decision/continue/hookSpecificOutput） | ✅ 完全兼容 |
| command hook env vars | `CLAUDE_PROJECT_DIR` / `CLAUDE_PLUGIN_ROOT` / `CLAUDE_PLUGIN_DATA` / `CLAUDE_PLUGIN_OPTION_*` | ✅ 完全兼容 |
| prompt hook | PromptHook + llm_factory，30s 超时 | ✅ 完全兼容 |
| http hook | HttpHook + reqwest，600s 超时，SSRF 防护 + CRLF 防护 | ✅ 完全兼容 |
| agent hook | 完整 agent 循环（max 50 turns），60s 超时，SyntheticOutputTool | ✅ 完全兼容 |
| exit code 语义 | 0=success+JSON解析, 1=non-blocking, 2=blocking | ✅ 完全兼容 |
| matcher 粗粒度匹配 | 精确/管道/正则三种模式 | ✅ 完全兼容 |
| if 细粒度匹配 | permission rule 语法，仅工具事件 | ✅ 完全兼容 |
| once 语义 | once_fired HashSet | ✅ 完全兼容 |
| async 语义 | tokio::spawn | ✅ 完全兼容 |
| asyncRewake 语义 | Phase 2 | ⏳ 暂不实现 |
| UserPromptSubmit 事件 | 新增到 Phase 1 | ✅ 完全兼容 |
| HookInput 基础字段 | session_id / transcript_path / cwd / permission_mode / agent_id / agent_type | ✅ 完全兼容 |
| HookSpecificOutput | PreToolUse / UserPromptSubmit / SessionStart 事件特定输出 | ✅ 完全兼容 |
| SSRF 防护 | ssrf_guard 模块（IPv4/IPv6 私有地址阻止） | ✅ 完全兼容 |
| CRLF 注入防护 | sanitize_header_value（移除 \r\n\0） | ✅ 完全兼容 |
| CLAUDE_PLUGIN_OPTION_* | 插件 userConfig → 环境变量 | ✅ 完全兼容 |
| $ARGUMENTS 替换 | variables.rs | ✅ 完全兼容 |
| ${CLAUDE_PLUGIN_ROOT/DATA} | variables.rs | ✅ 完全兼容 |
| allowedEnvVars 白名单 | HttpHook headers 替换 | ✅ 完全兼容 |
| URL 白名单 | allowedHttpHookUrls 配置 | ✅ 完全兼容 |
| hook 失败不阻断 | 所有执行器默认 Allow | ✅ 完全兼容 |
| plugin 上下文注入 | RegisteredHook.plugin_root / plugin_data_dir / plugin_options | ✅ 完全兼容 |
| hot-reload | Phase 2（需 PluginManager refresh） | ⏳ 暂不实现 |
| ${user_config.KEY} | Phase 2（需 userConfig 功能） | ⏳ 暂不实现 |
| CLAUDE_ENV_FILE | Phase 2（SessionStart/Setup/CwdChanged/FileChanged） | ⏳ 暂不实现 |
| 19+ 事件 | Phase 1 实现 13 个，Phase 2 补齐 | ⏳ 部分兼容 |
