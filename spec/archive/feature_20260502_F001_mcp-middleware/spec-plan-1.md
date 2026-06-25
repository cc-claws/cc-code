# MCP Middleware 执行计划（一）：核心组件

**目标:** 实现 MCP Client 核心基础设施——配置加载与合并、传输层适配、连接池管理、工具桥接

**技术栈:** Rust 2021, rmcp 0.14 (MCP Rust SDK), tokio async, serde_json, thiserror

**设计文档:** spec/feature_20260502_F001_mcp-middleware/spec-design.md

## 改动总览

本计划实现 MCP Client 核心组件层，新增 `peri-middlewares/src/mcp/` 模块（6 个子文件）+ 修改 `Cargo.toml` 和 `lib.rs`。Task 1 创建数据结构和配置加载，Task 2 和 Task 3 依赖 Task 1 的配置类型构建传输和连接池，Task 4 依赖 Task 3 的 `McpClientHandle` 实现工具桥接。关键决策：rmcp 0.14 统一在 Task 1 引入（避免多 Task 修改 Cargo.toml），`McpClientPool` 使用 `RunningService` 持有连接生命周期。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，特别是 Rust toolchain 版本满足 rmcp 0.14 的最低要求（>= 1.85）。

**执行步骤:**

- [ ] 验证 Rust toolchain 版本
  - `rustc --version`
  - 确认版本 >= 1.85（rmcp 使用 edition 2024）
- [ ] 验证 workspace 当前可构建
  - `cargo build 2>&1 | tail -5`
  - 预期: `Finished` 无错误
- [ ] 验证 workspace 当前测试可通过
  - `cargo test 2>&1 | tail -10`
  - 预期: `test result: ok` 无失败

**检查步骤:**

- [ ] Rust 版本满足 rmcp 要求
  - `rustc --version | grep -oP '\d+\.\d+' | head -1`
  - 预期: >= 1.85
- [ ] 当前 workspace 构建成功
  - `cargo build 2>&1 | grep -E "error|Finished"`
  - 预期: 包含 `Finished`，不含 `error`

---

### Task 1: McpConfig 配置加载与合并

**背景:**
[业务语境] — Peri 需要从全局 `settings.json` 和项目级 `.mcp.json` 加载 MCP 服务器配置，合并去重后供 McpClientPool 建立连接。配置中 `${VAR}` 占位符需展开为实际环境变量值。
[修改原因] — 当前代码中 `AppConfig.extra` 保留未知字段但未提供 MCP 配置解析能力，缺少 `.mcp.json` 项目级配置加载、双层合并、环境变量展开逻辑。
[上下游影响] — 本 Task 输出 `McpServerConfig` / `McpConfigFile` / `load_merged_config()` / `expand_env_vars()`，被 Task 2（传输层构建）和 Task 3（McpClientPool 初始化）直接依赖。

**涉及文件:**

- 新建: `peri-middlewares/src/mcp/mod.rs`
- 新建: `peri-middlewares/src/mcp/config.rs`
- 修改: `peri-middlewares/Cargo.toml`
- 修改: `peri-middlewares/src/lib.rs`

**执行步骤:**

- [ ] 在 `Cargo.toml` 添加 rmcp 依赖声明
  - 位置: `peri-middlewares/Cargo.toml` 末尾 `[dependencies]` 块追加
  - 追加内容:

    ```toml
    rmcp = { version = "0.14", features = [
        "client",
        "transport-child-process",
        "transport-streamable-http-client-reqwest",
    ] }
    ```

  - 原因: Task 1 仅需 serde 反序列化，但 rmcp 依赖在此统一声明，避免后续 Task 重复修改 Cargo.toml

- [ ] 创建 `mcp` 模块入口文件
  - 位置: 新建 `peri-middlewares/src/mcp/mod.rs`
  - 内容:

    ```rust
    pub mod config;

    pub use config::{
        McpConfigError, McpConfigFile, McpServerConfig, load_merged_config,
    };
    ```

  - 原因: 模块骨架，后续 Task 2-6 在此目录追加子模块

- [ ] 在 `lib.rs` 注册 `mcp` 模块并重导出关键类型
  - 位置: `peri-middlewares/src/lib.rs`，在 `pub mod skills;` 行之后追加
  - 追加内容:

    ```rust
    pub mod mcp;
    pub use mcp::{McpConfigError, McpConfigFile, McpServerConfig, load_merged_config};
    ```

  - 原因: 与现有模块注册模式一致（声明 + pub use 重导出）

- [ ] 在 `config.rs` 中定义 `McpServerConfig` 数据结构
  - 位置: 新建 `peri-middlewares/src/mcp/config.rs`，文件顶部
  - 关键逻辑:

    ```rust
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::path::Path;

    /// 单个 MCP 服务器配置
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpServerConfig {
        /// stdio 传输的可执行命令（如 "npx"）
        pub command: Option<String>,
        /// stdio 传输的命令参数
        #[serde(default)]
        pub args: Option<Vec<String>>,
        /// 传递给子进程的环境变量
        #[serde(default)]
        pub env: Option<HashMap<String, String>>,
        /// Streamable HTTP 传输的 URL
        pub url: Option<String>,
        /// HTTP 请求的自定义头
        #[serde(default)]
        pub headers: Option<HashMap<String, String>>,
    }

    /// MCP 配置文件顶层结构（.mcp.json / settings.json 中的 mcpServers 片段）
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct McpConfigFile {
        #[serde(default)]
        pub mcp_servers: HashMap<String, McpServerConfig>,
    }
    ```

  - 原因: 与 spec-design.md §McpConfig 定义一致，支持 stdio（command + args + env）和 HTTP（url + headers）两种传输

- [ ] 在 `config.rs` 中定义 `McpConfigError` 错误类型
  - 位置: `config.rs`，紧跟 `McpConfigFile` 定义之后
  - 关键逻辑:

    ```rust
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum McpConfigError {
        #[error("MCP 配置文件解析失败: {path}: {source}")]
        ParseError {
            path: String,
            #[source]
            source: serde_json::Error,
        },
        #[error("MCP 配置文件读取失败: {path}: {source}")]
        ReadError {
            path: String,
            #[source]
            source: std::io::Error,
        },
    }
    ```

  - 原因: 遵循项目编码规范（库 crate 用 thiserror），包装底层错误并保留文件路径上下文

