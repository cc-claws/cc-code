# Web 工具中间件 执行计划

**目标:** 新增 WebMiddleware，提供 WebFetch（URL 抓取 + HTML→Markdown）和 WebSearch（Exa 搜索）工具

**技术栈:** Rust, reqwest 0.13, html2text, url 2, serde_json

**设计文档:** spec/feature_20260505_F001_web-tools/spec-design.md

## 改动总览

- 新增 `web.rs`（~300 行）包含 WebFetchTool + WebSearchTool + WebMiddleware，修改 `Cargo.toml`（+html2text）、`mod.rs`（+pub mod web）、`hitl/mod.rs`（审批清单）、`agent.rs`（中间件链注册）、`lib.rs`（prelude 导出）、`CLAUDE.md`（文档）共 6 个现有文件
- Task 1 创建 web.rs 骨架 + WebFetchTool；Task 2 在同文件追加 WebSearchTool；Task 3 组装 WebMiddleware 并接入 HITL/TUI/prelude/文档。线性依赖，严格顺序执行
- 经代码确认 reqwest/url 已在 workspace 依赖中，仅新增 html2text 一个 crate；WebFetchTool/WebSearchTool 无状态（不需要 cwd），与 BashTool 模式不同但更简洁

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**

- [x] 验证 Rust 工具链可用
  - `cargo --version`
  - 预期: 输出 cargo 版本号
- [x] 验证项目可构建
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 编译成功，无 error

**检查步骤:**

- [x] 构建命令执行成功
  - `cargo build -p peri-middlewares 2>&1 | grep -E 'error|Finished'`
  - 预期: 输出包含 "Finished"，无 error
- [x] 测试命令可用
  - `cargo test -p peri-middlewares --lib --no-run 2>&1 | tail -3`
  - 预期: 编译成功，无配置错误

---

### Task 1: WebFetchTool 核心实现

**背景:**
实现 WebFetch 工具的核心逻辑——URL 安全校验、HTTP GET 请求、HTML→Markdown 转换、Content-Type 判断和响应截断。本 Task 仅实现 `WebFetchTool` struct 及其辅助函数，不包含 `WebMiddleware` struct（留给 Task 3）。Task 2（WebSearchTool）和 Task 3（集成注册）均依赖本 Task 创建的 `web.rs` 文件。

**涉及文件:**

- 新建: `peri-middlewares/src/middleware/web.rs`
- 修改: `peri-middlewares/Cargo.toml`（添加 html2text 依赖）
- 修改: `peri-middlewares/src/middleware/mod.rs`（添加 `pub mod web;`）

**执行步骤:**

- [x] 在 `peri-middlewares/Cargo.toml` 的 `[dependencies]` 段添加 html2text 依赖 — reqwest 和 url 已在 workspace 声明，无需新增
  - 位置: `Cargo.toml` 的 `reqwest.workspace = true` 行（~L35）之后
  - 追加: `html2text = "0.14"`

- [x] 在 `peri-middlewares/src/middleware/mod.rs` 添加模块声明
  - 位置: `mod.rs` 现有 `pub mod` 声明段（~L3 `pub mod terminal;` 之后）
  - 追加: `pub mod web;`
  - 注意: 本 Task 不添加 `pub use` re-export（WebMiddleware re-export 留到 Task 3）

- [x] 创建 `peri-middlewares/src/middleware/web.rs`，实现完整文件结构
  - 文件头部 imports:

    ```rust
    use async_trait::async_trait;
    use peri_agent::tools::BaseTool;
    use serde_json::Value;
    use std::net::IpAddr;
    use tokio::time::{timeout, Duration};
    use url::Url;
    ```

