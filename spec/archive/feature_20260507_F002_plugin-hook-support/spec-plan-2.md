# Plugin Hook Support 执行计划 (2/2)

**目标:** 实现 Claude Code 兼容的 hook 执行引擎上层——SSRF 防护、4 种执行器、HookMiddleware 集成、插件加载注册

**技术栈:** Rust 2021 / tokio / serde / regex / ipnet (新增依赖) / reqwest

**设计文档:** spec/feature_20260507_F002_plugin-hook-support/spec-design.md

## 改动总览

本文件包含 Task 5-10，实现 hook 系统的上层（SSRF 防护、4 种执行器、HookMiddleware 中间件、插件加载注册、AgentEvent 扩展）和完整验收。Task 5 新增 `ipnet` 依赖到 Cargo.toml 并实现 SSRF 防护。Task 6 实现 4 种执行器。Task 7 实现 HookMiddleware 并补充 `hooks/mod.rs` 导出（Task 3 已创建初始版）。Task 8 修改插件类型和加载流程。Task 9 扩展 `peri-agent` 的 `AgentEvent` 枚举并在 SubAgentMiddleware 中转发事件。Task 10 为完整验收。

**关键修正（对比初版）**：

- Task 7 不重复实现 matcher/variables/condition，直接调用 Task 2/3/1 的独立模块函数
- Task 7 不再次添加 `pub mod hooks;` 到 lib.rs（Task 3 已添加）
- Task 6 的 HTTP 执行器直接调用 `ssrf_guard::check_url()` 自由函数，不使用不存在的 `SsrfGuard` 结构体
- Task 8 的 hooks 聚合使用 `user_config`（`HashMap<String, serde_json::Value>`）而非不存在的 `PluginOption.value` 字段
- 新增 Task 9 覆盖 AgentEvent 扩展（design.md 要求的 6 个新变体）

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链可用。Task 5 新增了 `ipnet` 依赖，需先验证 Cargo.toml 修改后 workspace 解析正常。

**执行步骤:**

- [x] 验证 workspace 构建可用
  - `cargo build 2>&1 | tail -5`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 验证测试工具可用
  - `cargo test -p peri-middlewares --no-run 2>&1 | tail -5`
  - 预期: 编译成功，无配置错误

**检查步骤:**

- [x] 构建命令执行成功
  - `cargo build 2>&1 | grep -E "(Compiling|Finished|error)"`
  - 预期: 输出包含 "Finished" 且无 error

---### Task 5: SSRF 防护实现

**背景:**
HTTP hook 需要防止对内网私有地址的请求（SSRF 攻击），避免插件通过云 metadata 服务窃取凭证或扫描内网端口。当前代码没有 SSRF 防护机制。本 Task 实现 IP 范围检查模块，供 Task 6 的 HttpHook 执行器调用。

**涉及文件:**

- 新建: `peri-middlewares/src/hooks/ssrf_guard.rs`
- 修改: `peri-middlewares/Cargo.toml`

**执行步骤:**

- [x] 在 Cargo.toml 中添加 ipnet 依赖
  - 位置: `peri-middlewares/Cargo.toml` 的 `[dependencies]` 段落末尾
  - 添加一行: `ipnet = "2.10"`
  - 原因: ipnet crate 提供类型安全的 IP 网络范围匹配，支持 IPv4/IPv6 统一 API

- [x] 创建 ssrf_guard.rs 模块文件
  - 位置: `peri-middlewares/src/hooks/ssrf_guard.rs`（新建文件）
  - 定义公开函数 `check_url(url: &str) -> Result<(), String>`
  - 原因: 对齐 spec-design.md "SSRF 防护"章节的 API 签名

- [x] 实现 URL 解析和 DNS 解析逻辑
  - 位置: `ssrf_guard.rs` 的 `check_url()` 函数内（文件开头）
  - 使用 `reqwest::Url::parse()` 解析 URL，提取 host
  - 使用 `tokio::net::lookup_host()` 解析 DNS（同步包装），获取所有 IP 地址
  - 错误处理: URL 解析失败返回 `Err("Invalid URL")`，DNS 解析失败返回 `Err("DNS resolution failed")`
  - 原因: 必须先 DNS 解析再检查 IP，防止 DNS rebinding 攻击（URL 中的 hostname 可能指向公网，但实际解析到内网 IP）

- [x] 实现 IPv4 私有地址范围检查
  - 位置: `ssrf_guard.rs` 新增私有函数 `is_blocked_ipv4(ip: Ipv4Addr) -> bool`
  - 使用 `ipnet::Ipv4Net` 定义以下阻止范围（对齐 spec-design.md line 936-943）:
    - `0.0.0.0/8` — "this" network
    - `10.0.0.0/8` — private
    - `100.64.0.0/10` — CGNAT / shared address space（云 metadata）
    - `169.254.0.0/16` — link-local（云 metadata）
    - `172.16.0.0/12` — private
    - `192.168.0.0/16` — private
  - 排除 `127.0.0.0/8`（loopback，允许本地开发 hook）
  - 返回 `true` 表示阻止，`false` 表示允许
  - 原因: 云 metadata 服务通常在 169.254.169.254（AWS）或 100.100.100.200（阿里云），必须阻止

- [x] 实现 IPv6 私有地址范围检查
  - 位置: `ssrf_guard.rs` 新增私有函数 `is_blocked_ipv6(ip: Ipv6Addr) -> bool`
  - 使用 `ipnet::Ipv6Net` 定义以下阻止范围（对齐 spec-design.md line 945-949）:
    - `::` — unspecified
    - `fc00::/7` — unique local
    - `fe80::/10` — link-local
  - 排除 `::1`（loopback，允许本地开发 hook）
  - 处理 IPv4-mapped IPv6 地址（`::ffff:<v4>`），提取 IPv4 部分后调用 `is_blocked_ipv4()`
  - 原因: IPv6 也有私有地址空间，且 ::ffff:0:0/96 映射的 IPv4 地址需继承 IPv4 阻止规则

- [x] 实现 IP 地址迭代检查逻辑
  - 位置: `check_url()` 函数内，DNS 解析之后
  - 迭代 `lookup_host()` 返回的所有 `SocketAddr`，提取 IP
  - 对每个 IP 调用对应的 `is_blocked_ipv4()` 或 `is_blocked_ipv6()`
  - 任一 IP 在阻止范围内即返回 `Err(format!("Blocked: {}", ip))`
  - 所有 IP 都通过检查则返回 `Ok(())`
  - 原因: DNS 可能返回多个 IP（A/AAAA 记录），任一 IP 指向内网都应阻止

- [x] 在 hooks/mod.rs 中添加 ssrf_guard 模块声明
  - 位置: `peri-middlewares/src/hooks/mod.rs`
  - 在现有 `pub mod variables;` 之后添加: `pub mod ssrf_guard;`
  - 原因: 使 Task 6 的 HttpHook 执行器可以调用 `ssrf_guard::check_url()`