- [ ] 实现 `load_from_path()` 函数——从单个文件加载 McpConfigFile
  - 位置: `config.rs`，`McpConfigError` 之后
  - 关键逻辑:

    ```rust
    /// 从指定 JSON 文件加载 MCP 配置，文件不存在时返回空配置
    pub fn load_from_path(path: &Path) -> Result<McpConfigFile, McpConfigError> {
        if !path.exists() {
            return Ok(McpConfigFile::default());
        }
        let content = std::fs::read_to_string(path).map_err(|e| McpConfigError::ReadError {
            path: path.display().to_string(),
            source: e,
        })?;
        serde_json::from_str::<McpConfigFile>(&content).map_err(|e| McpConfigError::ParseError {
            path: path.display().to_string(),
            source: e,
        })
    }
    ```

  - 原因: 配置文件不存在时静默返回空（spec-design.md 错误处理策略），不中断 agent 启动

- [ ] 实现 `load_global_config()` 函数——从 settings.json extra 字段提取 mcpServers
  - 位置: `config.rs`，`load_from_path()` 之后
  - 关键逻辑:

    ```rust
    /// 从全局 settings.json 的 extra 字段中提取 mcpServers
    /// settings.json 中 mcpServers 与其他 config 字段同级，被 AppConfig.extra 保留
    pub fn load_global_config(settings_json_path: &Path) -> Result<McpConfigFile, McpConfigError> {
        if !settings_json_path.exists() {
            return Ok(McpConfigFile::default());
        }
        let content = std::fs::read_to_string(settings_json_path).map_err(|e| McpConfigError::ReadError {
            path: settings_json_path.display().to_string(),
            source: e,
        })?;
        let v: serde_json::Value = serde_json::from_str(&content).map_err(|e| McpConfigError::ParseError {
            path: settings_json_path.display().to_string(),
            source: e,
        })?;
        // 从顶层 value 中提取 "config"."mcpServers" 或 "mcpServers"
        let mcp_servers = v
            .get("config")
            .and_then(|c| c.get("mcpServers"))
            .or_else(|| v.get("mcpServers"))
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let config = McpConfigFile { mcp_servers: serde_json::from_value(mcp_servers).unwrap_or_default() };
        Ok(config)
    }
    ```

  - 原因: settings.json 结构为 `{ "config": { ...fields..., "mcpServers": {...} } }` 或直接 `{ "mcpServers": {...} }`，需兼容两种路径

- [ ] 实现 `expand_env_vars()` 函数——展开字符串中的 `${VAR}` 占位符
  - 位置: `config.rs`，`load_global_config()` 之后
  - 关键逻辑:

    ```rust
    /// 展开 s 中所有 ${VAR} 占位符为环境变量值
    /// 变量不存在时替换为空字符串，并输出 warn 日志
    pub fn expand_env_vars(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '$' && chars.peek() == Some(&'{') {
                chars.next(); // 消耗 '{'
                let var_name: String = chars.by_ref().take_while(|&ch| ch != '}').collect();
                if chars.peek() == Some(&'}') {
                    chars.next(); // 消耗 '}'
                }
                match std::env::var(&var_name) {
                    Ok(val) => result.push_str(&val),
                    Err(_) => {
                        tracing::warn!(var_name = %var_name, "MCP 配置环境变量 ${{{}}} 未设置，替换为空字符串", var_name);
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }
    ```

  - 原因: 手动解析 `${VAR}` 语法，避免引入 `shellexpand` 额外依赖；变量不存在时 warn 日志（spec-design.md 错误处理策略）

- [ ] 实现 `expand_server_config()` 函数——对单个 McpServerConfig 所有字符串字段展开环境变量
  - 位置: `config.rs`，`expand_env_vars()` 之后
  - 关键逻辑:

    ```rust
    /// 对 McpServerConfig 中所有字符串字段执行环境变量展开
    /// 注意: 日志中不打印 headers/env 值，防止泄露 API Key 等敏感信息
    pub fn expand_server_config(config: &McpServerConfig) -> McpServerConfig {
        McpServerConfig {
            command: config.command.as_ref().map(|s| expand_env_vars(s)),
            args: config.args.as_ref().map(|arr| {
                arr.iter().map(|s| expand_env_vars(s)).collect()
            }),
            env: config.env.as_ref().map(|map| {
                map.iter().map(|(k, v)| (k.clone(), expand_env_vars(v))).collect()
            }),
            url: config.url.as_ref().map(|s| expand_env_vars(s)),
            headers: config.headers.as_ref().map(|map| {
                map.iter().map(|(k, v)| (k.clone(), expand_env_vars(v))).collect()
            }),
        }
    }
    ```

  - 原因: 配置合并后统一展开，确保 command/args/url/headers/env 中的 `${VAR}` 全部替换

- [ ] 实现 `load_merged_config()` 函数——双层加载 + 合并 + 展开的主入口
  - 位置: `config.rs`，`expand_server_config()` 之后
  - 关键逻辑:

    ```rust
    /// 加载并合并 MCP 配置：全局 settings.json + 项目级 .mcp.json
    /// 同名 server 以项目级覆盖全局，所有字段执行 ${VAR} 展开
    pub fn load_merged_config(cwd: &Path) -> McpConfigFile {
        // 1. 加载全局配置
        let global_path = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".peri")
            .join("settings.json");
        let global = load_global_config(&global_path).unwrap_or_else(|e| {
            tracing::warn!(path = %global_path.display(), error = %e, "加载全局 MCP 配置失败，跳过");
            McpConfigFile::default()
        });

        // 2. 加载项目级配置
        let project_path = cwd.join(".mcp.json");
        let project = load_from_path(&project_path).unwrap_or_else(|e| {
            tracing::warn!(path = %project_path.display(), error = %e, "加载项目级 MCP 配置失败，跳过");
            McpConfigFile::default()
        });

        // 3. 合并：项目级覆盖全局
        let mut merged = global;
        for (name, server_config) in project.mcp_servers {
            merged.mcp_servers.insert(name, server_config);
        }

        // 4. 环境变量展开
        for (name, server_config) in &merged.mcp_servers {
            merged.mcp_servers.insert(name.clone(), expand_server_config(server_config));
        }

        merged
    }
    ```

  - 原因: spec-design.md §McpConfig 要求先全局后项目级、同名覆盖、加载时展开 `${VAR}`；加载失败 warn 日志不中断