- [x] 实现 `validate_url(url: &str) -> Result<Url, String>` 函数 — URL 安全校验，防止 SSRF
  - 位置: `web.rs` 顶部（struct 定义之前），作为模块级私有函数
  - 逻辑:

    ```
    1. Url::parse(url) → 失败则 Err("无效的 URL: ...")
    2. 检查 scheme，仅允许 "http" 或 "https" → 否则 Err("仅支持 http/https 协议")
    3. 获取 host:
       - url.host() 为 None → Err("URL 缺少主机名")
       - url.host() 为 Some(Host::Domain(_)) → 通过（域名不做 DNS 解析）
       - url.host() 为 Some(Host::Ipv4(ip) | Host::Ipv6(ip)) → 检查 IP:
         - is_loopback() → Err("禁止访问回环地址")
         - is_private() → Err("禁止访问私有地址")
         - is_link_local() → Err("禁止访问链路本地地址")
         - 等于 0.0.0.0 或 [::] → Err("禁止访问未指定地址")
    4. Ok(parsed_url)
    ```

- [x] 实现 `html_to_text(html: &str) -> String` 函数 — HTML 转 Markdown 纯文本
  - 位置: `validate_url` 之后
  - 逻辑: `html2text::from_read(html.as_bytes(), 120)` — 120 列宽度
  - 返回转换后的纯文本字符串

- [x] 实现 `truncate_content(content: &str, max_lines: usize) -> String` 函数 — 按行数截断
  - 位置: `html_to_text` 之后
  - 逻辑:

    ```
    1. content.lines() 收集为 Vec<&str>
    2. 行数 ≤ max_lines → 直接返回
    3. 行数 > max_lines → 取前 max_lines 行 join，附加 "\n[内容已截断，原始内容共 {total} 行]"
    4. 使用 chars().take() 等字符级操作，不做字节切片
    ```

- [x] 定义 `WebFetchTool` struct 和 `impl` — 无状态工具
  - 结构体:

    ```rust
    pub struct WebFetchTool;
    ```

  - 常量 `DESCRIPTION`（参考 terminal.rs 的 `BASH_DESCRIPTION` 模式）:

    ```rust
    const WEB_FETCH_DESCRIPTION: &str = r#"Fetches a web page by URL and returns its content as text.

Usage:

- Only http:// and https:// URLs are allowed
- HTML pages are converted to readable text; JSON is pretty-printed; plain text is returned as-is
- Binary content returns only type and size information
- Results are truncated at 2000 lines
- An optional 'prompt' parameter provides guidance for how to use the fetched content

Security:

- Internal/private/loopback IP addresses are blocked
- Maximum response size: 10MB
- Request timeout: 30 seconds
- Maximum redirects: 5"#;

    ```
  - `impl WebFetchTool`:
    ```rust
    pub fn new() -> Self { Self }
    ```

- [x] 实现 `#[async_trait] impl BaseTool for WebFetchTool` — 工具接口
  - `fn name(&self) -> &str` → `"WebFetch"`
  - `fn description(&self) -> &str` → `WEB_FETCH_DESCRIPTION`
  - `fn parameters(&self) -> Value` →

    ```rust
    serde_json::json!({
        "type": "object",
        "properties": {
            "url": {
                "type": "string",
                "description": "要抓取的完整 URL（http/https）"
            },
            "prompt": {
                "type": "string",
                "description": "可选。提取内容的指导提示，附在结果前供 LLM 参考"
            }
        },
        "required": ["url"]
    })
    ```

  - `async fn invoke(&self, input: Value) -> Result<String, Box<dyn Error + Send + Sync>>`:

    ```
    1. 提取 url: input["url"].as_str() → 缺失则 Err("Missing url parameter")
    2. 提取 prompt: input["prompt"].as_str()（可选）
    3. validate_url(url)? — 安全校验
    4. 构建 reqwest::Client:
       - Client::builder()
       - .timeout(Duration::from_secs(30))
       - .redirect(Policy::limited(5))
       - .user_agent("peri/1.0")
       - .build()?
    5. client.get(parsed_url).send().await?
       - 错误处理: 超时/连接失败 → 返回 Err 描述性消息
    6. 检查响应体大小: resp.content_length() > 10MB → 返回 "响应体超过 10MB 限制"
    7. 获取 Content-Type: resp.headers().get("content-type")
    8. 读取 body: resp.text().await?（用 timeout(Duration::from_secs(30)) 包裹）
    9. Content-Type 分支处理:
       - 包含 "text/html" → html_to_text(&body)
       - 包含 "text/plain" → body 直接使用
       - 包含 "application/json" → serde_json::from_str::<Value>(&body) → serde_json::to_string_pretty() 反序列化再序列化，失败则直接用 body
       - 其他 → 返回格式 "Content-Type: {ct}\nSize: {body.len()} bytes\n（不支持的内容类型）"
    10. truncate_content(&processed, 2000)
    11. prompt 处理: 有 prompt 则 format!("提示: {prompt}\n\n{content}")
    12. Ok(result)
    ```