- [x] 为 SSRF 防护模块编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/ssrf_guard.rs`（文件末尾 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `test_check_url_public`: 公网 URL（`https://example.com`）→ `Ok(())`
    - `test_check_url_loopback`: loopback 地址（`http://127.0.0.1:8080`, `http://[::1]:8080`）→ `Ok(())`
    - `test_check_url_private_ipv4`: 私有 IPv4（`http://10.0.0.1`, `http://192.168.1.1`, `http://172.16.0.1`, `http://169.254.169.254`, `http://100.100.100.200`）→ `Err("Blocked: ...")`
    - `test_check_url_private_ipv6`: 私有 IPv6（`http://[fc00::1]`, `http://[fe80::1]`）→ `Err("Blocked: ...")`
    - `test_check_url_ipv4_mapped`: IPv4-mapped IPv6（`http://[::ffff:192.168.1.1]`）→ `Err("Blocked: ...")`
    - `test_check_url_invalid_url`: 无效 URL（`not-a-url`）→ `Err("Invalid URL")`
  - 运行命令: `cargo test -p peri-middlewares --lib ssrf_guard`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 ipnet 依赖添加成功
  - `grep "ipnet" /Users/konghayao/code/ai/peri/peri-middlewares/Cargo.toml`
  - 预期: 输出包含 `ipnet = "2.10"`

- [x] 验证 ssrf_guard 模块编译通过
  - `cargo build -p peri-middlewares 2>&1 | grep -E "(Compiling|Finished|error)" | tail -5`
  - 预期: 输出包含 "Finished" 且无 error

- [x] 验证单元测试全部通过
  - `cargo test -p peri-middlewares --lib ssrf_guard 2>&1 | grep -E "(test result:|running)" | tail -3`
  - 预期: 输出包含 "test result: ok. X passed" 且无 failed

- [x] 验证公开函数签名正确
  - `grep -A 3 "pub fn check_url" /Users/konghayao/code/ai/peri/peri-middlewares/src/hooks/ssrf_guard.rs`
  - 预期: 输出包含 `pub fn check_url(url: &str) -> Result<(), String>`

**认知变更:**

- [x] [CLAUDE.md] HTTP hook SSRF 防护使用 `ipnet` crate 实现，阻止范围对齐 Claude Code `src/utils/hooks/ssrfGuard.ts`（IPv4: 0.0.0.0/8, 10.0.0.0/8, 100.64.0.0/10, 169.254.0.0/16, 172.16.0.0/12, 192.168.0.0/16；IPv6: ::, fc00::/7, fe80::/10, ::ffff:<v4> mapped），允许 127.0.0.0/8 和 ::1（本地开发 hook）
- [x] [CLAUDE.md] [TRAP] DNS rebinding 攻击防护：SSRF 检查必须先 DNS 解析再验证 IP，不能仅检查 URL 中的 hostname（攻击者可控制 DNS 响应将公网域名解析到内网 IP）。使用 `tokio::net::lookup_host()` 而非 `std::net::ToSocketAddrs` 确保异步安全。

---

---

---

---

### Task 6: Hook 执行器 — 实现 4 种执行器（Command/Prompt/HTTP/Agent）

**背景:**
本 Task 实现 Hook 系统的核心执行层——4 种 hook 类型的执行器，负责调用外部进程/LLM/HTTP/Agent 并将输出转换为 HookAction。当前代码无任何 hook 执行逻辑，需从零构建。Command 执行器使用 tokio::process::Command 实现 stdin/stdout JSON 协议，Prompt 执行器通过 llm_factory 调用 LLM，HTTP 执行器使用 reqwest 并集成 SSRF 防护，Agent 执行器创建完整 ReActAgent 循环（防递归）。本 Task 依赖 Task 1（类型定义）、Task 3（变量替换）、Task 4（输出解析）、Task 5（SSRF 防护），被 Task 7（HookMiddleware 集成）依赖。

**涉及文件:**

- 新建: `peri-middlewares/src/hooks/executor.rs`

**执行步骤:**

- [x] 创建 executor.rs 文件并实现 Command 执行器核心函数
  - 位置: 新建文件，文件顶部添加必要的 use 语句
  - 添加 use: `use crate::hooks::types::{HookType, HookInput, HookAction, RegisteredHook};`
  - 添加 use: `use crate::hooks::variables::resolve_hook_variables;`
  - 添加 use: `use crate::hooks::output_parser::parse_command_hook_output;`
  - 添加 use: `use tokio::process::Command;`
  - 添加 use: `use std::process::Stdio;`
  - 实现 `pub async fn execute_command_hook(hook: &HookType::Command, input: &HookInput, registered: &RegisteredHook) -> HookAction` 函数
  - 逻辑（完全按 spec-design.md 第 638-744 行伪代码实现）:
    1. 提取 shell（默认 "bash"）和 timeout（默认 600 秒）
    2. 创建 `Command::new(shell).arg("-c").arg(&hook.command)`，设置 current_dir 为 input.cwd
    3. 注入环境变量：CLAUDE_PROJECT_DIR（input.cwd）、CLAUDE_PLUGIN_ROOT（registered.plugin_root）、CLAUDE_PLUGIN_DATA（registered.plugin_data_dir）、CLAUDE_PLUGIN_OPTION_*（遍历 registered.plugin_options，key 转大写并替换非字母数字为下划线）
    4. 将 input 序列化为 JSON 字符串，失败时记录 warn 日志并返回 Allow
    5. 设置 stdin/stdout/stderr 为 Stdio::piped()
    6. 使用 `tokio::time::timeout(Duration::from_secs(timeout))` 包装执行
    7. spawn 子进程，向 stdin 写入 input JSON，flush 后调用 wait_with_output()
    8. 匹配退出码：
       - 0 → 调用 parse_command_hook_output(&stdout) 解析
       - 1 → 记录 warn 日志，返回 Allow（非阻塞错误）
       - 2 → 返回 Block { reason }（阻塞错误，优先用 stdout，空则用 stderr）
       - 其他 → 记录 warn 日志，返回 Allow
    9. 处理超时/spawn 失败 → 记录 warn 日志，返回 Allow
  - 原因: Command hook 是最基础的执行类型，需完整实现 stdin JSON 协议和退出码语义

- [x] 在同一文件中实现 Prompt 执行器函数
  - 位置: execute_command_hook 函数之后
  - 实现 `pub async fn execute_prompt_hook(hook: &HookType::Prompt, input: &HookInput, llm_factory: &Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>) -> HookAction` 函数
  - 逻辑（按 spec-design.md 第 811-838 行伪代码实现）:
    1. 提取 timeout（默认 30 秒）
    2. 创建 LLM 实例：`let mut llm = llm_factory();`
    3. 替换 prompt 中的 $ARGUMENTS 为 input 的 JSON 序列化
    4. 使用 `tokio::time::timeout(Duration::from_secs(timeout))` 包装调用
    5. 调用 `llm.generate_reasoning(&prompt).await`
    6. 成功 → 调用 parse_command_hook_output 解析（LLM 返回文本与 command stdout 相同）
    7. 失败 → 记录 warn 日志，返回 Allow
    8. 超时 → 记录 warn 日志，返回 Allow
  - 原因: Prompt hook 需调用 LLM 生成决策，复用 command 的输出解析逻辑

