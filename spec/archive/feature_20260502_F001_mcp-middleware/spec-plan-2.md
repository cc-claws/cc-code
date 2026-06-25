# MCP Middleware 执行计划（二）：集成与接入

**目标:** 实现 MCP 资源读取工具、中间件集成、HITL 审批扩展和 TUI 接入

**技术栈:** Rust 2021, rmcp 0.14, Middleware trait, BaseTool trait, TUI App

**设计文档:** spec/feature_20260502_F001_mcp-middleware/spec-design.md

## 改动总览

本计划实现 MCP 集成层：Task 5（资源读取工具）和 Task 6（中间件组装）将 Task 1-4 的核心组件串接为完整的 McpMiddleware；Task 7 扩展 HITL 审批规则（仅修改 1 个函数）；Task 8 在 TUI 层接入 pool 生命周期。关键依赖链：Task 5 和 Task 6 依赖 Plan 1 的全部产出，Task 8 依赖 Task 6。关键决策：MCP 工具通过 `mcp__` 前缀通配匹配进入 HITL 审批，`mcp_read_resource` 不拦截（只读操作）。

---

### Task 0: 环境准备

**背景:**
Plan 1 的环境准备和核心组件已就绪。本计划仅需验证 Plan 1 的产出可正常编译。

**执行步骤:**
- [ ] 验证 Plan 1 核心组件编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: `Finished` 无错误
- [ ] 验证 mcp 模块已有 5 个子文件
  - `ls peri-middlewares/src/mcp/`
  - 预期: 包含 mod.rs, config.rs, transport.rs, client.rs, tool_bridge.rs

**检查步骤:**
- [ ] Plan 1 产出完整
  - `cargo test -p peri-middlewares --lib -- mcp 2>&1 | tail -10`
  - 预期: 所有 mcp 模块测试通过

---

### Task 5: McpResourceTool 资源读取工具

**背景:**
[业务语境] — MCP 协议中 Resources 是服务器暴露的静态/半静态数据（如数据库 schema、文件内容、API 文档），与 Tools（可执行操作）不同。`McpResourceTool` 提供统一的资源读取入口，让 LLM 能按需查询已连接 MCP 服务器的资源内容。
[修改原因] — 当前代码中不存在 MCP 资源读取能力。`BaseTool` trait 仅被内置工具（Filesystem、Terminal、SubAgent 等）实现，缺少面向 MCP Resources 的适配器。需要新建 `resource_tool.rs`，将 `peer.read_resource()` 封装为 `BaseTool` 实现。
[上下游影响] — 本 Task 依赖 Task 3（`McpClientPool` 的 `get_client()`、`resource_summary()`、`has_resources()` 方法），输出 `McpResourceTool` 结构体，被 Task 6（McpMiddleware 在 `collect_tools()` 中按需创建实例）直接依赖。

**涉及文件:**
- 新建: `peri-middlewares/src/mcp/resource_tool.rs`
- 修改: `peri-middlewares/src/mcp/mod.rs`（添加 `pub mod resource_tool` + 重导出）

**执行步骤:**

- [ ] 在 `resource_tool.rs` 顶部定义 `McpResourceTool` 结构体和 `ResourceError` 错误类型
  - 位置: 新建 `peri-middlewares/src/mcp/resource_tool.rs`，文件顶部
  - 关键逻辑:
    ```rust
    use std::sync::Arc;
    use serde_json::{json, Value};
    use thiserror::Error;
    use async_trait::async_trait;

    use crate::tools::BaseTool;
    use super::client::{McpClientPool, McpClientHandle, ClientStatus};

    /// 资源读取工具错误
    #[derive(Debug, Error)]
    pub enum ResourceError {
        #[error("MCP 服务器 \"{server}\" 未找到")]
        ServerNotFound { server: String },
        #[error("MCP 服务器 \"{server}\" 未连接 (状态: {status:?})")]
        NotConnected { server: String, status: ClientStatus },
        #[error("MCP 资源读取失败: {server}: {reason}")]
        ReadFailed { server: String, reason: String },
        #[error("MCP 资源读取参数错误: {0}")]
        InvalidParam(String),
    }

    /// MCP 资源读取工具——统一资源读取入口
    /// description() 动态注入已连接 server 的可用 resource URI 列表
    pub struct McpResourceTool {
        client_pool: Arc<McpClientPool>,
    }
    ```
  - 原因: 遵循项目编码规范（库 crate 用 thiserror）；`McpResourceTool` 持有 `Arc<McpClientPool>` 引用，与 `McpToolBridge` 持有 `Arc<McpClientHandle>` 的模式一致；错误类型携带 server 上下文便于诊断

- [ ] 实现 `McpResourceTool::new()` 构造函数
  - 位置: `resource_tool.rs`，`McpResourceTool` 结构体定义之后
  - 关键逻辑:
    ```rust
    impl McpResourceTool {
        pub fn new(client_pool: Arc<McpClientPool>) -> Self {
            Self { client_pool }
        }
    }
    ```
  - 原因: 构造函数接收 `Arc<McpClientPool>` 共享引用，由 Task 6（McpMiddleware）在 `collect_tools()` 中创建实例时传入