- [x] 为 WebFetchTool 和辅助函数编写单元测试
  - 测试位置: `web.rs` 底部 `#[cfg(test)] mod tests` 块（参考 terminal.rs 测试模式）
  - 测试场景:
    - `test_validate_url_rejects_ftp`: `validate_url("ftp://example.com")` → Err 包含 "仅支持 http/https"
    - `test_validate_url_rejects_localhost`: `validate_url("http://127.0.0.1/test")` → Err 包含 "回环地址"
    - `test_validate_url_rejects_private_ip`: `validate_url("http://192.168.1.1/test")` → Err 包含 "私有地址"
    - `test_validate_url_rejects_link_local`: `validate_url("http://169.254.1.1/test")` → Err 包含 "链路本地"
    - `test_validate_url_accepts_https`: `validate_url("https://example.com/page")` → Ok
    - `test_validate_url_rejects_no_host`: `validate_url("http:///path")` → Err 包含 "主机名"
    - `test_truncate_content_no_truncation`: 10 行文本 → 原样返回
    - `test_truncate_content_with_truncation`: 3000 行文本 → 截断到 2000 行 + "[内容已截断]"
    - `test_html_to_text_basic`: `"<p>Hello</p>"` → 包含 "Hello"
    - `test_tool_name_is_web_fetch`: `WebFetchTool::new().name()` → "WebFetch"
    - `test_tool_parameters_required_url`: parameters 的 required 数组包含 "url"
  - 运行命令: `cargo test -p peri-middlewares --lib -- middleware::web::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 html2text 依赖已添加
  - `grep 'html2text' peri-middlewares/Cargo.toml`
  - 预期: 输出包含 `html2text = "0.14"`

- [x] 验证 web 模块已注册
  - `grep 'pub mod web' peri-middlewares/src/middleware/mod.rs`
  - 预期: 输出包含 `pub mod web;`

- [x] 验证 WebFetchTool 实现了 BaseTool trait
  - `grep 'impl BaseTool for WebFetchTool' peri-middlewares/src/middleware/web.rs`
  - 预期: 输出包含匹配行

- [x] 验证单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- middleware::web::tests 2>&1 | tail -20`
  - 预期: 所有 test 开头的行显示 "ok"，无 "FAILED"

- [x] 验证编译无错误
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 或 "Finished"，无 error

---

### Task 2: WebSearchTool 核心实现

**背景:**
本 Task 在 Task 1 已创建的 `web.rs` 文件中追加 `WebSearchTool`，实现通过 Exa Search API 的网页搜索功能。Task 1 已完成文件骨架和 `WebFetchTool`，本 Task 在同文件中追加搜索工具的 struct、API 调用、响应解析和格式化逻辑。Task 3 将依赖本 Task 的 `WebSearchTool` 完成中间件注册。

**涉及文件:**

- 修改: `peri-middlewares/src/middleware/web.rs`（在 Task 1 已有内容后追加 `WebSearchTool` struct 及实现）