- [x] 在同一文件中实现 HTTP 执行器函数
  - 位置: execute_prompt_hook 函数之后
  - 实现 `pub async fn execute_http_hook(hook: &HookType::Http, input: &HookInput) -> HookAction` 函数
  - 逻辑（按 spec-design.md 第 843-895 行伪代码实现）:
    1. 调用 `crate::hooks::ssrf_guard::check_url(&hook.url)` 检查 SSRF，失败时记录 warn 日志并返回 Allow
    2. 提取 timeout（默认 600 秒）
    3. 创建 `reqwest::Client::new()`
    4. 构建 POST 请求：`.post(&hook.url).timeout(Duration::from_secs(timeout)).json(&input)`
    5. 注入 headers：遍历 hook.headers，对每个 value 调用 sanitize_header_value（下一步实现）去除 \r\n\0，使用 `.header(key, sanitized)` 注入
    6. 调用 `.send().await`
    7. 成功 → 检查 status.is_success()，成功则调用 parse_http_hook_response(&body)，失败记录 warn 日志返回 Allow
    8. 失败 → 记录 warn 日志，返回 Allow
  - 原因: HTTP hook 需 SSRF 防护和 CRLF 注入防护，响应解析协议与 command 不同

- [x] 在同一文件中实现 CRLF 注入防护辅助函数
  - 位置: execute_http_hook 函数之后
  - 实现 `fn sanitize_header_value(value: &str, allowed_env_vars: &HashSet<String>) -> String` 函数
  - 逻辑（按 spec-design.md 第 897-903 行伪代码实现）:
    1. 调用 `crate::hooks::variables::resolve_hook_variables_with_env` 完成 env var 白名单替换
    2. 过滤字符串，移除 `\r`、`\n`、`\0` 三个字符
    3. 返回清理后的字符串
  - 原因: HTTP headers 注入需防止 CRLF 攻击，避免 header 污染导致请求伪造

- [x] 在同一文件中实现 Agent 执行器函数
  - 位置: sanitize_header_value 函数之后
  - 添加 use: `use peri_agent::ReActAgent;`
  - 添加 use: `use peri_agent::state::AgentState;`
  - 添加 use: `use peri_agent::input::AgentInput;`
  - 添加 use: `use peri_middlewares::filesystem::FilesystemMiddleware;`
  - 实现 `pub async fn execute_agent_hook(hook: &HookType::Agent, input: &HookInput, llm_factory: &Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>, cwd: &str) -> HookAction` 函数
  - 逻辑（按 spec-design.md 第 978-1012 行伪代码实现）:
    1. 提取 timeout（默认 60 秒）和 max_turns（固定 50）
    2. 替换 prompt 中的 $ARGUMENTS 为 input 的 JSON 序列化
    3. 使用 `tokio::time::timeout(Duration::from_secs(timeout))` 包装执行
    4. 创建子 agent：
       - 调用 `llm_factory()` 获取 LLM 实例
       - 创建 `ReActAgent::new(BaseModelReactLLM::new(llm)).max_iterations(max_turns as u32)`
       - 添加 FilesystemMiddleware（注册文件系统工具）
       - 不添加 HookMiddleware（防止递归）
       - 不添加 SubAgentMiddleware（防止嵌套）
    5. 创建 `AgentState::new(cwd)`
    6. 调用 `agent.execute(AgentInput::text(&prompt), &mut agent_state).await`
    7. 成功 → 调用 extract_structured_output（下一步实现）提取 SyntheticOutputTool 结果
    8. 超时 → 记录 warn 日志，返回 Allow
  - 原因: Agent hook 需完整 agent 循环（最多 50 轮），必须防止递归和嵌套

- [x] 在同一文件中实现 SyntheticOutputTool 结果提取函数
  - 位置: execute_agent_hook 函数之后
  - 实现 `fn extract_structured_output(output: &AgentOutput) -> HookAction` 函数
  - 逻辑:
    1. 遍历 output.messages，查找 tool_result 类型
    2. 检查 tool_name 是否为 "SyntheticOutputTool"
    3. 提取 tool_output，序列化为 JSON 字符串
    4. 调用 `parse_command_hook_output(&output_str)` 解析
    5. 未找到 SyntheticOutputTool → 返回 Allow
  - 原因: Agent hook 通过 SyntheticOutputTool 返回结构化结果，需从 agent 输出中提取

- [x] 验证执行器函数在 mod.rs 中导出（Task 7 补充 mod.rs 时会添加）
  - 位置: `peri-middlewares/src/hooks/mod.rs`
  - Task 7 的 mod.rs 步骤会添加 `pub use executor::{...};`
  - 原因: Task 7 的 HookMiddleware 需要调用这 4 个函数，必须通过模块导出

- [x] 为 execute_command_hook 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/executor.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景（使用 mock subprocess）:
    - [成功退出码 0]: mock 子进程读取 stdin JSON，返回 `{"continue": false}` → 返回 HookAction::PreventContinuation
    - [非阻塞错误退出码 1]: mock 子进程返回 exit code 1 → 返回 HookAction::Allow
    - [阻塞错误退出码 2]: mock 子进程返回 exit code 2，stdout "blocked" → 返回 HookAction::Block { reason: "blocked" }
    - [纯文本输出]: mock 子进程返回 stdout "hello world" → 返回 HookAction::Allow
    - [超时]: mock 子进程 sleep 10 秒，timeout 设置 1 秒 → 返回 HookAction::Allow
    - [环境变量注入]: mock 子进程验证 CLAUDE_PROJECT_DIR/CLAUDE_PLUGIN_ROOT/CLAUDE_PLUGIN_DATA 存在 → 验证通过
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::executor::test_execute_command`
  - 预期: 所有测试通过

- [x] 为 execute_prompt_hook 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/executor.rs`
  - 测试场景:
    - [LLM 返回有效 JSON]: mock llm_factory 返回 `{"decision": "block", "reason": "test"}` → 返回 HookAction::Block { reason: "test" }
    - [LLM 返回纯文本]: mock llm_factory 返回 "allow" → 返回 HookAction::Allow
    - [LLM 调用失败]: mock llm_factory 返回 Err → 记录 warn 日志，返回 HookAction::Allow
    - [超时]: mock llm_factory sleep 10 秒，timeout 设置 1 秒 → 返回 HookAction::Allow
    - [$ARGUMENTS 替换]: prompt 包含 "$ARGUMENTS"，input 序列化为 JSON → 替换成功
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::executor::test_execute_prompt`
  - 预期: 所有测试通过

- [x] 为 execute_http_hook 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/executor.rs`
  - 测试场景（使用 mock HTTP server）:
    - [成功响应 JSON]: mock server 返回 200 + body `{"continue": false}` → 返回 HookAction::PreventContinuation
    - [成功响应空 body]: mock server 返回 200 + body "" → 返回 HookAction::Allow
    - [成功响应非 JSON]: mock server 返回 200 + body "plain text" → 记录 warn 日志，返回 HookAction::Allow
    - [HTTP 错误]: mock server 返回 500 → 记录 warn 日志，返回 HookAction::Allow
    - [SSRF 阻断]: url 为 "<http://169.254.169.254/latest/meta-data/>" → ssrf_guard 返回 Err，返回 HookAction::Allow
    - [CRLF 注入防护]: headers 包含 "Value: \r\nX-Injected: true" → 清理后移除 \r\n
    - [环境变量白名单替换]: headers 包含 "${API_KEY}"，allowed_vars=["API_KEY"]，env API_KEY="sk-xxx" → 替换为 "sk-xxx"
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::executor::test_execute_http`
  - 预期: 所有测试通过

- [x] 为 execute_agent_hook 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/executor.rs`
  - 测试场景:
    - [成功返回 SyntheticOutputTool]: mock agent 返回包含 SyntheticOutputTool 调用的 output → 解析为 HookAction
    - [超时]: mock agent sleep 10 秒，timeout 设置 1 秒 → 返回 HookAction::Allow
    - [防递归验证]: 检查创建的 agent 不包含 HookMiddleware → 验证通过
    - [防嵌套验证]: 检查创建的 agent 不包含 SubAgentMiddleware → 验证通过
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::executor::test_execute_agent`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 executor.rs 文件编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|warning:.*executor)"`
  - 预期: 无 error，无 executor 相关 warning