- [ ] 实现 `BaseTool` trait 的 `name()` 和 `parameters()` 方法
  - 位置: `resource_tool.rs`，`McpResourceTool` impl 块
  - 关键逻辑:
    ```rust
    const TOOL_NAME: &str = "mcp_read_resource";

    #[async_trait::async_trait]
    impl BaseTool for McpResourceTool {
        fn name(&self) -> &str {
            TOOL_NAME
        }

        fn parameters(&self) -> Value {
            json!({
                "type": "object",
                "properties": {
                    "server_name": {
                        "type": "string",
                        "description": "MCP 服务器名称（配置中的 key）"
                    },
                    "uri": {
                        "type": "string",
                        "description": "要读取的资源 URI"
                    }
                },
                "required": ["server_name", "uri"]
            })
        }
    }
    ```
  - 原因: `name()` 返回固定值 `mcp_read_resource`（与 spec-design.md 一致）；`parameters()` 返回固定的 JSON Schema，`server_name` 和 `uri` 均为必填字符串参数

- [ ] 实现 `BaseTool` trait 的 `description()` 方法——动态生成包含资源列表的描述
  - 位置: `resource_tool.rs`，`BaseTool` impl 块，`parameters()` 之后
  - 关键逻辑:
    ```rust
    #[async_trait::async_trait]
    impl BaseTool for McpResourceTool {
        // ... name() 和 parameters() 同上 ...

        fn description(&self) -> String {
            let summary = self.client_pool.resource_summary();
            if summary.is_empty() {
                return "Read a resource from an MCP server. No resources currently available.".to_string();
            }
            format!(
                "Read a resource from an MCP server. Available resources:\n{}",
                summary
            )
        }
    }
    ```
  - 原因: `description()` 每次被调用时从 `client_pool.resource_summary()` 获取最新的资源列表摘要，使 LLM 在工具列表中能看到哪些资源可用；资源列表为空时返回降级描述

- [ ] 实现 `BaseTool` trait 的 `invoke()` 方法——调用 rmcp 读取资源并格式化返回
  - 位置: `resource_tool.rs`，`BaseTool` impl 块，`description()` 之后
  - 关键逻辑:
    ```rust
    use rmcp::model::ReadResourceRequestParam;

    const RESOURCE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

    #[async_trait::async_trait]
    impl BaseTool for McpResourceTool {
        // ... name(), parameters(), description() 同上 ...

        async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            // 1. 提取参数
            let server_name = input.get("server_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ResourceError::InvalidParam("缺少 server_name 参数".into()))?;
            let uri = input.get("uri")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ResourceError::InvalidParam("缺少 uri 参数".into()))?;

            // 2. 获取客户端句柄
            let handle = self.client_pool.get_client(server_name)
                .ok_or_else(|| ResourceError::ServerNotFound { server: server_name.to_string() })?;

            // 3. 检查连接状态
            if !matches!(handle.status, ClientStatus::Connected) {
                return Err(Box::new(ResourceError::NotConnected {
                    server: server_name.to_string(),
                    status: handle.status,
                }));
            }

            // 4. 调用 rmcp read_resource（带 120s 超时，与 Bash 工具对齐）
            let result = tokio::time::timeout(
                RESOURCE_READ_TIMEOUT,
                handle.peer.read_resource(ReadResourceRequestParam { uri: uri.to_string() })
            ).await;

            match result {
                Ok(Ok(resource_contents)) => {
                    // 5. 格式化资源内容为字符串
                    let mut output = Vec::new();
                    for content in &resource_contents.contents {
                        match &content.data {
                            rmcp::model::ResourceContents::Text { text, .. } => {
                                output.push(format!("[text/{}]", content.mime_type.as_deref().unwrap_or("plain")));
                                output.push(text.clone());
                            }
                            rmcp::model::ResourceContents::Blob { blob, .. } => {
                                output.push(format!("[blob/{}]", content.mime_type.as_deref().unwrap_or("octet-stream")));
                                output.push(format!("<{} bytes of binary data>", blob.len()));
                            }
                        }
                    }
                    Ok(output.join("\n"))
                }
                Ok(Err(e)) => Err(Box::new(ResourceError::ReadFailed {
                    server: server_name.to_string(),
                    reason: e.to_string(),
                })),
                Err(_) => Err(Box::new(ResourceError::ReadFailed {
                    server: server_name.to_string(),
                    reason: format!("资源读取超时 ({}s)", RESOURCE_READ_TIMEOUT.as_secs()),
                })),
            }
        }
    }
    ```
  - 原因: 参数提取使用 `ok_or_else` 模式，与 `McpToolBridge` 一致；超时 120s 与 Bash 工具对齐（spec-design.md）；`ResourceContents` 区分 `Text` 和 `Blob` 两种类型，文本资源直接返回内容，二进制资源返回元信息（大小和 MIME 类型）；rmcp 0.14 的 `ResourceContents` 枚举变体需根据实际 API 调整
  - **注意**: rmcp 0.14 的 `read_resource()` 返回类型、`ReadResourceRequestParam` 结构体字段名、`ResourceContents` 变体名需根据实际 crate API 确认。核心逻辑不变：参数校验 → 获取 client → 检查状态 → 调用 peer → 格式化返回

