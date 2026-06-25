# WebSearch/WebFetch Tavily 后端迁移 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 WebSearch 和 WebFetch 从 Bing HTML 解析 + reqwest 直连迁移到自托管 Tavily 兼容后端（`https://tavily.claude-code-best.win`），移除所有 Bing 特定代码和原始 HTTP 抓取逻辑。

**Architecture:** 两个工具各自调用 Tavily 兼容的 JSON API（`/search` 和 `/extract`），不再直接抓取和解析 HTML。`web_common.rs` 精简为只保留 `WEB_CREDIBILITY_WARNING` 常量。所有 Bing 解析函数、HTML 转换函数、SSRF 防护函数全部移除。

**Tech Stack:** `reqwest`（HTTP client，已存在）、`serde_json`（JSON 解析，已存在）。移除 `base64`、`urlencoding`、`html2text` 依赖（如无其他使用者）。

**Tavily API 规范：**

### POST /search
Request:
```json
{
  "query": "search keywords",
  "max_results": 10,
  "include_answer": false
}
```

Response:
```json
{
  "query": "...",
  "answer": "...",           // only if include_answer=true
  "results": [
    {
      "title": "Page Title",
      "url": "https://example.com/page",
      "content": "Full content snippet...",
      "score": 0.95
    }
  ]
}
```

### POST /extract
Request:
```json
{
  "urls": ["https://example.com/page"]
}
```

Response:
```json
{
  "results": [
    {
      "url": "https://example.com/page",
      "raw_content": "Full extracted text content..."
    }
  ],
  "failed_results": [
    {
      "url": "https://example.com/bad",
      "error": "..."
    }
  ]
}
```

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `peri-middlewares/src/middleware/web_search.rs` | **Rewrite** | WebSearchTool：调用 Tavily /search，解析 JSON 响应，格式化输出 |
| `peri-middlewares/src/middleware/web_fetch.rs` | **Rewrite** | WebFetchTool：调用 Tavily /extract，解析 JSON 响应，格式化输出 |
| `peri-middlewares/src/middleware/web_common.rs` | **Simplify** | 只保留 `WEB_CREDIBILITY_WARNING` 常量 |
| `peri-middlewares/src/middleware/web_test.rs` | **Rewrite** | 更新测试：移除 Bing/HTML 相关测试，新增 Tavily 响应解析测试 |
| `peri-middlewares/Cargo.toml` | **Modify** | 移除 `base64`、`urlencoding` 依赖（确认仅 web 模块使用） |
| `peri-middlewares/src/middleware/mod.rs` | **No change** | 模块声明不变 |
| `peri-middlewares/src/middleware/web.rs` | **No change** | WebMiddleware 注册不变 |

---

### Task 1: Rewrite `web_search.rs` — WebSearchTool 改用 Tavily /search

**Files:**
- Rewrite: `peri-middlewares/src/middleware/web_search.rs`

- [ ] **Step 1: Replace entire `web_search.rs` with Tavily implementation**

New file content:

```rust
use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::Value;

use super::web_common::WEB_CREDIBILITY_WARNING;

/// Tavily 搜索后端地址
const TAVILY_BASE_URL: &str = "https://tavily.claude-code-best.win";

/// 单条结果文本截断上限（字符数）
const MAX_RESULT_TEXT_CHARS: usize = 500;

/// 搜索结果
pub(crate) struct SearchResult {
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) content: Option<String>,
}

const WEBSEARCH_DESCRIPTION: &str = r#"Search the web using a search engine.

Usage:
- Provide a search query to find relevant web pages
- Returns results as a numbered Markdown list with titles, URLs, and text snippets
- Each result's text is truncated to 500 characters
- No API key required

IMPORTANT:
- Results may be irrelevant or low quality — always verify information before using it
- If results don't contain the information you need, do NOT fabricate or guess values
- Consider using WebFetch to directly access a specific URL for accurate information

Parameters:
- query (required): Search keywords
- num_results (optional): Number of results, default 10, max 20"#;

/// WebSearch 工具 — 通过 Tavily 兼容 API 搜索网页
pub struct WebSearchTool;

impl WebSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

/// 将搜索结果格式化为 Markdown 编号列表
pub(crate) fn format_search_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return format!("{WEB_CREDIBILITY_WARNING}No search results found.");
    }

    let mut output = format!("{WEB_CREDIBILITY_WARNING}## Search Results\n\n");
    for (i, r) in results.iter().enumerate() {
        output.push_str(&format!("{}. **{}** ({})\n", i + 1, r.title, r.url));
        if let Some(content) = &r.content {
            let truncated: String = content.chars().take(MAX_RESULT_TEXT_CHARS).collect();
            output.push_str(&format!("   {}\n\n", truncated.trim()));
        } else {
            output.push('\n');
        }
    }
    output
}

#[async_trait]
impl BaseTool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        WEBSEARCH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
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

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let query = input["query"]
            .as_str()
            .ok_or("Missing required parameter: query")?;
        let max_results = input["num_results"].as_u64().unwrap_or(10).clamp(1, 20) as usize;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let body = serde_json::json!({
            "query": query,
            "max_results": max_results,
        });

        let resp = client
            .post(format!("{TAVILY_BASE_URL}/search"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Search request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Search API returned HTTP {status}: {text}").into());
        }

        let json: Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse search response: {e}"))?;

        let mut results = Vec::new();
        if let Some(arr) = json["results"].as_array() {
            for item in arr {
                let title = item["title"].as_str().unwrap_or("No title").to_string();
                let url = item["url"].as_str().unwrap_or("").to_string();
                let content = item["content"].as_str().map(|s| s.to_string());
                if !url.is_empty() {
                    results.push(SearchResult { title, url, content });
                }
            }
        }

        Ok(format_search_results(&results))
    }
}
```

---

### Task 2: Rewrite `web_fetch.rs` — WebFetchTool 改用 Tavily /extract

**Files:**
- Rewrite: `peri-middlewares/src/middleware/web_fetch.rs`

- [ ] **Step 1: Replace entire `web_fetch.rs` with Tavily implementation**

New file content:

```rust
use async_trait::async_trait;
use peri_agent::tools::BaseTool;
use serde_json::Value;

use super::web_common::WEB_CREDIBILITY_WARNING;

/// Tavily 抓取后端地址
const TAVILY_BASE_URL: &str = "https://tavily.claude-code-best.win";

/// 内容截断行数上限
const MAX_CONTENT_LINES: usize = 2000;

/// WebFetch 工具 — 通过 Tavily 兼容 API 抓取 URL 内容
pub struct WebFetchTool;

const WEB_FETCH_DESCRIPTION: &str = r#"Fetches a web page by URL and returns its content as text.

Usage:
- Only http:// and https:// URLs are allowed
- Content is returned as clean text extracted from the page
- Results are truncated at 2000 lines
- An optional 'prompt' parameter provides guidance for how to use the fetched content

Security:
- Maximum response size: 10MB
- Request timeout: 30 seconds"#;

impl WebFetchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

/// 按行数截断内容
fn truncate_content(content: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= max_lines {
        content.to_string()
    } else {
        let truncated: String = lines[..max_lines].join("\n");
        format!("{truncated}\n[内容已截断，原始内容共 {} 行]", lines.len())
    }
}

#[async_trait]
impl BaseTool for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn description(&self) -> &str {
        WEB_FETCH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
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
    }

    async fn invoke(
        &self,
        input: Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = input["url"].as_str().ok_or("Missing url parameter")?;
        let prompt = input["prompt"].as_str();

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let body = serde_json::json!({
            "urls": [url]
        });

        let resp = client
            .post(format!("{TAVILY_BASE_URL}/extract"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Extract request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Extract API returned HTTP {status}: {text}").into());
        }

        let json: Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse extract response: {e}"))?;

        // 检查 failed_results
        if let Some(failed) = json["failed_results"].as_array() {
            if !failed.is_empty() {
                let errors: Vec<String> = failed
                    .iter()
                    .filter_map(|f| f["error"].as_str().map(|s| s.to_string()))
                    .collect();
                if !errors.is_empty() {
                    return Err(format!("Extract failed: {}", errors.join("; ")).into());
                }
            }
        }

        // 从 results 中提取内容
        let raw_content = json["results"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|r| r["raw_content"].as_str())
            .unwrap_or("");

        if raw_content.is_empty() {
            return Ok(format!("{WEB_CREDIBILITY_WARNING}No content extracted from the URL."));
        }

        let truncated = truncate_content(raw_content, MAX_CONTENT_LINES);

        let result = match prompt {
            Some(p) => format!("{WEB_CREDIBILITY_WARNING}提示: {p}\n\n{truncated}"),
            None => format!("{WEB_CREDIBILITY_WARNING}{truncated}"),
        };

        Ok(result)
    }
}
```