- [x] 验证函数导出正确
  - `grep -n "pub use executor" peri-middlewares/src/hooks/mod.rs`
  - 预期: 输出包含 `pub use executor::{execute_command_hook, execute_prompt_hook, execute_http_hook, execute_agent_hook};`

- [x] 验证单元测试覆盖所有场景
  - `cargo test -p peri-middlewares --lib hooks::executor 2>&1 | grep -E "test result:|running \d+ test"`
  - 预期: 输出包含 "running 16 test" 和 "test result: ok"

- [x] 验证 Command 执行器 stdin JSON 协议
  - `cargo test -p peri-middlewares --lib hooks::executor::test_execute_command::test_stdin_json_protocol 2>&1 | tail -5`
  - 预期: 测试通过，mock 子进程接收到完整的 HookInput JSON（含 session_id/cwd/hook_event_name 字段）

- [x] 验证 HTTP 执行器 SSRF 防护
  - `cargo test -p peri-middlewares --lib hooks::executor::test_execute_http::test_ssrf_blocking 2>&1 | tail -5`
  - 预期: 测试通过，对 169.254.169.254（metadata IP）的请求被阻止

- [x] 验证 Agent 执行器防递归
  - `cargo test -p peri-middlewares --lib hooks::executor::test_execute_agent::test_no_recursion 2>&1 | tail -5`
  - 预期: 测试通过，创建的子 agent 不包含 HookMiddleware

---

---

### Task 7: HookMiddleware 实现

**背景:**
HookMiddleware 是 hook 系统的核心，负责在 agent 生命周期关键节点触发已注册的 hook 规则并执行相应的动作（Block/ModifyInput/PermissionOverride/PreventContinuation 等）。本 Task 将 HookMiddleware 集成到中间件链（位置 10，HITL 之后、SubAgent 之前），实现完整的生命周期事件触发机制。

**涉及文件:**

- 新建: `peri-middlewares/src/hooks/middleware.rs`
- 修改: `peri-middlewares/src/hooks/mod.rs`（追加 executor/loader/middleware 声明和导出）

**执行步骤:**

- [x] 创建 HookMiddleware 结构体和构造函数
  - 位置: `peri-middlewares/src/hooks/middleware.rs` 文件开头
  - 定义结构体字段：

    ```rust
    pub struct HookMiddleware {
        hooks: Arc<RwLock<HashMap<HookEvent, Vec<RegisteredHook>>>>,
        llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
        cwd: String,
        session_id: String,
        transcript_path: String,
        permission_mode: String,
        current_model: String,
        once_fired: Arc<Mutex<HashSet<String>>>,
    }
    ```

  - 实现 `new()` 构造函数，接收 registered_hooks、llm_factory、cwd、session_id、transcript_path、permission_mode、current_model 参数
  - 初始化 hooks 为 Arc<RwLock<HashMap>>，按 event 分组存储
  - 初始化 once_fired 为空的 Arc<Mutex<HashSet>>
  - 原因: HookMiddleware 需要持有所有已注册的 hook 和运行时上下文，支持异步并发访问

- [x] 实现 Middleware trait 的 name() 和 collect_tools() 方法
  - 位置: `peri-middlewares/src/hooks/middleware.rs` 的 `impl<S: State> Middleware<S> for HookMiddleware` 块
  - `name()` 返回 `"HookMiddleware"`
  - `collect_tools()` 返回空 Vec（HookMiddleware 不提供工具）
  - 原因: HookMiddleware 是生命周期钩子中间件，不暴露工具给 agent

- [x] 实现 before_agent() 方法 - 触发 SessionStart 事件
  - 位置: `middleware.rs` 的 Middleware trait 实现块
  - 构建 `HookInput::session_start(source, model)`，source 为 "startup"，model 为 self.current_model
  - 调用 `self.fire_event(HookEvent::SessionStart, input).await`
  - 忽略返回的 HookAction（SessionStart 的 systemMessage/initialUserMessage 由后续步骤处理）
  - 返回 `Ok(())`
  - 原因: 每次 agent 执行前需要触发 SessionStart hooks，用于初始化上下文或注入系统消息

- [x] 实现 before_tool() 方法 - 触发 PreToolUse 和 PermissionRequest 事件
  - 位置: `middleware.rs` 的 Middleware trait 实现块
  - 构建 `HookInput::tool_call(tool_call)`（包含 tool_name、tool_input、tool_use_id）
  - 调用 `self.fire_event(HookEvent::PreToolUse, input.clone()).await`，保存为 pre_action
  - 调用 `self.fire_event(HookEvent::PermissionRequest, input.clone()).await`，保存为 perm_action
  - 合并两个 action：创建 `modified_call = tool_call.clone()`
  - 遍历 [pre_action, perm_action]：
    - `HookAction::Block { reason }` → 返回 `Err(AgentError::ToolRejected { tool: tool_call.name.clone(), reason })`
    - `HookAction::ModifyInput { new_input }` → 设置 `modified_call.input = new_input`
    - `HookAction::PermissionOverride { decision, reason }` → 记录权限覆盖（TODO: 通过 state 注入，Phase 2 实现）
    - `HookAction::PreventContinuation { stop_reason }` → 返回 `Err(AgentError::ToolRejected { tool: tool_call.name.clone(), reason: stop_reason.unwrap_or_else(|| "Hook prevented continuation".into()) })`
  - 返回 `Ok(modified_call)`
  - 原因: PreToolUse hooks 可以阻止工具调用或修改输入参数，PermissionRequest hooks 可以覆盖 HITL 权限决策

- [x] 实现 after_tool() 方法 - 触发 PostToolUse/PostToolUseFailure 事件
  - 位置: `middleware.rs` 的 Middleware trait 实现块
  - 根据 `result.is_error` 选择事件：`HookEvent::PostToolUseFailure` 或 `HookEvent::PostToolUse`
  - 构建 `HookInput::tool_result(tool_call, result)`（包含 tool_name、tool_input、tool_output）
  - 调用 `self.fire_event(event, input).await`
  - 忽略返回的 HookAction（PostToolUse hooks 不阻塞流程）
  - 返回 `Ok(())`
  - 原因: 工具执行后需要触发 hooks 用于日志记录、监控或副作用操作，失败不阻断 agent

