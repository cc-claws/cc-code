# Plugin Hook Support 执行计划 (1/2)

**目标:** 实现 Claude Code 兼容的 hook 执行引擎基础层——数据类型、匹配引擎、变量替换、输出解析、SSRF 防护

**技术栈:** Rust 2021 / tokio / serde / regex / ipnet (新增依赖)

**设计文档:** spec/feature_20260507_F002_plugin-hook-support/spec-design.md

## 改动总览

本文件包含 Task 1-4，实现 hook 系统的基础层（数据类型、匹配、变量替换、输出解析）。全部为 `peri-middlewares/src/hooks/` 下新增文件。Task 3 创建 `hooks/mod.rs` 模块入口并在 `lib.rs` 中声明。Task 1 仅创建 `hooks/types.rs`，不创建 `mod.rs`（由 Task 3 统一创建）。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 workspace 构建可用
  - `cargo build 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证测试工具可用
  - `cargo test -p peri-middlewares --no-run 2>&1 | tail -5`
  - 预期: 编译成功，无配置错误

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build 2>&1 | grep -E "(Compiling|Finished|error)"`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 测试框架可用
  - `cargo test -p peri-middlewares --no-run 2>&1 | grep -E "(Compiling peri-middlewares|Finished|error)"`
  - 预期: 编译成功，无配置错误

---

### Task 1: Hook 数据类型定义

**背景:**
本 Task 实现完整的 Hook 系统数据模型，为后续执行器、中间件、加载器提供类型基础。当前 `peri-middlewares` 没有 `hooks` 模块，所有类型需从零定义。类型定义对齐 Claude Code 的 hooks JSON schema，确保插件配置兼容性。

**涉及文件:**
- 新建: `peri-middlewares/src/hooks/types.rs`

**执行步骤:**
- [x] 创建 hooks 模块目录
  - 位置: `peri-middlewares/src/hooks/`
  - 仅创建目录（`mkdir -p`），不创建 `mod.rs`（由 Task 3 统一创建模块入口文件）
  - 原因: 后续 Task (middleware/executor/loader) 都依赖这些类型定义

- [x] 定义 HookEvent 枚举（13 个 Phase 1 事件）
  - 位置: `peri-middlewares/src/hooks/types.rs` 文件顶部
  - 完整定义：
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
  - 添加 `#[serde(rename_all = "PascalCase")]` 确保序列化对齐 Claude Code (如 "PreToolUse" 而非 "pre_tool_use")
  - 原因: 事件名需与 hooks.json 中的 key 精确匹配

- [x] 定义 HookType 枚举（4 种执行类型）
  - 位置: `peri-middlewares/src/hooks/types.rs`，HookEvent 定义之后
  - 使用 `#[serde(tag = "type")]` 外部 tag 实现 discriminated union
  - 定义 4 个变体：Command/Prompt/Http/Agent
  - Command 变体字段：command, shell, timeout, status_message, once, async_run, async_rewake, matcher, condition
  - Prompt 变体字段：prompt, timeout, model, status_message, once, matcher, condition
  - Http 变体字段：url, timeout, headers, allowed_env_vars, status_message, once, matcher, condition
  - Agent 变体字段：prompt, timeout, model, status_message, once, matcher, condition
  - 所有 Option 字段添加 `#[serde(default)]`
  - async/asyncRewake 字段使用 `#[rename = "async"]` / `#[rename = "asyncRewake"]` 避免关键字冲突
  - condition 字段使用 `#[rename = "if"]` 对齐 JSON 配置
  - 原因: serde tag enum 解析 hooks.json 中 `{"type": "command", "command": "..."}` 格式

- [x] 定义 HookInput 结构体（对齐 BaseHookInputSchema）
  - 位置: `peri-middlewares/src/hooks/types.rs`，HookType 定义之后
  - 基础字段：session_id, transcript_path, cwd, permission_mode, agent_id, agent_type
  - 事件判别字段：hook_event_name: HookEvent
  - 工具事件字段（Option）：tool_name, tool_input, tool_use_id, tool_output
  - UserPromptSubmit 事件字段（Option）：prompt
  - SessionStart 事件字段（Option）：source, model
  - Subagent 事件字段（Option）：subagent_name, subagent_result
  - 所有 Option 字段添加 `#[serde(skip_serializing_if = "Option::is_none")]`
  - 原因: stdin JSON 协议需完整输入数据，序列化时跳过 None 字段减小体积