- [ ] 修改 `mcp/mod.rs` 添加 `pub mod resource_tool` 声明和重导出
  - 位置: `peri-middlewares/src/mcp/mod.rs`，在 `pub mod client;` 行之后追加
  - 追加内容:
    ```rust
    pub mod resource_tool;

    pub use resource_tool::{McpResourceTool, ResourceError};
    ```
  - 原因: 与 Task 1 建立的模块注册模式一致（声明 + pub use 重导出）；Task 6（McpMiddleware）通过 `use super::resource_tool::McpResourceTool` 引用

- [ ] 为 McpResourceTool 资源读取工具编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/resource_tool.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_name_returns_mcp_read_resource`: 创建 `McpResourceTool`（传入空 pool）→ 调用 `name()` → 返回 `"mcp_read_resource"`
    - `test_parameters_schema`: 调用 `parameters()` → 返回的 JSON Value 包含 `properties.server_name` 和 `properties.uri`，`required` 数组包含两者
    - `test_description_empty_pool`: 传入无资源的空 pool → `description()` 返回包含 `"No resources currently available"` 的字符串
    - `test_description_with_resources`: 手动构造 pool，插入有资源的 Connected handle → `description()` 返回包含 server 名称和资源 URI 的字符串
    - `test_invoke_missing_server_name`: 调用 `invoke(json!({"uri": "file:///test"}))` → 返回 `Err`，错误信息包含 `"server_name"`
    - `test_invoke_missing_uri`: 调用 `invoke(json!({"server_name": "test"}))` → 返回 `Err`，错误信息包含 `"uri"`
    - `test_invoke_server_not_found`: 调用 `invoke(json!({"server_name": "nonexistent", "uri": "test://x"}))` → 返回 `Err`，错误信息包含 `"未找到"`
    - `test_invoke_server_not_connected`: 手动构造 pool 插入 Failed 状态的 handle → 调用 `invoke()` → 返回 `Err`，错误信息包含 `"未连接"`
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::resource_tool::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 resource_tool.rs 编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证 resource_tool.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- mcp::resource_tool::tests 2>&1 | tail -15`
  - 预期: 所有 `test_*` 测试通过，输出 `test result: ok`

- [ ] 验证 mod.rs 正确声明 resource_tool 子模块
  - `grep -n "pub mod resource_tool" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod resource_tool;`

- [ ] 验证 mod.rs 正确重导出 resource_tool 类型
  - `grep -n "pub use resource_tool" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub use resource_tool::{McpResourceTool, ResourceError};`

- [ ] 验证 McpResourceTool 实现了 BaseTool trait
  - `grep -n "impl BaseTool for McpResourceTool" peri-middlewares/src/mcp/resource_tool.rs`
  - 预期: 输出包含 `impl BaseTool for McpResourceTool`

- [ ] 验证 invoke 方法包含 120s 超时
  - `grep -n "RESOURCE_READ_TIMEOUT\|from_secs(120)" peri-middlewares/src/mcp/resource_tool.rs`
  - 预期: 输出包含 `120` 秒超时常量定义或使用

---

### Task 6: McpMiddleware 中间件实现

**背景:**
[业务语境] — McpMiddleware 是 MCP 子系统对外的唯一集成入口，实现 `Middleware<S>` trait，在 `collect_tools()` 时将所有 MCP 服务器的工具（McpToolBridge）和资源读取工具（McpResourceTool）注入 ReAct 循环的工具注册表，使 LLM 能自动发现和调用 MCP 工具。
[修改原因] — 当前代码中不存在 MCP 中间件，需要新建 `middleware.rs` 将 Task 4（McpToolBridge）和 Task 5（McpResourceTool）组装为标准中间件接口，与 FilesystemMiddleware / TerminalMiddleware 等现有中间件同级注册到中间件链。
[上下游影响] — 本 Task 依赖 Task 3（McpClientPool）、Task 4（build_tool_bridges）、Task 5（McpResourceTool），输出 `McpMiddleware`，被 Task 8（TUI 集成）在 `run_universal_agent()` 中注册到中间件链。

**涉及文件:**
- 新建: `peri-middlewares/src/mcp/middleware.rs`
- 修改: `peri-middlewares/src/mcp/mod.rs`（添加 `pub mod middleware` + 重导出 McpMiddleware）
- 修改: `peri-middlewares/src/lib.rs`（添加 `pub use mcp::McpMiddleware`）

**执行步骤:**

- [ ] 新建 `middleware.rs` 并定义 `McpMiddleware` 结构体
  - 位置: 新建 `peri-middlewares/src/mcp/middleware.rs`，文件顶部
  - 关键逻辑:
    ```rust
    use std::sync::Arc;
    use async_trait::async_trait;
    use peri_agent::agent::state::State;
    use peri_agent::middleware::r#trait::Middleware;
    use peri_agent::tools::BaseTool;

    use super::client::McpClientPool;
    use super::tool_bridge::build_tool_bridges;
    use super::resource_tool::McpResourceTool;

    /// MCP 中间件 —— 将所有已连接 MCP 服务器的工具和资源注入 ReAct 循环
    ///
    /// 通过 `collect_tools()` 返回所有 McpToolBridge + 可选的 McpResourceTool。
    /// `before_agent()` / `after_agent()` 为空操作（连接由 McpClientPool 统一管理）。
    pub struct McpMiddleware {
        pool: Arc<McpClientPool>,
    }
    ```
  - 原因: `Arc<McpClientPool>` 共享连接池，与 spec-design.md §McpMiddleware 设计一致；中间件本身无状态，所有运行时数据来自 pool