- [x] 实现 after_agent() 方法 - 触发 Stop 事件
  - 位置: `middleware.rs` 的 Middleware trait 实现块
  - 构建 `HookInput::agent_output(output)`（包含最终输出内容）
  - 调用 `self.fire_event(HookEvent::Stop, input).await`，保存为 stop_action
  - 处理 stop_action：
    - `HookAction::AdditionalContext { context }` → 通过 state 追加到消息历史（TODO: 实现 state 消息追加）
    - `HookAction::InitialUserMessage { message }` → 通过 state 追加初始用户消息（TODO: 实现）
    - `HookAction::SystemMessage { message }` → 通过 state 注入系统消息（TODO: 实现）
    - `HookAction::PreventContinuation { stop_reason }` → 记录停止原因（TODO: 实现）
  - 返回 `Ok(output.clone())`
  - 原因: Agent 完成后需要触发 Stop hooks 用于结果后处理、上下文注入等

- [x] 实现 fire_event() 核心方法 - 事件触发和 hook 执行循环
  - 位置: `middleware.rs` 的 `impl HookMiddleware` 块
  - 签名: `async fn fire_event(&self, event: HookEvent, input: HookInput) -> HookAction`
  - 读取 `self.hooks.read().await`，获取 event 对应的 `matchers: Vec<RegisteredHook>`
  - 若 event 无注册 hooks，返回 `HookAction::Allow`
  - 初始化 `final_action = HookAction::Allow`
  - 遍历 matchers：
    1. **once 检查**: 若 `is_once_hook(registered)` 且 `was_once_fired(registered)`，continue
    2. **matcher 粗粒度匹配**: 若 `registered.matcher` 为 `Some(matcher)`，调用 `crate::hooks::matcher::matches_matcher(matcher, &input.tool_name.unwrap_or(""))`，不匹配则 continue
    3. **if 细粒度条件匹配**: 若 `registered.hook.get_condition()` 为 `Some(condition)`，调用 `crate::hooks::matcher::matches_if_condition(condition, &input.tool_name.unwrap_or(""), &input.tool_input.clone().unwrap_or(serde_json::Value::Null))`，不匹配则 continue
    4. **变量替换**: 调用 `crate::hooks::variables::resolve_hook_variables(...)` 替换 hook 配置中的变量，返回 resolved_hook
    5. **执行 hook**: 根据 HookType 调用对应执行器（execute_command/execute_prompt/execute_http/execute_agent），获取 action
    6. **async 语义处理**: 若 hook 为 `async: true`，使用 `tokio::spawn` 后台执行，action 设为 `Allow`，continue
    7. **once 标记**: 若 `is_once_hook(&registered)`，调用 `mark_once_fired(&registered)`
    8. **短路检查**: 若 action 为 `Block { .. }` 或 `PreventContinuation { .. }`，立即返回 action
    9. **合并 ModifyInput**: 若 action 为 `ModifyInput { .. }`，设置 `final_action = action`（后执行覆盖先执行）
  - 返回 `final_action`
  - 原因: fire_event 是 hook 系统的核心循环，负责匹配、执行、合并所有注册的 hook，处理 once/async/短路等语义

- [x] 实现 once 追踪辅助方法
  - 位置: `middleware.rs` 的 `impl HookMiddleware` 块
  - `fn is_once_hook(hook: &HookType) -> bool`: 调用 `hook.is_once()`（Task 1 已在 HookType 上实现）
  - `fn was_once_fired(&self, registered: &RegisteredHook) -> bool`: 检查 `once_fired` HashSet 是否包含 `{plugin_id}:{event}:{matcher}`
  - `fn mark_once_fired(&self, registered: &RegisteredHook)`: 向 `once_fired` 插入 `{plugin_id}:{event}:{matcher}`
  - 原因: once 语义需要追踪已执行的 hook，避免重复触发

- [x] 补充 hooks/mod.rs 模块入口文件（追加 executor/loader/middleware 声明和导出）
  - 位置: `peri-middlewares/src/hooks/mod.rs`
  - 在 Task 3/5 已创建的内容基础上，追加以下模块声明和导出：

    ```rust
    pub mod executor;
    pub mod loader;
    pub mod middleware;

    pub use middleware::HookMiddleware;
    pub use executor::{execute_command_hook, execute_prompt_hook, execute_http_hook, execute_agent_hook};
    pub use types::*;
    ```

  - 注意：不重复添加 `pub mod ssrf_guard;`（Task 5 已添加）
  - 原因: 提供统一的模块入口，简化外部引用。Task 3 已声明 types/matcher/variables/output_parser，Task 5 已声明 ssrf_guard

- [x] 为 HookMiddleware 编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/middleware.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_fire_event_no_hooks`: 无注册 hooks 时返回 Allow
    - `test_fire_event_once_semantic`: once hook 只触发一次
    - `test_fire_event_matcher_filter`: matcher 不匹配时跳过 hook
    - `test_fire_event_condition_filter`: if 条件不匹配时跳过 hook
    - `test_fire_event_block_short_circuit`: Block action 立即短路返回
    - `test_fire_event_modify_input_merge`: 后执行的 ModifyInput 覆盖先执行的
    - `test_before_tool_block`: PreToolUse hook 返回 Block 时 before_tool 返回 ToolRejected 错误
    - `test_before_tool_modify_input`: PreToolUse hook 返回 ModifyInput 时 before_tool 返回修改后的 ToolCall
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::middleware`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 HookMiddleware 结构体和构造函数编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -A5 "HookMiddleware"`
  - 预期: 无类型错误或未定义字段错误

- [x] 验证 Middleware trait 实现完整
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "not implemented|missing method"`
  - 预期: 无"not implemented"或"missing method"错误

- [x] 验证模块导出正确
  - `grep -n "pub use middleware::HookMiddleware" peri-middlewares/src/hooks/mod.rs`
  - 预期: 找到一行导出语句

- [x] 验证 fire_event 核心逻辑编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "fire_event|matches_matcher|matches_if_condition"`
  - 预期: 无类型错误或未定义方法错误

- [x] 验证单元测试通过
  - `cargo test -p peri-middlewares --lib hooks::middleware 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok" 且无 "FAILED" 标记

**认知变更:**

- [x] [CLAUDE.md] HookMiddleware 插入中间件链位置 10（HITL 之后、SubAgent 之前），在 agent 生命周期关键节点触发 hook 规则
- [x] [CLAUDE.md] HookMiddleware 的 fire_event() 方法是核心循环，按顺序执行：once 检查 → matcher 匹配 → if 条件匹配 → 变量替换 → 执行 hook → once 标记 → 短路检查 → 合并 action
- [x] [CLAUDE.md] Block/PreventContinuation action 立即短路返回，不再执行后续 hooks；ModifyInput action 以后执行的覆盖先执行的
- [x] [CLAUDE.md] async hook 使用 tokio::spawn 后台执行，不阻塞 agent 主流程，返回的 action 被忽略（Phase 2 实现 asyncRewake）
- [x] [CLAUDE.md] once hook 通过 once_fired: HashSet<String> 追踪，key 格式为 `{plugin_id}:{event}:{matcher}`

---

---

### Task 8: 插件 Hook 加载与注册

**背景:**
插件系统需要支持 hooks 配置加载，从插件的 `hooks/hooks.json` 文件或 `plugin.json` 内的 `hooks` 字段提取 hook 规则，并注册到 HookMiddleware。当前 `PluginManifest.hooks` 字段为 `Option<serde_json::Value>`，仅预留未实现。`LoadedPlugin` 不持有 hooks 数据。本 Task 修改插件加载流程，新增 `extract_hooks()` 函数，扩展 `PluginLoadResult` 聚合 hooks，并在 `run_universal_agent()` 中创建 HookMiddleware 注入中间件链位置 10。