- [x] 定义 SyncHookResponse 结构体（对齐 syncHookResponseSchema）
  - 位置: `peri-middlewares/src/hooks/types.rs`，HookInput 定义之后
  - 字段：continue_run, suppress_output, stop_reason, decision, reason, system_message, hook_specific_output
  - 所有字段 Option + `#[serde(default)]`
  - 添加 `#[derive(Default)]` 支持
  - 原因: stdout JSON 解析为结构体后转换为内部 HookAction

- [x] 定义 HookDecision / PermissionDecision / HookSpecificOutput 枚举
  - 位置: `peri-middlewares/src/hooks/types.rs`，SyncHookResponse 定义之后
  - HookDecision: Approve/Block，`#[serde(rename_all = "lowercase")]`（"approve"/"block"）
  - PermissionDecision: Ask/Deny/Allow/Passthrough，`#[serde(rename_all = "lowercase")]`
  - HookSpecificOutput: `#[serde(tag = "hookEventName", rename_all = "PascalCase")]` 外部 tag
  - HookSpecificOutput.PreToolUse 变体：permission_decision, permission_decision_reason, updated_input, additional_context
  - HookSpecificOutput.UserPromptSubmit 变体：additional_context
  - HookSpecificOutput.SessionStart 变体：additional_context, initial_user_message, watch_paths
  - HookSpecificOutput.Other 变体：`#[serde(other)]` 捕获未定义事件
  - 原因: HookSpecificOutput 是 discriminated union，tag 字段区分事件类型

- [x] 定义 HookAction 枚举（内部处理动作，不需要 Serialize/Deserialize）
  - 位置: `peri-middlewares/src/hooks/types.rs`，HookSpecificOutput 定义之后
  - 变体：Allow, Block { reason: String }, ModifyInput { new_input: serde_json::Value }, PermissionOverride { decision: PermissionDecision, reason: Option<String> }, PreventContinuation { stop_reason: Option<String> }, SystemMessage { message: String }, AdditionalContext { context: String }, InitialUserMessage { message: String }
  - 不添加 serde 属性（仅内部使用，不涉及序列化）
  - 原因: 外部 JSON 解析为 SyncHookResponse 后转换为内部 HookAction，执行器返回此类型

- [x] 定义 HookMatchRule / HooksConfig 类型
  - 位置: `peri-middlewares/src/hooks/types.rs`，HookAction 定义之后
  - HookMatchRule 结构体：matcher: Option<String>, hooks: Vec<HookType>
  - HooksConfig: `pub type HooksConfig = HashMap<HookEvent, Vec<HookMatchRule>>`
  - 原因: hooks.json 格式为 `{"PreToolUse": [{"matcher": "Bash", "hooks": [...]}]}`

- [x] 定义 RegisteredHook 结构体（运行时注册）
  - 位置: `peri-middlewares/src/hooks/types.rs`，文件末尾
  - 字段：hook, event, matcher, plugin_name, plugin_id, plugin_root, plugin_data_dir, plugin_options
  - plugin_root/plugin_data_dir 类型：PathBuf
  - plugin_options 类型：HashMap<String, serde_json::Value>
  - 不添加 serde 属性（运行时结构，不涉及序列化）
  - 原因: HookMiddleware 持有已注册 hook，包含插件上下文用于变量替换

- [x] 为 HookType 实现 getter 辅助方法
  - 位置: `peri-middlewares/src/hooks/types.rs`，RegisteredHook 定义之后
  - 实现 `impl HookType` 块，添加方法：
    - `fn get_matcher(&self) -> Option<&String>` — 返回各变体的 matcher 字段
    - `fn get_condition(&self) -> Option<&String>` — 返回各变体的 condition 字段
    - `fn is_once(&self) -> bool` — Command/Prompt/Http/Agent 的 once 字段，Agent/Prompt 默认 false
    - `fn is_async(&self) -> bool` — Command 的 async_run 字段，其他类型默认 false
  - 使用 match 表达式分发到各变体
  - 原因: HookMiddleware 需统一访问不同变体的公共字段，避免重复 match 逻辑