- [ ] 为 McpConfig 配置加载与合并编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/config.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_load_from_nonexistent_path`: 传入不存在的路径 → 返回空 `McpConfigFile::default()`
    - `test_load_from_valid_json`: 用 `tempfile::NamedTempFile` 创建包含 `{"mcpServers":{"fs":{"command":"npx"}}}` 的临时 JSON 文件 → 解析成功，`mcp_servers` 含 1 个条目
    - `test_load_from_invalid_json`: 创建包含无效 JSON 的临时文件 → 返回 `McpConfigError::ParseError`
    - `test_load_global_config`: 创建 `{"config":{"mcpServers":{"gh":{"url":"https://api.github.com"}}}}` 的临时文件 → 解析出 `gh` server，`url` 正确
    - `test_load_global_config_top_level`: 创建 `{"mcpServers":{"gh":{"command":"npx"}}}` 的临时文件（无 config 包装） → 解析出 `gh` server
    - `test_expand_env_vars`: 设置 `std::env::set_var("TEST_MCP_VAR", "hello")`，调用 `expand_env_vars("prefix_${TEST_MCP_VAR}_suffix")` → 返回 `"prefix_hello_suffix"`
    - `test_expand_env_vars_missing`: 调用 `expand_env_vars("${NONEXISTENT_MCP_VAR_12345}")` → 返回空字符串
    - `test_expand_env_vars_no_braces`: 调用 `expand_env_vars("$NO_BRACE")` → 返回 `"$NO_BRACE"`（不展开无花括号格式）
    - `test_merge_project_overrides_global`: 构造两个 `McpConfigFile`，全局含 `fs: {command: "npx"}`，项目级含 `fs: {command: "uvx"}` → 合并后 `fs.command` 为 `"uvx"`
    - `test_merge_project_adds_new_server`: 全局含 `fs`，项目级含 `gh` → 合并后两者均存在
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::config::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 mcp 模块编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证 config.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- mcp::config::tests 2>&1 | tail -15`
  - 预期: 所有 `test_*` 测试通过，输出 `test result: ok`

- [ ] 验证 lib.rs 正确重导出 mcp 类型
  - `grep -n "pub use mcp" peri-middlewares/src/lib.rs`
  - 预期: 输出包含 `pub use mcp::{McpConfigError, McpConfigFile, McpServerConfig, load_merged_config};`

- [ ] 验证 McpServerConfig 字段与 spec-design.md 一致
  - `grep -E "pub (command|args|env|url|headers)" peri-middlewares/src/mcp/config.rs`
  - 预期: 5 个字段均存在，类型匹配（command/url 为 `Option<String>`，args 为 `Option<Vec<String>>`，env/headers 为 `Option<HashMap<String, String>>`）

---

### Task 2: 传输层构建工厂

**背景:**
[业务语境] — MCP 协议支持多种传输方式，Peri 需要根据用户配置（`command` 或 `url` 字段）自动构建对应的 rmcp Transport 实例，供 McpClientPool 建立连接。传输层是 MCP 客户端与服务器之间的通信基础设施，屏蔽底层协议差异。
[修改原因] — 当前代码中不存在 MCP 传输层适配能力，需要新建 `transport.rs`，将 `McpServerConfig`（Task 1 产出）转换为 rmcp 可消费的 `Transport` trait 对象，支持 stdio（子进程）和 Streamable HTTP 两种传输协议。
[上下游影响] — 本 Task 依赖 Task 1（`McpServerConfig` 数据结构），输出 `build_transport()` 工厂函数，被 Task 3（McpClientPool 初始化）在遍历配置建立连接时调用。

**涉及文件:**

- 新建: `peri-middlewares/src/mcp/transport.rs`
- 修改: `peri-middlewares/src/mcp/mod.rs`（添加 `pub mod transport`）

**执行步骤:**

- [ ] 在 `transport.rs` 顶部定义 `TransportError` 错误类型和 `TransportConfig` 枚举
  - 位置: 新建 `peri-middlewares/src/mcp/transport.rs`，文件顶部
  - 关键逻辑:

    ```rust
    use std::collections::HashMap;
    use thiserror::Error;

    /// 传输层配置枚举，从 McpServerConfig 派生
    #[derive(Debug, Clone)]
    pub enum TransportConfig {
        Stdio {
            command: String,
            args: Vec<String>,
            env: HashMap<String, String>,
        },
        StreamableHttp {
            url: String,
            headers: HashMap<String, String>,
        },
    }

    /// 传输层构建错误
    #[derive(Debug, Error)]
    pub enum TransportError {
        #[error("MCP 服务器配置无效: 缺少 command 或 url 字段")]
        InvalidConfig,
        #[error("MCP stdio 传输子进程启动失败: {0}")]
        StdioLaunchFailed(String),
        #[error("MCP HTTP 传输配置失败: {0}")]
        HttpConfigFailed(String),
    }
    ```

  - 原因: 遵循项目编码规范（库 crate 用 thiserror）；`TransportConfig` 枚举与 spec-design.md §传输层适配一致；`TransportError` 覆盖配置缺失、子进程启动失败、HTTP 配置失败三类错误

- [ ] 实现 `From<&McpServerConfig>` 转换——将 McpServerConfig 解析为 TransportConfig
  - 位置: `transport.rs`，`TransportError` 定义之后
  - 关键逻辑:

    ```rust
    use super::config::McpServerConfig;

    impl TryFrom<&McpServerConfig> for TransportConfig {
        type Error = TransportError;

        fn try_from(config: &McpServerConfig) -> Result<Self, Self::Error> {
            match (&config.command, &config.url) {
                (Some(command), _) => Ok(TransportConfig::Stdio {
                    command: command.clone(),
                    args: config.args.clone().unwrap_or_default(),
                    env: config.env.clone().unwrap_or_default(),
                }),
                (_, Some(url)) => Ok(TransportConfig::StreamableHttp {
                    url: url.clone(),
                    headers: config.headers.clone().unwrap_or_default(),
                }),
                (None, None) => Err(TransportError::InvalidConfig),
            }
        }
    }
    ```

  - 原因: 配置解析优先级与 spec-design.md 一致——有 `command` 字段时走 stdio，有 `url` 字段时走 Streamable HTTP；两者均无时返回 `InvalidConfig` 错误

- [ ] 实现 `build_transport()` 工厂函数——将 TransportConfig 构建为 rmcp Transport trait 对象
  - 位置: `transport.rs`，`TryFrom` impl 之后
  - 关键逻辑:

    ```rust
    use rmcp::transport::IntoTransport;
    use rmcp::transport::child_process::TokioChildProcess;
    use rmcp::transport::streamable_http_client_reqwest::StreamableHttpClientTransport;

    /// 构建 MCP Transport 实例
    /// stdio: 通过 TokioChildProcess 启动子进程，传入 args 和 env
    /// HTTP: 通过 StreamableHttpClientTransport 建立 HTTP 连接，传入 headers
    pub fn build_transport(
        config: &McpServerConfig,
    ) -> Result<impl IntoTransport<rmcp::RoleClient>, TransportError> {
        let transport_config = TransportConfig::try_from(config)?;
        match transport_config {
            TransportConfig::Stdio { command, args, env } => {
                let mut child = std::process::Command::new(&command);
                child.args(&args);
                child.envs(&env);
                // 禁用子进程继承父进程的 stdin（避免信号传递干扰）
                child.stdin(std::process::Stdio::piped());
                child.stdout(std::process::Stdio::piped());
                child.stderr(std::process::Stdio::piped());
                Ok(TokioChildProcess::new(child)?)
            }
            TransportConfig::StreamableHttp { url, headers } => {
                let mut builder = StreamableHttpClientTransport::from_uri(&url);
                for (key, value) in &headers {
                    builder = builder.header(key.as_str(), value.as_str());
                }
                Ok(builder)
            }
        }
    }
    ```

  - 原因: `TokioChildProcess::new()` 接受 `Command` 对象，支持 args 和 env 注入；`StreamableHttpClientTransport::from_uri()` 返回 builder 模式，支持链式追加 headers；返回 `impl IntoTransport<RoleClient>` 而非具体类型，使调用方（Task 3 的 `serve_client()`）无需关心传输细节
  - **注意**: rmcp 0.14 的 `TokioChildProcess::new()` 签名为 `fn new(cmd: Command) -> Result<TokioChildProcess, io::Error>`；`StreamableHttpClientTransport::from_uri()` 签名需确认是否直接返回实例或 builder。实现时根据 rmcp 0.14 实际 API 调整，核心逻辑不变