**涉及文件:**

- 新建: `peri-middlewares/src/hooks/loader.rs`
- 修改: `peri-middlewares/src/plugin/types.rs`
- 修改: `peri-middlewares/src/plugin/loader.rs`
- 修改: `peri-middlewares/src/plugin/mod.rs`
- 修改: `peri-tui/src/app/agent.rs`
- 修改: `peri-tui/src/app/agent_ops.rs`

**执行步骤:**

- [x] 在 plugin/types.rs 中修改 PluginManifest.hooks 字段类型
  - 位置: `peri-middlewares/src/plugin/types.rs:104`
  - 将 `pub hooks: Option<serde_json::Value>` 改为 `pub hooks: Option<HooksConfig>`
  - 在文件顶部添加导入: `use crate::hooks::types::HooksConfig;`
  - 原因: 类型安全，避免运行时 JSON 解析错误，`HooksConfig = HashMap<HookEvent, Vec<HookMatchRule>>`

- [x] 在 plugin/loader.rs 中为 LoadedPlugin 添加 hooks_config 字段
  - 位置: `peri-middlewares/src/plugin/loader.rs:66` 的 `LoadedPlugin` 结构体
  - 在 `data_path` 字段后添加: `pub hooks_config: Option<HooksConfig>,`
  - 在文件顶部添加导入: `use crate::hooks::types::HooksConfig;`
  - 原因: 存储解析后的 hooks 配置，供 HookMiddleware 注册使用

- [x] 在 plugin/loader.rs 中修改 load_plugins 函数调用 extract_hooks
  - 位置: `peri-middlewares/src/plugin/loader.rs:275` 的 `load_plugins()` 函数
  - 在第 290 行 `let mcp_servers = extract_mcp_servers(&manifest, &plugin.install_path);` 之后添加:

    ```rust
    let hooks_config = extract_hooks(&manifest, &plugin.install_path);
    ```

  - 在第 293 行 `result.push(LoadedPlugin {` 的结构体初始化中，`data_path,` 之后添加 `hooks_config,`
  - 原因: 加载插件时提取 hooks 配置并存储到 LoadedPlugin

- [x] 更新 plugin/loader.rs 中所有直接构造 LoadedPlugin 的测试用例，添加 hooks_config 字段
  - 位置: `peri-middlewares/src/plugin/loader.rs` 的 `#[cfg(test)] mod tests` 模块
  - 搜索所有 `LoadedPlugin {` 构造（约 line 858, 888, 1048, 1091），在每个构造体的 `data_path: PathBuf::new(),` 之后添加 `hooks_config: None,`
  - 原因: 新增 `hooks_config` 字段后，所有直接构造 LoadedPlugin 的测试必须提供该字段，否则编译失败

- [x] 在 hooks/loader.rs 中实现 extract_hooks 函数
  - 位置: 新建文件 `peri-middlewares/src/hooks/loader.rs`
  - 实现函数:

    ```rust
    use std::path::Path;
    use std::fs;
    use crate::hooks::types::{HooksConfig, HookEvent};
    use crate::plugin::types::PluginManifest;
    
    pub(crate) fn extract_hooks(
        manifest: &PluginManifest,
        install_path: &Path,
    ) -> Option<HooksConfig> {
        // 优先级 1: hooks/hooks.json 文件
        let hooks_file = install_path.join("hooks").join("hooks.json");
        if hooks_file.exists() {
            if let Ok(content) = fs::read_to_string(&hooks_file) {
                if let Ok(config) = serde_json::from_str::<HooksConfig>(&content) {
                    return Some(config);
                }
            }
        }
    
        // 优先级 2: plugin.json 内的 hooks 字段
        if let Some(hooks) = &manifest.hooks {
            return Some(hooks.clone());
        }
    
        None
    }
    ```

  - 原因: 对齐 Claude Code hooks 加载优先级（hooks.json 文件优先于 plugin.json 内嵌字段）

- [x] 在 plugin/loader.rs 中扩展 PluginLoadResult 聚合 hooks
  - 位置: `peri-middlewares/src/plugin/loader.rs:368` 的 `PluginLoadResult` 结构体
  - 在 `all_commands: Vec<CommandEntry>,` 之后添加: `pub all_hooks: Vec<RegisteredHook>,`
  - 在文件顶部添加导入: `use crate::hooks::types::RegisteredHook;`
  - 原因: 聚合所有插件的 hooks，供 TUI 层传递给 HookMiddleware

- [x] 在 plugin/loader.rs 中实现 hooks 聚合逻辑
  - 位置: `peri-middlewares/src/plugin/loader.rs:377` 的 `load_enabled_plugins_aggregated()` 函数
  - 在第 398 行 `let all_commands: Vec<CommandEntry> = plugins.iter().flat_map(|p| p.commands.clone()).collect();` 之后添加:

    ```rust
    let all_hooks: Vec<RegisteredHook> = plugins
        .iter()
        .filter_map(|plugin| {
            let config = plugin.hooks_config.as_ref()?;
            let mut hooks = Vec::new();
            for (event, matchers) in config {
                for rule in matchers {
                    for hook_def in &rule.hooks {
                        hooks.push(RegisteredHook {
                            hook: hook_def.clone(),
                            event: event.clone(),
                            // matcher 优先级：HookMatchRule.matcher > HookType 内 matcher
                            matcher: rule.matcher.clone().or_else(|| hook_def.get_matcher().cloned()),
                            plugin_name: plugin.name.clone(),
                            plugin_id: plugin.manifest.id.clone(),
                            plugin_root: plugin.install_path.clone(),
                            plugin_data_dir: plugin.data_path.clone(),
                            plugin_options: plugin.manifest.options
                                .as_ref()
                                .unwrap_or(&vec![])
                                .iter()
                                .filter_map(|opt| {
                                    opt.default.as_ref().map(|v| (opt.name.clone(), v.clone()))
                                })
                                .collect(),
                        });
                    }
                }
            }
            Some(hooks)
        })
        .flatten()
        .collect();
    ```

  - 在第 400 行 `PluginLoadResult {` 的返回值初始化中，`all_commands,` 之后添加 `all_hooks,`
  - 在第 387 行静默失败返回的默认值中，`all_commands: vec![],` 之后添加 `all_hooks: vec![],`
  - 原因: 将所有插件的 hooks 配置转换为 RegisteredHook，供 HookMiddleware 使用

- [x] 在 plugin/mod.rs 中导出 hooks 相关类型
  - 位置: `peri-middlewares/src/plugin/mod.rs`
  - 在文件顶部添加: `pub use crate::hooks::types::{HooksConfig, RegisteredHook};`
  - 原因: 方便其他模块访问 hooks 类型

- [x] 验证 loader 在 mod.rs 中导出（Task 7 补充 mod.rs 时会添加）
  - 位置: `peri-middlewares/src/hooks/mod.rs`
  - Task 7 的 mod.rs 步骤会添加 `pub mod loader;`
  - 原因: 使 extract_hooks 函数可被 plugin/loader 调用