- [x] 为 HookInput 实现构造函数（按事件类型）
  - 位置: `peri-middlewares/src/hooks/types.rs`，impl HookType 之后
  - 实现 `impl HookInput` 块，添加方法：
    - `fn session_start(session_id, transcript_path, cwd, source, model) -> Self` — 填充 SessionStart 事件字段
    - `fn tool_call(tool_name, tool_input, tool_use_id) -> Self` — 填充 PreToolUse/PermissionRequest 字段
    - `fn tool_result(tool_name, tool_input, tool_output, is_error) -> Self` — 填充 PostToolUse/PostToolUseFailure 字段
    - `fn user_prompt_submit(prompt) -> Self` — 填充 UserPromptSubmit 字段
    - `fn subagent_start(subagent_name) -> Self` — 填充 SubagentStart 字段
    - `fn subagent_stop(subagent_name, result) -> Self` — 填充 SubagentStop 字段
  - 所有方法设置 hook_event_name 字段
  - 原因: HookMiddleware 触发事件时构造 HookInput，避免手动填充所有字段

- [x] 为本 Task 核心逻辑编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/types_tests.rs`（或在 types.rs 末尾添加 `#[cfg(test)] mod tests`）
  - 测试场景:
    - [反序列化 HookType]: JSON `{"type": "command", "command": "echo test"}` → HookType::Command 变体
    - [反序列化 HookEvent]: JSON `"PreToolUse"` → HookEvent::PreToolUse
    - [序列化 HookInput]: HookInput 结构体 → JSON 字符串，验证字段命名（PascalCase）和 None 字段跳过
    - [反序列化 SyncHookResponse]: JSON `{"continue": false, "stopReason": "blocked"}` → SyncHookResponse 结构体
    - [HookSpecificOutput tag 解析]: JSON `{"hookEventName": "PreToolUse", "permissionDecision": "deny"}` → HookSpecificOutput::PreToolUse
    - [HookType getter 方法]: 创建 Command{once: true}，验证 is_once() 返回 true
    - [HookInput 构造函数]: 调用 HookInput::tool_call()，验证 hook_event_name/tool_name/tool_input 字段正确填充
    - [HooksConfig 反序列化]: 完整 hooks.json 片段 → HashMap<HookEvent, Vec<HookMatchRule>>
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::types`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证类型定义编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|warning:.*hooks::types)"`
  - 预期: 无 error，无 hooks::types 相关 warning

- [x] 验证 serde 属性正确性
  - `cargo test -p peri-middlewares --lib hooks::types::tests::test_hooktype_deser 2>&1 | tail -3`
  - 预期: 测试通过，JSON `{"type": "command", "command": "echo"}` 成功解析为 HookType::Command

- [x] 验证 PascalCase 序列化
  - `cargo test -p peri-middlewares --lib hooks::types::tests::test_hookevent_serialize 2>&1 | grep -o '"PreToolUse"'`
  - 预期: 输出包含 `"PreToolUse"`（非 "pre_tool_use"）


---

### Task 2: Hook 匹配引擎

**背景:**
Hook 系统需要双层匹配机制来精确控制 hook 执行时机——matcher 粗粒度匹配在进程启动前快速过滤不相关事件，if 细粒度匹配基于工具输入内容做精细化判断。当前代码中没有可复用的匹配逻辑实现，需从零构建。本 Task 被 Task 4（HookMiddleware 实现）依赖，middleware 的 fire_event 方法需要调用这两个匹配函数来决定是否执行 hook。

**涉及文件:**
- 新建: `peri-middlewares/src/hooks/matcher.rs`

