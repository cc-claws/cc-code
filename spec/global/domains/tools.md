# 工具系统 领域

## 领域综述

工具输出截断、持久化和通用工具基础设施。

## 核心流程

（后续填充）

## 技术方案总结

| 维度 | 选型 |
|------|------|
（后续填充）

---

## Issue 经验附录

### issue_2026-05-15-tool-output-truncation-with-disk-persist

**摘要:** 工具输出超长时截断 + 持久化磁盘 + 提示 Read 读取剩余内容
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 输出截断, 磁盘持久化, 工具输出, output_persist
**问题本质:** 截断后的工具输出直接丢弃，LLM 需要重新执行整个工具（浪费 token），无法获取完整数据
**通用模式:** 截断时完整数据写入临时文件，截断结果中附文件路径提示。LLM 可按需 Read 完整内容，避免重复工具调用
**技术决策:** 共享函数 `persist_truncated_output` 统一处理 7 个工具的截断持久化（Bash/Grep/Glob/FolderOperations/WebFetch/MCP ToolBridge/MCP ResourceTool），Read/WebSearch 排除
**涉及文件:** peri-middlewares/src/tools/output_persist.rs, terminal.rs, grep.rs, glob.rs, folder.rs, web_fetch.rs, tool_bridge.rs, resource_tool.rs
**CLAUDE.md 链接:** false

### issue_2026-05-23-migrate-web-tools-to-tavily-backend

**摘要:** WebSearch/WebFetch 后端迁移至 Tavily 兼容接口
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** Tavily, WebSearch, WebFetch, Bing, 后端迁移
**问题本质:** Bing HTML 解析不稳定且维护成本高，需迁移到统一 API 后端
**通用模式:** 外部依赖（搜索引擎）应通过统一 API 封装，避免直接解析 HTML；API 迁移时需完整移除旧实现防止代码残留
**涉及文件:** peri-middlewares/src/middleware/web_search.rs, peri-middlewares/src/middleware/web_fetch.rs, peri-middlewares/src/middleware/web_common.rs
**CLAUDE.md 链接:** false