- [x] 在 agent_ops.rs 中提取 plugin_hooks 传递给 agent
  - 位置: `peri-tui/src/app/agent_ops.rs:184`
  - 在第 184 行 `let mcp_init_rx = self.mcp_init_rx.clone();` 之后添加:

    ```rust
    let plugin_hooks = self
        .plugin_data
        .as_ref()
        .map(|pd| pd.all_hooks.clone())
        .unwrap_or_default();
    ```

  - 在第 209 行 `agent::run_universal_agent(agent::AgentRunConfig {` 的参数列表末尾，`preload_skills,` 之后添加 `plugin_hooks,`
  - 原因: 将聚合的 hooks 传递给 agent 创建流程

- [x] 在 agent.rs 的 AgentRunConfig 中添加 plugin_hooks 字段
  - 位置: `peri-tui/src/app/agent.rs` 的 `AgentRunConfig` 结构体定义
  - 在结构体中添加字段: `pub plugin_hooks: Vec<RegisteredHook>,`
  - 在文件顶部添加导入: `use peri_middlewares::hooks::types::RegisteredHook;`
  - 原因: 接收来自 TUI 的 hooks 配置

- [x] 在 agent.rs 的 run_universal_agent 中创建 HookMiddleware
  - 位置: `peri-tui/src/app/agent.rs` 的 `run_universal_agent()` 函数
  - 在第 281 行 `.add_middleware(Box::new(hitl))` 之后、第 282 行 `.add_middleware(Box::new(subagent))` 之前插入:

    ```rust
    .add_middleware(Box::new(peri_middlewares::hooks::HookMiddleware::new(
        config.plugin_hooks,
        llm_factory.clone(),
        cwd.to_string(),
        thread_id.clone(),
        format!("{}/transcripts/{}.json", cwd, thread_id),
        permission_mode.to_string(),
        provider.display_name().to_string(),
    )))
    ```

  - 在文件顶部添加导入: `use peri_middlewares::hooks::HookMiddleware;`
  - 原因: 将 HookMiddleware 插入中间件链位置 10（HITL 之后、SubAgent 之前）

- [x] 为 extract_hooks 函数编写单元测试
  - 测试文件: `peri-middlewares/src/hooks/loader.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景:
    - [hooks.json 文件优先]: 创建模拟插件目录，hooks/hooks.json 存在且有效 → 返回 HooksConfig
    - [plugin.json hooks 字段回退]: hooks.json 不存在，plugin.json 内 hooks 字段有效 → 返回 HooksConfig
    - [两者都不存在]: hooks.json 和 plugin.json hooks 都不存在 → 返回 None
    - [hooks.json 解析失败]: hooks.json 存在但 JSON 格式错误 → 回退到 plugin.json hooks
    - [空 hooks 配置]: hooks.json 内容为 `{}` → 返回空 HashMap
  - 运行命令: `cargo test -p peri-middlewares --lib hooks::loader`
  - 预期: 所有测试通过

- [x] 为 hooks 聚合逻辑编写集成测试
  - 测试文件: `peri-middlewares/src/plugin/loader.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景:
    - [单插件 hooks 聚合]: 加载单个带 hooks 的插件 → PluginLoadResult.all_hooks 包含正确数量的 RegisteredHook
    - [多插件 hooks 聚合]: 加载多个带 hooks 的插件 → all_hooks 包含所有插件的 hooks
    - [matcher 优先级]: HookMatchRule.matcher 和 HookType 内 matcher 都存在 → 优先使用 HookMatchRule.matcher
    - [plugin_options 转换]: plugin.options 包含 userConfig 值 → 正确转换为 HashMap
  - 运行命令: `cargo test -p peri-middlewares --lib plugin::loader::test_hooks_aggregation`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 PluginManifest.hooks 类型修改编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "hooks|HooksConfig" | head -10`
  - 预期: 无类型错误，输出显示 HooksConfig 类型正确导入

- [x] 验证 LoadedPlugin.hooks_config 字段存在
  - `grep -n "pub hooks_config" /Users/konghayao/code/ai/peri/peri-middlewares/src/plugin/loader.rs`
  - 预期: 找到一行字段声明

- [x] 验证 extract_hooks 函数编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "extract_hooks|loader" | head -5`
  - 预期: 无编译错误，extract_hooks 函数签名正确

- [x] 验证 PluginLoadResult.all_hooks 字段存在
  - `grep -n "pub all_hooks" /Users/konghayao/code/ai/peri/peri-middlewares/src/plugin/loader.rs`
  - 预期: 找到一行字段声明

- [x] 验证 hooks 聚合逻辑编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "all_hooks|RegisteredHook" | head -5`
  - 预期: 无类型错误，RegisteredHook 正确导入

- [x] 验证 AgentRunConfig.plugin_hooks 字段存在
  - `grep -n "pub plugin_hooks" /Users/konghayao/code/ai/peri/peri-tui/src/app/agent.rs`
  - 预期: 找到一行字段声明

- [x] 验证 HookMiddleware 创建代码编译通过
  - `cargo check -p peri-tui --bin peri-tui 2>&1 | grep -E "HookMiddleware|hooks" | head -10`
  - 预期: 无编译错误，HookMiddleware 正确导入和调用

- [x] 验证 extract_hooks 单元测试通过
  - `cargo test -p peri-middlewares --lib hooks::loader 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok" 且无 "FAILED" 标记

