# 上下文压缩增强 领域

## 领域综述

上下文压缩增强领域负责 Micro-compact 和 Full Compact 策略的全面增强，包括可压缩工具白名单、9 段结构化摘要模板和压缩后重新注入。

核心职责：
- Micro-compact 可压缩工具白名单 + 时间衰减清除策略
- Full Compact 9 段结构化摘要模板对齐 Claude Code
- 压缩后重新注入最近读取文件和激活 Skills
- 工具对完整性保护确保 tool_use + tool_result 不被拆开
- CompactConfig 通过 settings.json 配置，环境变量可覆盖

## 核心流程

### Micro-compact 流程

```
触发条件: context_usage 70%-85%
  → 白名单工具结果可压缩（bash/read/glob/search/write/edit）
  → 时间衰减: 超过 micro_compact_stale_steps(5) 步的旧结果
  → 图片替换: [image] 或 [compacted: image ~{tokens} tokens]
  → 文档替换: [document] 或 [compacted: document ~{tokens} tokens]
  → 工具对保护: adjust_index_to_preserve_invariants() 确保 tool_use + tool_result 不拆开
```

### Full Compact 流程

```
触发条件: context_usage > 85%
  → 9 段结构化摘要模板:
      Primary Request → Technical Concepts → Files → Errors & Fixes →
      Problem Solving → User Messages → Pending Tasks → Current Work → Next Step
  → 调用 LLM 生成摘要
  → 移除 <analysis> 块，保留 <summary>
  → PTL 降级重试: 按消息步数组逐步删除最旧组，最多重试 3 次
  → re_inject: 提取最近文件路径 + Skills → System 消息重新注入
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| Micro-compact | 可压缩白名单 + 时间衰减 + 图片/文档替换 + 工具对保护 |
| Full Compact | 9 段摘要模板 + LLM 调用 + PTL 降级重试 |
| 重新注入 | extract_recent_files() + extract_skills_paths() → System 消息 |
| 配置 | CompactConfig 支持环境变量覆盖 |
| 核心层分离 | 纯消息操作在核心层，TUI 层仅触发和展示 |

## Feature 附录

### feature_20260428_F001_compact-redesign
**摘要:** 全面增强 Micro/Full Compact 策略与压缩后重新注入
**关键决策:**
- Micro-compact 引入可压缩工具白名单 + 时间衰减清除策略
- Full Compact 采用 9 段结构化摘要模板对齐 Claude Code
- 压缩后重新注入最近读取文件和激活 Skills（System 消息形式）
- 工具对完整性保护：确保 tool_use 和 tool_result 不被拆开
- PTL 降级重试：按消息步数组逐步删除最旧组，最多重试 3 次
- CompactConfig 通过 settings.json 配置，环境变量可覆盖
- 核心层实现纯消息操作，TUI 层仅负责触发和 UI 展示
**归档:** [链接](../../archive/feature_20260428_F001_compact-redesign/)
**归档日期:** 2026-04-30

---

## Issue 经验附录

### issue_2026-05-11-auto-compact-no-resubmit
**摘要:** Auto Compact 后 Agent 未自动 Resubmit 继续执行
**状态:** Fixed + Verify
**归档日期:** 2026-05-13
**关键词:** last_user_input, auto-compact, resubmit, 状态保留
**问题本质:** last_user_input 在 compact 异步执行期间可能为 None 或被覆盖，导致 handle_compact_done 的 resubmit 被静默跳过，无任何日志或用户提示
**通用模式:** 跨异步操作的状态依赖（如 compact 后需要原始输入 resubmit）应在操作开始时保存到独立字段，防止异步执行期间被清理。静默跳过关键操作（如 resubmit）是危险的，应至少记录 warn 日志
**技术决策:** compact 开始时保存 last_user_input 到独立字段，防止异步期间被清理
**涉及文件:** peri-tui/src/app/agent_compact.rs, peri-tui/src/app/agent_submit.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/agent_comm.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-compact-auto-continue-scenarios
**摘要:** Compact 自动继续功能在不应触发的场景（手动 /compact、Done 后 auto-compact）下仍然 resubmit
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** auto-continue, compact 触发来源, resubmit 控制, instructions 参数
**问题本质:** handle_compact_done 的 resubmit 逻辑不区分 compact 触发来源——手动 /compact 和 Done 后 auto-compact 也被错误地 resubmit。用户手动压缩后期望停下来查看结果，agent 完成任务后 compact 再用原始输入重新执行没有意义。
**通用模式:** 异步操作的触发来源（auto vs manual）需要作为上下文传递到完成后处理逻辑。用 instructions 参数区分来源，通过独立 flag（compact_should_resubmit）控制后续行为。两个合理的 resubmit 场景（auto-compact 在 agent 执行中、后台任务完成后）和两个不合理的场景（手动 compact、Done 后 compact）需要精确区分。
**涉及文件:** peri-tui/src/app/agent_compact.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/agent_comm.rs
**CLAUDE.md 链接:** false

### issue_2026-05-20-compact-command-not-triggering
**摘要:** /compact 命令作为普通文本发给 LLM，未触发压缩
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** /compact 命令, ACP compact 通道, loading spinner, session 同步
**问题本质:** /compact 命令处理器未接入 ACP compact 管道，将命令文本当作普通用户消息发送给 LLM；compact 期间缺少 loading 状态和用户可见错误反馈
**通用模式:** 所有 TUI 命令必须通过正确的 ACP 协议通道（compact/set_model/set_mode 等）触发操作，不能将命令文本作为普通消息提交；compact 这类异步操作需要完整的 UI 状态管理（loading spinner + 错误反馈）
**架构影响:** Compact 触发路径统一收敛到 ACP compact 通道（acp_client.compact() → ACP Server → compact_runner），命令处理器和 auto-compact 虽触发点不同但最终汇合
**技术决策:** TUI 命令 → ACP client → ACP server → compact runner 的分层架构，命令处理器不直接操作 compact 逻辑
**涉及文件:** peri-tui/src/command/session/compact.rs, peri-tui/src/app/agent_compact.rs, peri-tui/src/app/agent_ops/polling.rs, peri-tui/src/app/agent_comm.rs, peri-tui/src/app/thread_ops.rs
**CLAUDE.md 链接:** true

### issue_2026-05-20-auto-compact-empty-messages-400
**摘要:** Auto compact 后 LLM 请求 messages 为空导致 400 错误
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** compact messages 为空, BaseMessage::system vs human, LLM 适配器提取, DeepSeek 400
**问题本质:** Compact 摘要被放入 BaseMessage::system()，LLM 适配器（messages_to_json/messages_to_anthropic）将 System 消息提取到 system 字段不进入 messages 数组，导致发给 API 的 messages 数组为空
**通用模式:** 发给 LLM API 的 messages 数组必须始终包含至少一条非 System 消息（Human 或 Ai）；任何向消息列表插入的内容如果可能被 LLM 适配器提取到顶层字段（system、tools 等），必须验证剩余 messages 数组非空
**架构影响:** Compact 架构从「外层 loop + resubmit」改为「CompactMiddleware 作为 before_model 钩子在 ReAct 循环内原地处理」，消除了 compact 后独立 LLM 调用的脆弱性
**技术决策:** CompactMiddleware 替代 compact_runner 的 before_model 钩子模式，摘要始终使用 BaseMessage::human() 确保 LLM 适配器提取 System 后 messages 数组有效
**涉及文件:** peri-middlewares/src/compact_middleware.rs, peri-acp/src/session/compact_runner.rs, peri-acp/src/session/executor.rs, peri-tui/src/acp_server/compact.rs
**CLAUDE.md 链接:** true

### issue_2026-05-26-manual-compact-long-loading-skeleton
**摘要:** 手动 /compact 后聊天区域长时间显示 loading 骨架屏（30s+）
**状态:** Fixed
**归档日期:** 2026-05-26
**关键词:** compact loading, set_loading, manual vs auto compact
**问题本质:** handle_compact_completed() 在 full compact 路径故意不调 set_loading(false)——设计对 auto-compact 正确（executor 循环继续→Done 清除），对手动 compact 错误（独立操作无 Done 事件）
**通用模式:** 同一处理函数服务于两条执行路径时，必须区分路径语义。auto-compact 嵌套在 ReAct 循环内（有后续 Done/Error），manual compact 是独立操作（需自行清理 loading）。缺少路径标志导致状态泄漏。
**涉及文件:** peri-tui/src/app/agent_compact.rs, peri-tui/src/acp_server/compact.rs, peri-tui/src/app/agent_comm.rs, peri-tui/src/command/session/compact.rs
**CLAUDE.md 链接:** false

### issue_2026-05-23-micro-compact-repeated-triggering
**摘要:** Micro Compact 重复触发，每轮工具调用后都显示"自动清理"通知
**状态:** verify
**归档日期:** 2026-05-27
**关键词:** micro compact, repeated triggering, once-per-prompt guard, AtomicBool
**问题本质:** CompactMiddleware 缺少 once-per-prompt 守卫。micro compact 压缩量 < 新增量，永远降不到 70% 阈值以下，每轮都重复触发
**通用模式:** 有副作用的 per-prompt 操作（如 compact、通知）必须加 once-per-prompt 守卫。同一 execute_prompt 内只应触发一次 micro compact，之后由 full compact 接管
**技术决策:** 用 `AtomicBool` 做守卫——每次 execute_prompt 创建新 CompactMiddleware 实例，标志天然 per-prompt 作用域
**涉及文件:** peri-middlewares/src/compact_middleware.rs, peri-middlewares/src/compact_middleware_test.rs
**CLAUDE.md 链接:** true

---

## 相关 Feature
- → [token-tracking.md](./token-tracking.md) — Token 追踪触发压缩
- → [tui.md](./tui.md) — TUI /compact 命令