- [ ] 实现 `McpMiddleware::new()` 构造函数
  - 位置: `middleware.rs`，`McpMiddleware` 定义之后
  - 关键逻辑:
    ```rust
    impl McpMiddleware {
        pub fn new(pool: Arc<McpClientPool>) -> Self {
            Self { pool }
        }
    }
    ```
  - 原因: 接受 `Arc<McpClientPool>` 引用，与 App 层共享同一个 pool 实例（spec-design.md §TUI 集成点）

- [ ] 实现 `Middleware<S>` trait —— `name()` + `collect_tools()`
  - 位置: `middleware.rs`，`McpMiddleware` impl 块之后
  - 关键逻辑:
    ```rust
    #[async_trait]
    impl<S: State> Middleware<S> for McpMiddleware {
        fn name(&self) -> &str {
            "McpMiddleware"
        }

        fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
            let mut tools = build_tool_bridges(&self.pool);

            // 如果任何已连接的 server 提供资源，注入 McpResourceTool
            if self.pool.has_resources() {
                tools.push(Box::new(McpResourceTool::new(Arc::clone(&self.pool))));
            }

            tools
        }

        // before_agent / after_agent 使用 trait 默认空实现，无需覆盖
        // 连接已在 McpClientPool::initialize() 时建立，无需重复初始化
    }
    ```
  - 原因: `collect_tools()` 是本中间件的核心方法——调用 `build_tool_bridges()`（Task 4）获取所有 MCP 工具桥接，再检查 `pool.has_resources()`（Task 3）决定是否追加 `McpResourceTool`（Task 5）；`_cwd` 参数未使用因为 MCP 工具名和描述在连接发现时已缓存，不依赖工作目录；`before_agent` / `after_agent` 为空操作与 spec-design.md §中间件生命周期一致（连接已建立，无需重复初始化/关闭）

- [ ] 修改 `mcp/mod.rs` 添加 `pub mod middleware` 声明和重导出
  - 位置: `peri-middlewares/src/mcp/mod.rs`，在最后一个 `pub mod` 声明之后追加
  - 追加内容:
    ```rust
    pub mod middleware;

    pub use middleware::McpMiddleware;
    ```
  - 原因: 与 Task 1-5 建立的模块注册模式一致（声明 + pub use 重导出）；`McpMiddleware` 是 Task 8（TUI 集成）直接使用的核心类型

- [ ] 修改 `lib.rs` 添加 McpMiddleware 重导出
  - 位置: `peri-middlewares/src/lib.rs`，在现有的 `pub use mcp::{...}` 行中追加 `McpMiddleware`
  - 修改方式: 找到 `pub use mcp::{` 开头的行，在闭合 `};` 前追加 `McpMiddleware,`
  - 原因: 与现有模块重导出模式一致，使外部 crate 可通过 `peri_middlewares::McpMiddleware` 直接引用

- [ ] 为 McpMiddleware 中间件编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/middleware.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_name_returns_mcp_middleware`: 创建 `McpMiddleware::new(pool)` → `middleware.name()` 返回 `"McpMiddleware"`
    - `test_collect_tools_empty_pool`: 构造空 `McpClientPool`（无已连接客户端）→ `collect_tools("/tmp")` 返回空 `Vec`
    - `test_collect_tools_with_bridges`: 构造 pool 含 1 个 Connected handle（2 个 tools）→ `collect_tools("")` 返回 2 个工具，名称均为 `mcp__{server}__{tool}` 格式
    - `test_collect_tools_includes_resource_tool_when_resources_exist`: 构造 pool 含 1 个 Connected handle 且 `resources` 非空 → `collect_tools("")` 返回工具列表中包含名为 `"mcp_read_resource"` 的工具
    - `test_collect_tools_excludes_resource_tool_when_no_resources`: 构造 pool 含 1 个 Connected handle 但 `resources` 为空 → `collect_tools("")` 返回的工具列表中不包含 `"mcp_read_resource"`
    - `test_collect_tools_filters_disconnected_clients`: 构造 pool 含 1 个 Connected + 1 个 Failed handle → `collect_tools("")` 仅返回 Connected handle 的工具，Failed handle 的工具不出现在结果中
    - `test_collect_tools_multiple_servers`: 构造 pool 含 2 个 Connected handle（server "fs" 有 1 个 tool，server "gh" 有 2 个 tools）→ `collect_tools("")` 返回 3 个工具，名称前缀分别为 `mcp__fs__` 和 `mcp__gh__`
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::middleware::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 middleware.rs 编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证 middleware.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- mcp::middleware::tests 2>&1 | tail -15`
  - 预期: 所有 `test_*` 测试通过，输出 `test result: ok`

- [ ] 验证 mod.rs 正确声明 middleware 子模块
  - `grep -n "pub mod middleware" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod middleware;`

- [ ] 验证 mod.rs 正确重导出 McpMiddleware
  - `grep -n "pub use middleware" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub use middleware::McpMiddleware;`

- [ ] 验证 lib.rs 正确重导出 McpMiddleware
  - `grep -n "McpMiddleware" peri-middlewares/src/lib.rs`
  - 预期: 输出包含 `McpMiddleware`

