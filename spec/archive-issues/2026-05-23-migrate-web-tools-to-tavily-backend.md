> 归档于 2026-05-31，原路径 spec/issues/2026-05-23-migrate-web-tools-to-tavily-backend.md

# WebSearch/WebFetch 后端迁移至 Tavily 兼容接口

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-23
**更新日期**：2026-05-30

## 问题描述

当前 WebSearch 通过直接请求 Bing HTML 并解析搜索结果页实现，WebFetch 通过 reqwest 直连抓取 URL。需要将两者统一迁移到自托管的 Tavily 兼容后端（`https://tavily.claude-code-best.win`），移除所有 Bing 相关代码和原始 HTTP 抓取逻辑。接口兼容 Tavily API，无需 API Key。

## 现状

### WebSearch（`web_search.rs`）
- 直接请求 `https://www.bing.com/search?q=...`，使用浏览器 User-Agent 伪装
- 手动解析 Bing HTML 提取搜索结果（`extract_bing_results`、`resolve_bing_url` 等）
- 包含大量 Bing 特定逻辑：重定向 URL 解码、HTML 实体解码、Base64 解码、CSS 选择器匹配

### WebFetch（`web_fetch.rs`）
- 使用 reqwest 直连目标 URL 抓取内容
- 自行处理 HTML→文本转换、JSON 格式化、内容截断
- 包含 SSRF 防护（`web_common.rs` 的 `validate_url`）

### 共用模块（`web_common.rs`）
- `validate_url`：URL 安全校验 / SSRF 防护
- `html_to_text`：HTML 转纯文本
- `truncate_content`：按行数截断
- `WEB_CREDIBILITY_WARNING`：可信度警告前缀
- `MAX_RESPONSE_BYTES`：响应体大小上限

## 期望改进方向

1. **WebSearch**：调用 `POST https://tavily.claude-code-best.win/search`，传入 `query` 和 `max_results`，解析 JSON 响应
2. **WebFetch**：调用 `POST https://tavily.claude-code-best.win/extract`，传入 `urls`，解析 JSON 响应
3. **移除所有 Bing 特定代码**：`BROWSER_HEADERS`、`extract_bing_results`、`resolve_bing_url`、`decode_html_entities`、`strip_html_tags`、`extract_snippet` 等
4. **移除 WebFetch 的原始 reqwest 直连逻辑**
5. **评估 `web_common.rs` 保留项**：`WEB_CREDIBILITY_WARNING` 可能仍需保留，`validate_url`/`html_to_text`/`truncate_content` 视 Tavily 响应格式决定是否保留
6. **无需 API Key**：后端不需要认证

## 涉及文件

- `peri-middlewares/src/middleware/web_search.rs`（~317 行）—— Bing 搜索实现，需完全重写
- `peri-middlewares/src/middleware/web_fetch.rs`（~146 行）—— 原始 HTTP 抓取，需完全重写
- `peri-middlewares/src/middleware/web_common.rs`（~77 行）—— 共用工具函数，需评估保留
- `peri-middlewares/src/middleware/web.rs`（~41 行）—— WebMiddleware 注册，基本不变
- `peri-middlewares/src/middleware/web_test.rs` —— 测试文件，需同步更新
- `peri-middlewares/src/middleware/mod.rs` —— 模块声明，可能需调整
- `peri-middlewares/src/hitl/mod.rs` —— HITL 审批配置（WebFetch/WebSearch 在审批列表中）
- `peri-middlewares/src/tool_search/core_tools.rs` —— 核心工具列表声明