**执行步骤:**
- [x] 创建 `peri-middlewares/src/hooks/matcher.rs` 文件并实现粗粒度匹配函数
  - 位置: 新建文件，文件顶部添加 `use regex::Regex;` 引入正则表达式依赖
  - 实现 `pub fn matches_matcher(matcher: &str, tool_name: &str) -> bool` 函数
  - 逻辑（完全按 spec-design.md 第 433-447 行伪代码实现）:
    - matcher 为 "*" 或空字符串 → 返回 true（匹配所有）
    - matcher 包含 "|" → 按管道符分割，任一 trim 后等于 tool_name 即返回 true
    - matcher 仅含字母数字下划线（`matcher.chars().all(|c| c.is_alphanumeric() || c == '_')`）→ 精确匹配判断
    - 其他情况 → 用 Regex::new(matcher) 编译正则，失败返回 false，成功则用 re.is_match(tool_name) 判断
  - 原因: 粗粒度匹配在 hook 进程启动前执行，必须支持精确匹配、列表匹配、正则匹配三种模式以覆盖 Claude Code 的 hook 配置语法

- [x] 在同一文件中实现细粒度条件匹配函数
  - 位置: `matches_matcher` 函数之后
  - 实现 `pub fn matches_if_condition(condition: &str, tool_name: &str, tool_input: &serde_json::Value) -> bool` 函数
  - 逻辑（按 spec-design.md 第 449-462 行伪代码实现）:
    - 解析 condition 字符串为 `"ToolName(rule)"` 格式，提取 cond_tool 和 cond_rule
    - cond_tool 与 tool_name 不匹配 → 返回 false
    - cond_rule 为空 → 返回 true（仅匹配工具名，不检查输入内容）
    - cond_rule 非空 → 调用 `match_tool_rule(tool_name, tool_input, cond_rule)` 判断
  - 在同一文件中实现辅助函数 `fn match_tool_rule(tool_name: &str, tool_input: &serde_json::Value, rule: &str) -> bool`
  - `match_tool_rule` 逻辑: 将 tool_input 序列化为 JSON 字符串，使用 `contains()` 检查 rule 是否为子串（与 Claude Code 行为一致，简单字符串包含匹配）
  - 原因: HITL 模块的 classify 方法使用 LLM 做分类，不适合直接复用；if 条件仅用于 4 种工具事件，简单字符串匹配即可满足需求

- [x] 验证匹配函数在 mod.rs 中导出（Task 3 创建 mod.rs 时已包含）
  - 位置: `peri-middlewares/src/hooks/mod.rs` 的 pub use 导出区域
  - 确认已包含: `pub use matcher::{matches_matcher, matches_if_condition};`（由 Task 3 的 mod.rs 步骤统一添加）
  - 原因: HookMiddleware 需要调用这两个函数，必须通过模块导出

- [x] 为匹配引擎编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/matcher.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景:
    - [matcher 通配符]: `matches_matcher("*", "Bash")` → 返回 true
    - [matcher 精确匹配]: `matches_matcher("Write", "Write")` → 返回 true；`matches_matcher("Write", "Edit")` → 返回 false
    - [matcher 管道列表]: `matches_matcher("Write|Edit|Grep", "Grep")` → 返回 true；`matches_matcher("Write|Edit", "Grep")` → 返回 false
    - [matcher 正则匹配]: `matches_matcher("^Bash.*", "Bash -c echo")` → 返回 true；`matches_matcher("^Bash", "Write")` → 返回 false
    - [matcher 正则非法]: `matches_matcher("[invalid", "Write")` → 返回 false（正则编译失败）
    - [if 条件工具名匹配]: `matches_if_condition("Bash(git)", "Bash", &json!({"command": "git commit"}))` → 返回 true
    - [if 条件工具名不匹配]: `matches_if_condition("Bash(git)", "Write", &json!({"path": "file.txt"}))` → 返回 false
    - [if 条件规则为空]: `matches_if_condition("Bash()", "Bash", &json!({}))` → 返回 true（仅匹配工具名）
    - [if 条件内容包含]: `matches_if_condition("Bash(git commit)", "Bash", &json!({"command": "git commit -m msg"}))` → 返回 true（包含子串）
    - [if 条件内容不包含]: `matches_if_condition("Bash(git)", "Bash", &json!({"command": "npm install"}))` → 返回 false
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::matcher`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 matcher.rs 文件编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|warning:.*matcher)"`
  - 预期: 无错误输出，仅有 compiler warnings（如有）不包含 matcher 相关错误