- [ ] 修改 `mcp/mod.rs` 添加 `pub mod transport` 声明和重导出
  - 位置: `peri-middlewares/src/mcp/mod.rs`，在 `pub mod config;` 行之后追加
  - 追加内容:

    ```rust
    pub mod transport;

    pub use transport::{TransportConfig, TransportError, build_transport};
    ```

  - 原因: 与 Task 1 建立的模块注册模式一致（声明 + pub use 重导出）；`build_transport` 是 Task 3（McpClientPool）直接依赖的核心函数

- [ ] 为传输层构建工厂编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/transport.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_try_from_stdio_config`: 构造 `McpServerConfig { command: Some("npx".into()), args: Some(vec!["-y".into()]), env: Some(HashMap::from([("KEY".into(), "val".into())])), url: None, headers: None }` → `TransportConfig::try_from()` 返回 `Ok(TransportConfig::Stdio { command: "npx", args: ["-y"], env: {"KEY": "val"} })`
    - `test_try_from_http_config`: 构造 `McpServerConfig { command: None, args: None, env: None, url: Some("https://example.com/mcp".into()), headers: Some(HashMap::from([("Auth".into(), "Bearer token".into())])) }` → 返回 `Ok(TransportConfig::StreamableHttp { url: "https://example.com/mcp", headers: {"Auth": "Bearer token"} })`
    - `test_try_from_empty_config`: 构造 `McpServerConfig { command: None, url: None, args: None, env: None, headers: None }` → 返回 `Err(TransportError::InvalidConfig)`
    - `test_try_from_stdio_priority_over_url`: 构造 `McpServerConfig { command: Some("npx".into()), url: Some("https://example.com".into()), args: None, env: None, headers: None }` → 返回 `Ok(TransportConfig::Stdio { ... })`（command 优先）
    - `test_try_from_defaults`: 构造 `McpServerConfig { command: Some("cat".into()), args: None, env: None, url: None, headers: None }` → 返回 `Ok(TransportConfig::Stdio { args: [], env: {} })`（缺失字段使用默认空值）
    - `test_build_transport_stdio_echo`: 构造 `McpServerConfig { command: Some("echo".into()), args: Some(vec!["hello".into()]), env: None, url: None, headers: None }` → `build_transport()` 返回 `Ok(_)` 不 panic（echo 命令始终可用）
    - `test_build_transport_invalid_command`: 构造 `McpServerConfig { command: Some("nonexistent_binary_xyz_12345".into()), args: None, env: None, url: None, headers: None }` → `build_transport()` 返回 `Err(TransportError::StdioLaunchFailed(_))`
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::transport::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 transport.rs 编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证 transport.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- mcp::transport::tests 2>&1 | tail -15`
  - 预期: 所有 `test_*` 测试通过，输出 `test result: ok`

- [ ] 验证 mod.rs 正确声明 transport 子模块
  - `grep -n "pub mod transport" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod transport;`

- [ ] 验证 mod.rs 正确重导出 transport 类型
  - `grep -n "pub use transport" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub use transport::{TransportConfig, TransportError, build_transport};`

- [ ] 验证 build_transport 函数签名接受 McpServerConfig 引用
  - `grep -n "pub fn build_transport" peri-middlewares/src/mcp/transport.rs`
  - 预期: 输出包含 `pub fn build_transport(config: &McpServerConfig)`

- [ ] 验证 TransportConfig 枚举包含 Stdio 和 StreamableHttp 两个变体
  - `grep -E "Stdio|StreamableHttp" peri-middlewares/src/mcp/transport.rs`
  - 预期: 输出包含两个变体的定义

---

### Task 3: McpClientPool 连接池管理

**背景:**
[业务语境] — McpClientPool 维护所有 MCP 服务器连接的活跃状态，在 agent 启动时一次性初始化，整个生命周期复用，为 Task 4（McpToolBridge）和 Task 5（McpResourceTool）提供 `Arc<McpClientHandle>` 数据源。
[修改原因] — 当前代码中不存在 MCP 客户端连接管理能力，需要新建 `client.rs` 实现 `McpClientHandle`、`ClientStatus`、`McpClientPool` 三大类型，封装 rmcp 的 `RunningService<RoleClient, ()>` 生命周期。
[上下游影响] — 本 Task 依赖 Task 1（`McpServerConfig` / `load_merged_config()`）和 Task 2（`build_transport()` 工厂函数），输出 `McpClientPool` / `McpClientHandle` / `ClientStatus`，被 Task 4（工具桥接）、Task 5（资源读取）、Task 6（中间件）、Task 8（TUI 集成）直接依赖。

**涉及文件:**

- 新建: `peri-middlewares/src/mcp/client.rs`
- 修改: `peri-middlewares/src/mcp/mod.rs`（添加 `pub mod client` + 重导出）

**执行步骤:**

- [ ] 在 `client.rs` 顶部定义 `ClientStatus` 枚举和 `McpPoolError` 错误类型
  - 位置: 新建 `peri-middlewares/src/mcp/client.rs`，文件顶部
  - 关键逻辑:

    ```rust
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::path::Path;
    use thiserror::Error;
    use rmcp::ServiceExt;
    use rmcp::service::RunningService;
    use rmcp::model::{Tool, Resource};
    use tokio_util::sync::CancellationToken;

    /// MCP 客户端连接状态
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ClientStatus {
        Connected,
        Failed(String),        // 失败原因
        Disconnected,
    }

    /// 连接池级别错误
    #[derive(Debug, Error)]
    pub enum McpPoolError {
        #[error("MCP 服务器 \"{server}\" 连接失败: {reason}")]
        ConnectionFailed { server: String, reason: String },
        #[error("MCP 服务器 \"{server}\" 工具发现失败: {reason}")]
        ToolDiscoveryFailed { server: String, reason: String },
        #[error("MCP 服务器 \"{server}\" 未连接 (状态: {status:?})")]
        NotConnected { server: String, status: ClientStatus },
        #[error("MCP 服务器 \"{server}\" 调用超时")]
        CallTimeout { server: String },
    }
    ```

  - 原因: 遵循项目编码规范（库 crate 用 thiserror）；`ClientStatus::Failed` 携带原因字符串便于诊断；错误类型包含 server 上下文