---

### Task 3: Simplify `web_common.rs`

**Files:**
- Modify: `peri-middlewares/src/middleware/web_common.rs`

- [ ] **Step 1: Remove all functions, keep only `WEB_CREDIBILITY_WARNING`**

Replace entire file with:

```rust
/// 网络来源可信度警告（附在 WebFetch/WebSearch 输出前）
pub(crate) const WEB_CREDIBILITY_WARNING: &str =
    "⚠ Web content may be inaccurate or outdated. Verify critical information before relying on it.\n\n";
```

`validate_url`、`html_to_text`、`truncate_content`、`MAX_RESPONSE_BYTES` 全部移除。`truncate_content` 在 `web_fetch.rs` 中有本地副本。

---

### Task 4: Rewrite `web_test.rs`

**Files:**
- Rewrite: `peri-middlewares/src/middleware/web_test.rs`

- [ ] **Step 1: Replace test file**

移除所有 Bing 特定测试（`test_resolve_bing_url_*`、`test_decode_html_entities`、`test_extract_bing_results_*`）和 `validate_url`/`html_to_text`/`truncate_content` 测试。

新增测试覆盖：
1. `format_search_results` — 空结果、有内容、截断、无 content
2. `WebSearchTool::name`/`parameters` — 基本属性
3. `WebFetchTool::name`/`parameters` — 基本属性
4. `truncate_content` — 本地截断函数（从 `web_fetch.rs`）
5. `WebSearchTool::invoke` 缺少 query 参数