- [x] 验证匹配函数正确导出
  - `grep -n "pub use matcher" peri-middlewares/src/hooks/mod.rs`
  - 预期: 输出包含 `pub use matcher::{matches_matcher, matches_if_condition};`

- [x] 验证单元测试覆盖所有场景
  - `cargo test -p peri-middlewares --lib hooks::matcher 2>&1 | grep -E "test result:|running \d+ test"`
  - 预期: 输出包含 "running 9 test" 和 "test result: ok"

---

---

### Task 3: 变量替换 — 实现 ${CLAUDE_PLUGIN_ROOT/DATA} / $ARGUMENTS / env 白名单替换

**背景:**
Hook 配置中的字符串字段（command/prompt/url）支持变量替换，用于动态注入插件上下文和事件数据。当前代码无此能力，需新建 variables 模块提供统一的替换函数。本 Task 的输出被 Task 6（Hook 执行器）依赖，在执行 hook 前调用变量替换完成字符串展开。

**涉及文件:**
- 新建: `peri-middlewares/src/hooks/variables.rs`
- 新建: `peri-middlewares/src/hooks/mod.rs`（模块入口，导出公共 API）
- 修改: `peri-middlewares/src/lib.rs`（添加 `pub mod hooks;`）

**执行步骤:**
- [x] 创建 hooks 模块目录和 mod.rs 入口文件
  - 位置: 新建 `peri-middlewares/src/hooks/mod.rs`
  - 添加模块文档注释，说明 hooks 模块的职责
  - 添加子模块声明和 variables 模块的公共 API 导出：
    ```rust
    pub mod types;
    pub mod matcher;
    pub mod variables;
    pub mod output_parser;

    pub use variables::resolve_hook_variables;
    pub use variables::resolve_hook_variables_with_env;
    pub use matcher::{matches_matcher, matches_if_condition};
    pub use output_parser::{parse_command_hook_output, parse_http_hook_response};
    ```
  - 注意：后续 Task 7 会追加 ssrf_guard/executor/loader/middleware 的声明和导出
  - 原因: hooks 模块后续会包含多个子模块（types/executor/matcher 等），需统一入口管理

- [x] 实现 resolve_hook_variables 函数，处理插件路径变量和 $ARGUMENTS 替换
  - 位置: `peri-middlewares/src/hooks/variables.rs`（新建文件）
  - 函数签名: `pub fn resolve_hook_variables(input: &str, plugin_root: &Path, plugin_data_dir: &Path, arguments: &str) -> String`
  - 实现逻辑:
    1. 替换 `${CLAUDE_PLUGIN_ROOT}` 为 `plugin_root` 的绝对路径（POSIX 格式，Windows 也用 `/` 分隔符）
    2. 替换 `${CLAUDE_PLUGIN_DATA}` 为 `plugin_data_dir` 的绝对路径（POSIX 格式）
    3. 替换 `${ARGUMENTS}` 和 `$ARGUMENTS` 为 `arguments` 参数值
    4. 返回替换后的字符串
  - 路径转换使用 `path.to_string_lossy()` 处理非 UTF-8 字符，Windows 路径替换 `\` 为 `/`
  - 原因: Command/Prompt/Http 三种 hook 类型都需要这些基础变量，统一处理避免重复代码

- [x] 实现 resolve_hook_variables_with_env 函数，增加环境变量白名单替换
  - 位置: `peri-middlewares/src/hooks/variables.rs`
  - 函数签名: `pub fn resolve_hook_variables_with_env(input: &str, plugin_root: &Path, plugin_data_dir: &Path, arguments: &str, allowed_env_vars: &HashSet<String>) -> String`
  - 实现逻辑:
    1. 先调用 `resolve_hook_variables` 完成插件路径和 ARGUMENTS 替换
    2. 使用 `shellexpand::env_with_context` 进行 env var 展开，传入自定义 context
    3. Context 闭包仅展开 `allowed_env_vars` 白名单内的环境变量，白名单外返回空字符串
    4. 同时支持 `${VAR}` 和 `$VAR` 两种格式（shellexpand 默认支持）
    5. 返回最终替换结果
  - 原因: HTTP hook headers 需要环境变量注入，但必须限制白名单防止敏感信息泄露

- [x] 为 resolve_hook_variables 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/variables.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景:
    - 基础替换: `"echo ${CLAUDE_PLUGIN_ROOT}"` + plugin_root="/tmp/plugin" → `"echo /tmp/plugin"`
    - 多变量替换: `"${CLAUDE_PLUGIN_ROOT}/${CLAUDE_PLUGIN_DATA}"` → `"/tmp/plugin /tmp/data"`
    - ARGUMENTS 替换: `"prompt: $ARGUMENTS"` + arguments='{"tool":"Bash"}' → `"prompt: {\"tool\":\"Bash\"}"`
    - Windows 路径格式: plugin_root="C:\\plugins\\test" → 替换后为 `"C:/plugins/test"`（`\` 转为 `/`）
    - 空输入处理: `""` → `""`
    - 无变量字符串: `"bash -c 'echo hello'"` → 原样返回
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::variables`
  - 预期: 所有测试通过