**执行步骤:**

- [x] 在 `web.rs` 中定义搜索结果数据结构 — 用于反序列化 Exa API 的 JSON 响应
  - 位置: `web.rs` 文件中 `WebFetchTool` 及其 `impl` 块之后
  - 定义:

    ```rust
    /// Exa 搜索结果单条条目
    #[derive(Debug, serde::Deserialize)]
    struct ExaSearchResult {
        title: String,
        url: String,
        text: Option<String>,
    }

    /// Exa 搜索 API 响应
    #[derive(Debug, serde::Deserialize)]
    struct ExaSearchResponse {
        results: Vec<ExaSearchResult>,
    }
    ```

  - 原因: Exa API 返回 `{"results": [{"title": "...", "url": "...", "text": "..."}]}`，`text` 字段在 `contents.text=true` 时返回，可能为 null

- [x] 定义 `WebSearchTool` 无状态 struct — 遵循 `BashTool` 的无状态模式
  - 位置: `web.rs`，Exa 数据结构定义之后
  - 定义:

    ```rust
    const WEBSEARCH_DESCRIPTION: &str = r#"Search the web using Exa Search API.

Usage:

- Provide a search query to find relevant web pages
- Returns results as a numbered Markdown list with titles, URLs, and text snippets
- Each result's text is truncated to 500 characters
- Requires EXA_API_KEY environment variable to be set

Parameters:

- query (required): Search keywords
- num_results (optional): Number of results, default 10, max 20"#;

    pub struct WebSearchTool;

    impl WebSearchTool {
        pub fn new() -> Self { Self }
    }

    ```

- [x] 实现 `format_search_results` 辅助函数 — 将搜索结果格式化为 Markdown

  - 位置: `WebSearchTool` struct 定义之后、`impl BaseTool` 之前
  - 关键逻辑:

    ```rust
    /// 单条结果文本截断上限（字符数）
    const MAX_RESULT_TEXT_CHARS: usize = 500;

    /// 将搜索结果格式化为 Markdown 编号列表
    fn format_search_results(results: &[ExaSearchResult]) -> String {
        if results.is_empty() {
            return "No search results found.".to_string();
        }

        let mut output = String::from("## Search Results\n\n");
        for (i, r) in results.iter().enumerate() {
            output.push_str(&format!("{}. **{}** - {}\n", i + 1, r.title, r.url));
            if let Some(text) = &r.text {
                let truncated: String = text.chars().take(MAX_RESULT_TEXT_CHARS).collect::<String>();
                output.push_str(&format!("   {}\n\n", truncated.trim()));
            } else {
                output.push('\n');
            }
        }
        output
    }
    ```

  - 截断使用 `chars().take(500)` — 字符级截断，符合 CLAUDE.md 编码规范
  - 原因: spec-design.md 要求单条 text 超过 500 字符时截断，Markdown 格式为 `1. **标题** - url` + 缩进摘要