```rust
use super::*;
use crate::middleware::web_search::{format_search_results, SearchResult};
use serde_json::Value;

// --- WebFetchTool tests ---

#[test]
fn test_tool_name_is_web_fetch() {
    assert_eq!(WebFetchTool::new().name(), "WebFetch");
}

#[test]
fn test_tool_parameters_required_url() {
    let params = WebFetchTool::new().parameters();
    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&Value::String("url".to_string())));
}

// --- WebSearchTool tests ---

#[test]
fn test_websearch_name() {
    assert_eq!(WebSearchTool::new().name(), "WebSearch");
}

#[test]
fn test_websearch_parameters_required() {
    let params = WebSearchTool::new().parameters();
    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&Value::String("query".to_string())));
}

// --- format_search_results ---

#[test]
fn test_format_search_results_empty() {
    let result = format_search_results(&[]);
    assert!(result.contains("No search results found."));
    assert!(result.contains("Web content may be inaccurate"));
}

#[test]
fn test_format_search_results_with_content() {
    let results = vec![
        SearchResult {
            title: "Test Page".to_string(),
            url: "https://example.com".to_string(),
            content: Some("A sample snippet.".to_string()),
        },
        SearchResult {
            title: "Another Page".to_string(),
            url: "https://example.org".to_string(),
            content: Some("Another snippet here.".to_string()),
        },
    ];
    let output = format_search_results(&results);
    assert!(output.contains("## Search Results"));
    assert!(output.contains("1. **Test Page** (https://example.com)"));
    assert!(output.contains("2. **Another Page** (https://example.org)"));
    assert!(output.contains("A sample snippet."));
}

#[test]
fn test_format_search_results_text_truncation() {
    let long_text = "x".repeat(600);
    let results = vec![SearchResult {
        title: "Long Text".to_string(),
        url: "https://example.com".to_string(),
        content: Some(long_text),
    }];
    let output = format_search_results(&results);
    let snippet_start = output.find("   ").unwrap() + 3;
    let snippet_end = output[snippet_start..].find("\n\n").unwrap();
    let snippet = &output[snippet_start..snippet_start + snippet_end];
    assert_eq!(snippet.chars().count(), 500);
}

#[test]
fn test_format_search_results_no_content() {
    let results = vec![SearchResult {
        title: "No Content".to_string(),
        url: "https://example.com".to_string(),
        content: None,
    }];
    let output = format_search_results(&results);
    assert!(output.contains("**No Content** (https://example.com)"));
}

// --- invoke with missing params ---

#[tokio::test]
async fn test_websearch_missing_query() {
    let tool = WebSearchTool::new();
    let result = tool.invoke(serde_json::json!({})).await;
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("Missing required parameter: query"),
        "实际: {err}"
    );
}

#[tokio::test]
async fn test_webfetch_missing_url() {
    let tool = WebFetchTool::new();
    let result = tool.invoke(serde_json::json!({})).await;
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Missing url parameter"),
        "实际: {err}"
    );
}
```

---

### Task 5: Clean up Cargo.toml dependencies

**Files:**
- Modify: `peri-middlewares/Cargo.toml`

- [ ] **Step 1: Remove unused dependencies**

移除以下依赖（确认仅 web 模块使用）：
- `base64 = "0.22"` — 仅 `web_search.rs` 的 `resolve_bing_url` 使用
- `urlencoding = "2"` — 仅 `web_search.rs` 的 Bing URL 编码使用

保留以下依赖（可能被其他模块使用，需确认）：
- `reqwest` — Tavily API 调用仍需要
- `html2text` — 仅 `web_common.rs` 和 `web_fetch.rs` 使用，迁移后不再需要。需确认是否有其他使用者
- `regex` — 需确认是否有其他使用者

如果 `html2text` 和 `regex` 仅被 web 模块使用，也一并移除。

---

### Task 6: Build and verify

- [ ] **Step 1: Run cargo build**

Run: `cargo build -p peri-middlewares`
Expected: BUILD SUCCEEDED

- [ ] **Step 2: Run tests**

Run: `cargo test -p peri-middlewares --lib -- web`
Expected: ALL TESTS PASS

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -p peri-middlewares`
Expected: NO WARNINGS

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: migrate WebSearch/WebFetch to Tavily-compatible backend

- Replace Bing HTML scraping with Tavily /search API
- Replace direct HTTP fetch with Tavily /extract API
- Remove all Bing-specific code (URL resolution, HTML parsing, etc.)
- Remove base64, urlencoding, html2text, regex dependencies
- Simplify web_common.rs to only keep credibility warning"
```

---

## Self-Review

### Spec coverage
- ✅ WebSearch → Tavily /search
- ✅ WebFetch → Tavily /extract
- ✅ Remove all Bing code
- ✅ Remove direct HTTP fetch from WebFetch
- ✅ web_common.rs simplified
- ✅ Tests updated
- ✅ Dependencies cleaned

### Placeholder scan
- No TBD/TODO found
- All code blocks contain complete implementations

### Type consistency
- `SearchResult` struct changed `snippet` → `content` (field rename consistent across `web_search.rs` and `web_test.rs`)
- `format_search_results` signature unchanged (`&[SearchResult]`)
- `WebFetchTool`/`WebSearchTool` names unchanged
- `WEB_CREDIBILITY_WARNING` still imported from `web_common`
