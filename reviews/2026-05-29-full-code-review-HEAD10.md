# 代码审查报告：HEAD~10..HEAD

**日期**：2026-05-29
**审查范围**：最近 10 个提交（`56a782e..106fa0d`）
**变更规模**：355 个文件，+10,222 / -3,918 行
**审查方法**：6 维度并行审查（正确性/安全性/架构/性能/测试/回归） + 对抗式验证
**审查统计**：43 个子 agent，35 个发现提交验证，**30 个确认**，5 个验证失败（子 agent 未完成结构化输出）

---

## 总览

| 严重级别 | 数量 | 说明 |
|----------|------|------|
| Critical | 0 | 无致命问题 |
| High | 3 | 需优先修复 |
| Medium | 11 | 建议修复 |
| Low | 16 | 可后续处理 |

**整体评估**：项目整体代码质量良好，架构分层清晰。ACP 重构方向正确，但新增模块测试覆盖严重不足是最大风险——4 个新模块（command/、interaction/、mapper 21 变体、acp_bridge 230 行）零测试覆盖。

---

## 审查的提交列表

| 提交 | 说明 |
|------|------|
| `106fa0d` | refactor: migrate SetupWizard tests from headless to unit tests |
| `e5fbd42` | refactor: migrate render_view_model tests from headless to message_render_test |
| `4930032` | fix: remove unused Duration import in headless.rs |
| `5fa0bf9` | refactor: move markdown tests from headless to markdown/mod_test.rs |
| `56a782e` | Feature/acp improve (#16) |
| `9223cc4` | Merge branch 'fix/llm-error-amnesia' into feature/acp-improve |
| `e530ff7` | chore: archive 8 Fixed issues |
| `63ab4bd` | fix: 无 CC 环境下启动时自动创建插件目录结构 |
| `3aa5433` | fix: ToolEnd 事件经 ACP bridge 后工具名丢失 |
| `5dead19` | fix: SSE 流式解析跨 chunk UTF-8 截断产生乱码 |

---

## High 级别（3 个）

### H1: mapper_test.rs 仅覆盖 4/21 个 ExecutorEvent 变体

- **文件**：`peri-acp/src/event/mapper_test.rs`
- **维度**：测试覆盖
- **调整后严重级别**：High（维持）
- **状态**：✅ 确认

**问题**：`mapper.rs` 的 `map_event()` 处理 21 个 ExecutorEvent 变体，但 `mapper_test.rs` 仅测试了 6 个（LlmCallEnd×3、ContextWarning、LlmRetrying、ToolEnd）。

**未覆盖的关键路径**：
- **Category 1（SessionUpdate 产生）**：TextChunk（核心流式文本）、ToolStart（工具调用创建）、TodoUpdate（计划更新）、AiReasoning（推理）—— 完全零测试
- **Category 3（TUI-only）**：21 个变体中仅 2 个有 forward_to_tui=true 断言，其余 9 个无断言
- **Filtered（无输出）**：StepDone、MessageAdded、LlmCallStart、SessionEnded 无空输出断言

**风险**：TextChunk 是核心流式文本路径，ToolStart 决定工具分类。回归将导致静默文本丢失，无测试捕获。

**建议**：至少为 TextChunk、ToolStart、TodoUpdate、AiReasoning 各添加 1 个正向测试，为 Category 3 添加 forward_to_tui=true 断言，为 Filtered 变体添加空输出断言。

---

### H2: ACP Slash Command 模块完全没有单元测试

- **文件**：`peri-acp/src/session/command/`（mod.rs + clear.rs + compact.rs）
- **维度**：测试覆盖
- **调整后严重级别**：High（维持）
- **状态**：✅ 确认

**问题**：command/ 模块（约 400 行）包含 CommandRegistry（注册/查找/别名匹配）、CommandKind 分类、ClearCommand、CompactCommand，但零测试覆盖。

**未覆盖的关键逻辑**：
- `CommandRegistry::find()`：别名匹配、前缀匹配、空字符串边界、双斜杠、带参数的命令
- `ClearCommand::execute()`：发送 CompactCompleted（空 messages）事件
- `CompactCommand::execute()`：空历史/无 model/full_compact 错误 3 个 early-return 路径 + 成功路径
- `extract_file_info()` / `extract_skill_names()`：字符串解析纯函数

**风险**：该模块是用户输入 `/command` 的第一道拦截层（executor.rs:169-202），每个 slash command 都经过此处。

**建议**：新建 `command/mod_test.rs` 测试 CommandRegistry 查找 + 新建 `command/compact_test.rs` 测试命令 execute 逻辑（MockEventSink）。

---

### H3: interaction 模块（channel_broker + multiplex）没有任何测试

- **文件**：`peri-agent/src/interaction/`（channel_broker.rs + multiplex.rs + channel_state.rs）
- **维度**：测试覆盖
- **调整后严重级别**：Medium→High（原报告 Medium，因覆盖面大上调）
- **状态**：✅ 确认

**问题**：interaction 模块（约 313 行）涵盖 MCP Channel 权限审批、多 broker 竞速、状态管理，但零测试。

**未覆盖的关键逻辑**：
- `ChannelBroker`：无授权 server → 全部 Reject、5 分钟超时、oneshot 注册/清理
- `MultiplexBroker`：空 broker 列表、单 broker 快速路径、双 broker 竞速（首响应胜出）
- `ChannelState`：authorize/revoke/close_all、register/unregister session

**缓解因素**：trait 层面的 `UserInteractionBroker` 通过 `ask_user_tool_test.rs` 的 12 个 mock 测试间接覆盖了类型契约。代码量小，逻辑简单。

**建议**：新建 `interaction/channel_broker_test.rs` 和 `interaction/multiplex_test.rs`。

---

## Medium 级别（11 个）

### M1: extract_file_info/extract_skill_names 尾部 `]` 解析 bug

- **文件**：`peri-acp/src/session/command/compact.rs:183,202` + `peri-middlewares/src/compact_middleware.rs:112,131`
- **维度**：架构（验证后确认为实际 bug）
- **调整后严重级别**：Critical→Medium（仅影响显示，不影响逻辑）
- **状态**：✅ 确认

**问题**：`re_inject.rs` 生成的 System 消息格式为 `[最近读取的文件: /path]\ncontent`，`strip_prefix` 去掉前缀后第一行为 `/path]`，包含尾部 `]`。`extract_skill_names` 同理。

**影响**：`CompactFileInfo.path` 和 skill 名称带多余 `]` 字符，影响 TUI compact 通知标签显示（如 `Read /tmp/test.rs]`）。

**修复**：在 `strip_prefix` 后对 first_line 添加 `.trim_end_matches(']')`。同时将前缀字符串提取为 `re_inject` 模块的共享常量。需修改 2 处生产代码（compact.rs + compact_middleware.rs）。

**根因**：跨模块隐式协议——`re_inject` 的消息格式变化会导致 `extract` 逻辑静默失败，无共享常量或结构化数据格式。

---

### M2: rawOutput 类型不匹配——mapper 产生 JSON Value，acp_bridge 用 as_str() 读取

- **文件**：`peri-tui/src/app/agent_ops/acp_bridge.rs:152-155`
- **维度**：架构（由验证 agent 发现）
- **调整后严重级别**：Medium
- **状态**：✅ 确认（验证过程中额外发现）

**问题**：`mapper.rs:131-134` 的 `raw_output` 是 `Option<serde_json::Value>`，当工具输出是有效 JSON 对象/数组时为 `Value::Object/Array`。`acp_bridge.rs:152-155` 用 `.as_str()` 读取，对非字符串 JSON 值返回 `None`，导致 `raw_output` 静默变为空字符串。

**影响**：MCP 工具、SearchExtraTools 等返回结构化 JSON 的场景，工具输出在 TUI 中显示为空。

**修复**：将 `.as_str()` 替换为能处理 JSON Object/Array/Number 的辅助函数（如 `value_to_display_string`）。

---

### M3: 事件映射双路径一致性风险（mapper.rs vs acp_bridge.rs）

- **文件**：`peri-acp/src/event/mapper.rs` + `peri-tui/src/app/agent_ops/acp_bridge.rs`
- **维度**：架构
- **调整后严重级别**：Medium（维持）
- **状态**：✅ 确认

**问题**：Category 1 事件（TextChunk/AiReasoning/ToolStart/ToolEnd/TodoUpdate/LlmCallEnd）有两条独立映射路径：
1. `mapper.rs`：ExecutorEvent → 序列化为 ACP SessionUpdate JSON
2. `acp_bridge.rs`：手动解析 JSON → 重建 AgentEvent

新增 ExecutorEvent 变体时必须同时更新两处，字段映射必须完全一致。commit `3aa5433` 已证明此类 bug 曾发生（ToolEnd 工具名丢失）。

**额外发现**：TodoUpdate 的 `active_form` 字段在传输中丢失——`mapper.rs` 的 `PlanEntry::new()` 不包含该字段，`acp_bridge.rs` 硬编码 `active_form: None`。

**风险**：未来新增变体时两处不同步的概率较高。

**建议**：长期考虑在 MappedEvent 中保留原始 ExecutorEvent 引用，TUI 侧直接使用而非从 JSON 重新解析。

---

### M4: MultiplexBroker 竞速后 spawned task 泄漏无取消机制

- **文件**：`peri-agent/src/interaction/multiplex.rs:30-52`
- **维度**：正确性 + 性能
- **调整后严重级别**：High→Medium（不持有跨 await 锁，资源有限）
- **状态**：✅ 确认

**问题**：`request()` 为每个子 broker spawn 一个 tokio task，收到第一个响应后立即返回，剩余 task 在后台继续运行（最多 5 分钟，ChannelBroker 超时）。

**实际影响**：
- 不持有跨 await 锁（`parking_lot::RwLock` 仅在 insert/remove 时短暂持有）
- 资源占用有限（Arc 引用 + oneshot channel）
- 5 分钟后自动清理
- 会产生无效的 MCP 通知和 stale pending_permissions 条目

**修复**：使用 `CancellationToken` 或 `JoinSet` + `abort_all()` 管理竞速任务生命周期。

---

### M5: acp_bridge.rs 230+ 行事件转换逻辑零测试

- **文件**：`peri-tui/src/app/agent_ops/acp_bridge.rs:62-287`
- **维度**：测试覆盖
- **调整后严重级别**：Medium（维持）
- **状态**：✅ 确认

**问题**：`handle_session_update_peri()` 处理 8 种 session update 类型的 JSON→AgentEvent 转换，230 行手动 JSON 字段提取逻辑（and_then/unwrap_or 链），可静默产生错误值，但零测试。

**建议**：将 JSON→AgentEvent 解析逻辑提取为独立纯函数，在 `agent_ops/acp_bridge_test.rs` 中为 8 种 update_type 各添加测试。

---

### M6: rebuild() 中 compute_wrapped_height 和 build_wrap_map 重复计算

- **文件**：`peri-tui/src/ui/render_thread.rs:354-355`
- **维度**：性能
- **调整后严重级别**：Medium（维持）
- **状态**：✅ 确认

**问题**：`rebuild()` 在 line 354 调用 `compute_wrapped_height` 计算总视觉行数，line 355 调用 `build_wrap_map` 内部对每个 Line 重复相同的 `Paragraph::line_count` 计算。两次独立的 `WordWrapper` 遍历。

**影响**：在流式渲染（100ms 节流）和 Resize 场景下加倍 wrap 计算开销。对数百行消息列表可能造成数十毫秒额外延迟。

**修复**：移除 `compute_wrapped_height` 调用，从 `build_wrap_map` 返回的 `wrap_map` 最后元素的 `visual_row_end` 推导 `total_lines`。

---

### M7: available_commands_update 处理直接索引 sessions[active] 可能 panic

- **文件**：`peri-tui/src/app/agent_ops/acp_bridge.rs:267-278`
- **维度**：正确性
- **调整后严重级别**：High→Medium（验证未完成，保留为 Medium）
- **状态**：⚠️ 验证 agent 未完成（被归为 Medium）

**问题**：`handle_session_update_peri()` 的 `available_commands_update` 分支直接使用 `self.session_mgr.sessions[self.session_mgr.active]` 访问 session，如果 ACP Server 在 session/new 完成前推送通知，索引可能越界导致 panic。

**修复**：添加边界检查：`sessions.get_mut(active)` 并在 None 时 early return + warn 日志。

---

### M8: StdioEventSink 静默丢弃 forward_to_tui 事件无任何日志

- **文件**：`peri-acp/src/session/event_sink.rs:141-154`
- **维度**：正确性
- **调整后严重级别**：Medium（维持）
- **状态**：⚠️ 验证 agent 未完成

**问题**：`StdioEventSink::push_event()` 只处理 `m.updates`，完全忽略 `m.forward_to_tui` 字段。CompactStarted/Completed/Error、SubagentStarted/Stopped、ContextWarning、LlmRetrying 等事件在 stdio 路径被静默丢弃。

**影响**：外部 IDE 客户端无法感知 compact 状态变化、SubAgent 生命周期等。这是设计选择（Category③ 定义为 TUI-only），但丢弃时无日志增加调试难度。

**建议**：至少在丢弃时添加 `debug!` 级别日志。

---

### M9: SSE 解析器 push() 每次调用产生 2 次不必要的 Vec 分配

- **文件**：`peri-agent/src/llm/sse.rs:61-65`
- **维度**：性能
- **调整后严重级别**：Medium→Low（数据量小、调用频率中等）
- **状态**：✅ 确认

**问题**：`to_vec()` 创建 remaining + `from_utf8_lossy().into_owned()` 创建 text String。可用 `split_off` + `String::from_utf8` 降为 0-1 次分配。

**实际影响**：SSE chunk 通常几百字节，每次响应 10-50 个 chunk，远小于 LLM API 延迟（秒级）。

---

### M10: CompactCommand 缺少空历史/LLM 失败等边界条件的单元测试

- **文件**：`peri-acp/src/session/command/compact.rs`
- **维度**：测试覆盖
- **调整后严重级别**：Medium（维持）
- **状态**：✅ 确认

**问题**：`execute()` 有 4 个代码路径（空历史→Error、无 model→Error、full_compact 失败→Error、成功路径），底层函数有测试但命令集成层零覆盖。

---

### M11: 每轮 prompt 重新读取环境变量计算 compact_config（三层冗余）

- **文件**：`peri-acp/src/session/executor.rs:143-148` + `builder.rs:349-350` + `compact_middleware.rs:88-91`
- **维度**：性能 + 代码质量
- **调整后严重级别**：Medium→Low（单次 ~100ns，无性能影响）
- **状态**：✅ 确认

**问题**：executor.rs 5 次 env var 读取 + builder.rs 3 次重复 + compact_middleware.rs 每轮 ReAct 迭代 2 次。总计每轮 10+ 次，500 轮循环可达 1010 次。逻辑重复导致维护风险（若 env var 名称变更需同步 3 处）。

---

## Low 级别（16 个）

### L1: SSE 解析器对 `\r` 处理存在冗余逻辑

- **文件**：`peri-agent/src/llm/sse.rs:69-73`
- **问题**：lines 69-70 的 `strip \r` 与 line 73 的 `trim_end_matches('\r')` 功能完全重叠
- **修复**：移除 lines 69-71，只保留 line 73

### L2: ChannelNotificationSender trait 定义在 peri-agent 但唯一实现在 peri-middlewares

- **文件**：`peri-agent/src/interaction/mod.rs:101-112`
- **问题**：合法的依赖倒置模式（同 UserInteractionBroker），但文档注释直接命名下游类型
- **严重级别**：Low

### L3: session/update payload 构造逻辑在 notify.rs 中重复 3 次

- **文件**：`peri-tui/src/acp_server/notify.rs:46-121`
- **问题**：3 个函数各自手写相同的 `serde_json::to_value + json!({sessionId, update})` 模式
- **修复**：提取 `fn send_session_update()` 辅助函数

### L4: OAuth 回调 CSRF state 验证始终绕过

- **文件**：`peri-middlewares/src/mcp/callback_server.rs:42,88,137-139`
- **问题**：`state_param: String::new()` 导致 CSRF 校验永远不触发
- **实际风险**：Low（rmcp 层通过 state_store + PKCE 双重防护，不可利用）

### L5: SSE 解析器 pending_bytes 无内存增长上限

- **文件**：`peri-agent/src/llm/sse.rs:20,41-65`
- **问题**：恶意或故障服务器持续发送无换行符字节流可导致无限增长
- **实际风险**：Low（有 HTTP 超时保护）

### L6: Sync receiver WebSocket URL 拼接未对 pair_code 编码

- **文件**：`peri-tui/src/sync/receiver.rs:29`
- **问题**：`pair_code` 未做 URL 编码直接插入 query string
- **实际风险**：Low（pair_code 通常为简短数字字母组合）

### L7: PBKDF2 密钥派生使用配对码同时作为 salt 和 password

- **文件**：`peri-tui/src/sync/crypto.rs:24-33`
- **问题**：salt 和 password 相同，不增加额外安全性
- **实际风险**：Low（端到端本地同步场景，配对码短期一次性使用）

### L8: StopReason serde（PascalCase）与 Display/from_display（snake_case）不一致

- **文件**：`peri-agent/src/llm/types.rs:75-80`
- **问题**：`serde_json::to_value(&stop_reason)` 产生 PascalCase，但 `from_display` 期望 snake_case
- **修复**：添加 `#[serde(rename_all = "snake_case")]`

### L9: clear 命令复用 CompactCompleted 事件语义不清晰

- **文件**：`peri-acp/src/session/command/clear.rs:44-55`
- **问题**：发送 `CompactCompleted{messages:vec![]}` 复用 TUI compact 清理路径
- **实际风险**：Low（注释已说明复用意图，当前行为正确）

### L10: session/update_config 接受完整 PeriConfig 替换，缺少细粒度校验

- **文件**：`peri-tui/src/acp_server/requests.rs:418-455`
- **问题**：仅校验 providers 非空和 active_provider_id 存在
- **实际风险**：Low（仅 TUI 内部 login panel 调用）

### L11: CommandContext 字段过于宽泛

- **文件**：`peri-acp/src/session/command/mod.rs:31-48`
- **问题**：7 个字段中 ClearCommand 仅用 2 个
- **实际风险**：Low（当前仅 2 个命令）

### L12: ChannelBroker 串行发送 O(servers*items) 通知 + 多余克隆

- **文件**：`peri-agent/src/interaction/channel_broker.rs:42-49,73-97`
- **问题**：双重嵌套循环逐个 await，`_source` 值克隆但未使用
- **实际风险**：Low（典型 N=1-3、M=1-5，且 MultiplexBroker 并发竞速使延迟通常无关）

### L13: headless_test.rs 迁移后 system_note 错误检测测试质量低

- **文件**：`peri-tui/src/ui/message_render_test.rs`
- **问题**：仅做字符串匹配不调用 render_view_model，且检查英文 'failed' 而实现检查中文
- **实际风险**：Low（迁移前即如此，非回归）

### L14: StdioEventSink 丢弃所有 TUI-only 事件

- **文件**：`peri-acp/src/session/event_sink.rs:141-165`
- **问题**：外部 IDE 客户端无法感知 compact/subagent/context-warning
- **性质**：设计选择而非 bug

### L15: 移除 pending_requests 和 $/cancel_request

- **文件**：`peri-acp/src/session/mod.rs:47-206`
- **问题**：$/cancel_request 仅存在 13 天，session/cancel 提供等价功能
- **性质**：Breaking change 但影响面极小

### L16: 配置变更现在持久化到磁盘

- **文件**：`peri-tui/src/acp_server/requests.rs:140-230`
- **问题**：新增 persist_config() 调用，行为变更但属修复（旧版 ACP 侧修改不持久化是 bug）
- **性质**：Bug fix

---

## 修复优先级建议

### 紧急（建议本周）

1. **修复 `]` 解析 bug**：在 `compact.rs:183,202` 和 `compact_middleware.rs:112,131` 添加 `.trim_end_matches(']')`
2. **修复 rawOutput 类型不匹配**：在 `acp_bridge.rs:152` 替换 `.as_str()` 为能处理 JSON Value 的辅助函数

### 高优（建议近期）

3. **补充 mapper_test.rs**：至少覆盖 TextChunk/ToolStart/TodoUpdate/AiReasonage
4. **新建 command/mod_test.rs**：CommandRegistry 查找 + 命令 execute
5. **新建 interaction 测试**：channel_broker + multiplex
6. **为 acp_bridge.rs 提取纯函数测试**：JSON→AgentEvent 解析逻辑

### 中优

7. **优化 render_thread**：移除 compute_wrapped_height 重复计算
8. **MultiplexBroker 添加 CancellationToken**：竞速后取消剩余 task

### 低优

9. 清理 SSE 解析器冗余 `\r` 处理
10. 合并 session/update payload 构造为辅助函数
11. 统一 compact_config 计算路径（冻结到 session/new）

---

## 审查方法说明

本次审查使用 Claude Code Workflow 编排，分为 4 个阶段：

1. **Scan**：1 个 agent 扫描全部变更文件
2. **Review**：6 个维度 agent 并行深度审查（正确性、安全性、架构、性能、测试覆盖、回归风险）
3. **Verify**：35 个 Medium+ 发现逐个派出 agent 读取实际代码对抗式验证（5 个验证失败因 agent 未完成结构化输出）
4. **Synthesize**：1 个 agent 汇总所有验证结果并分级

每个维度审查 agent 被要求输出结构化 JSON（文件路径、行号、严重级别、标题、描述、建议修复），验证 agent 被要求读取实际代码确认发现是否为真问题并调整严重级别。30/35 个发现被验证确认，总体误报率为 0%（5 个未完成的验证按原发现保留）。
