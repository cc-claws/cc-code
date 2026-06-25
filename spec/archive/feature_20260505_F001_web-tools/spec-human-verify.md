# Web 工具中间件 人工验收清单

**生成时间:** 2026-05-05 12:00
**关联计划:** spec/feature_20260505_F001_web-tools/spec-plan.md
**关联设计:** spec/feature_20260505_F001_web-tools/spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链: `cargo --version`
- [x] [AUTO] 编译项目: `cargo build`

### 测试数据准备
- 无需额外测试数据（单元测试自包含，WebSearch 使用 mock 场景，无需真实 EXA_API_KEY）

---

## 验收项目

### 场景 1：构建与编译

#### - [x] 1.1 peri-middlewares 编译通过
- **来源:** spec-plan.md Task 4 / spec-design.md §文件变更清单
- **目的:** 确认新增 web.rs 及依赖集成无编译错误
- **操作步骤:**
  1. [A] `cargo build -p peri-middlewares 2>&1 | tail -5` → 期望包含: Finished

---

#### - [x] 1.2 peri-tui 编译通过
- **来源:** spec-plan.md Task 3 / spec-design.md §中间件集成
- **目的:** 确认 TUI agent 中间件链注册无误
- **操作步骤:**
  1. [A] `cargo build -p peri-tui 2>&1 | tail -5` → 期望包含: Finished

---

### 场景 2：WebFetch URL 安全校验（SSRF 防护）

#### - [x] 2.1 URL 校验拦截非法协议与内网地址
- **来源:** spec-plan.md Task 1 / spec-design.md §安全措施
- **目的:** 确认 SSRF 防护覆盖 FTP/localhost/私有IP/链路本地
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_validate_url 2>&1 | grep -E '^test '`
     → 期望包含: test_validate_url_rejects_ftp ... ok
     → 期望包含: test_validate_url_rejects_localhost ... ok
     → 期望包含: test_validate_url_rejects_private_ip ... ok
     → 期望包含: test_validate_url_rejects_link_local ... ok
     → 期望包含: test_validate_url_accepts_https ... ok
     → 期望包含: test_validate_url_rejects_invalid_url ... ok

---

### 场景 3：WebFetch 内容处理

#### - [x] 3.1 HTML 转换与内容截断
- **来源:** spec-plan.md Task 1 / spec-design.md §WebFetch 执行流程
- **目的:** 确认 HTML→文本、截断逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_html_to_text_basic 2>&1 | grep 'test '` → 期望包含: test_html_to_text_basic ... ok
  2. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_truncate_content 2>&1 | grep 'test '` → 期望包含: test_truncate_content_no_truncation ... ok
     → 期望包含: test_truncate_content_with_truncation ... ok

---

#### - [x] 3.2 WebFetch 工具注册正确
- **来源:** spec-plan.md Task 1 / spec-design.md §工具参数
- **目的:** 确认工具名和参数 schema 符合设计
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_tool_name_is_web_fetch 2>&1 | grep 'test '` → 期望包含: test_tool_name_is_web_fetch ... ok
  2. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_tool_parameters_required_url 2>&1 | grep 'test '` → 期望包含: test_tool_parameters_required_url ... ok

---

### 场景 4：WebSearch 搜索与格式化

#### - [x] 4.1 搜索结果 Markdown 格式化
- **来源:** spec-plan.md Task 2 / spec-design.md §WebSearch 执行流程
- **目的:** 确认空结果/正常/截断/无摘要四种场景格式化正确
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_format_search 2>&1 | grep 'test '` → 期望包含: test_format_search_results_empty ... ok
     → 期望包含: test_format_search_results_with_snippet ... ok
     → 期望包含: test_format_search_results_text_truncation ... ok
     → 期望包含: test_format_search_results_no_snippet ... ok

---

#### - [x] 4.2 WebSearch 工具注册与错误处理
- **来源:** spec-plan.md Task 2 / spec-design.md §工具参数
- **目的:** 确认工具名、参数 schema、缺失 query 报错
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_websearch_name 2>&1 | grep 'test '` → 期望包含: test_websearch_name ... ok
  2. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_websearch_parameters_required 2>&1 | grep 'test '` → 期望包含: test_websearch_parameters_required ... ok
  3. [A] `cargo test -p peri-middlewares --lib -- middleware::web::tests::test_websearch_missing_query 2>&1 | grep 'test '` → 期望包含: test_websearch_missing_query ... ok

---

### 场景 5：HITL 审批集成

#### - [x] 5.1 WebFetch/WebSearch 默认需审批
- **来源:** spec-plan.md Task 3 / spec-design.md §HITL 集成
- **目的:** 确认两个 Web 工具在默认审批清单中
- **操作步骤:**
  1. [A] `cargo test -p peri-middlewares --lib -- hitl::tests::test_default_requires_approval 2>&1 | grep 'test '` → 期望包含: test_default_requires_approval ... ok

---

### 场景 6：中间件注册与导出

#### - [x] 6.1 WebMiddleware re-export 和 prelude 导出
- **来源:** spec-plan.md Task 3 / spec-design.md §中间件集成
- **目的:** 确认 WebMiddleware 通过 mod.rs 和 lib.rs 正确导出
- **操作步骤:**
  1. [A] `grep 'pub use web::WebMiddleware' peri-middlewares/src/middleware/mod.rs` → 期望包含: pub use web::WebMiddleware;
  2. [A] `grep 'WebMiddleware' peri-middlewares/src/lib.rs` → 期望包含: WebMiddleware

---

#### - [x] 6.2 WebMiddleware 注册到 TUI 中间件链
- **来源:** spec-plan.md Task 3 / spec-design.md §注册位置
- **目的:** 确认 WebMiddleware 在 agent.rs 中间件链中
- **操作步骤:**
  1. [A] `grep 'WebMiddleware' peri-tui/src/app/agent.rs` → 期望包含: WebMiddleware::new()

---

#### - [x] 6.3 CLAUDE.md 文档同步
- **来源:** spec-plan.md Task 3 / spec-design.md §文件变更清单
- **目的:** 确认中间件链和 HITL 审批文档已更新
- **操作步骤:**
  1. [A] `grep 'WebMiddleware' CLAUDE.md` → 期望包含: WebMiddleware
  2. [A] `grep -E 'WebFetch.*WebSearch' CLAUDE.md` → 期望包含: WebFetch 和 WebSearch

---

### 场景 7：全量回归

#### - [x] 7.1 全 workspace 测试通过
- **来源:** spec-plan.md Task 4 / spec-design.md §概述
- **目的:** 确认无回归，所有测试通过
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -30` → 期望包含: test result: ok
     → 期望精确: 无 FAILED

---

## 验收后清理

- 无需清理（验收不启动后台服务）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | peri-middlewares 编译 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | peri-tui 编译 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | URL 校验拦截非法协议与内网 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | HTML 转换与内容截断 | 2 | 0 | ✅ |
| 场景 3 | 3.2 | WebFetch 工具注册 | 2 | 0 | ✅ |
| 场景 4 | 4.1 | 搜索结果格式化 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | WebSearch 注册与错误处理 | 3 | 0 | ✅ |
| 场景 5 | 5.1 | HITL 审批集成 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | re-export 和 prelude | 2 | 0 | ✅ |
| 场景 6 | 6.2 | TUI 中间件链注册 | 1 | 0 | ✅ |
| 场景 6 | 6.3 | CLAUDE.md 文档同步 | 2 | 0 | ✅ |
| 场景 7 | 7.1 | 全 workspace 测试 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