- [x] 实现 `#[async_trait] impl BaseTool for WebSearchTool` — 工具注册和入口
  - 位置: `format_search_results` 之后
  - 关键逻辑:

    ```rust
    #[async_trait::async_trait]
    impl BaseTool for WebSearchTool {
        fn name(&self) -> &str { "WebSearch" }

        fn description(&self) -> &str { WEBSEARCH_DESCRIPTION }

        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search keywords"
                    },
                    "num_results": {
                        "type": "integer",
                        "description": "Number of results, default 10, max 20"
                    }
                },
                "required": ["query"]
            })
        }

        async fn invoke(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            // 1. 提取并校验参数
            let query = input["query"].as_str()
                .ok_or("Missing required parameter: query")?;
            let num_results = input["num_results"].as_u64()
                .unwrap_or(10)
                .clamp(1, 20) as usize;

            // 2. 检查 EXA_API_KEY 环境变量
            let api_key = std::env::var("EXA_API_KEY").map_err(|_| {
                "EXA_API_KEY environment variable is not set. Please set it to use WebSearch."
                    .to_string()
            })?;

            // 3. 构建请求并发送
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

            let body = serde_json::json!({
                "query": query,
                "numResults": num_results,
                "type": "auto",
                "contents": { "text": true }
            });

            let resp = client
                .post("https://api.exa.ai/search")
                .header("x-api-key", &api_key)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Exa API request failed: {e}"))?;

            // 4. 检查 HTTP 状态码
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(format!("Exa API returned error (HTTP {status}): {text}").into());
            }

            // 5. 解析响应
            let search_resp: ExaSearchResponse = resp.json().await
                .map_err(|e| format!("Failed to parse Exa API response: {e}"))?;

            // 6. 格式化为 Markdown 编号列表
            Ok(format_search_results(&search_resp.results))
        }
    }
    ```

  - 原因: 遵循 `BashTool::invoke` 的错误处理模式，API 错误用 `Err` 返回

- [x] 为 WebSearchTool 编写单元测试
  - 测试文件: `peri-middlewares/src/middleware/web.rs`（`#[cfg(test)] mod tests` 块内，追加在 Task 1 的测试之后）
  - 测试场景:
    - `test_websearch_name`: `WebSearchTool::new().name()` 返回 `"WebSearch"`
    - `test_websearch_parameters_required`: `parameters()` 返回的 JSON 中 `required` 包含 `"query"`
    - `test_format_search_results_empty`: 空结果 → `"No search results found."`
    - `test_format_search_results_with_text`: 2 条结果（均有 text） → 输出包含 `"## Search Results"`、编号、`**title**`、url、缩进文本
    - `test_format_search_results_text_truncation`: 构造 text 超过 500 字符的结果 → 截断后恰好 500 字符
    - `test_format_search_results_no_text`: 结果 text 为 None → 输出包含标题和 url，无摘要行
    - `test_websearch_missing_query`: `invoke(json!({}))` → 返回错误包含 `"Missing required parameter: query"`
  - 运行命令: `cargo test -p peri-middlewares --lib -- middleware::web::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 编译通过，无类型错误
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Compiling peri-middlewares` 且最终无 error

- [x] 单元测试全部通过
  - `cargo test -p peri-middlewares --lib -- middleware::web::tests 2>&1 | tail -20`
  - 预期: 所有 `test_websearch_*` 测试显示 `test result: ok`

- [x] `WebSearchTool` struct 和 `impl BaseTool` 均存在
  - `grep -n 'WebSearchTool' peri-middlewares/src/middleware/web.rs`
  - 预期: 出现多处（struct 定义、new()、impl BaseTool）

---

### Task 3: 集成与注册

**背景:**
Task 1 和 Task 2 已在 `web.rs` 中定义 WebFetchTool 和 WebSearchTool，本 Task 将它们组装为 WebMiddleware 并接入系统——注册到中间件链、加入 HITL 默认审批清单、更新 prelude 导出和 CLAUDE.md 文档。本 Task 是功能上线的最后一环，完成后 LLM 即可使用 WebFetch/WebSearch 工具。

**涉及文件:**

- 修改: `peri-middlewares/src/middleware/web.rs`（追加 WebMiddleware struct）
- 修改: `peri-middlewares/src/middleware/mod.rs`（添加 `pub use web::WebMiddleware;`）
- 修改: `peri-middlewares/src/hitl/mod.rs`（default_requires_approval 添加 WebFetch/WebSearch）
- 修改: `peri-middlewares/src/lib.rs`（prelude 添加 WebMiddleware）
- 修改: `peri-tui/src/app/agent.rs`（中间件链添加 WebMiddleware::new()）
- 修改: `CLAUDE.md`（更新中间件链文档和 HITL 审批清单）

**执行步骤:**