- [x] 为 resolve_hook_variables_with_env 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/variables.rs`
  - 测试场景:
    - 白名单内 env var 替换: `"Token: ${API_KEY}"` + allowed=["API_KEY"] + env API_KEY="sk-xxx" → `"Token: sk-xxx"`
    - 白名单外 env var 阻断: `"${SECRET_KEY}"` + allowed=["API_KEY"] → `"${SECRET_KEY}"`（保持原样或替换为空，根据实现决定）
    - 混合替换: `"${CLAUDE_PLUGIN_ROOT}/${ENV_VAR}"` → plugin_root + env_var 都正确展开
    - $VAR 和 ${VAR} 格式: `"$HOME ${HOME}"` + allowed=["HOME"] → 两者都替换为实际值
    - 未定义环境变量: `"$UNDEFINED_VAR"` + allowed=["UNDEFINED_VAR"] → 替换为空字符串（shellexpand 默认行为）
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::variables::resolve_hook_variables_with_env`
  - 预期: 所有测试通过

- [x] 在 lib.rs 中导出 hooks 模块
  - 位置: `peri-middlewares/src/lib.rs` 的 `pub mod` 声明区域（~L30）
  - 在 `pub mod plugin;` 之后添加: `pub mod hooks;`
  - 原因: 使 hooks 模块的公共 API 对外可见，Task 6 的 executor 才能调用

**检查步骤:**
- [x] 验证模块编译通过
  - `cargo check -p peri-middlewares`
  - 预期: 编译成功，无错误

- [x] 验证函数导出正确
  - `grep -n "pub use variables::resolve_hook_variables" peri-middlewares/src/hooks/mod.rs`
  - 预期: 找到导出声明

- [x] 验证单元测试通过
  - `cargo test -p peri-middlewares --lib hooks::variables`
  - 预期: 所有测试通过，输出显示测试通过的数量

---

### Task 4: 输出解析器 — 实现 SyncHookResponse → HookAction 转换

**背景:**
Hook 执行器需要将外部输出（command hook stdout / HTTP hook response body）转换为内部 HookAction 枚举，以便 HookMiddleware 统一处理。当前代码无此解析层，需新建 output_parser 模块。本 Task 被 Task 6（Hook 执行器）依赖，executor 在获取 hook 输出后调用解析函数完成转换。

**涉及文件:**
- 新建: `peri-middlewares/src/hooks/output_parser.rs`

**执行步骤:**
- [x] 创建 output_parser.rs 文件并实现 parse_command_hook_output 函数
  - 位置: 新建文件，文件顶部添加 `use crate::hooks::types::{HookAction, SyncHookResponse};`
  - 实现 `pub fn parse_command_hook_output(stdout: &str) -> HookAction` 函数
  - 逻辑（完全按 spec-design.md 第 747-764 行伪代码实现）:
    - trim stdout 字符串，检查是否以 `{` 开头
    - 不以 `{` 开头 → 返回 HookAction::Allow（纯文本输出）
    - 以 `{` 开头 → 调用 `serde_json::from_str::<SyncHookResponse>` 解析 JSON
    - 解析成功 → 调用 `sync_response_to_action` 转换
    - 解析失败 → 记录 warn 日志，返回 HookAction::Allow
  - 原因: command hook stdout 可能是纯文本或 JSON，需区分处理