- [ ] 验证 McpMiddleware 实现了 Middleware trait
  - `grep -n "impl.*Middleware.*for McpMiddleware" peri-middlewares/src/mcp/middleware.rs`
  - 预期: 输出包含 `impl<S: State> Middleware<S> for McpMiddleware`

- [ ] 验证 collect_tools 调用了 build_tool_bridges
  - `grep -n "build_tool_bridges" peri-middlewares/src/mcp/middleware.rs`
  - 预期: 输出包含 `build_tool_bridges`，确认 collect_tools 使用了 Task 4 的工厂函数

- [ ] 验证 collect_tools 包含 McpResourceTool 条件判断
  - `grep -n "has_resources\|McpResourceTool" peri-middlewares/src/mcp/middleware.rs`
  - 预期: 输出包含 `has_resources` 和 `McpResourceTool`，确认资源工具按条件注入

- [ ] 验证 before_agent / after_agent 未被覆盖（使用默认空实现）
  - `grep -n "before_agent\|after_agent" peri-middlewares/src/mcp/middleware.rs`
  - 预期: 无匹配输出，确认未覆盖这两个方法，使用 trait 默认空实现

---

### Task 7: HITL 审批扩展

**背景:**
[业务语境] — MCP 工具连接外部服务器执行任意操作（如创建 GitHub issue、执行数据库查询），属于不可控的敏感操作，在非 YOLO 模式下必须经过用户审批才能执行，与内置敏感工具（Bash、Write、Edit 等）行为一致。
[修改原因] — 当前 `default_requires_approval()` 函数（`peri-middlewares/src/hitl/mod.rs:40-48`）仅匹配内置工具名，不含任何 MCP 工具名匹配规则。MCP 工具以 `mcp__` 为前缀命名（格式 `mcp__{server}__{tool}`），需增加前缀通配匹配使其走标准 HITL 审批流程。`mcp_read_resource` 是只读操作（类比内置 `Read` 工具不拦截），不以 `mcp__` 开头（双下划线），不在拦截范围内。
[上下游影响] — 本 Task 不依赖其他 MCP Task，可独立执行。其输出影响 Task 8（TUI 集成）——MCP 工具在非 YOLO 模式下的审批行为由本 Task 控制。

**涉及文件:**
- 修改: `peri-middlewares/src/hitl/mod.rs`

**执行步骤:**

- [ ] 在 `default_requires_approval()` 函数中添加 `mcp__` 前缀匹配规则
  - 位置: `peri-middlewares/src/hitl/mod.rs:40-48`，`default_requires_approval()` 函数体
  - 关键逻辑: 在现有 `|| tool_name.starts_with("rm_")` 条件之后追加一个分支：
    ```rust
    || tool_name.starts_with("mcp__")
    ```
  - 修改后完整函数体：
    ```rust
    pub fn default_requires_approval(tool_name: &str) -> bool {
        tool_name == "Bash"
            || tool_name == "folder_operations"
            || tool_name == "Agent"
            || tool_name == "Write"
            || tool_name == "Edit"
            || tool_name.starts_with("delete_")
            || tool_name.starts_with("rm_")
            || tool_name.starts_with("mcp__")
    }
    ```
  - 原因: MCP 工具命名格式为 `mcp__{server}__{tool}`（双下划线分隔），`starts_with("mcp__")` 精确匹配所有 MCP 工具；`mcp_read_resource`（单下划线，只读操作）不匹配，不拦截，与内置 `Read` 工具策略一致

- [ ] 在 `test_default_requires_approval` 测试中追加 MCP 工具名断言
  - 位置: `peri-middlewares/src/hitl/mod.rs:454-469`，`test_default_requires_approval()` 函数体
  - 关键逻辑: 在现有 `assert!(default_requires_approval("Agent"));` 之后追加 MCP 工具的正向断言：
    ```rust
    // MCP 工具需审批
    assert!(default_requires_approval("mcp__filesystem__read_file"));
    assert!(default_requires_approval("mcp__filesystem__write_file"));
    assert!(default_requires_approval("mcp__github__create_issue"));
    assert!(default_requires_approval("mcp__database__query"));
    assert!(default_requires_approval("mcp__web__fetch"));
    ```
  - 在现有 `assert!(!default_requires_approval("ask_user"));` 之后追加 MCP 相关的反向断言：
    ```rust
    // mcp_read_resource 不以 mcp__（双下划线）开头，不拦截
    assert!(!default_requires_approval("mcp_read_resource"));
    ```
  - 原因: 正向断言覆盖多种 MCP 工具名模式（不同 server、不同工具），确认 `mcp__` 前缀通配有效；反向断言确认 `mcp_read_resource`（只读资源读取）不被拦截