- [x] 在 web.rs 末尾追加 WebMiddleware struct — 参考 TerminalMiddleware 模式（terminal.rs L215-246）
  - 位置: `peri-middlewares/src/middleware/web.rs`，在文件末尾（WebSearchTool 定义之后）追加
  - 实现:

    ```rust
    /// Web 中间件，提供 WebFetch 和 WebSearch 工具
    pub struct WebMiddleware;

    impl WebMiddleware {
        pub fn new() -> Self { Self }
    }

    impl Default for WebMiddleware {
        fn default() -> Self { Self::new() }
    }

    #[async_trait]
    impl<S: State> Middleware<S> for WebMiddleware {
        fn name(&self) -> &str { "WebMiddleware" }
        fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
            vec![
                Box::new(WebFetchTool::new()),
                Box::new(WebSearchTool::new()),
            ]
        }
    }
    ```

  - 需在文件顶部 imports 中追加: `use peri_agent::agent::state::State;` 和 `use peri_agent::middleware::r#trait::Middleware;`（若 Task 1 尚未包含）
  - 原因: 无状态中间件，与 TerminalMiddleware/FilesystemMiddleware 模式一致

- [x] 在 mod.rs 添加 WebMiddleware re-export — 使外部 crate 可通过 `middleware::WebMiddleware` 导入
  - 位置: `peri-middlewares/src/middleware/mod.rs` L10（`pub use todo::TodoMiddleware;` 之后）
  - 追加: `pub use web::WebMiddleware;`
  - 原因: Task 1 已添加 `pub mod web;`，此处补齐 re-export

- [x] 在 HITL default_requires_approval 中添加 WebFetch 和 WebSearch — 确保网络请求默认需用户审批
  - 位置: `peri-middlewares/src/hitl/mod.rs` L40-49，`default_requires_approval` 函数体
  - 在 `|| tool_name.starts_with("mcp__")` 之前追加: `|| tool_name == "WebFetch" || tool_name == "WebSearch"`
  - 原因: 网络请求涉及外部数据获取，需 HITL 审控

- [x] 在 HITL 测试中添加 WebFetch/WebSearch 断言 — 验证审批规则生效
  - 位置: `peri-middlewares/src/hitl/mod.rs` L453 `test_default_requires_approval` 测试函数
  - 在 `assert!(default_requires_approval("mcp__web__fetch"));` 之后（约 L466），添加:

    ```rust
    // Web 工具需审批
    assert!(default_requires_approval("WebFetch"));
    assert!(default_requires_approval("WebSearch"));
    ```

- [x] 在 lib.rs prelude 中添加 WebMiddleware — 供下游 crate 一次性导入
  - 位置: `peri-middlewares/src/lib.rs` L74
  - 将 `pub use crate::middleware::{FilesystemMiddleware, TerminalMiddleware, TodoMiddleware};` 改为:
    `pub use crate::middleware::{FilesystemMiddleware, TerminalMiddleware, TodoMiddleware, WebMiddleware};`
  - 原因: 与其他中间件保持一致的导出方式

- [x] 在 TUI agent 中间件链中注册 WebMiddleware — 插入到 TerminalMiddleware 之后、TodoMiddleware 之前
  - 位置: `peri-tui/src/app/agent.rs` L255-256
  - 在 `.add_middleware(Box::new(TerminalMiddleware::new()))` (L255) 之后、`.add_middleware(Box::new(TodoMiddleware::new(todo_tx)))` (L256) 之前，插入:

    ```rust
    .add_middleware(Box::new(WebMiddleware::new()))
    ```

  - 原因: agent.rs L8 使用 `use peri_middlewares::prelude::*;`，WebMiddleware 已通过 prelude 导入，直接用 `WebMiddleware` 即可

- [x] 更新 CLAUDE.md 中间件链文档 — 反映 WebMiddleware 的注册位置
  - 位置: `CLAUDE.md` "中间件链执行顺序" 代码块
  - 在第 6 行 `6. TerminalMiddleware` 之后插入新行: `7. WebMiddleware           ← WebFetch/WebSearch 工具`
  - 将原 7-11 的序号分别改为 8-12
  - 原因: 文档与代码保持同步