- [x] 在同一文件中实现 parse_http_hook_response 函数
  - 位置: parse_command_hook_output 函数之后
  - 实现 `pub fn parse_http_hook_response(body: &str) -> HookAction` 函数
  - 逻辑（按 spec-design.md 第 906-929 行伪代码实现）:
    - trim body 字符串，检查是否为空
    - 空字符串 → 返回 HookAction::Allow（空 body 视为有效 JSON）
    - 非空且不以 `{` 开头 → 记录 warn 日志（HTTP hook 必须返回 JSON），返回 HookAction::Allow
    - 以 `{` 开头 → 调用 `serde_json::from_str::<SyncHookResponse>` 解析 JSON
    - 解析成功 → 调用 `sync_response_to_action` 转换
    - 解析失败 → 记录 warn 日志，返回 HookAction::Allow
  - 原因: HTTP hook 协议要求返回 JSON，空 body 视为 {}，非 JSON body 视为错误

- [x] 在同一文件中实现 sync_response_to_action 核心转换函数
  - 位置: parse_http_hook_response 函数之后
  - 实现 `fn sync_response_to_action(response: &SyncHookResponse) -> HookAction` 函数
  - 逻辑（完全按 spec-design.md 第 767-794 行伪代码实现，优先级严格按顺序）:
    1. 检查 `response.continue_run == Some(false)` → 返回 `HookAction::PreventContinuation { stop_reason: response.stop_reason.clone() }`
    2. 检查 `response.decision == Some(HookDecision::Block)` → 返回 `HookAction::Block { reason: response.reason.clone().unwrap_or_else(|| "Blocked by hook".into()) }`
    3. 检查 `response.system_message` 存在 → 返回 `HookAction::SystemMessage { message: response.system_message.clone().unwrap() }`
    4. 检查 `response.hook_specific_output` 存在 → 调用 `hook_specific_to_action` 转换（下一步实现）
    5. 以上都不满足 → 返回 `HookAction::Allow`
  - 原因: SyncHookResponse 是扁平 JSON，需按优先级转换为内部 Action 枚举

- [x] 在同一文件中实现 hook_specific_to_action 辅助函数
  - 位置: sync_response_to_action 函数之后
  - 实现 `fn hook_specific_to_action(specific: &HookSpecificOutput) -> HookAction` 函数
  - 逻辑（按 spec-design.md 第 789-791 行伪代码实现）:
    - match specific 分发：
      - `HookSpecificOutput::PreToolUse { updated_input: Some(input), .. }` → 返回 `HookAction::ModifyInput { new_input: input.clone() }`
      - `HookSpecificOutput::PreToolUse { permission_decision: Some(decision), .. }` → 返回 `HookAction::PermissionOverride { decision: decision.clone(), reason: None }`
      - `HookSpecificOutput::UserPromptSubmit { additional_context: Some(ctx), .. }` → 返回 `HookAction::AdditionalContext { context: ctx.clone() }`
      - `HookSpecificOutput::SessionStart { initial_user_message: Some(msg), .. }` → 返回 `HookAction::InitialUserMessage { message: msg.clone() }`
      - 其他情况 → 返回 `HookAction::Allow`
  - 原因: HookSpecificOutput 是 discriminated union，需提取各变体字段转换为对应 Action

- [x] 验证解析函数在 mod.rs 中导出（Task 3 创建 mod.rs 时已包含）
  - 位置: `peri-middlewares/src/hooks/mod.rs` 的 pub use 导出区域
  - 确认已包含: `pub use output_parser::{parse_command_hook_output, parse_http_hook_response};`（由 Task 3 的 mod.rs 步骤统一添加）
  - 原因: Task 6 的 executor 需要调用这两个函数，必须通过模块导出

