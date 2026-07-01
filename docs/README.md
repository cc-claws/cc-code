# cc-code 文档索引

## 快速开始

- [项目主页](../README.md) — 项目介绍、安装指南、核心功能
- [贡献指南](../CONTRIBUTING.md) — 开发环境、编码规范、提交流程

## Crate 文档

| Crate | README | CLAUDE.md | 说明 |
|-------|--------|-----------|------|
| peri-agent | [README](../peri-agent/README.md) | [CLAUDE.md](../CLAUDE.md) | 核心 Agent 框架 |
| peri-middlewares | [README](../peri-middlewares/README.md) | [CLAUDE.md](../peri-middlewares/CLAUDE.md) | 中间件实现 |
| peri-tui | [README](../peri-tui/README.md) | [CLAUDE.md](../peri-tui/CLAUDE.md) | TUI 应用 |
| peri-acp | [README](../peri-acp/README.md) | - | ACP 服务层 |
| peri-widgets | [README](../peri-widgets/README.md) | - | Widget 组件库 |
| peri-lsp | [README](../peri-lsp/README.md) | - | LSP 客户端 |
| langfuse-client | [README](../langfuse-client/README.md) | - | 遥测客户端 |

## 架构文档

### 全局架构

- [概述](../spec/global/overview.md) — 项目整体架构
- [索引](../spec/global/index.md) — 文档索引
- [功能列表](../spec/global/features.md) — 功能特性
- [问题追踪](../spec/global/problems.md) — 已知问题

### 领域文档

- [Agent](../spec/global/domains/agent.md) — Agent 架构和执行流程
- [TUI](../spec/global/domains/tui.md) — TUI 架构和组件
- [消息管线](../spec/global/domains/message-pipeline.md) — 消息处理流程
- [系统提示词](../spec/global/domains/system-prompt.md) — 提示词管理
- [上下文压缩](../spec/global/domains/compact.md) — 压缩机制
- [同步](../spec/global/domains/sync.md) — 文件同步
- [工具](../spec/global/domains/tools.md) — 工具系统
- [TUI Widgets](../spec/global/domains/tui-widgets.md) — Widget 组件

### 产品需求文档 (PRD)

- [Shell 状态指示器](../spec/prd-shell-status-indicator.md)
- [推理渲染](../spec/prd/reasoning-markdown-rendering.md)
- [Web 搜索增强](../spec/prd/web-search-enhance.md)
- [屏幕选区](../spec/features/screen-selection-prd.md)

## Issue 文档

详细的 Issue 分析文档位于 [spec/issues/](../spec/issues/)，包含：

- 架构影响分析
- 复现条件
- 根因分析
- 修复方案

## Review 文档

代码审查记录位于 [spec/reviews/](../spec/reviews/)，包含：

- 架构评审
- 重构指南
- 实现计划

## 版本记录

- [CHANGELOG.md](../CHANGELOG.md) — 版本变更记录

## 外部资源

- [Agent Client Protocol](https://agentclientprotocol.com/) — ACP 协议规范
- [Ratatui](https://ratatui.rs) — TUI 框架
- [Langfuse](https://langfuse.com) — LLM 可观测性
- [rmcp](https://github.com/anthropics/rmcp) — Rust MCP 客户端

## 文档贡献

文档使用 Markdown 格式，遵循以下规范：

1. 每个 crate 都应有 README.md
2. 复杂模块应有 CLAUDE.md 作为开发指南
3. 陷阱和注意事项使用 `[TRAP]` 标记
4. 代码示例应可直接运行
5. 中英文混排时，英文单词前后加空格