- [x] 更新 CLAUDE.md HITL 审批清单 — 反映新增的需审批工具
  - 位置: `CLAUDE.md` "HITL 审批" 部分 "默认需审批工具" 行
  - 在 `mcp__*` 后追加 `、`WebFetch`、`WebSearch``
  - 原因: 文档与代码保持同步

- [x] 为 HITL default_requires_approval 新增断言运行单元测试
  - 测试文件: `peri-middlewares/src/hitl/mod.rs`（`test_default_requires_approval` 函数内，步骤 4 已添加断言）
  - 运行命令: `cargo test -p peri-middlewares --lib -- hitl::tests::test_default_requires_approval`
  - 预期: 所有测试通过，包括新增的 WebFetch/WebSearch 断言

**检查步骤:**

- [x] 验证 WebMiddleware 编译通过
  - `cargo build -p peri-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Compiling peri-middlewares` 且无 error
- [x] 验证 HITL 测试通过
  - `cargo test -p peri-middlewares --lib -- hitl::tests::test_default_requires_approval`
  - 预期: 所有测试通过，包括新增的 WebFetch/WebSearch 断言
- [x] 验证 TUI 编译通过
  - `cargo build -p peri-tui 2>&1 | tail -5`
  - 预期: 编译成功，无 error
- [x] 验证 WebMiddleware re-export 可达
  - `grep -n 'WebMiddleware' peri-middlewares/src/middleware/mod.rs peri-middlewares/src/lib.rs`
  - 预期: mod.rs 包含 `pub use web::WebMiddleware`，lib.rs L74 包含 `WebMiddleware`
- [x] 验证 CLAUDE.md 已更新
  - `grep -n 'WebMiddleware\|WebFetch\|WebSearch' CLAUDE.md`
  - 预期: 中间件链和 HITL 清单中均包含 WebMiddleware/WebFetch/WebSearch

---

### Task 4: Web 工具中间件 验收

**前置条件:**

- 构建命令: `cargo build`
- 测试命令: `cargo test`
- 环境变量: 无（WebSearch 测试使用 mock 场景，无需真实 EXA_API_KEY）

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -30`
   - 预期: 全部测试通过，无 FAILED
   - 失败排查: 检查各 Task 的测试步骤，重点关注 `middleware::web::tests` 和 `hitl::tests::test_default_requires_approval`

2. 验证 WebFetchTool URL 校验正确拦截内网地址
   - `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_validate_url 2>&1 | tail -15`
   - 预期: 所有 `test_validate_url_*` 测试通过（FTP 拒绝、localhost 拒绝、私有 IP 拒绝、https 通过）
   - 失败排查: 检查 Task 1 的 `validate_url` 实现

3. 验证 WebSearchTool 格式化输出符合 Markdown 规范
   - `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_format_search 2>&1 | tail -15`
   - 预期: 所有 `test_format_search_results_*` 测试通过（空结果、正常结果、截断、无 text）
   - 失败排查: 检查 Task 2 的 `format_search_results` 实现

4. 验证 HITL 默认审批清单包含 WebFetch/WebSearch
   - `cargo test -p peri-middlewares --lib -- hitl::tests::test_default_requires_approval 2>&1 | tail -10`
   - 预期: 测试通过，WebFetch 和 WebSearch 均被标记为需审批
   - 失败排查: 检查 Task 3 的 `default_requires_approval` 修改

5. 验证全 workspace 编译通过
   - `cargo build 2>&1 | tail -10`
   - 预期: 所有 crate 编译成功，无 error
   - 失败排查: 检查 Task 3 的集成步骤，重点关注 `agent.rs` 中间件链注册和 `lib.rs` prelude 导出
