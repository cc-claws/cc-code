# 项目概述

## 项目目标

CC Code 是一个用 Rust 构建的终端 AI 编程 Agent，提供 AI 编程助手的完整工作流（ReAct 推理、工具调用、HITL 审批、子 Agent 委派），开箱即用、性能可靠。

核心价值：

- **高兼容**：完全复用已有的 `.claude/agents/`、`.claude/skills/`、`CLAUDE.md` / `AGENTS.md` 配置，零迁移成本
- **可插拔**：中间件模式，按需组合文件系统、终端、HITL、子 Agent 等能力
- **生产可用**：异步运行时、线程持久化，面向实际部署

## 核心用户

**应用开发者**：希望在自己的产品中集成 AI Agent 能力，例如构建代码辅助工具、自动化运维系统、交互式命令行助手。

**工具作者**：需要快速原型验证 Agent 行为，通过 TUI 交互界面实时调试 ReAct 循环和工具调用链路。

## 系统边界

系统与以下外部服务交互：

- **Anthropic API**：Claude 系列模型，支持扩展思考（Extended Thinking）和 Prompt Cache
- **OpenAI 兼容接口**：任意支持 `/chat/completions` 接口的服务（OpenAI、DeepSeek、本地模型等）
- **SQLite（本地）**：会话线程持久化，路径 `~/.cc-code/threads/threads.db`
- **文件系统（本地）**：Agent 工具对本地文件的读写操作
- **Shell / 终端**：`bash` 工具执行系统命令

## 核心业务流程

### 1. ReAct 推理循环

用户提交消息 → 中间件 `before_agent` 预处理 → LLM 生成推理 → 循环执行工具调用（每轮调用 before/after 钩子）→ 获取最终答案 → 中间件 `after_agent` 后处理 → 返回结果。

### 2. HITL 审批流程

敏感工具（`bash`、写文件、编辑文件等）调用前，`HitlMiddleware` 拦截 → 发送 `ApprovalNeeded` 事件到 TUI → 用户选择 Approve / Edit / Reject / Respond → 结果通过 oneshot channel 返回给 Agent 继续执行。

### 3. SubAgent 委派

父 Agent 调用 `Agent` 工具 → 读取 `.claude/agents/{agent_id}.md` 定义 → 创建子 Agent 实例（独立 LLM、继承或过滤工具集）→ 子 Agent 恒后台执行子任务 → 结果经完成通知回流父 Agent 并自动续接。

### 4. 上下文压缩

Agent 运行中 Token 累积达到各自阈值时触发自动压缩：Micro-compact 在 70% 触发（零 API 调用，清除可压缩工具结果和图片/文档），Full Compact 在 85% 触发（调用 LLM 生成自由 Markdown 摘要并在原会话内替换历史）。压缩后重新注入最近读取文件和激活 Skills。

### 5. 权限模式

2 档权限模式控制工具调用审批策略：**Auto**（默认；先跑零成本的确定性层——硬黑名单/人写规则/只读白名单，再走 Jev 语义门，只有判不准时才弹一次窗）与 **Bypass**（全部放行）。Shift+Tab 在两者间循环，状态栏实时显示当前模式；未显式开启免审批时一律为 Auto（fail-closed）。

---
*最后更新: 2026-10-22 — 对照 v0.6.182 代码维护：删除已移除的 Relay 配置同步与 OpenTelemetry 描述，修正产品定位、数据路径与压缩阈值口径*