- [ ] 定义 `McpClientHandle` 结构体——封装单个 MCP 服务器连接的所有运行时状态
  - 位置: `client.rs`，紧跟 `McpPoolError` 之后
  - 关键逻辑:

    ```rust
    /// 单个 MCP 服务器的客户端句柄，通过 Arc 在多个 McpToolBridge 之间共享
    pub struct McpClientHandle {
        pub name: String,
        pub peer: rmcp::service::Peer<rmcp::service::RoleClient>,
        pub tools: Vec<Tool>,
        pub resources: Vec<Resource>,
        pub status: ClientStatus,
        /// 取消令牌，用于 shutdown 时通知后台任务退出
        cancel_token: CancellationToken,
    }
    ```

  - 原因: `Peer<RoleClient>` 是 rmcp 提供的线程安全客户端接口（内部通过 mpsc channel 通信），`Arc<McpClientHandle>` 可安全跨 task 共享；`cancel_token` 由 pool 持有，shutdown 时统一触发

- [ ] 定义 `McpClientPool` 结构体和 `RunningService` 持有列表
  - 位置: `client.rs`，紧跟 `McpClientHandle` 之后
  - 关键逻辑:

    ```rust
    /// MCP 客户端连接池，管理所有 MCP 服务器的连接生命周期
    pub struct McpClientPool {
        clients: HashMap<String, Arc<McpClientHandle>>,
        /// 持有所有 RunningService 实例，确保后台任务不被提前 drop
        services: Vec<RunningService<rmcp::service::RoleClient, ()>>,
    }
    ```

  - 原因: `RunningService` 内部持有 `JoinHandle` 和 `DropGuard`，必须在 pool 中持有引用以保证连接存活；`RunningService` 实现了 `Deref<Target = Peer<RoleClient>>`，初始化时可通过 `*service.list_all_tools()` 直接调用 peer 方法

- [ ] 实现 `McpClientPool::initialize()` —— 连接初始化主入口
  - 位置: `client.rs`，`McpClientPool` impl 块
  - 关键逻辑:

    ```rust
    const STDIO_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
    const HTTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

    impl McpClientPool {
        /// 一次性初始化所有 MCP 服务器连接
        /// 遍历合并后的配置，为每个 server 创建 transport → serve_client → 发现工具和资源
        /// 单个 server 失败不中断整体初始化，warn 日志后跳过
        pub async fn initialize(cwd: &Path) -> Self {
            let config = super::load_merged_config(cwd);
            let mut pool = Self {
                clients: HashMap::new(),
                services: Vec::new(),
            };

            for (name, server_config) in &config.mcp_servers {
                let transport = match super::transport::build_transport(server_config) {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(server = %name, error = %e, "MCP 服务器传输层构建失败，跳过");
                        pool.clients.insert(name.clone(), Arc::new(McpClientHandle {
                            name: name.clone(),
                            peer: // 占位 peer，不会实际使用（见下方说明）
                            tools: vec![],
                            resources: vec![],
                            status: ClientStatus::Failed(format!("传输层构建失败: {e}")),
                            cancel_token: CancellationToken::new(),
                        }));
                        continue;
                    }
                };

                let timeout = if server_config.url.is_some() {
                    HTTP_CONNECT_TIMEOUT
                } else {
                    STDIO_CONNECT_TIMEOUT
                };

                match tokio::time::timeout(timeout, rmcp::service::serve_client((), transport)).await {
                    Ok(Ok(running_service)) => {
                        // 发现工具
                        let tools = match running_service.list_all_tools().await {
                            Ok(t) => t,
                            Err(e) => {
                                tracing::warn!(server = %name, error = %e, "MCP 服务器工具发现失败");
                                vec![]
                            }
                        };
                        // 发现资源
                        let resources = match running_service.list_all_resources().await {
                            Ok(r) => r,
                            Err(e) => {
                                tracing::warn!(server = %name, error = %e, "MCP 服务器资源发现失败");
                                vec![]
                            }
                        };

                        tracing::info!(
                            server = %name,
                            tools_count = tools.len(),
                            resources_count = resources.len(),
                            "MCP 服务器连接成功"
                        );

                        let peer = running_service.peer().clone();
                        let cancel_token = running_service.cancellation_token();
                        let handle = Arc::new(McpClientHandle {
                            name: name.clone(),
                            peer,
                            tools,
                            resources,
                            status: ClientStatus::Connected,
                            cancel_token: CancellationToken::new(), // pool 级别的 token
                        });
                        pool.clients.insert(name.clone(), handle);
                        pool.services.push(running_service);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(server = %name, error = %e, "MCP 服务器连接失败，跳过");
                        // ... 插入 Failed 状态的 handle（同上模式）
                    }
                    Err(_) => {
                        tracing::warn!(server = %name, timeout_secs = timeout.as_secs(), "MCP 服务器连接超时，跳过");
                        // ... 插入 Failed 状态的 handle
                    }
                }
            }

            pool
        }
    ```

  - 原因: spec-design.md 要求一次性初始化、连接失败跳过不影响其他 server；超时按传输类型区分（stdio 10s / HTTP 30s）；使用 `()` 作为 `ClientHandler`（rmcp 提供的空实现）；`list_all_tools()` / `list_all_resources()` 自动处理分页
  - **注意**: `RunningService` 通过 `Deref` 暴露 `Peer` 方法，初始化阶段可同时获取 peer 引用和调用发现方法；获取 peer 引用后 `RunningService` 必须移入 `pool.services` 保持存活

- [ ] 实现 `McpClientPool` 查询方法——`get_client()`、`get_all_clients()`、`has_resources()`
  - 位置: `client.rs`，`McpClientPool` impl 块，`initialize()` 之后
  - 关键逻辑:

    ```rust
    impl McpClientPool {
        /// 获取指定名称的客户端句柄
        pub fn get_client(&self, name: &str) -> Option<&Arc<McpClientHandle>> {
            self.clients.get(name)
        }

        /// 获取所有已连接的客户端句柄
        pub fn get_all_clients(&self) -> Vec<&Arc<McpClientHandle>> {
            self.clients.values()
                .filter(|c| matches!(c.status, ClientStatus::Connected))
                .collect()
        }

        /// 判断是否有任何已连接的 server 提供资源
        pub fn has_resources(&self) -> bool {
            self.clients.values().any(|c| {
                matches!(c.status, ClientStatus::Connected) && !c.resources.is_empty()
            })
        }

        /// 获取所有已连接 server 的资源摘要，用于 McpResourceTool 动态 description
        pub fn resource_summary(&self) -> String {
            let mut lines = Vec::new();
            for client in self.clients.values() {
                if matches!(client.status, ClientStatus::Connected) && !client.resources.is_empty() {
                    lines.push(format!(
                        "- server \"{}\": {} ({} resources)",
                        client.name,
                        client.resources.iter().map(|r| r.uri.clone())
                            .collect::<Vec<_>>()
                            .join(", "),
                        client.resources.len()
                    ));
                }
            }
            lines.join("\n")
        }
    }
    ```

  - 原因: `get_all_clients()` 供 Task 6（McpMiddleware.collect_tools）遍历创建 McpToolBridge；`resource_summary()` 供 Task 5（McpResourceTool）生成动态 description