- [ ] 为 default_requires_approval 的 MCP 匹配逻辑编写单元测试
  - 测试文件: `peri-middlewares/src/hitl/mod.rs`（文件底部 `#[cfg(test)] mod tests` 块，追加新测试函数）
  - 测试场景:
    - `test_mcp_tools_require_approval`: 传入多个 `mcp__` 前缀工具名（`mcp__a__b`、`mcp__server__tool_name`、`mcp__x__y__z`）→ 全部返回 `true`
    - `test_mcp_prefix_edge_cases`: 传入 `mcp_`（单下划线，无内容）、`mcp_`（单下划线 + 内容如 `mcp_read_resource`）、`mcp`（无下划线）→ 全部返回 `false`
    - `test_is_edit_tool_excludes_mcp`: 调用 `is_edit_tool("mcp__filesystem__write_file")` → 返回 `false`，确认 MCP 工具在 AcceptEdits 模式下仍需审批（不被视为编辑工具）
  - 运行命令: `cargo test -p peri-middlewares --lib -- hitl::tests::test_mcp`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 default_requires_approval 包含 mcp__ 前缀匹配
  - `grep -n 'starts_with("mcp__")' peri-middlewares/src/hitl/mod.rs`
  - 预期: 输出包含 `starts_with("mcp__")`，确认匹配规则已添加

- [ ] 验证 is_edit_tool 未被修改（MCP 工具不属于编辑工具）
  - `grep -n 'mcp' peri-middlewares/src/hitl/mod.rs`
  - 预期: 仅在 `default_requires_approval` 函数和测试中出现 `mcp`，`is_edit_tool` 函数体中不包含 `mcp`

- [ ] 验证 hitl 模块编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证 hitl 模块全部测试通过（含新增 MCP 断言）
  - `cargo test -p peri-middlewares --lib -- hitl::tests 2>&1 | tail -20`
  - 预期: 所有 `test_*` 测试通过，输出 `test result: ok`

---

### Task 8: TUI 集成

**背景:**
[业务语境] — McpMiddleware 需要接入 TUI App 生命周期才能生效：连接池在 App 层持久化并跨多次对话复用，App 退出时统一清理连接和子进程。用户无需额外操作，MCP 工具自动出现在 LLM 可用工具列表中。
[修改原因] — 当前 App 结构体不含 MCP 连接池字段，`AgentRunConfig` 不传递 MCP pool，`run_universal_agent()` 不注册 McpMiddleware，App 退出时不调用 `pool.shutdown()`。需要在三个文件中完成桥接。
[上下游影响] — 本 Task 依赖 Task 3（McpClientPool 的 `initialize()` 和 `shutdown()` 方法）、Task 6（McpMiddleware），是整个 MCP 子系统的最终接入点。

**涉及文件:**
- 修改: `peri-tui/src/app/mod.rs`（App 结构体新增 `mcp_pool` 字段 + 退出清理）
- 修改: `peri-tui/src/app/agent.rs`（AgentRunConfig 新增 `mcp_pool` 字段 + 中间件注册 + parent_tools 扩展）
- 修改: `peri-tui/src/app/agent_ops.rs`（惰性初始化 pool 并传入 AgentRunConfig）
- 修改: `peri-tui/src/app/panel_ops.rs`（`new_headless()` 补充 `mcp_pool: None` 字段）
- 修改: `peri-tui/src/main.rs`（App 退出时调用 `mcp_pool.shutdown()`）

**执行步骤:**

- [ ] 在 App 结构体中新增 `mcp_pool` 字段
  - 位置: `peri-tui/src/app/mod.rs:73-93`，`App` 结构体定义中，在 `config_path_override` 字段之后追加
  - 关键逻辑:
    ```rust
    /// MCP 连接池：首次 agent 启动时惰性初始化，App 退出时 shutdown
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    ```
  - 原因: `Option<Arc<McpClientPool>>` 表示 pool 可能为 None（未配置或初始化失败），通过 Arc 共享给每次 `run_universal_agent()` 调用

- [ ] 在 `App::new()` 构造函数中将 `mcp_pool` 初始化为 None
  - 位置: `peri-tui/src/app/mod.rs:154-183`，`Self { ... }` 构造块中，在 `config_path_override: None,` 行之后追加
  - 追加内容:
    ```rust
    mcp_pool: None,
    ```
  - 原因: App 创建时不立即初始化 MCP 连接（避免阻塞启动），首次执行 agent 时惰性初始化

- [ ] 在 `AgentRunConfig` 结构体中新增 `mcp_pool` 字段
  - 位置: `peri-tui/src/app/agent.rs:18-34`，`AgentRunConfig` 定义中，在 `permission_mode` 字段之后追加
  - 关键逻辑:
    ```rust
    pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    ```
  - 原因: `AgentRunConfig` 是 `run_universal_agent()` 的参数集合，MCP pool 通过此结构体从 App 层传入

- [ ] 在 `run_universal_agent()` 函数解构中提取 `mcp_pool`
  - 位置: `peri-tui/src/app/agent.rs:36-52`，`let AgentRunConfig { ... } = cfg;` 解构块中，在 `permission_mode,` 行之后追加
  - 追加内容:
    ```rust
    mcp_pool,
    ```
  - 原因: 解构后将 `mcp_pool` 用于后续中间件注册

- [ ] 在 `run_universal_agent()` 中构建 parent_tools 时包含 MCP 工具
  - 位置: `peri-tui/src/app/agent.rs:164-166`，`parent_tools` 构建处，在 `parent_tools.extend(TerminalMiddleware::build_tools(&cwd));` 之后追加
  - 关键逻辑:
    ```rust
    // 将 MCP 工具加入 parent_tools，供 SubAgent 继承
    if let Some(ref pool) = mcp_pool {
        let mcp_tools = peri_middlewares::mcp::McpMiddleware::new(Arc::clone(pool))
            .collect_tools(&cwd);
        for tool in mcp_tools {
            parent_tools.push(tool);
        }
    }
    ```
  - 原因: `parent_tools` 传入 `SubAgentMiddleware`，子 agent 继承这些工具。MCP 工具通过 `McpMiddleware::collect_tools()` 获取（返回所有 `McpToolBridge` + 可选 `McpResourceTool`），添加到 `parent_tools` 使子 agent 可调用 MCP 工具

