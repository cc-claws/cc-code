# Web 工具中间件设计

## 概述

新增 `WebMiddleware`，作为 `peri-middlewares` 的一个中间件，提供两个工具：

| 工具名 | 功能 | 核心依赖 |
|--------|------|----------|
| `WebFetch` | 抓取 URL 内容，HTML→Markdown 转换 | reqwest + html2text |
| `WebSearch` | 通过 Exa REST API 搜索网页 | reqwest |

**环境变量：**

| 变量 | 说明 |
|------|------|
| `EXA_API_KEY` | Exa 搜索 API 密钥（WebSearch 必需，缺失时返回错误提示） |

## 工具参数

### WebFetch

```json
{
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
}
```

### WebSearch

```json
{
  "type": "object",
  "properties": {
    "query": {
      "type": "string",
      "description": "搜索关键词"
    },
    "num_results": {
      "type": "integer",
      "description": "返回结果数量，默认 10，最大 20"
    }
  },
  "required": ["query"]
}
```

## 核心流程

### WebFetch 执行流程

```
LLM 调用 WebFetch(url, prompt?)
  ↓
1. URL 校验
   - 必须是 http:// 或 https:// 协议
   - 禁止内网 IP（127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12,
     192.168.0.0/16, 169.254.0.0/16, [::1], 0.0.0.0）
2. HITL 审批（默认需审批）
3. reqwest GET
   - 超时 30s，响应体上限 10MB
   - 最多跟随 5 次重定向
   - User-Agent: peri/1.0
4. Content-Type 判断：
   - text/html → html2text 转 Markdown
   - text/plain → 直接使用
   - application/json → pretty-print JSON
   - 其他二进制 → 返回 Content-Type + 大小信息
5. 截断：超过 2000 行截断尾部，附加 "[内容已截断]"
6. 如果有 prompt → 附在结果前作为指导说明
7. 返回文本结果
```

### WebSearch 执行流程

```
LLM 调用 WebSearch(query, num_results?)
  ↓
1. 检查 EXA_API_KEY 环境变量（缺失时返回错误提示）
2. HITL 审批（默认需审批）
3. reqwest POST https://api.exa.ai/search
   Headers:
     x-api-key: EXA_API_KEY
     Content-Type: application/json
   Body:
     {
       "query": "search terms",
       "numResults": 10,
       "type": "auto",
       "contents": { "text": true }
     }
   超时 30s
4. 解析响应：
   - 提取每条结果的 title + url + text 摘要
   - 截断单条 text 超过 500 字符
5. 格式化为 Markdown 编号列表：
   ```

## 搜索结果

   1. **标题** - url
      摘要文本...

   2. **标题** - url
      摘要文本...

   ```
6. 返回搜索结果文本
```

## 安全措施

| 措施 | 说明 |
|------|------|
| URL 协议限制 | 仅允许 `http://` 和 `https://` |
| 内网 IP 过滤 | 禁止 RFC 1918/loopback/link-local 地址段 |
| 响应大小限制 | 10MB 上限 |
| 请求超时 | WebFetch 30s，WebSearch 30s |
| 重定向限制 | 最多 5 次 |
| HITL 审批 | WebFetch/WebSearch 默认需审批 |

## 中间件集成

### 注册位置

中间件链中位于 `TerminalMiddleware` 之后、`TodoMiddleware` 之前：

```
...
5. FilesystemMiddleware
6. TerminalMiddleware
7. WebMiddleware          ← 新增
8. TodoMiddleware
...
```

### HITL 集成

在 `HumanInTheLoopMiddleware` 默认拦截清单中添加 `WebFetch`、`WebSearch`。

### 文件变更清单

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `peri-middlewares/src/middleware/web.rs` | 新增 | WebMiddleware + WebFetchTool + WebSearchTool（~300 行） |
| `peri-middlewares/src/middleware/mod.rs` | 修改 | 添加 `pub mod web;` |
| `peri-middlewares/src/hitl/mod.rs` | 修改 | 默认拦截清单添加 `WebFetch`、`WebSearch` |
| `peri-middlewares/Cargo.toml` | 修改 | 添加 `html2text` 依赖 |
| `peri-tui/src/app/agent.rs` | 修改 | 中间件链添加 `WebMiddleware::new()` |
| `CLAUDE.md` | 修改 | 中间件链文档更新 |

## 新增依赖

| crate | 用途 | 版本 |
|-------|------|------|
| `html2text` | HTML→Markdown 纯文本转换 | 最新稳定版 |

`reqwest` 已在 `peri-middlewares` 依赖中存在，无需新增。

## 代码结构

```rust
// peri-middlewares/src/middleware/web.rs

/// Web 中间件，提供 WebFetch 和 WebSearch 工具
pub struct WebMiddleware;

impl<S: State> Middleware<S> for WebMiddleware {
    fn name(&self) -> &str { "WebMiddleware" }
    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(WebFetchTool::new()),
            Box::new(WebSearchTool::new()),
        ]
    }
}

pub struct WebFetchTool { /* 无状态 */ }
pub struct WebSearchTool { /* 无状态 */ }

impl WebFetchTool {
    pub fn new() -> Self { Self }
    /// 校验 URL 安全性（协议 + 内网 IP 过滤）
    fn validate_url(url: &str) -> Result<reqwest::Url, String> { ... }
    /// 将 HTML 转换为 Markdown
    fn html_to_markdown(html: &str) -> String { ... }
}

impl WebSearchTool {
    pub fn new() -> Self { Self }
    /// 调用 Exa Search API
    async fn exa_search(query: &str, num_results: usize) -> Result<Vec<SearchResult>, String> { ... }
}
```

## 不做的事（YAGNI）

- **响应缓存**：首次实现不加缓存，后续按需添加
- **LLM 摘要**：不做 Haiku 二次摘要（claude-code 有 `applyPromptToMarkdown`），直接返回原文
- **域名白名单**：不做预审批域名列表（claude-code 有 130+ 域名白名单），所有请求统一走 HITL
- **多搜索后端**：仅支持 Exa，不做 Bing/Brave 适配器
- **OAuth**：Exa 仅需 API Key，无需 OAuth 流程