- [ ] 实现 `McpClientPool::shutdown()` —— 优雅关闭所有连接
  - 位置: `client.rs`，`McpClientPool` impl 块末尾
  - 关键逻辑:

    ```rust
    const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

    impl McpClientPool {
        /// 关闭所有 MCP 服务器连接和子进程资源
        /// 逐个调用 close_with_timeout()，超时后放弃等待
        pub async fn shutdown(&mut self) {
            for handle in &self.clients {
                if matches!(handle.1.status, ClientStatus::Connected) {
                    tracing::info!(server = %handle.0, "关闭 MCP 服务器连接");
                }
                handle.1.status = ClientStatus::Disconnected;
            }
            // 逐个关闭 RunningService（包含 stdio 子进程的 graceful shutdown）
            for service in &mut self.services {
                match service.close_with_timeout(SHUTDOWN_TIMEOUT).await {
                    Ok(Some(reason)) => tracing::debug!(?reason, "MCP 连接已关闭"),
                    Ok(None) => tracing::warn!("MCP 连接关闭超时"),
                    Err(e) => tracing::warn!(error = %e, "MCP 连接关闭异常"),
                }
            }
            self.services.clear();
        }
    }
    ```

  - 原因: spec-design.md 要求 App 退出时统一 `pool.shutdown()`；`close_with_timeout` 确保不会无限阻塞；stdio transport 的 `RunningService` 内部会触发子进程 graceful shutdown

- [ ] 修改 `mcp/mod.rs` 添加 `pub mod client` 声明和重导出
  - 位置: `peri-middlewares/src/mcp/mod.rs`，在 `pub mod config;` 之后追加
  - 追加内容:

    ```rust
    pub mod client;

    pub use client::{ClientStatus, McpClientHandle, McpClientPool, McpPoolError};
    ```

  - 原因: 与 Task 1 建立的模块注册模式一致（声明 + pub use 重导出）

- [ ] 为 McpClientPool 连接池管理编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/client.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_empty_config_creates_empty_pool`: 传入空目录（无 `.mcp.json` 且全局配置无 mcpServers）→ `initialize()` 返回空 pool，`clients` 为空
    - `test_pool_get_all_clients_filters_disconnected`: 手动构造 pool，插入 1 个 Connected + 1 个 Failed handle → `get_all_clients()` 仅返回 1 个
    - `test_pool_has_resources`: 手动构造 pool，插入 1 个有 resources 的 Connected handle + 1 个无 resources 的 Connected handle → `has_resources()` 返回 `true`
    - `test_pool_has_no_resources`: 手动构造 pool，所有 handle 的 resources 为空 → `has_resources()` 返回 `false`
    - `test_resource_summary_format`: 手动构造 pool，插入已知 name/resources 的 handle → `resource_summary()` 包含 server 名称和资源 URI
    - `test_client_status_enum`: 验证 `ClientStatus::Connected != ClientStatus::Failed("...")` 且 `ClientStatus::Failed("a") != ClientStatus::Failed("b")`（携带原因的变体）
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::client::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 mcp 模块整体编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证 client.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- mcp::client::tests 2>&1 | tail -15`
  - 预期: 所有 `test_*` 测试通过，输出 `test result: ok`

- [ ] 验证 mod.rs 正确声明 client 子模块
  - `grep -n "pub mod client" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod client;`

- [ ] 验证 mod.rs 正确重导出 client 类型
  - `grep -n "pub use client" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub use client::{ClientStatus, McpClientHandle, McpClientPool, McpPoolError};`

- [ ] 验证 McpClientHandle 包含 spec-design.md 要求的所有字段
  - `grep -E "pub (name|peer|tools|resources|status)" peri-middlewares/src/mcp/client.rs`
  - 预期: 5 个 pub 字段均存在，类型为 `String` / `Peer<RoleClient>` / `Vec<Tool>` / `Vec<Resource>` / `ClientStatus`

---

### Task 4: McpToolBridge 工具桥接

**背景:**
[业务语境] — 将 MCP 服务器暴露的每个工具包装为项目统一的 `BaseTool` 实现，使 ReAct 循环执行器能像调用内置工具（Read/Write/Bash 等）一样调用 MCP 远程工具，LLM 通过工具名 `mcp__{server}__{tool}` 和带 `[MCP:{server}]` 前缀的 description 识别工具来源。
[修改原因] — 当前代码中不存在 MCP 工具桥接能力，需要新建 `tool_bridge.rs` 实现 `McpToolBridge` 结构体及其 `BaseTool` trait impl，将 rmcp 的 `Tool` 元数据和 `call_tool` 调用适配为项目标准接口。
[上下游影响] — 本 Task 依赖 Task 3（`McpClientHandle` 提供 `peer` 和 `ClientStatus`），输出 `McpToolBridge` + `ToolCallError` + `build_tool_bridges()` 工厂函数，被 Task 6（McpMiddleware.collect_tools 遍历调用）直接依赖。

**涉及文件:**

- 新建: `peri-middlewares/src/mcp/tool_bridge.rs`
- 修改: `peri-middlewares/src/mcp/mod.rs`（添加 `pub mod tool_bridge` + 重导出）

**执行步骤:**