- [ ] 在 `run_universal_agent()` 中注册 McpMiddleware 到中间件链
  - 位置: `peri-tui/src/app/agent.rs:232-233`，在 `.add_middleware(Box::new(subagent))` 之后、`.with_event_handler(Arc::clone(&handler))` 之前追加
  - 关键逻辑:
    ```rust
    // MCP 中间件：仅在 pool 初始化成功时注册
    if let Some(pool) = mcp_pool {
        executor = executor.add_middleware(Box::new(
            peri_middlewares::mcp::McpMiddleware::new(pool)
        ));
    }
    ```
  - 原因: McpMiddleware 注册在 SubAgentMiddleware 之后（spec-design.md §中间件注册位置第 10 位），仅在 pool 不为 None 时注册（pool 初始化成功才注入）；`executor` 需要重新赋值因为 `add_middleware` 返回新实例

- [ ] 在 `submit_message()` 中惰性初始化 MCP pool 并传入 AgentRunConfig
  - 位置: `peri-tui/src/app/agent_ops.rs:141-162`，`tokio::spawn` 闭包中构建 `AgentRunConfig` 的位置
  - 步骤 1: 在 `let permission_mode = self.permission_mode.clone();` 之后、`tokio::spawn` 之前，添加 pool 初始化逻辑:
    ```rust
    // 惰性初始化 MCP 连接池（仅在首次 agent 启动时创建，后续复用）
    if self.mcp_pool.is_none() {
        let pool_cwd = self.cwd.clone();
        let pool = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(
                peri_middlewares::mcp::McpClientPool::initialize(&pool_cwd)
            )
        });
        match pool {
            Ok(p) => {
                tracing::info!("MCP 连接池初始化成功");
                self.mcp_pool = Some(Arc::new(p));
            }
            Err(e) => {
                tracing::warn!(error = %e, "MCP 连接池初始化失败，MCP 工具不可用");
            }
        }
    }
    let mcp_pool = self.mcp_pool.clone();
    ```
  - 步骤 2: 在 `AgentRunConfig { ... }` 构造块中，在 `permission_mode,` 行之后追加:
    ```rust
    mcp_pool,
    ```
  - 原因: 惰性初始化——首次 `submit_message()` 时调用 `McpClientPool::initialize(cwd)` 读取配置并建立所有连接，结果缓存到 `self.mcp_pool`；后续调用直接复用已有 pool。使用 `block_in_place` + `block_on` 因为 `submit_message()` 是同步方法，需要阻塞等待异步初始化完成。初始化失败时记录 warn 日志但不中断 agent 启动（MCP 工具不可用，内置工具正常工作）

- [ ] 在 `new_headless()` 中补充 `mcp_pool: None` 字段
  - 位置: `peri-tui/src/app/panel_ops.rs:300-321`，`App { ... }` 构造块中，在 `config_path_override: Some(test_config_path),` 行之后追加
  - 追加内容:
    ```rust
    mcp_pool: None,
    ```
  - 原因: headless 测试不启动真实 MCP 连接，`mcp_pool` 始终为 None，避免测试中产生网络/子进程副作用

- [ ] 在 App 退出时调用 `mcp_pool.shutdown()` 清理所有 MCP 连接
  - 位置: `peri-tui/src/main.rs:226-233`，`'event_loop` 退出后、Langfuse flush 之前
  - 关键逻辑:
    ```rust
    // 关闭 MCP 连接池（断开所有 MCP 服务器连接，清理子进程）
    if let Some(pool) = app.mcp_pool.take() {
        tracing::info!("正在关闭 MCP 连接池...");
        let pool_ref = Arc::try_unwrap(pool).unwrap_or_else(|arc| {
            // 仍有其他 Arc 引用（不应发生，agent 任务已结束），克隆内部状态
            tracing::warn!("MCP pool 仍有多个 Arc 引用，使用 clone 清理");
            // 通过 McpClientPool::shutdown(&self) 静态方法或直接调用
            // 这里 take 后 pool 独占引用，正常情况下 try_unwrap 成功
            unreachable!("event_loop 退出后不应有其他 Arc 引用")
        });
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(pool_ref.shutdown())
        });
        tracing::info!("MCP 连接池已关闭");
    }
    ```
  - 原因: App 退出时需优雅关闭所有 MCP 连接（stdio transport 的子进程需被 kill、HTTP 连接需关闭）。`app.mcp_pool.take()` 取出所有权后通过 `block_in_place` 执行异步 shutdown（与 Langfuse flush 模式一致）。正常情况下 event_loop 退出后 agent 任务已完成，pool 的 Arc 引用计数为 1，`try_unwrap` 成功