- [x] 验证 hooks 聚合集成测试通过
  - `cargo test -p peri-middlewares --lib plugin::loader::test_hooks 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok" 且无 "FAILED" 标记

**认知变更:**

- [x] [CLAUDE.md] 插件 hooks 配置加载优先级：`hooks/hooks.json` 文件优先于 `plugin.json` 内的 `hooks` 字段，对齐 Claude Code 行为
- [x] [CLAUDE.md] LoadedPlugin 新增 `hooks_config: Option<HooksConfig>` 字段，在 `load_plugins()` 时通过 `extract_hooks()` 提取
- [x] [CLAUDE.md] PluginLoadResult 新增 `all_hooks: Vec<RegisteredHook>` 字段，聚合所有插件的 hooks，供 TUI 层传递给 HookMiddleware
- [x] [CLAUDE.md] [TRAP] PluginManifest.hooks 类型从 `Option<serde_json::Value>` 改为 `Option<HooksConfig>` 后，现有测试中的 `"hooks": {}` 仍能正确反序列化为空 HashMap，但显式 `null` 会反序列化为 `None`。新增测试时必须验证这两种情况。
- [x] [CLAUDE.md] HookMiddleware 插入中间件链位置 10（HITL 之后、SubAgent 之前），在 `run_universal_agent()` 中创建，接收 `plugin_hooks: Vec<RegisteredHook>` 参数
- [x] [CLAUDE.md] RegisteredHook.plugin_options 从 `PluginManifest.options` 的 `default` 值转换（`PluginOption` 结构体没有 `value` 字段，使用 `opt.default`），key 为 `opt.name`，value 为 `opt.default`（`serde_json::Value` 类型）

---

### Task 9: AgentEvent 扩展与 SubAgent 事件转发

**背景:**
HookMiddleware 需要监听 agent 生命周期事件（SubagentStart/Stop、SessionEnd、PreCompact/PostCompact、UserPromptSubmit）来触发对应的 hook。当前 `peri-agent/src/agent/events.rs` 的 `AgentEvent` 枚举没有这些变体。同时 `SubAgentMiddleware` 在执行子 agent 前后需要发出 `SubagentStarted`/`SubagentStopped` 事件，HookMiddleware 通过 event_handler 监听这些事件并触发 hook。

**涉及文件:**

- 修改: `peri-agent/src/agent/events.rs`（新增 5 个 AgentEvent 变体）
- 修改: `peri-middlewares/src/subagent/tool.rs`（子 agent 执行前后发出事件）

**执行步骤:**

- [x] 在 AgentEvent 枚举中新增 5 个变体
  - 位置: `peri-agent/src/agent/events.rs` 的 `AgentEvent` 枚举，在 `BackgroundTaskCompleted` 变体之后
  - 新增变体（对齐 spec-design.md 第 1163-1169 行）：

    ```rust
    /// 子 agent 开始执行
    SubagentStarted { agent_name: String },
    /// 子 agent 执行完成
    SubagentStopped { agent_name: String, result: String },
    /// Session 结束
    SessionEnded,
    /// 上下文压缩开始
    CompactStarted,
    /// 上下文压缩完成
    CompactCompleted,
    ```

  - 不添加 `UserPromptSubmitted` 变体（此事件由 TUI 层在 `submit_message()` 时直接触发，不经过 AgentEvent 通道）
  - 原因: HookMiddleware 通过 event_handler 监听这些事件，触发对应的 hook 规则

- [x] 在 SubAgentMiddleware 的子 agent 执行前后发出事件
  - 位置: `peri-middlewares/src/subagent/tool.rs`，子 agent 执行逻辑中
  - 在子 agent 调用 `agent.execute()` **之前**，通过 event_handler 发出 `AgentEvent::SubagentStarted { agent_name: agent_id.clone() }`
  - 在子 agent `execute()` 返回**之后**，通过 event_handler 发出 `AgentEvent::SubagentStopped { agent_name: agent_id.clone(), result: output_summary.clone() }`
  - event_handler 从 SubAgentMiddleware 构造函数中获取（已持有 `Arc<dyn AgentEventHandler>`）
  - 若 event_handler 为 None（SubAgentMiddleware 在非 TUI 环境中），跳过事件发出
  - output_summary 为子 agent 最终输出的截断文本（最多 500 字符，使用 `s.chars().take(500).collect::<String>()`）
  - 原因: HookMiddleware 监听 SubagentStarted/Stopped 事件触发 SubagentStart/SubagentStop hooks

- [x] 为新增的 AgentEvent 变体编写序列化测试
  - 测试文件: `peri-agent/src/agent/events.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景:
    - [SubagentStarted 序列化]: `AgentEvent::SubagentStarted { agent_name: "test-agent".into() }` → JSON 包含 `"type":"subagent_started"` 和 `"agent_name":"test-agent"`
    - [SubagentStopped 序列化]: `AgentEvent::SubagentStopped { agent_name: "test-agent".into(), result: "done".into() }` → JSON 包含 `"type":"subagent_stopped"`
    - [SessionEnded 序列化]: `AgentEvent::SessionEnded` → JSON 包含 `"type":"session_ended"`
    - [CompactStarted 序列化]: `AgentEvent::CompactStarted` → JSON 包含 `"type":"compact_started"`
    - [CompactCompleted 序列化]: `AgentEvent::CompactCompleted` → JSON 包含 `"type":"compact_completed"`
  - 运行命令: `cargo test -p peri-agent --lib agent::events`
  - 预期: 所有测试通过

- [x] 为 SubAgent 事件转发编写单元测试
  - 测试文件: `peri-middlewares/src/subagent/tool.rs` 的 `#[cfg(test)] mod tests` 模块
  - 测试场景:
    - [事件发出验证]: 创建 SubAgentMiddleware，传入 mock event_handler，执行子 agent 后验证 SubagentStarted 和 SubagentStopped 事件被发出
    - [无 event_handler 时不 panic]: 不传入 event_handler，执行子 agent 不产生错误
  - 运行命令: `cargo test -p peri-middlewares --lib subagent::tool`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 AgentEvent 新变体编译通过
  - `cargo check -p peri-agent --lib 2>&1 | grep -E "(error|SubagentStarted|SessionEnded)"`
  - 预期: 无 error

- [x] 验证 SubAgent 事件转发编译通过
  - `cargo check -p peri-middlewares --lib 2>&1 | grep -E "(error|subagent)"`
  - 预期: 无 error

- [x] 验证 AgentEvent 序列化测试通过
  - `cargo test -p peri-agent --lib agent::events 2>&1 | grep "test result:"`
  - 预期: test result: ok

- [x] 验证 SubAgent 事件转发测试通过
  - `cargo test -p peri-middlewares --lib subagent::tool 2>&1 | grep "test result:"`
  - 预期: test result: ok

**认知变更:**

- [x] [CLAUDE.md] AgentEvent 新增 5 个变体：`SubagentStarted`/`SubagentStopped`/`SessionEnded`/`CompactStarted`/`CompactCompleted`，用于 HookMiddleware 监听 agent 生命周期事件
- [x] [CLAUDE.md] SubAgentMiddleware 在子 agent 执行前后通过 event_handler 发出 `SubagentStarted`/`SubagentStopped` 事件
- [x] [CLAUDE.md] UserPromptSubmit hook 由 TUI 层在 `submit_message()` 时直接触发，不经过 AgentEvent 通道

---

### Task 10: Plugin Hook Support 完整验收

**前置条件:**

- Task 1-9 全部完成
- 构建命令: `cargo build`

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -15`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤

2. 验证 hooks 模块完整编译和导出
   - `cargo test -p peri-middlewares --lib hooks 2>&1 | grep -E "test result:"`
   - 预期: hooks 模块全部测试通过
   - 失败排查: 检查 mod.rs 导出是否完整

3. 验证 TUI 层集成编译通过
   - `cargo build -p peri-tui 2>&1 | tail -5`
   - 预期: 输出包含 "Finished" 且无 error
   - 失败排查: 检查 Task 8 的 agent.rs / agent_ops.rs 修改是否引入编译错误

4. 验证 PluginManifest.hooks 类型变更向后兼容
   - `cargo test -p peri-middlewares --lib plugin::types::tests 2>&1 | grep -E "test result:"`
   - 预期: plugin/types.rs 现有测试全部通过（`"hooks": {}` 正确反序列化为空 HashMap）
   - 失败排查: 检查 Task 8 的类型变更是否破坏现有测试

5. 验证 HookMiddleware 在中间件链中的位置正确
   - `grep -n "add_middleware" peri-tui/src/app/agent.rs`
   - 预期: HookMiddleware 出现在 hitl 和 subagent 之间
   - 失败排查: 检查 Task 8 的中间件注入顺序

6. 验证 AgentEvent 扩展编译通过
   - `cargo test -p peri-agent --lib agent::events 2>&1 | grep "test result:"`
   - 预期: test result: ok
   - 失败排查: 检查 Task 9 的新增变体序列化

7. 验证 SubAgent 事件转发
   - `cargo test -p peri-middlewares --lib subagent::tool 2>&1 | grep "test result:"`
   - 预期: test result: ok
   - 失败排查: 检查 Task 9 的 subagent/tool.rs 修改

---