- [ ] 在 `tool_bridge.rs` 顶部定义 `ToolCallError` 错误类型和 `McpToolBridge` 结构体
  - 位置: 新建 `peri-middlewares/src/mcp/tool_bridge.rs`，文件顶部
  - 关键逻辑:

    ```rust
    use std::sync::Arc;
    use async_trait::async_trait;
    use peri_agent::tools::BaseTool;
    use thiserror::Error;

    /// MCP 工具调用错误
    #[derive(Debug, Error)]
    pub enum ToolCallError {
        #[error("MCP 服务器 \"{server}\" 未连接 (状态: {status:?})")]
        NotConnected { server: String, status: String },
        #[error("MCP 服务器 \"{server}\" 工具 \"{tool}\" 调用失败: {reason}")]
        CallFailed { server: String, tool: String, reason: String },
        #[error("MCP 服务器 \"{server}\" 工具 \"{tool}\" 调用超时 ({timeout_secs}s)")]
        Timeout { server: String, tool: String, timeout_secs: u64 },
    }

    /// 将单个 MCP tool 包装为 BaseTool 实现
    pub struct McpToolBridge {
        server_name: String,
        tool_name: String,
        full_name: String,            // "mcp__{server}__{tool}"
        description: String,          // "[MCP:{server}] {原始 description}"
        input_schema: serde_json::Value,
        client: Arc<super::client::McpClientHandle>,
    }
    ```

  - 原因: 遵循项目编码规范（库 crate 用 thiserror）；结构体字段与 spec-design.md §McpToolBridge 一致；`Arc<McpClientHandle>` 共享连接句柄，多个 bridge 可并发调用同一 peer（rmcp Peer 内部线程安全）

- [ ] 实现 `McpToolBridge::new()` 构造函数
  - 位置: `tool_bridge.rs`，`McpToolBridge` 定义之后
  - 关键逻辑:

    ```rust
    const TOOL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

    impl McpToolBridge {
        /// 从 rmcp Tool 元数据和客户端句柄创建 McpToolBridge
        pub fn new(
            server_name: &str,
            tool: &rmcp::model::Tool,
            client: Arc<super::client::McpClientHandle>,
        ) -> Self {
            let tool_name = tool.name.clone();
            let full_name = format!("mcp__{}__{}", server_name, tool_name);
            let description = format!("[MCP:{}] {}", server_name, tool.description.as_deref().unwrap_or(""));
            Self {
                server_name: server_name.to_string(),
                tool_name,
                full_name,
                description,
                input_schema: tool.input_schema.clone().unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                client,
            }
        }
    }
    ```

  - 原因: `full_name` 格式 `mcp__{server}__{tool}` 与 spec-design.md 一致，双下划线分隔保证命名空间隔离；`description` 前缀 `[MCP:{server}]` 让 LLM 识别工具来源；`input_schema` 缺失时用空 JSON Object 兜底

- [ ] 实现 `BaseTool` trait 的 `name()` / `description()` / `parameters()` 方法
  - 位置: `tool_bridge.rs`，`McpToolBridge` impl 块
  - 关键逻辑:

    ```rust
    #[async_trait]
    impl BaseTool for McpToolBridge {
        fn name(&self) -> &str {
            &self.full_name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn parameters(&self) -> serde_json::Value {
            self.input_schema.clone()
        }
    ```

  - 原因: MCP Tool 的 `inputSchema` 本身就是 JSON Schema 格式，直接透传无需转换（spec-design.md 明确指出）；`name()` 返回 `mcp__{server}__{tool}` 格式，与 HITL 前缀匹配规则（Task 7）对齐

- [ ] 实现 `BaseTool::invoke()` —— 核心调用逻辑：peer.call_tool + 结果格式化 + 超时
  - 位置: `tool_bridge.rs`，`BaseTool` impl 块，`parameters()` 之后
  - 关键逻辑:

    ```rust
        async fn invoke(
            &self,
            input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            use super::client::ClientStatus;
            use rmcp::model::{CallToolRequestParam, GetServerInfoParam};

            // 1. 检查连接状态
            match self.client.status {
                ClientStatus::Connected => {},
                other => {
                    return Err(Box::new(ToolCallError::NotConnected {
                        server: self.server_name.clone(),
                        status: format!("{:?}", other),
                    }));
                }
            }

            // 2. 构建 rmcp 请求参数
            let request = CallToolRequestParam {
                name: self.tool_name.clone(),
                arguments: Some(input.as_object().cloned().unwrap_or_default()),
            };

            // 3. 带超时调用 peer.call_tool()
            let result = tokio::time::timeout(TOOL_CALL_TIMEOUT, self.client.peer.call_tool(request))
                .await
                .map_err(|_| ToolCallError::Timeout {
                    server: self.server_name.clone(),
                    tool: self.tool_name.clone(),
                    timeout_secs: TOOL_CALL_TIMEOUT.as_secs(),
                })?
                .map_err(|e| ToolCallError::CallFailed {
                    server: self.server_name.clone(),
                    tool: self.tool_name.clone(),
                    reason: e.to_string(),
                })?;

            // 4. 处理 is_error 标志
            if result.is_error.unwrap_or(false) {
                let error_text = format_content(&result.content);
                return Err(Box::new(ToolCallError::CallFailed {
                    server: self.server_name.clone(),
                    tool: self.tool_name.clone(),
                    reason: error_text,
                }));
            }

            // 5. 格式化返回
            Ok(format_content(&result.content))
        }
    }
    ```

  - 原因: 先检查连接状态避免对已断开 peer 发请求；超时 120s 与 Bash 工具对齐（spec-design.md §连接管理策略）；`is_error=true` 时将错误内容包装为 `ToolCallError::CallFailed` 返回，由 LLM 决定重试

- [ ] 实现 `format_content()` 辅助函数——将 `Vec<Content>` 格式化为字符串
  - 位置: `tool_bridge.rs`，`BaseTool` impl 块之后（模块级自由函数）
  - 关键逻辑:

    ```rust
    use rmcp::model::Content;

    /// 将 MCP CallToolResult 的 content 列表格式化为纯文本字符串
    /// TextContent → 直接拼接文本内容
    /// ImageContent → 返回 [image: {mimetype}] 占位符（TUI 不支持图片渲染）
    /// ResourceContent → 返回 [resource: {uri}] 占位符
    /// 其他变体 → 返回 [unknown content]
    fn format_content(contents: &[Content]) -> String {
        let mut parts = Vec::new();
        for content in contents {
            match content {
                Content::Text(text_content) => {
                    parts.push(text_content.text.clone());
                }
                Content::Image(image_content) => {
                    parts.push(format!("[image: {}]", image_content.mime_type.clone().unwrap_or_else(|| "unknown".into())));
                }
                Content::Resource(resource_content) => {
                    parts.push(format!("[resource: {}]", resource_content.resource.uri.clone()));
                }
                _ => {
                    parts.push("[unknown content]".to_string());
                }
            }
        }
        parts.join("\n")
    }
    ```

  - 原因: rmcp 的 `Content` 枚举有多种变体（Text/Image/Resource/Audio），TUI 环境仅支持文本输出，图片和资源以占位符表示；`join("\n")` 保证多个 content 块之间有明确分隔