- [ ] 为 TUI MCP 集成编写单元测试
  - 测试文件: `peri-tui/src/app/agent_ops.rs`（文件底部 `#[cfg(test)] mod tests` 块追加新测试函数）
  - 测试场景:
    - `test_submit_message_passes_mcp_pool_none`: 构造 headless App（mcp_pool 为 None）→ 调用 `submit_message("test")`（会被拦截，因为无 provider）→ 验证 `app.mcp_pool` 仍为 None（无配置文件时初始化失败但不应 panic）
    - `test_agent_run_config_includes_mcp_pool_field`: 验证 `AgentRunConfig` 结构体包含 `mcp_pool` 字段——通过编译检查确认（字段存在且类型为 `Option<Arc<McpClientPool>>`）
    - `test_new_headless_has_mcp_pool_none`: 调用 `App::new_headless(80, 24)` → 返回的 `app.mcp_pool` 为 `None`
  - 运行命令: `cargo test -p peri-tui --lib -- app::agent_ops::tests::test_mcp 2>&1 | tail -15`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 App 结构体包含 mcp_pool 字段
  - `grep -n "mcp_pool" peri-tui/src/app/mod.rs`
  - 预期: 输出包含 `pub mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,`

- [ ] 验证 App::new() 初始化 mcp_pool 为 None
  - `grep -n "mcp_pool: None" peri-tui/src/app/mod.rs`
  - 预期: 输出包含 `mcp_pool: None,`

- [ ] 验证 AgentRunConfig 包含 mcp_pool 字段
  - `grep -n "mcp_pool" peri-tui/src/app/agent.rs`
  - 预期: 输出至少 3 行：结构体字段定义、解构提取、条件注册

- [ ] 验证 McpMiddleware 在 SubAgentMiddleware 之后注册
  - `grep -n -A2 "add_middleware.*subagent" peri-tui/src/app/agent.rs`
  - 预期: subagent 注册之后紧跟 McpMiddleware 的条件注册代码

- [ ] 验证 parent_tools 包含 MCP 工具扩展逻辑
  - `grep -n "collect_tools" peri-tui/src/app/agent.rs`
  - 预期: 输出包含 parent_tools 构建中的 MCP collect_tools 调用

- [ ] 验证 submit_message 包含 pool 惰性初始化逻辑
  - `grep -n "McpClientPool::initialize\|mcp_pool" peri-tui/src/app/agent_ops.rs`
  - 预期: 输出包含 `McpClientPool::initialize` 调用和 `mcp_pool` 传递

- [ ] 验证 new_headless 补充了 mcp_pool 字段
  - `grep -n "mcp_pool" peri-tui/src/app/panel_ops.rs`
  - 预期: 输出包含 `mcp_pool: None,`

- [ ] 验证 main.rs 包含 MCP pool shutdown 逻辑
  - `grep -n "mcp_pool\|shutdown" peri-tui/src/main.rs`
  - 预期: 输出包含 `app.mcp_pool.take()` 和 `pool_ref.shutdown()`

- [ ] 验证 TUI crate 编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证新增测试通过
  - `cargo test -p peri-tui --lib -- app::agent_ops::tests::test_mcp 2>&1 | tail -15`
  - 预期: 所有测试通过，输出 `test result: ok`

- [ ] 验证 TUI crate 全部测试无回归
  - `cargo test -p peri-tui --lib 2>&1 | tail -20`
  - 预期: 全部测试通过，输出 `test result: ok`

---

### Acceptance: MCP 功能总体验收

**前置条件:**
- Plan 1 和 Plan 2 所有 Task（Task 1-8）的单元测试已通过
- `cargo build` 成功（所有 crate 编译通过）

**端到端验证:**

1. 运行完整 workspace 测试套件确保无回归
   - `cargo test 2>&1 | tail -20`
   - 预期: 所有测试通过，`test result: ok`（各 crate）
   - 失败排查: 检查对应 crate 的 Task 测试步骤

2. 验证 MCP 模块完整结构
   - `ls peri-middlewares/src/mcp/`
   - 预期: 包含 `mod.rs`, `config.rs`, `transport.rs`, `client.rs`, `tool_bridge.rs`, `resource_tool.rs`, `middleware.rs` 7 个文件
   - 失败排查: 检查 Plan 1 和 Plan 2 各 Task 是否遗漏文件创建

3. 验证 HITL 审批规则包含 MCP 工具
   - `grep "mcp__" peri-middlewares/src/hitl/mod.rs`
   - 预期: `default_requires_approval()` 中包含 `tool_name.starts_with("mcp__")` 判断
   - 失败排查: 检查 Task 7 的修改步骤

4. 验证 TUI App 集成 McpMiddleware
   - `grep -n "McpMiddleware" peri-tui/src/app/agent.rs`
   - 预期: 包含 `.add_middleware(Box::new(McpMiddleware::new(...)))` 注册行
   - 失败排查: 检查 Task 8 的中间件注册步骤

5. 验证 App 结构体包含 mcp_pool 字段
   - `grep "mcp_pool" peri-tui/src/app/mod.rs`
   - 预期: 包含 `mcp_pool: Option<Arc<McpClientPool>>` 字段声明
   - 失败排查: 检查 Task 8 的 App 结构体修改步骤

6. 验证 headless 测试不受 MCP 影响
   - `cargo test -p peri-tui --lib 2>&1 | tail -15`
   - 预期: 所有测试通过，MCP pool 为 None 时不影响 headless 测试
   - 失败排查: 检查 Task 8 的 pool 惰性初始化逻辑