- [x] 为 parse_command_hook_output 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/output_parser.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景:
    - [纯文本输出]: `parse_command_hook_output("hello world")` → 返回 HookAction::Allow
    - [JSON 解析成功 continue=false]: `parse_command_hook_output(r#"{"continue": false}"#)` → 返回 HookAction::PreventContinuation { stop_reason: None }
    - [JSON 解析成功 decision=block]: `parse_command_hook_output(r#"{"decision": "block", "reason": "test"}"#)` → 返回 HookAction::Block { reason: "test" }
    - [JSON 解析成功 systemMessage]: `parse_command_hook_output(r#"{"systemMessage": "warning"}"#)` → 返回 HookAction::SystemMessage { message: "warning" }
    - [JSON 解析失败]: `parse_command_hook_output("{invalid json}")` → 返回 HookAction::Allow（日志 warn）
    - [空字符串]: `parse_command_hook_output("")` → 返回 HookAction::Allow
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::output_parser::test_parse_command`
  - 预期: 所有测试通过

- [x] 为 parse_http_hook_response 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/output_parser.rs`
  - 测试场景:
    - [空 body]: `parse_http_hook_response("")` → 返回 HookAction::Allow
    - [空格 body]: `parse_http_hook_response("   ")` → 返回 HookAction::Allow
    - [非 JSON body]: `parse_http_hook_response("plain text")` → 返回 HookAction::Allow（日志 warn）
    - [JSON 解析成功]: `parse_http_hook_response(r#"{"continue": false, "stopReason": "test"}"#)` → 返回 HookAction::PreventContinuation { stop_reason: Some("test".to_string()) }
    - [JSON 解析失败]: `parse_http_hook_response("{invalid}")` → 返回 HookAction::Allow（日志 warn）
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::output_parser::test_parse_http`
  - 预期: 所有测试通过

- [x] 为 sync_response_to_action 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/output_parser.rs`
  - 测试场景:
    - [优先级 continue=false]: SyncHookResponse { continue_run: Some(false), decision: Some(Block) } → 返回 PreventContinuation（continue 优先级高于 decision）
    - [优先级 decision=block]: SyncHookResponse { decision: Some(Block), reason: Some("blocked".into()) } → 返回 Block { reason: "blocked" }
    - [优先级 systemMessage]: SyncHookResponse { system_message: Some("msg".into()) } → 返回 SystemMessage { message: "msg" }
    - [hookSpecificOutput.PreToolUse.updatedInput]: SyncHookResponse { hook_specific_output: Some(PreToolUse { updated_input: Some(json!({"key":"val"})), .. }) } → 返回 ModifyInput
    - [默认 Allow]: SyncHookResponse { ..default() } → 返回 Allow
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::output_parser::test_sync_response`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 output_parser.rs 文件编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|warning:.*output_parser)"`
  - 预期: 无 error，无 output_parser 相关 warning

- [x] 验证函数导出正确
  - `grep -n "pub use output_parser" peri-middlewares/src/hooks/mod.rs`
  - 预期: 输出包含 `pub use output_parser::{parse_command_hook_output, parse_http_hook_response};`

- [x] 验证单元测试覆盖所有场景
  - `cargo test -p peri-middlewares --lib hooks::output_parser 2>&1 | grep -E "test result:|running \d+ test"`
  - 预期: 输出包含 "running 15 test" 和 "test result: ok"

---

### Task 5: 基础层验收

**前置条件:**
- Task 1-4 全部完成
- 构建命令: `cargo build -p peri-middlewares`

**端到端验证:**

1. 运行 hooks 模块全部测试确保无回归
   - `cargo test -p peri-middlewares --lib hooks 2>&1 | tail -10`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤

2. 验证 hooks 模块公共 API 完整导出
   - `cargo test -p peri-middlewares --lib hooks::types hooks::matcher hooks::variables hooks::output_parser 2>&1 | grep -E "test result:"`
   - 预期: 4 个模块的测试全部通过
   - 失败排查: 检查 mod.rs 导出是否完整

3. 验证 serde 反序列化兼容性
   - `cargo test -p peri-middlewares --lib hooks::types::tests 2>&1 | grep -E "test result:"`
   - 预期: 所有类型测试通过（HookType/HookEvent/HookInput/SyncHookResponse/HookSpecificOutput/HooksConfig）
   - 失败排查: 检查 serde 属性（rename_all、tag、default）

---