- [ ] 实现 `build_tool_bridges()` 工厂函数——从连接池批量创建 McpToolBridge
  - 位置: `tool_bridge.rs`，`format_content()` 之后
  - 关键逻辑:

    ```rust
    use super::client::McpClientPool;

    /// 从 McpClientPool 的所有已连接客户端中批量创建 McpToolBridge
    /// 返回 Vec<Box<dyn BaseTool>>，可直接注入到工具注册表
    pub fn build_tool_bridges(pool: &McpClientPool) -> Vec<Box<dyn BaseTool>> {
        let mut bridges: Vec<Box<dyn BaseTool>> = Vec::new();
        for client in pool.get_all_clients() {
            for tool in &client.tools {
                bridges.push(Box::new(McpToolBridge::new(
                    &client.name,
                    tool,
                    Arc::clone(client),
                )));
            }
        }
        bridges
    }
    ```

  - 原因: `get_all_clients()` 已过滤 `Failed` / `Disconnected` 状态，仅遍历已连接的客户端；每个 `Tool` 元数据创建独立的 `McpToolBridge` 实例，共享同一个 `Arc<McpClientHandle>`；返回 `Vec<Box<dyn BaseTool>>` 与 `Middleware::collect_tools()` 返回类型一致

- [ ] 修改 `mcp/mod.rs` 添加 `pub mod tool_bridge` 声明和重导出
  - 位置: `peri-middlewares/src/mcp/mod.rs`，在 `pub mod client;` 行之后追加
  - 追加内容:

    ```rust
    pub mod tool_bridge;

    pub use tool_bridge::{McpToolBridge, ToolCallError, build_tool_bridges};
    ```

  - 原因: 与 Task 1 建立的模块注册模式一致（声明 + pub use 重导出）；`build_tool_bridges` 是 Task 6（McpMiddleware）直接依赖的核心工厂函数

- [ ] 为 McpToolBridge 工具桥接编写单元测试
  - 测试文件: `peri-middlewares/src/mcp/tool_bridge.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_new_creates_correct_full_name`: 构造 `Tool { name: "read_file".into(), description: Some("Read a file".into()), input_schema: Some(json!({"type":"object","properties":{"path":{"type":"string"}}})) }` + `McpClientHandle { name: "fs", status: Connected, .. }` → `McpToolBridge::new("fs", &tool, handle)` 的 `name()` 返回 `"mcp__fs__read_file"`
    - `test_new_creates_correct_description`: 同上输入 → `description()` 返回 `"[MCP:fs] Read a file"`
    - `test_new_preserves_input_schema`: 同上输入 → `parameters()` 返回与 `tool.input_schema` 一致的 JSON Schema
    - `test_new_empty_description`: 构造 `Tool { description: None, .. }` → `description()` 返回 `"[MCP:fs] "`（空 description 不 panic）
    - `test_new_missing_input_schema`: 构造 `Tool { input_schema: None, .. }` → `parameters()` 返回空 JSON Object `{}`
    - `test_invoke_not_connected`: 构造 `McpClientHandle { status: Failed("connection lost".into()), .. }` → `invoke(json!({}))` 返回 `Err(ToolCallError::NotConnected { .. })`
    - `test_format_content_text_only`: 调用 `format_content(&[Content::Text(TextContent { text: "hello".into(), .. })])` → 返回 `"hello"`
    - `test_format_content_mixed`: 调用 `format_content(&[text("line1"), text("line2")])` → 返回 `"line1\nline2"`
    - `test_format_content_image`: 调用 `format_content(&[Content::Image(ImageContent { mime_type: Some("image/png".into()), .. })])` → 返回 `"[image: image/png]"`
    - `test_build_tool_bridges_empty_pool`: 构造空 `McpClientPool` → `build_tool_bridges(&pool)` 返回空 `Vec`
    - `test_build_tool_bridges_filters_disconnected`: 构造 pool 含 1 个 Connected（2 个 tools）+ 1 个 Failed → `build_tool_bridges()` 返回 2 个 bridge
  - 运行命令: `cargo test -p peri-middlewares --lib -- mcp::tool_bridge::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 tool_bridge.rs 编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出 `Finished` 且无编译错误

- [ ] 验证 tool_bridge.rs 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- mcp::tool_bridge::tests 2>&1 | tail -15`
  - 预期: 所有 `test_*` 测试通过，输出 `test result: ok`

- [ ] 验证 mod.rs 正确声明 tool_bridge 子模块
  - `grep -n "pub mod tool_bridge" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod tool_bridge;`

- [ ] 验证 mod.rs 正确重导出 tool_bridge 类型
  - `grep -n "pub use tool_bridge" peri-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub use tool_bridge::{McpToolBridge, ToolCallError, build_tool_bridges};`

- [ ] 验证 McpToolBridge 实现了 BaseTool trait
  - `grep -n "impl BaseTool for McpToolBridge" peri-middlewares/src/mcp/tool_bridge.rs`
  - 预期: 输出包含 `impl BaseTool for McpToolBridge`

- [ ] 验证工具调用超时常量与 Bash 工具一致（120s）
  - `grep -n "TOOL_CALL_TIMEOUT" peri-middlewares/src/mcp/tool_bridge.rs`
  - 预期: 输出包含 `Duration::from_secs(120)`

- [ ] 验证 invoke 方法包含连接状态检查
  - `grep -A5 "NotConnected" peri-middlewares/src/mcp/tool_bridge.rs | grep -c "ClientStatus::Connected"`
  - 预期: 输出大于 0，确认 invoke 在调用前检查连接状态

---

### Acceptance: MCP 核心组件验收

**前置条件:**

- 启动命令: `cargo build -p peri-middlewares`
- 所有前置 Task（Task 1-4）的单元测试已通过

**端到端验证:**

1. 运行 peri-middlewares 完整测试套件确保无回归
   - `cargo test -p peri-middlewares 2>&1 | tail -15`
   - 预期: 所有测试通过，`test result: ok`
   - 失败排查: 检查 Task 1（config）、Task 2（transport）、Task 3（client）、Task 4（tool_bridge）的测试步骤

2. 验证 mcp 模块完整结构
   - `ls -la peri-middlewares/src/mcp/`
   - 预期: 包含 `mod.rs`, `config.rs`, `transport.rs`, `client.rs`, `tool_bridge.rs` 5 个文件
   - 失败排查: 检查对应 Task 是否遗漏文件创建步骤

3. 验证 lib.rs 正确导出所有 mcp 类型
   - `grep "pub use mcp::" peri-middlewares/src/lib.rs`
   - 预期: 包含 McpConfigError, McpConfigFile, McpServerConfig, load_merged_config, McpMiddleware 等导出
   - 失败排查: 检查 Task 1 和 Task 6 的 lib.rs 修改步骤

4. 验证整体编译无错误无警告
   - `cargo build -p peri-middlewares 2>&1 | grep -E "error|warning" | head -20`
   - 预期: 无编译错误，允许少量无关警告
   - 失败排查: 检查各 Task 中类型引用是否正确（特别是 rmcp 的 Peer/Tool/Resource 类型路径）
