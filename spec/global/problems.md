# 问题索引

按关键词索引已归档 issue，遇到相似问题时快速定位历史经验。

## 关键词索引

### HashMap 顺序
- [多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定](domains/message-pipeline.md#issue_2026-05-12-deferred-tool-list-nondeterministic-order) — message-pipeline

### Prompt Cache
- [多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定](domains/message-pipeline.md#issue_2026-05-12-deferred-tool-list-nondeterministic-order) — message-pipeline
- [Skill Preload 注入消息到历史最前面导致首轮 Prompt Cache 失效](domains/message-pipeline.md#issue_2026-05-12-skill-preload-invalidates-prompt-cache) — message-pipeline
- [System prompt 动态内容导致 Anthropic prompt cache 频繁失效，边界标记拆分修复](domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-cache-invalidation) — system-prompt
- [AskUserQuestion 导致缓存命中率极速下降](domains/system-prompt.md#issue_2026-05-13-askuserquestion-cache-hit-rate-drop) — system-prompt
- [82% system 未缓存 + 断点在 tool_result-only 消息上静默失效](domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency) — message-pipeline

### 缓存前缀
- [多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定](domains/message-pipeline.md#issue_2026-05-12-deferred-tool-list-nondeterministic-order) — message-pipeline

### ToolSearchIndex
- [多处 HashMap 非确定性顺序导致 Anthropic Prompt Cache 前缀不稳定](domains/message-pipeline.md#issue_2026-05-12-deferred-tool-list-nondeterministic-order) — message-pipeline

### prepend_message
- [Skill Preload 注入消息到历史最前面导致首轮 Prompt Cache 失效](domains/message-pipeline.md#issue_2026-05-12-skill-preload-invalidates-prompt-cache) — message-pipeline
- [prepend_message 的 insert(0) 右移导致 StateSnapshot 包含 System 消息](domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls) — system-prompt
- [System Prompt 每轮重复注入 prepend_message 导致上下文倍数膨胀](domains/system-prompt.md#issue_2026-05-20-rapid-context-expansion) — system-prompt

### add_message
- [Skill Preload 注入消息到历史最前面导致首轮 Prompt Cache 失效](domains/message-pipeline.md#issue_2026-05-12-skill-preload-invalidates-prompt-cache) — message-pipeline

### cache_control
- [Skill Preload 注入消息到历史最前面导致首轮 Prompt Cache 失效](domains/message-pipeline.md#issue_2026-05-12-skill-preload-invalidates-prompt-cache) — message-pipeline
- [82% system 未缓存 + 断点在 tool_result-only 消息上静默失效](domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency) — message-pipeline

### SystemNote
- [SystemNote 在 RebuildAll 后堆积到消息列表末尾](domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild) — message-pipeline

### RebuildAll
- [SystemNote 在 RebuildAll 后堆积到消息列表末尾](domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild) — message-pipeline
- [CacheWarning 消息被 RebuildAll 立即丢弃，用户无法看到](domains/message-pipeline.md#issue_2026-05-12-cache-warning-discarded-by-rebuild) — message-pipeline
- [Compact 完成后残留 "正在压缩上下文…" 系统通知](domains/message-pipeline.md#issue_2026-05-12-compact-ephemeral-notes-not-cleared) — message-pipeline

### ephemeral_notes
- [SystemNote 在 RebuildAll 后堆积到消息列表末尾](domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild) — message-pipeline
- [Compact 完成后残留 "正在压缩上下文…" 系统通知](domains/message-pipeline.md#issue_2026-05-12-compact-ephemeral-notes-not-cleared) — message-pipeline

### 锚点机制
- [SystemNote 在 RebuildAll 后堆积到消息列表末尾](domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild) — message-pipeline

### CacheWarning
- [CacheWarning 消息被 RebuildAll 立即丢弃，用户无法看到](domains/message-pipeline.md#issue_2026-05-12-cache-warning-discarded-by-rebuild) — message-pipeline

### AiReasoning
- [流式过程中 AI 文本不可见（工具调用场景）](domains/agent.md#issue_2026-05-11-streaming-text-invisible-with-tools) — agent

### TextChunk
- [流式过程中 AI 文本不可见（工具调用场景）](domains/agent.md#issue_2026-05-11-streaming-text-invisible-with-tools) — agent

### 事件类型语义
- [流式过程中 AI 文本不可见（工具调用场景）](domains/agent.md#issue_2026-05-11-streaming-text-invisible-with-tools) — agent

### frozen_subagent_vms
- [Background Agent 三个 Bug：显示消失、subagent_type 限制、continuation 不触发](domains/agent.md#issue_2026-05-12-background-agent-display-and-continuation-bugs) — agent

### continuation 竞态
- [Background Agent 三个 Bug：显示消失、subagent_type 限制、continuation 不触发](domains/agent.md#issue_2026-05-12-background-agent-display-and-continuation-bugs) — agent

### fork+background
- [Background Agent 三个 Bug：显示消失、subagent_type 限制、continuation 不触发](domains/agent.md#issue_2026-05-12-background-agent-display-and-continuation-bugs) — agent

### SubAgent
- [Background Agent 工具继承缺失——子 agent 仅能使用 TodoWrite](domains/agent.md#issue_2026-05-11-background-agent-missing-tools) — agent
- [同步子 Agent（Normal/Fork）事件溢出到主 Agent 消息流](domains/agent.md#issue_2026-05-13-sync-subagent-events-leak-to-parent) — agent
- [同步 SubAgent Ctrl+C 中断——handle_interrupted() 的 in_subagent() 守卫静默吞掉父 Agent 中断事件](domains/agent.md#issue_2026-05-26-sync-subagent-cancel-fix-attempts-log) — agent

### in_subagent
- [同步子 Agent（Normal/Fork）事件溢出到主 Agent 消息流](domains/agent.md#issue_2026-05-13-sync-subagent-events-leak-to-parent) — agent
- [同步 SubAgent Ctrl+C 中断——handle_interrupted() 的 in_subagent() 守卫静默吞掉父 Agent 中断事件](domains/agent.md#issue_2026-05-26-sync-subagent-cancel-fix-attempts-log) — agent

### StateSnapshot 守卫
- [同步子 Agent（Normal/Fork）事件溢出到主 Agent 消息流](domains/agent.md#issue_2026-05-13-sync-subagent-events-leak-to-parent) — agent
- [流式渲染中 map_executor_event 丢弃中间 StateSnapshot](domains/message-pipeline.md#issue_2026-05-13-streaming-text-tool-aggregation-visual-issues) — message-pipeline

### 事件溢出
- [同步子 Agent（Normal/Fork）事件溢出到主 Agent 消息流](domains/agent.md#issue_2026-05-13-sync-subagent-events-leak-to-parent) — agent
- [并发前台SubAgent调用时UI感知延迟，SubAgentGroup卡片不可见](domains/tui.md#issue_2026-05-15-concurrent-subagent-display-delay) — tui

### map_executor_event
- [流式渲染中 map_executor_event 丢弃中间 StateSnapshot](domains/message-pipeline.md#issue_2026-05-13-streaming-text-tool-aggregation-visual-issues) — message-pipeline

### 双写路径
- [后台 Agent 完成后 input_history 消息重复导致 Prompt Cache 失效](domains/agent.md#issue_2026-05-13-input-history-message-duplication-after-background-tasks) — agent

### agent_state_messages
- [后台 Agent 完成后 input_history 消息重复导致 Prompt Cache 失效](domains/agent.md#issue_2026-05-13-input-history-message-duplication-after-background-tasks) — agent
- [prepend_message 的 insert(0) 右移导致 StateSnapshot 包含 System 消息](domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-parts-duplicated-in-consecutive-calls) — system-prompt
- [DeepSeek多轮对话中agent_state_messages消息重复导致API 400错误](domains/agent.md#issue_2026-05-14-deepseek-multi-turn-tool-result-duplication) — agent

### tool_call_id 重复
- [后台 Agent 完成后 input_history 消息重复导致 Prompt Cache 失效](domains/agent.md#issue_2026-05-13-input-history-message-duplication-after-background-tasks) — agent

### 流式渲染
- [多轮对话中 AI message 和 thinking 在进行时不可见](domains/message-pipeline.md#issue_2026-05-13-ai-message-thinking-invisible-during-multi-turn) — message-pipeline
- [流式渲染中 map_executor_event 丢弃中间 StateSnapshot](domains/message-pipeline.md#issue_2026-05-13-streaming-text-tool-aggregation-visual-issues) — message-pipeline

### has_snapshot_this_round
- [多轮对话中 AI message 和 thinking 在进行时不可见](domains/message-pipeline.md#issue_2026-05-13-ai-message-thinking-invisible-during-multi-turn) — message-pipeline
- [并发前台SubAgent调用时UI感知延迟，SubAgentGroup卡片不可见](domains/tui.md#issue_2026-05-15-concurrent-subagent-display-delay) — tui

### 边界标记
- [System prompt 动态内容导致 Anthropic prompt cache 频繁失效，边界标记拆分修复](domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-cache-invalidation) — system-prompt

### __SYSTEM_PROMPT_DYNAMIC_BOUNDARY__
- [System prompt 动态内容导致 Anthropic prompt cache 频繁失效，边界标记拆分修复](domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-cache-invalidation) — system-prompt

### split_system_blocks
- [System prompt 动态内容导致 Anthropic prompt cache 频繁失效，边界标记拆分修复](domains/system-prompt.md#issue_2026-05-13-system-prompt-dynamic-cache-invalidation) — system-prompt

### SkillPreloadMiddleware
- [主 Agent 中间件链缺少 SkillPreloadMiddleware，预加载失效](domains/system-prompt.md#issue_2026-05-13-missing-skillpreload-in-main-agent) — system-prompt
- [主 Agent SkillPreloadMiddleware preload_skills 硬编码为空，/skill-name 不注入全文](domains/agent.md#issue_2026-05-25-skill-preload-no-tool-calls-in-history) — agent

### 中间件链缺失
- [主 Agent 中间件链缺少 SkillPreloadMiddleware，预加载失效](domains/system-prompt.md#issue_2026-05-13-missing-skillpreload-in-main-agent) — system-prompt

### 工具继承
- [Background Agent 工具继承缺失——子 agent 仅能使用 TodoWrite](domains/agent.md#issue_2026-05-11-background-agent-missing-tools) — agent

### register_tool
- [Background Agent 工具继承缺失——子 agent 仅能使用 TodoWrite](domains/agent.md#issue_2026-05-11-background-agent-missing-tools) — agent

### merge_frozen_subagents
- [并发前台SubAgent调用时UI感知延迟，SubAgentGroup卡片不可见](domains/tui.md#issue_2026-05-15-concurrent-subagent-display-delay) — tui

### reasoning
- [GLM 模型 reasoning 字段未被解析，thinking 内容跨轮次丢失](domains/agent.md#issue_2026-05-12-glm-reasoning-field-not-parsed) — agent

### reasoning_content
- [GLM 模型 reasoning 字段未被解析，thinking 内容跨轮次丢失](domains/agent.md#issue_2026-05-12-glm-reasoning-field-not-parsed) — agent

### GLM
- [GLM 模型 reasoning 字段未被解析，thinking 内容跨轮次丢失](domains/agent.md#issue_2026-05-12-glm-reasoning-field-not-parsed) — agent

### context_window
- [OpenAI 兼容第三方 Provider 上下文用量计算不准确](domains/token-tracking.md#issue_2026-05-11-context-usage-miscalculation-openai-compatible) — token-tracking
- [Model 面板添加 1M 上下文开关](domains/tui.md#issue_2026-05-16-model-panel-1m-context-toggle) — tui

### 缓存命中率
- [OpenAI 兼容第三方 Provider 上下文用量计算不准确](domains/token-tracking.md#issue_2026-05-11-context-usage-miscalculation-openai-compatible) — token-tracking
- [状态栏缓存百分比在对话停止后消失](domains/token-tracking.md#issue_2026-05-12-cache-percentage-disappears-after-done) — token-tracking

### last_user_input
- [Auto Compact 后 Agent 未自动 Resubmit 继续执行](domains/compact.md#issue_2026-05-11-auto-compact-no-resubmit) — compact

### auto-compact
- [Auto Compact 后 Agent 未自动 Resubmit 继续执行](domains/compact.md#issue_2026-05-11-auto-compact-no-resubmit) — compact

### CJK 宽度
- [输入框鼠标点击光标定位不准](domains/tui.md#issue_2026-05-12-textarea-mouse-click-cursor-misposition-cjk) — tui

### unicode-width
- [输入框鼠标点击光标定位不准](domains/tui.md#issue_2026-05-12-textarea-mouse-click-cursor-misposition-cjk) — tui
- [Form Edit 字段标签硬编码英文，未使用 i18n](domains/tui.md#issue_2026-05-16-setup-form-edit-labels-hardcoded) — tui

### 鼠标定位
- [输入框鼠标点击光标定位不准](domains/tui.md#issue_2026-05-12-textarea-mouse-click-cursor-misposition-cjk) — tui

### on_error 回调
- [LSP transport 层错误处理缺陷（进程退出未更新状态 + 崩溃后无自动重连）](domains/lsp.md#issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit) — lsp

### LSP 重连
- [LSP transport 层错误处理缺陷（进程退出未更新状态 + 崩溃后无自动重连）](domains/lsp.md#issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit) — lsp

### parking_lot::MutexGuard !Send
- [LSP transport 层错误处理缺陷（进程退出未更新状态 + 崩溃后无自动重连）](domains/lsp.md#issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit) — lsp

### transport 断开
- [LSP transport 层错误处理缺陷（进程退出未更新状态 + 崩溃后无自动重连）](domains/lsp.md#issue_2026-05-12-lsp-transport-no-fast-fail-on-process-exit) — lsp

### Grep工具
- [Grep 工具声明参数未实现 + 标准 grep 能力缺失](domains/agent.md#issue_2026-05-14-grep-tool-capability-gap) — agent

### 参数声明
- [Grep 工具声明参数未实现 + 标准 grep 能力缺失](domains/agent.md#issue_2026-05-14-grep-tool-capability-gap) — agent

### 接口契约
- [Grep 工具声明参数未实现 + 标准 grep 能力缺失](domains/agent.md#issue_2026-05-14-grep-tool-capability-gap) — agent

### 工具标准能力
- [Grep 工具声明参数未实现 + 标准 grep 能力缺失](domains/agent.md#issue_2026-05-14-grep-tool-capability-gap) — agent

### thinking block
- [SkillPreloadMiddleware 注入的伪 assistant 消息不含 thinking block，DeepSeek API 400](domains/agent.md#issue_2026-05-14-deepseek-anthropic-thinking-block-dropped) — agent
- [Thinking/Reasoning数据流：占位thinking缺signature + AiReasoning死代码](domains/agent.md#issue_2026-05-12-thinking-reasoning-dataflow-issues) — agent

### redacted_thinking
- [SkillPreloadMiddleware 注入的伪 assistant 消息不含 thinking block，DeepSeek API 400](domains/agent.md#issue_2026-05-14-deepseek-anthropic-thinking-block-dropped) — agent

### SkillPreload
- [SkillPreloadMiddleware 注入的伪 assistant 消息不含 thinking block，DeepSeek API 400](domains/agent.md#issue_2026-05-14-deepseek-anthropic-thinking-block-dropped) — agent

### DeepSeek
- [SkillPreloadMiddleware 注入的伪 assistant 消息不含 thinking block，DeepSeek API 400](domains/agent.md#issue_2026-05-14-deepseek-anthropic-thinking-block-dropped) — agent

### tool_result闭合
- [并发工具执行中部分路径提前返回导致 tool_result 缺失](domains/agent.md#issue_2026-05-14-orphaned-tool-use-without-tool-result) — agent

### 并发工具
- [并发工具执行中部分路径提前返回导致 tool_result 缺失](domains/agent.md#issue_2026-05-14-orphaned-tool-use-without-tool-result) — agent

### 工具错误处理
- [工具调用参数错误导致Agent停止而非自动重试](domains/agent.md#issue_2026-05-15-tool-execution-error-stops-agent) — agent

### deferred_error
- [并发工具执行中部分路径提前返回导致 tool_result 缺失](domains/agent.md#issue_2026-05-14-orphaned-tool-use-without-tool-result) — agent

### 延迟写入
- [stop_reason与内容不一致导致孤儿tool_use触发Anthropic API 400](domains/agent.md#issue_2026-05-15-orphaned-tool-use-after-concurrent-tool-error) — agent

### stop_reason
- [stop_reason与内容不一致导致孤儿tool_use触发Anthropic API 400](domains/agent.md#issue_2026-05-15-orphaned-tool-use-after-concurrent-tool-error) — agent

### 孤儿tool_use
- [并发工具执行中部分路径提前返回导致 tool_result 缺失](domains/agent.md#issue_2026-05-14-orphaned-tool-use-without-tool-result) — agent
- [stop_reason与内容不一致导致孤儿tool_use触发Anthropic API 400](domains/agent.md#issue_2026-05-15-orphaned-tool-use-after-concurrent-tool-error) — agent

### tool_result id
- [GLM Anthropic兼容端口tool_result block缺少id属性导致500错误](domains/agent.md#issue_2026-05-15-glm-anthropic-tool-result-id-attribute-error) — agent

### 第三方API
- [GLM Anthropic兼容端口tool_result block缺少id属性导致500错误](domains/agent.md#issue_2026-05-15-glm-anthropic-tool-result-id-attribute-error) — agent
- [stop_reason与内容不一致导致孤儿tool_use触发Anthropic API 400](domains/agent.md#issue_2026-05-15-orphaned-tool-use-after-concurrent-tool-error) — agent

### max_tokens
- [Write工具超长内容触发max_tokens截断导致file_path缺失](domains/agent.md#issue_2026-05-15-write-tool-missing-filepath-max-tokens) — agent

### 消息重复
- [DeepSeek多轮对话中agent_state_messages消息重复导致API 400错误](domains/agent.md#issue_2026-05-14-deepseek-multi-turn-tool-result-duplication) — agent

### last_message_count
- [DeepSeek多轮对话中agent_state_messages消息重复导致API 400错误](domains/agent.md#issue_2026-05-14-deepseek-multi-turn-tool-result-duplication) — agent

### 死代码
- [24 处 #[allow(dead_code/unused)] 抑制了真正的死代码和未完成功能](domains/code-architecture.md#issue_2026-05-14-dead-code-unfinished-features-cleanup) — code-architecture

### allow注解
- [24 处 #[allow(dead_code/unused)] 抑制了真正的死代码和未完成功能](domains/code-architecture.md#issue_2026-05-14-dead-code-unfinished-features-cleanup) — code-architecture

### 代码清理
- [24 处 #[allow(dead_code/unused)] 抑制了真正的死代码和未完成功能](domains/code-architecture.md#issue_2026-05-14-dead-code-unfinished-features-cleanup) — code-architecture

### 编译器警告
- [24 处 #[allow(dead_code/unused)] 抑制了真正的死代码和未完成功能](domains/code-architecture.md#issue_2026-05-14-dead-code-unfinished-features-cleanup) — code-architecture

### 测试分离
- [89.8% 源文件内联测试违反规范，两轮分离后 152 个文件外部化](domains/code-architecture.md#issue_2026-05-14-test-separation-convention-debt) — code-architecture

### include!
- [89.8% 源文件内联测试违反规范，两轮分离后 152 个文件外部化](domains/code-architecture.md#issue_2026-05-14-test-separation-convention-debt) — code-architecture

### #[path]
- [89.8% 源文件内联测试违反规范，两轮分离后 152 个文件外部化](domains/code-architecture.md#issue_2026-05-14-test-separation-convention-debt) — code-architecture

### 模块可见性
- [89.8% 源文件内联测试违反规范，两轮分离后 152 个文件外部化](domains/code-architecture.md#issue_2026-05-14-test-separation-convention-debt) — code-architecture

### Reasoning渲染
- [最后一条 AI 消息无正文时展示思考最后 1 行](domains/message-pipeline.md#issue_2026-05-15-thinking-tail-preview) — message-pipeline

### tail_lines
- [最后一条 AI 消息无正文时展示思考最后 1 行](domains/message-pipeline.md#issue_2026-05-15-thinking-tail-preview) — message-pipeline

### ContentBlockView
- [最后一条 AI 消息无正文时展示思考最后 1 行](domains/message-pipeline.md#issue_2026-05-15-thinking-tail-preview) — message-pipeline

### Hash设计
- [最后一条 AI 消息无正文时展示思考最后 1 行](domains/message-pipeline.md#issue_2026-05-15-thinking-tail-preview) — message-pipeline

### 断点回退
- [82% system 未缓存 + 断点在 tool_result-only 消息上静默失效](domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency) — message-pipeline

### 缓存驱逐
- [82% system 未缓存 + 断点在 tool_result-only 消息上静默失效](domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency) — message-pipeline

### system缓存
- [82% system 未缓存 + 断点在 tool_result-only 消息上静默失效](domains/message-pipeline.md#issue_2026-05-14-cache-breakpoint-structural-inefficiency) — message-pipeline

### Resize事件
- [流式加载期间拖动窗口宽度，Resize 事件无节流导致 CPU 暴涨](domains/tui.md#issue_2026-05-14-streaming-resize-cpu-spike) — tui

### 去抖/节流
- [流式加载期间拖动窗口宽度，Resize 事件无节流导致 CPU 暴涨](domains/tui.md#issue_2026-05-14-streaming-resize-cpu-spike) — tui

### 渲染线程
- [流式加载期间拖动窗口宽度，Resize 事件无节流导致 CPU 暴涨](domains/tui.md#issue_2026-05-14-streaming-resize-cpu-spike) — tui

### CPU暴涨
- [流式加载期间拖动窗口宽度，Resize 事件无节流导致 CPU 暴涨](domains/tui.md#issue_2026-05-14-streaming-resize-cpu-spike) — tui

### .get()
- [active_provider 越界无保护可导致 render panic](domains/tui.md#issue_2026-05-16-setup-active-provider-oob-panic) — tui

### _lc 参数
- [Language 步骤完全硬编码中英混合文本，忽略 i18n](domains/tui.md#issue_2026-05-16-setup-language-step-hardcoded-no-i18n) — tui

### agent_id 校验
- [SubAgent 跨轮次 frozen_subagent_vms 累积导致批次与单个 SubAgentGroup 重复显示](domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication) — message-pipeline

### begin_round 清理
- [SubAgent 跨轮次 frozen_subagent_vms 累积导致批次与单个 SubAgentGroup 重复显示](domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication) — message-pipeline

### chars().count()
- [API Key 遮罩使用字节长度而非字符数](domains/tui.md#issue_2026-05-16-setup-api-key-mask-byte-vs-char) — tui

### CJK 显示
- [API Key 遮罩使用字节长度而非字符数](domains/tui.md#issue_2026-05-16-setup-api-key-mask-byte-vs-char) — tui

### Ctrl+C 拦截
- [Ctrl+C 在 Setup Wizard 中完全被拦截——无法退出](domains/tui.md#issue_2026-05-16-setup-ctrlc-blocked-cannot-exit) — tui

### curl-pipe-bash
- [update.rs 应简化为 curl 远程脚本 | bash](domains/cli.md#issue_2026-05-16-self-update-simplify-to-curl-pipe-bash) — cli

### debug_assert
- [Language 步骤空选项下取模 panic 风险](domains/tui.md#issue_2026-05-16-setup-mod-zero-empty-options) — tui

### format_args_summary
- [工具调用参数显示截断过短](domains/tui.md#issue_2026-05-16-tool-args-display-truncation-too-short) — tui

### format_tool_args
- [工具调用参数显示截断过短](domains/tui.md#issue_2026-05-16-tool-args-display-truncation-too-short) — tui

### frozen_vms
- [SubAgent 跨轮次 frozen_subagent_vms 累积导致批次与单个 SubAgentGroup 重复显示](domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication) — message-pipeline

### FTL 未使用
- [Language 步骤完全硬编码中英混合文本，忽略 i18n](domains/tui.md#issue_2026-05-16-setup-language-step-hardcoded-no-i18n) — tui

### i18n 忽略
- [Language 步骤完全硬编码中英混合文本，忽略 i18n](domains/tui.md#issue_2026-05-16-setup-language-step-hardcoded-no-i18n) — tui

### i18n 未使用
- [Form Edit 字段标签硬编码英文，未使用 i18n](domains/tui.md#issue_2026-05-16-setup-form-edit-labels-hardcoded) — tui

### len() 陷阱
- [API Key 遮罩使用字节长度而非字符数](domains/tui.md#issue_2026-05-16-setup-api-key-mask-byte-vs-char) — tui

### output_persist
- [工具输出超长时截断 + 持久化磁盘 + 提示 Read 读取剩余内容](domains/tools.md#issue_2026-05-15-tool-output-truncation-with-disk-persist) — tools

### ProviderType
- [Form Edit 字段标签硬编码英文，未使用 i18n](domains/tui.md#issue_2026-05-16-setup-form-edit-labels-hardcoded) — tui

### save-before-load
- [save_setup 覆盖已有配置文件导致数据永久丢失](domains/tui.md#issue_2026-05-16-setup-save-destroys-existing-config) — tui

### 字节 vs 字符
- [API Key 遮罩使用字节长度而非字符数](domains/tui.md#issue_2026-05-16-setup-api-key-mask-byte-vs-char) — tui

### 磁盘持久化
- [工具输出超长时截断 + 持久化磁盘 + 提示 Read 读取剩余内容](domains/tools.md#issue_2026-05-15-tool-output-truncation-with-disk-persist) — tools

### 代码去重
- [update.rs 应简化为 curl 远程脚本 | bash](domains/cli.md#issue_2026-05-16-self-update-simplify-to-curl-pipe-bash) — cli

### 导航键冲突
- [Edit 模式 ProviderType 切换静默重置所有已编辑数据](domains/tui.md#issue_2026-05-16-setup-provider-type-toggle-resets-data) — tui

### 多层截断
- [工具调用参数显示截断过短](domains/tui.md#issue_2026-05-16-tool-args-display-truncation-too-short) — tui

### 防御性编程
- [active_provider 越界无保护可导致 render panic](domains/tui.md#issue_2026-05-16-setup-active-provider-oob-panic) — tui
- [Language 步骤空选项下取模 panic 风险](domains/tui.md#issue_2026-05-16-setup-mod-zero-empty-options) — tui

### 工具输出
- [工具输出超长时截断 + 持久化磁盘 + 提示 Read 读取剩余内容](domains/tools.md#issue_2026-05-15-tool-output-truncation-with-disk-persist) — tools

### 键过载
- [Edit 模式 ProviderType 切换静默重置所有已编辑数据](domains/tui.md#issue_2026-05-16-setup-provider-type-toggle-resets-data) — tui

### 静默失败
- [Browse 模式 Submit 失败时无任何反馈](domains/tui.md#issue_2026-05-16-setup-browse-submit-no-feedback) — tui

### 裸索引
- [active_provider 越界无保护可导致 render panic](domains/tui.md#issue_2026-05-16-setup-active-provider-oob-panic) — tui

### 配置覆盖
- [save_setup 覆盖已有配置文件导致数据永久丢失](domains/tui.md#issue_2026-05-16-setup-save-destroys-existing-config) — tui

### 全局处理器
- [Ctrl+C 在 Setup Wizard 中完全被拦截——无法退出](domains/tui.md#issue_2026-05-16-setup-ctrlc-blocked-cannot-exit) — tui

### 确认提示
- [Edit 模式 ProviderType 切换静默重置所有已编辑数据](domains/tui.md#issue_2026-05-16-setup-provider-type-toggle-resets-data) — tui

### 输出截断
- [工具输出超长时截断 + 持久化磁盘 + 提示 Read 读取剩余内容](domains/tools.md#issue_2026-05-15-tool-output-truncation-with-disk-persist) — tools

### 事件拦截
- [Ctrl+C 在 Setup Wizard 中完全被拦截——无法退出](domains/tui.md#issue_2026-05-16-setup-ctrlc-blocked-cannot-exit) — tui

### 数据丢失
- [save_setup 覆盖已有配置文件导致数据永久丢失](domains/tui.md#issue_2026-05-16-setup-save-destroys-existing-config) — tui
- [Edit 模式 ProviderType 切换静默重置所有已编辑数据](domains/tui.md#issue_2026-05-16-setup-provider-type-toggle-resets-data) — tui

### 双份实现
- [update.rs 应简化为 curl 远程脚本 | bash](domains/cli.md#issue_2026-05-16-self-update-simplify-to-curl-pipe-bash) — cli

### 退出流程
- [Ctrl+C 在 Setup Wizard 中完全被拦截——无法退出](domains/tui.md#issue_2026-05-16-setup-ctrlc-blocked-cannot-exit) — tui

### 维护负担
- [update.rs 应简化为 curl 远程脚本 | bash](domains/cli.md#issue_2026-05-16-self-update-simplify-to-curl-pipe-bash) — cli

### 位置匹配
- [SubAgent 跨轮次 frozen_subagent_vms 累积导致批次与单个 SubAgentGroup 重复显示](domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication) — message-pipeline

### 无反馈
- [Browse 模式 Submit 失败时无任何反馈](domains/tui.md#issue_2026-05-16-setup-browse-submit-no-feedback) — tui

### 先写后读
- [save_setup 覆盖已有配置文件导致数据永久丢失](domains/tui.md#issue_2026-05-16-setup-save-destroys-existing-config) — tui

### 显示阈值
- [工具调用参数显示截断过短](domains/tui.md#issue_2026-05-16-tool-args-display-truncation-too-short) — tui

### 用户体验
- [Browse 模式 Submit 失败时无任何反馈](domains/tui.md#issue_2026-05-16-setup-browse-submit-no-feedback) — tui

### 硬编码标签
- [Form Edit 字段标签硬编码英文，未使用 i18n](domains/tui.md#issue_2026-05-16-setup-form-edit-labels-hardcoded) — tui

### 硬编码混合文本
- [Language 步骤完全硬编码中英混合文本，忽略 i18n](domains/tui.md#issue_2026-05-16-setup-language-step-hardcoded-no-i18n) — tui

### 越界检查
- [active_provider 越界无保护可导致 render panic](domains/tui.md#issue_2026-05-16-setup-active-provider-oob-panic) — tui

### 跨轮次累积
- [SubAgent 跨轮次 frozen_subagent_vms 累积导致批次与单个 SubAgentGroup 重复显示](domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication) — message-pipeline

### 取模零除
- [Language 步骤空选项下取模 panic 风险](domains/tui.md#issue_2026-05-16-setup-mod-zero-empty-options) — tui

### 错误提示
- [Browse 模式 Submit 失败时无任何反馈](domains/tui.md#issue_2026-05-16-setup-browse-submit-no-feedback) — tui

### 1M context
- [Model 面板添加 1M 上下文开关](domains/tui.md#issue_2026-05-16-model-panel-1m-context-toggle) — tui

### ACP
- [ACP 未实现 $/cancel_request 与 AvailableCommandsUpdate](domains/acp.md#issue_2026-05-16-acp-cancel-request-unimplemented) — acp

### agent_id 匹配
- [并发 SubAgent 工具调用路由错误 + 死锁修复](domains/agent.md#issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background) — agent

### AvailableCommandsUpdate
- [ACP 未实现 $/cancel_request 与 AvailableCommandsUpdate](domains/acp.md#issue_2026-05-16-acp-cancel-request-unimplemented) — acp

### cancel_request
- [ACP 未实现 $/cancel_request 与 AvailableCommandsUpdate](domains/acp.md#issue_2026-05-16-acp-cancel-request-unimplemented) — acp

### char vs byte
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### CODEX migrate
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### ContextBudget
- [Model 面板添加 1M 上下文开关](domains/tui.md#issue_2026-05-16-model-panel-1m-context-toggle) — tui

### Ctrl 修饰符
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### empty state
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### env_get
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### form validation
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### language
- [Setup 向导缺少语言配置步骤](domains/tui.md#issue_2026-05-16-i18n-language-not-in-setup) — tui

### model panel
- [Model 面板添加 1M 上下文开关](domains/tui.md#issue_2026-05-16-model-panel-1m-context-toggle) — tui

### needs_setup
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### oneshot
- [ACP 未实现 $/cancel_request 与 AvailableCommandsUpdate](domains/acp.md#issue_2026-05-16-acp-cancel-request-unimplemented) — acp

### paste newline
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### pending requests
- [ACP 未实现 $/cancel_request 与 AvailableCommandsUpdate](domains/acp.md#issue_2026-05-16-acp-cancel-request-unimplemented) — acp

### setup wizard
- [Setup 向导缺少语言配置步骤](domains/tui.md#issue_2026-05-16-i18n-language-not-in-setup) — tui

### setup wizard polish
- [Setup Wizard 波兰系列（8 个 UI 小修）](domains/tui.md#issue_2026-05-16-setup-polish-series) — tui

### SetupStep
- [Setup 向导缺少语言配置步骤](domains/tui.md#issue_2026-05-16-i18n-language-not-in-setup) — tui

### source_agent_id routing
- [并发 SubAgent 工具调用路由错误 + 死锁修复](domains/agent.md#issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background) — agent

### streaming cancellation
- [并发 SubAgent 工具调用路由错误 + 死锁修复](domains/agent.md#issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background) — agent

### SubAgent 并发
- [并发 SubAgent 工具调用路由错误 + 死锁修复](domains/agent.md#issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background) — agent
- [多 Agent 工具调用串行执行而非并发](domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially) — agent

### 通道容量
- [并发 SubAgent 工具调用路由错误 + 死锁修复](domains/agent.md#issue_2026-05-16-concurrent-subagent-tool-call-routing-and-background) — agent

### LLM 适配器
- [LLM 适配器模块化：anthropic.rs 1983 行、openai.rs 1065 行，按职责维度拆分为子模块](domains/agent.md#issue_2026-05-14-llm-adapter-modularization) — agent

### agent_done_pending_bg
- [Background task 完成后未触发 agent continuation（竞态条件）：pre_done_bg_completions 缓冲乱序到达](domains/agent.md#issue_2026-05-13-background-task-completion-race-condition) — agent

### anthropic
- [LLM 适配器模块化：anthropic.rs 1983 行、openai.rs 1065 行，按职责维度拆分为子模块](domains/agent.md#issue_2026-05-14-llm-adapter-modularization) — agent

### background task
- [Background task 完成后未触发 agent continuation（竞态条件）：pre_done_bg_completions 缓冲乱序到达](domains/agent.md#issue_2026-05-13-background-task-completion-race-condition) — agent

### continuation
- [Background task 完成后未触发 agent continuation（竞态条件）：pre_done_bg_completions 缓冲乱序到达](domains/agent.md#issue_2026-05-13-background-task-completion-race-condition) — agent

### openai
- [LLM 适配器模块化：anthropic.rs 1983 行、openai.rs 1065 行，按职责维度拆分为子模块](domains/agent.md#issue_2026-05-14-llm-adapter-modularization) — agent

### 大文件拆分
- [LLM 适配器模块化：anthropic.rs 1983 行、openai.rs 1065 行，按职责维度拆分为子模块](domains/agent.md#issue_2026-05-14-llm-adapter-modularization) — agent
- [超长函数拆分：event.rs（1120 行）和 agent_ops.rs（890 行）等 5 个超长单函数拆分](domains/code-architecture.md#issue_2026-05-14-mega-functions-split) — code-architecture

### 时序耦合
- [Background task 完成后未触发 agent continuation（竞态条件）：pre_done_bg_completions 缓冲乱序到达](domains/agent.md#issue_2026-05-13-background-task-completion-race-condition) — agent

### 模块化
- [LLM 适配器模块化：anthropic.rs 1983 行、openai.rs 1065 行，按职责维度拆分为子模块](domains/agent.md#issue_2026-05-14-llm-adapter-modularization) — agent
- [超长函数拆分：event.rs（1120 行）和 agent_ops.rs（890 行）等 5 个超长单函数拆分](domains/code-architecture.md#issue_2026-05-14-mega-functions-split) — code-architecture

### 竞态条件
- [Background task 完成后未触发 agent continuation（竞态条件）：pre_done_bg_completions 缓冲乱序到达](domains/agent.md#issue_2026-05-13-background-task-completion-race-condition) — agent

### child_handler_factory
- [多 Agent 工具调用串行执行而非并发](domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially) — agent

### tool_dispatch
- [多 Agent 工具调用串行执行而非并发](domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially) — agent

### join_all
- [多 Agent 工具调用串行执行而非并发](domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially) — agent

### session index 竞态
- [分屏模式下非活跃 Session 命令浮层显示异常](domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active) — tui

### 分屏
- [分屏模式下非活跃 Session 命令浮层显示异常](domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active) — tui

### 指示符号统一
- [TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙](domains/tui.md#issue_2026-05-18-tui-dot-and-scrollbar-rendering) — tui

### 滚动条 track
- [TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙](domains/tui.md#issue_2026-05-18-tui-dot-and-scrollbar-rendering) — tui

### box-drawing 字符
- [TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙](domains/tui.md#issue_2026-05-18-tui-dot-and-scrollbar-rendering) — tui

### mod.rs 内聚
- [Mod.rs 子模块内聚度问题](domains/code-architecture.md#issue_2026-05-17-mod-rs-cohesion) — code-architecture

### command 分组
- [Mod.rs 子模块内聚度问题](domains/code-architecture.md#issue_2026-05-17-mod-rs-cohesion) — code-architecture

### 插件懒初始化
- [~/.claude 目录不存在时插件面板无法使用](domains/plugin.md#issue_2026-05-18-claude-dir-missing-plugin-panel-empty) — plugin

### ~/.claude 自动创建
- [~/.claude 目录不存在时插件面板无法使用](domains/plugin.md#issue_2026-05-18-claude-dir-missing-plugin-panel-empty) — plugin

### marketplace 首次刷新
- [~/.claude 目录不存在时插件面板无法使用](domains/plugin.md#issue_2026-05-18-claude-dir-missing-plugin-panel-empty) — plugin

### std::mem::take
- [分屏模式下非活跃 Session 命令浮层显示异常](domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active) — tui

### ACP dispatch 拆分
- [ACP dispatch.rs 请求分发逻辑过度集中（1044 行）](domains/acp.md#issue_2026-05-17-acp-dispatch-heavy-file) — acp

### box-drawing 字符
- [TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙](domains/tui.md#issue_2026-05-18-tui-dot-and-scrollbar-rendering) — tui

### child_handler_factory
- [多 Agent 工具调用串行执行而非并发](domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially) — agent

### command 分组
- [Mod.rs 子模块内聚度问题：command/mod.rs + sync/mod.rs + panels/mod.rs + mcp/mod.rs](domains/code-architecture.md#issue_2026-05-17-mod-rs-cohesion) — code-architecture

### CommandRegistry
- [分屏模式下非活跃 Session 命令浮层显示异常](domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active) — tui

### core/panel/session 组织
- [Mod.rs 子模块内聚度问题：command/mod.rs + sync/mod.rs + panels/mod.rs + mcp/mod.rs](domains/code-architecture.md#issue_2026-05-17-mod-rs-cohesion) — code-architecture

### GPU 终端兼容
- [TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙](domains/tui.md#issue_2026-05-18-tui-dot-and-scrollbar-rendering) — tui

### handler 分组
- [ACP dispatch.rs 请求分发逻辑过度集中（1044 行）](domains/acp.md#issue_2026-05-17-acp-dispatch-heavy-file) — acp

### initialize/session/prompt/permission
- [ACP dispatch.rs 请求分发逻辑过度集中（1044 行）](domains/acp.md#issue_2026-05-17-acp-dispatch-heavy-file) — acp

### join_all
- [多 Agent 工具调用串行执行而非并发](domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially) — agent

### langfuse types 拆分
- [langfuse-client/src/types.rs 所有类型定义集中（1008 行）](domains/langfuse.md#issue_2026-05-17-langfuse-types-monolithic) — langfuse

### layout/event_handler 分离
- [peri-tui/src/ui/main_ui.rs 主 UI 布局逻辑集中（852 行）](domains/tui.md#issue_2026-05-17-main-ui-heavy-file) — tui

### marketplace 首次刷新
- [~/.claude 目录不存在时插件面板 Discover/Marketplaces 视图无法使用](domains/plugin.md#issue_2026-05-18-claude-dir-missing-plugin-panel-empty) — plugin

### message_pipeline 拆分
- [Pipeline/渲染层大文件拆分：message_pipeline.rs + message_view.rs](domains/message-pipeline.md#issue_2026-05-17-pipeline-render-heavy-files) — message-pipeline

### message_view 布局分离
- [Pipeline/渲染层大文件拆分：message_pipeline.rs + message_view.rs](domains/message-pipeline.md#issue_2026-05-17-pipeline-render-heavy-files) — message-pipeline

### middleware 拆分
- [Middleware 层大文件：subagent/tool.rs + plugin/installer.rs + plugin/marketplace.rs](domains/code-architecture.md#issue_2026-05-17-middleware-heavy-files) — code-architecture

### mod.rs 内聚
- [Mod.rs 子模块内聚度问题：command/mod.rs + sync/mod.rs + panels/mod.rs + mcp/mod.rs](domains/code-architecture.md#issue_2026-05-17-mod-rs-cohesion) — code-architecture

### PanelComponent
- [Panel 文件过度肥大：mcp_panel.rs + login_panel.rs + setup_wizard.rs](domains/tui.md#issue_2026-05-17-panel-heavy-files) — tui

### plugin/installer
- [Middleware 层大文件：subagent/tool.rs + plugin/installer.rs + plugin/marketplace.rs](domains/code-architecture.md#issue_2026-05-17-middleware-heavy-files) — code-architecture

### plugin/marketplace
- [Middleware 层大文件：subagent/tool.rs + plugin/installer.rs + plugin/marketplace.rs](domains/code-architecture.md#issue_2026-05-17-middleware-heavy-files) — code-architecture

### reconcile
- [Pipeline/渲染层大文件拆分：message_pipeline.rs + message_view.rs](domains/message-pipeline.md#issue_2026-05-17-pipeline-render-heavy-files) — message-pipeline
- [LLM 错误路径 round_start_vm_idx 被重置后视图清空闪烁](domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker) — message-pipeline

### session index 竞态
- [分屏模式下非活跃 Session 命令浮层显示异常](domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active) — tui

### state/ops/ui 三层分离
- [Panel 文件过度肥大：mcp_panel.rs + login_panel.rs + setup_wizard.rs](domains/tui.md#issue_2026-05-17-panel-heavy-files) — tui

### subagent/tool
- [Middleware 层大文件：subagent/tool.rs + plugin/installer.rs + plugin/marketplace.rs](domains/code-architecture.md#issue_2026-05-17-middleware-heavy-files) — code-architecture

### tool_dispatch
- [多 Agent 工具调用串行执行而非并发](domains/agent.md#issue_2026-05-18-agent-tool-calls-execute-serially) — agent

### trace/span/generation/score
- [langfuse-client/src/types.rs 所有类型定义集中（1008 行）](domains/langfuse.md#issue_2026-05-17-langfuse-types-monolithic) — langfuse

### view_model
- [Pipeline/渲染层大文件拆分：message_pipeline.rs + message_view.rs](domains/message-pipeline.md#issue_2026-05-17-pipeline-render-heavy-files) — message-pipeline

### ~/.claude 自动创建
- [~/.claude 目录不存在时插件面板 Discover/Marketplaces 视图无法使用](domains/plugin.md#issue_2026-05-18-claude-dir-missing-plugin-panel-empty) — plugin

### 主 UI 拆分
- [peri-tui/src/ui/main_ui.rs 主 UI 布局逻辑集中（852 行）](domains/tui.md#issue_2026-05-17-main-ui-heavy-file) — tui

### 分屏
- [分屏模式下非活跃 Session 命令浮层显示异常](domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active) — tui

### 命令浮层
- [分屏模式下非活跃 Session 命令浮层显示异常](domains/tui.md#issue_2026-05-12-split-session-command-hint-only-shows-active) — tui

### 插件懒初始化
- [~/.claude 目录不存在时插件面板 Discover/Marketplaces 视图无法使用](domains/plugin.md#issue_2026-05-18-claude-dir-missing-plugin-panel-empty) — plugin

### 指示符号统一
- [TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙](domains/tui.md#issue_2026-05-18-tui-dot-and-scrollbar-rendering) — tui

### 滚动条 track
- [TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙](domains/tui.md#issue_2026-05-18-tui-dot-and-scrollbar-rendering) — tui

### 面板拆分
- [Panel 文件过度肥大：mcp_panel.rs + login_panel.rs + setup_wizard.rs](domains/tui.md#issue_2026-05-17-panel-heavy-files) — tui

### 领域类型分离
- [langfuse-client/src/types.rs 所有类型定义集中（1008 行）](domains/langfuse.md#issue_2026-05-17-langfuse-types-monolithic) — langfuse

### ACP 路由
- [ACP 协议能力缺失：Session 生命周期路由、能力声明、SessionUpdate 通知变体](domains/acp.md#issue_2026-05-19-acp-missing-capabilities) — acp

### CJK
- [Markdown 表格第一列中文多时列宽过窄，CJK 字符显示为竖排](domains/tui-widgets.md#issue_2026-05-18-md-table-cjk-column-too-narrow) — tui-widgets

### ConfigOptionUpdate
- [ACP 状态变更后不发通知：ConfigOptionUpdate / AvailableCommandsUpdate / SessionInfoUpdate](domains/acp.md#issue_2026-05-19-acp-missing-state-notifications) — acp

### PromptResult
- [ACP PromptResponse 的 StopReason 全部硬编码为 EndTurn，无法区分取消/超限/正常完成](domains/acp.md#issue_2026-05-19-acp-stopreason-hardcoded-endturn) — acp

### Session 生命周期
- [ACP 协议能力缺失：Session 生命周期路由、能力声明、SessionUpdate 通知变体](domains/acp.md#issue_2026-05-19-acp-missing-capabilities) — acp

### SessionUpdate
- [ACP 协议能力缺失：Session 生命周期路由、能力声明、SessionUpdate 通知变体](domains/acp.md#issue_2026-05-19-acp-missing-capabilities) — acp
- [ACP 状态变更后不发通知：ConfigOptionUpdate / AvailableCommandsUpdate / SessionInfoUpdate](domains/acp.md#issue_2026-05-19-acp-missing-state-notifications) — acp

### StopReason
- [ACP PromptResponse 的 StopReason 全部硬编码为 EndTurn，无法区分取消/超限/正常完成](domains/acp.md#issue_2026-05-19-acp-stopreason-hardcoded-endturn) — acp

### SubAgent ID 重复
- [并发同类型 SubAgent 共享相同 ID，导致事件路由错误到第一个实例](domains/agent.md#issue_2026-05-19-concurrent-subagent-duplicate-id) — agent

### SubAgent 重复卡片
- [SubAgent 完成后显示重复卡片：ToolStart 和 SubAgentStart 事件双重创建 SubAgentState](domains/message-pipeline.md#issue_2026-05-18-subagent-duplicate-state-on-completion) — message-pipeline

### ToolStart/SubAgentStart 竞态
- [SubAgent 完成后显示重复卡片：ToolStart 和 SubAgentStart 事件双重创建 SubAgentState](domains/message-pipeline.md#issue_2026-05-18-subagent-duplicate-state-on-completion) — message-pipeline

### auto-continue
- [Compact 自动继续功能在不应触发的场景（手动 /compact、Done 后 auto-compact）下仍然 resubmit](domains/compact.md#issue_2026-05-12-compact-auto-continue-scenarios) — compact

### compact 触发来源
- [Compact 自动继续功能在不应触发的场景（手动 /compact、Done 后 auto-compact）下仍然 resubmit](domains/compact.md#issue_2026-05-12-compact-auto-continue-scenarios) — compact

### instructions 参数
- [Compact 自动继续功能在不应触发的场景（手动 /compact、Done 后 auto-compact）下仍然 resubmit](domains/compact.md#issue_2026-05-12-compact-auto-continue-scenarios) — compact

### resubmit 控制
- [Compact 自动继续功能在不应触发的场景（手动 /compact、Done 后 auto-compact）下仍然 resubmit](domains/compact.md#issue_2026-05-12-compact-auto-continue-scenarios) — compact

### tool_call_id
- [并发同类型 SubAgent 共享相同 ID，导致事件路由错误到第一个实例](domains/agent.md#issue_2026-05-19-concurrent-subagent-duplicate-id) — agent

### 列宽缩放
- [Markdown 表格第一列中文多时列宽过窄，CJK 字符显示为竖排](domains/tui-widgets.md#issue_2026-05-18-md-table-cjk-column-too-narrow) — tui-widgets

### 单函数过长
- [超长函数拆分：event.rs（1120 行）和 agent_ops.rs（890 行）等 5 个超长单函数拆分](domains/code-architecture.md#issue_2026-05-14-mega-functions-split) — code-architecture

### 双重创建
- [SubAgent 完成后显示重复卡片：ToolStart 和 SubAgentStart 事件双重创建 SubAgentState](domains/message-pipeline.md#issue_2026-05-18-subagent-duplicate-state-on-completion) — message-pipeline

### 多客户端
- [ACP 状态变更后不发通知：ConfigOptionUpdate / AvailableCommandsUpdate / SessionInfoUpdate](domains/acp.md#issue_2026-05-19-acp-missing-state-notifications) — acp

### 并发路由
- [并发同类型 SubAgent 共享相同 ID，导致事件路由错误到第一个实例](domains/agent.md#issue_2026-05-19-concurrent-subagent-duplicate-id) — agent

### 枚举映射
- [ACP PromptResponse 的 StopReason 全部硬编码为 EndTurn，无法区分取消/超限/正常完成](domains/acp.md#issue_2026-05-19-acp-stopreason-hardcoded-endturn) — acp

### 最小宽度
- [Markdown 表格第一列中文多时列宽过窄，CJK 字符显示为竖排](domains/tui-widgets.md#issue_2026-05-18-md-table-cjk-column-too-narrow) — tui-widgets

### 终止原因
- [ACP PromptResponse 的 StopReason 全部硬编码为 EndTurn，无法区分取消/超限/正常完成](domains/acp.md#issue_2026-05-19-acp-stopreason-hardcoded-endturn) — acp

### 能力声明
- [ACP 协议能力缺失：Session 生命周期路由、能力声明、SessionUpdate 通知变体](domains/acp.md#issue_2026-05-19-acp-missing-capabilities) — acp

### 认知复杂度
- [超长函数拆分：event.rs（1120 行）和 agent_ops.rs（890 行）等 5 个超长单函数拆分](domains/code-architecture.md#issue_2026-05-14-mega-functions-split) — code-architecture

### 表格渲染
- [Markdown 表格第一列中文多时列宽过窄，CJK 字符显示为竖排](domains/tui-widgets.md#issue_2026-05-18-md-table-cjk-column-too-narrow) — tui-widgets

### 身份传播
- [并发同类型 SubAgent 共享相同 ID，导致事件路由错误到第一个实例](domains/agent.md#issue_2026-05-19-concurrent-subagent-duplicate-id) — agent

### 状态同步
- [ACP 状态变更后不发通知：ConfigOptionUpdate / AvailableCommandsUpdate / SessionInfoUpdate](domains/acp.md#issue_2026-05-19-acp-missing-state-notifications) — acp

### session恢复
- [Session 恢复后 System Prompt 和 Compact Summary 被渲染为可见消息](domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt) — message-pipeline

### System消息过滤
- [Session 恢复后 System Prompt 和 Compact Summary 被渲染为可见消息](domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt) — message-pipeline

### messages_to_view_models
- [Session 恢复后 System Prompt 和 Compact Summary 被渲染为可见消息](domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt) — message-pipeline

### SystemNote泄漏
- [Session 恢复后 System Prompt 和 Compact Summary 被渲染为可见消息](domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt) — message-pipeline

### /compact 命令
- [手动 /compact 命令作为普通文本发给 LLM 未触发压缩](domains/compact.md#issue_2026-05-20-compact-command-not-triggering) — compact

### ACP compact 通道
- [手动 /compact 命令作为普通文本发给 LLM 未触发压缩](domains/compact.md#issue_2026-05-20-compact-command-not-triggering) — compact

### loading spinner
- [手动 /compact 命令作为普通文本发给 LLM 未触发压缩](domains/compact.md#issue_2026-05-20-compact-command-not-triggering) — compact

### session 同步
- [手动 /compact 命令作为普通文本发给 LLM 未触发压缩](domains/compact.md#issue_2026-05-20-compact-command-not-triggering) — compact

### compact messages 为空
- [Auto compact 摘要放入 BaseMessage::system 导致 LLM 适配器提取后 messages 数组为空](domains/compact.md#issue_2026-05-20-auto-compact-empty-messages-400) — compact

### BaseMessage::system vs human
- [Auto compact 摘要放入 BaseMessage::system 导致 LLM 适配器提取后 messages 数组为空](domains/compact.md#issue_2026-05-20-auto-compact-empty-messages-400) — compact

### LLM 适配器提取
- [Auto compact 摘要放入 BaseMessage::system 导致 LLM 适配器提取后 messages 数组为空](domains/compact.md#issue_2026-05-20-auto-compact-empty-messages-400) — compact

### DeepSeek 400
- [Auto compact 摘要放入 BaseMessage::system 导致 LLM 适配器提取后 messages 数组为空](domains/compact.md#issue_2026-05-20-auto-compact-empty-messages-400) — compact

### round_start_vm_idx
- [LLM 错误路径 round_start_vm_idx 被重置后视图清空闪烁](domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker) — message-pipeline

### AgentExecutionFailed
- [LLM 错误路径 round_start_vm_idx 被重置后视图清空闪烁](domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker) — message-pipeline

### 视图清空
- [LLM 错误路径 round_start_vm_idx 被重置后视图清空闪烁](domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker) — message-pipeline

### LLM 错误路径
- [LLM 错误路径 round_start_vm_idx 被重置后视图清空闪烁](domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker) — message-pipeline

### Frozen Session Data
- [System Prompt 每轮重复注入 prepend_message 导致上下文倍数膨胀](domains/system-prompt.md#issue_2026-05-20-rapid-context-expansion) — system-prompt

### system prompt 膨胀
- [System Prompt 每轮重复注入 prepend_message 导致上下文倍数膨胀](domains/system-prompt.md#issue_2026-05-20-rapid-context-expansion) — system-prompt

### StateSnapshot
- [System Prompt 每轮重复注入 prepend_message 导致上下文倍数膨胀](domains/system-prompt.md#issue_2026-05-20-rapid-context-expansion) — system-prompt

### 上下文爆炸
- [System Prompt 每轮重复注入 prepend_message 导致上下文倍数膨胀](domains/system-prompt.md#issue_2026-05-20-rapid-context-expansion) — system-prompt

### VS Code 终端
- [Mac Option+Backspace 在 VS Code 终端被映射为 PageUp 导致滚动而非删除](domains/tui.md#issue_2026-05-12-macos-option-backspace-scrolls-when-content-present) — tui

### Option+Backspace
- [Mac Option+Backspace 在 VS Code 终端被映射为 PageUp 导致滚动而非删除](domains/tui.md#issue_2026-05-12-macos-option-backspace-scrolls-when-content-present) — tui

### PageUp 映射
- [Mac Option+Backspace 在 VS Code 终端被映射为 PageUp 导致滚动而非删除](domains/tui.md#issue_2026-05-12-macos-option-backspace-scrolls-when-content-present) — tui

### 词删除
- [Mac Option+Backspace 在 VS Code 终端被映射为 PageUp 导致滚动而非删除](domains/tui.md#issue_2026-05-12-macos-option-backspace-scrolls-when-content-present) — tui

### TERM_PROGRAM
- [Mac Option+Backspace 在 VS Code 终端被映射为 PageUp 导致滚动而非删除](domains/tui.md#issue_2026-05-12-macos-option-backspace-scrolls-when-content-present) — tui

### AskUser弹窗
- [AskUser 弹窗内容溢出不可滚动且选项描述丢失](domains/tui.md#issue_2026-05-23-ask-user-overflow-and-description-missing) — tui

### Elicitation description
- [AskUser 弹窗内容溢出不可滚动且选项描述丢失](domains/tui.md#issue_2026-05-23-ask-user-overflow-and-description-missing) — tui

### Arc共享配置
- [Setup 向导完成后 ACP Server 配置未刷新，API key 未生效](domains/tui.md#issue_2026-05-21-setup-wizard-settings-not-reloaded) — tui

### Ctrl+C取消
- [Ctrl+C 在流式输出和工具执行中 UI 中断但底层请求未停止](domains/tui.md#issue_2026-05-24-cancel-ineffective-during-streaming-and-tool-execution) — tui

### /clear命令
- [/clear 命令只清 TUI 界面，不清 ACP Server 上下文](domains/tui.md#issue_2026-05-21-clear-command-doesnt-clear-live-context) — tui

### 历史消息清理
- [/clear 命令只清 TUI 界面，不清 ACP Server 上下文](domains/tui.md#issue_2026-05-21-clear-command-doesnt-clear-live-context) — tui

### AgentPool
- [build_agent 每轮重建大对象产生瞬态分配碎片](domains/agent.md#issue_2026-05-24-build-agent-per-turn-arc-transient-fragmentation) — agent

### LLM实例复用
- [build_agent 每轮重建大对象产生瞬态分配碎片](domains/agent.md#issue_2026-05-24-build-agent-per-turn-arc-transient-fragmentation) — agent

### jemalloc碎片
- [build_agent 每轮重建大对象产生瞬态分配碎片](domains/agent.md#issue_2026-05-24-build-agent-per-turn-arc-transient-fragmentation) — agent

### bg_event_sender
- [Background Agent 完成后 SubAgent 卡片消失且无数据回传](domains/agent.md#issue_2026-05-23-background-agent-card-disappears-no-result) — agent

### Compact后系统提示词
- [Compact 后 Langfuse 遥测丢失系统提示词](domains/langfuse.md#issue_2026-05-23-langfuse-missing-system-prompt-after-compact) — langfuse

### System消息前缀
- [Compact 后 Langfuse 遥测丢失系统提示词](domains/langfuse.md#issue_2026-05-23-langfuse-missing-system-prompt-after-compact) — langfuse

### Langfuse OTLP
- [Langfuse agent-run 根节点缺失（native ingestion 迁移后回归）](domains/langfuse.md#issue_2026-05-23-langfuse-agent-run-root-missing) — langfuse

### native ingestion
- [Langfuse agent-run 根节点缺失（native ingestion 迁移后回归）](domains/langfuse.md#issue_2026-05-23-langfuse-agent-run-root-missing) — langfuse

### skip_serializing_if
- [Langfuse agent-run 根节点缺失（native ingestion 迁移后回归）](domains/langfuse.md#issue_2026-05-23-langfuse-agent-run-root-missing) — langfuse

### Deferred Tools
- [Deferred Tools 段注入 system prompt 动态区域导致跨会话 Cache 部分失效](domains/system-prompt.md#issue_2026-05-23-mcp-tools-instability-breaks-anthropic-cache) — system-prompt

### -c/--continue
- [-c/--continue 未实现：启动后显示空会话](domains/cli.md#issue_2026-05-23-continue-flag-not-implemented) — cli

### 会话恢复
- [-c/--continue 未实现：启动后显示空会话](domains/cli.md#issue_2026-05-23-continue-flag-not-implemented) — cli

### 图片粘贴
- [TUI 粘贴图片后 LLM 仅收到文本而非图片内容](domains/acp.md#issue_2026-05-23-tui-image-sent-as-text) — acp

### MessageContent
- [TUI 粘贴图片后 LLM 仅收到文本而非图片内容](domains/acp.md#issue_2026-05-23-tui-image-sent-as-text) — acp

### 多模态数据流
- [TUI 粘贴图片后 LLM 仅收到文本而非图片内容](domains/acp.md#issue_2026-05-23-tui-image-sent-as-text) — acp

### ACP InitializeResponse
- [ACP Stdio InitializeResponse 缺少 session 能力声明，Zed 客户端报错](domains/acp.md#issue_2026-05-21-acp-stdio-missing-session-capabilities) — acp

### session_capabilities
- [ACP Stdio InitializeResponse 缺少 session 能力声明，Zed 客户端报错](domains/acp.md#issue_2026-05-21-acp-stdio-missing-session-capabilities) — acp

### ACP stdio权限
- [ACP stdio 默认权限模式为 AutoMode，与 TUI/-p 模式不一致](domains/acp.md#issue_2026-05-21-acp-stdio-default-permission-should-be-bypass) — acp

### 默认Bypass
- [ACP stdio 默认权限模式为 AutoMode，与 TUI/-p 模式不一致](domains/acp.md#issue_2026-05-21-acp-stdio-default-permission-should-be-bypass) — acp

### AvailableCommands
- [Skills 未作为 ACP AvailableCommands 传递给 IDE 客户端](domains/acp.md#issue_2026-05-21-skills-not-passed-as-acp-commands) — acp

### FrozenSessionData
- [Skills 未作为 ACP AvailableCommands 传递给 IDE 客户端](domains/acp.md#issue_2026-05-21-skills-not-passed-as-acp-commands) — acp

### include!宏分组
- [TUI app/ 目录模块化拆分——48 个子模块、多个 1000+ 行文件](domains/code-architecture.md#issue_2026-05-14-tui-app-mod-decomposition) — code-architecture

### Theme trait
- [Markdown 与 Theme 颜色体系脱节，存在多处分叉硬编码](domains/tui-widgets.md#issue_2026-05-20-theme-markdown-color-decoupling) — tui-widgets

### 颜色解耦
- [Markdown 与 Theme 颜色体系脱节，存在多处分叉硬编码](domains/tui-widgets.md#issue_2026-05-20-theme-markdown-color-decoupling) — tui-widgets

### MarkdownTheme
- [Markdown 与 Theme 颜色体系脱节，存在多处分叉硬编码](domains/tui-widgets.md#issue_2026-05-20-theme-markdown-color-decoupling) — tui-widgets

### 适配器模式
- [Markdown 与 Theme 颜色体系脱节，存在多处分叉硬编码](domains/tui-widgets.md#issue_2026-05-20-theme-markdown-color-decoupling) — tui-widgets

### Ctrl+C 中断
- [Ctrl+C 中断后支持撤回并重发上一条用户消息](domains/agent.md#issue_2026-05-25-interrupt-undo-last-user-message) — agent

### 消息撤回
- [Ctrl+C 中断后支持撤回并重发上一条用户消息](domains/agent.md#issue_2026-05-25-interrupt-undo-last-user-message) — agent

### 事件路由
- [Ctrl+C 中断后支持撤回并重发上一条用户消息](domains/agent.md#issue_2026-05-25-interrupt-undo-last-user-message) — agent
- [同步 SubAgent Ctrl+C 中断——handle_interrupted() 的 in_subagent() 守卫静默吞掉父 Agent 中断事件](domains/agent.md#issue_2026-05-26-sync-subagent-cancel-fix-attempts-log) — agent

### 历史回滚
- [Ctrl+C 中断后支持撤回并重发上一条用户消息](domains/agent.md#issue_2026-05-25-interrupt-undo-last-user-message) — agent

### 索引漂移
- [Ctrl+C 中断后支持撤回并重发上一条用户消息](domains/agent.md#issue_2026-05-25-interrupt-undo-last-user-message) — agent

### 并发 background agent
- [并发 Background Agent 只收到一次完成通知，父 Agent 永久等待](domains/agent.md#issue_2026-05-24-concurrent-bg-agent-only-one-completion) — agent

### TOCTOU
- [并发 Background Agent 只收到一次完成通知，父 Agent 永久等待](domains/agent.md#issue_2026-05-24-concurrent-bg-agent-only-one-completion) — agent

### 事件丢失
- [并发 Background Agent 只收到一次完成通知，父 Agent 永久等待](domains/agent.md#issue_2026-05-24-concurrent-bg-agent-only-one-completion) — agent

### 竞态
- [并发 Background Agent 只收到一次完成通知，父 Agent 永久等待](domains/agent.md#issue_2026-05-24-concurrent-bg-agent-only-one-completion) — agent

### SubAgent Ctrl+C
- [同步 SubAgent Ctrl+C 中断——handle_interrupted() 的 in_subagent() 守卫静默吞掉父 Agent 中断事件](domains/agent.md#issue_2026-05-26-sync-subagent-cancel-fix-attempts-log) — agent

### handle_interrupted
- [同步 SubAgent Ctrl+C 中断——handle_interrupted() 的 in_subagent() 守卫静默吞掉父 Agent 中断事件](domains/agent.md#issue_2026-05-26-sync-subagent-cancel-fix-attempts-log) — agent

### 二分法追踪
- [同步 SubAgent Ctrl+C 中断——handle_interrupted() 的 in_subagent() 守卫静默吞掉父 Agent 中断事件](domains/agent.md#issue_2026-05-26-sync-subagent-cancel-fix-attempts-log) — agent

### Anthropic 400
- [AtMention/SkillPreload 注入的 fake Read 工具消息导致 Anthropic API 400 错误](domains/agent.md#issue_2026-05-25-fake-read-tool-message-anthropic-400) — agent

### fake Read
- [AtMention/SkillPreload 注入的 fake Read 工具消息导致 Anthropic API 400 错误](domains/agent.md#issue_2026-05-25-fake-read-tool-message-anthropic-400) — agent
- [主 Agent SkillPreloadMiddleware preload_skills 硬编码为空，/skill-name 不注入全文](domains/agent.md#issue_2026-05-25-skill-preload-no-tool-calls-in-history) — agent

### messages_to_anthropic
- [AtMention/SkillPreload 注入的 fake Read 工具消息导致 Anthropic API 400 错误](domains/agent.md#issue_2026-05-25-fake-read-tool-message-anthropic-400) — agent

### preload_skills
- [主 Agent SkillPreloadMiddleware preload_skills 硬编码为空，/skill-name 不注入全文](domains/agent.md#issue_2026-05-25-skill-preload-no-tool-calls-in-history) — agent

### middleware self-detection
- [主 Agent SkillPreloadMiddleware preload_skills 硬编码为空，/skill-name 不注入全文](domains/agent.md#issue_2026-05-25-skill-preload-no-tool-calls-in-history) — agent

### compact loading
- [手动 /compact 后聊天区域长时间显示 loading 骨架屏（30s+）](domains/compact.md#issue_2026-05-26-manual-compact-long-loading-skeleton) — compact

### set_loading
- [手动 /compact 后聊天区域长时间显示 loading 骨架屏（30s+）](domains/compact.md#issue_2026-05-26-manual-compact-long-loading-skeleton) — compact

### manual vs auto compact
- [手动 /compact 后聊天区域长时间显示 loading 骨架屏（30s+）](domains/compact.md#issue_2026-05-26-manual-compact-long-loading-skeleton) — compact

### 路径标志
- [手动 /compact 后聊天区域长时间显示 loading 骨架屏（30s+）](domains/compact.md#issue_2026-05-26-manual-compact-long-loading-skeleton) — compact

### mimalloc
- [mimalloc 替换 jemalloc 后内存峰值反而更高，回退到系统默认分配器](domains/code-architecture.md#issue_2026-05-25-mimalloc-worse-than-jemalloc) — code-architecture

### global allocator
- [mimalloc 替换 jemalloc 后内存峰值反而更高，回退到系统默认分配器](domains/code-architecture.md#issue_2026-05-25-mimalloc-worse-than-jemalloc) — code-architecture

### 内存分配器
- [mimalloc 替换 jemalloc 后内存峰值反而更高，回退到系统默认分配器](domains/code-architecture.md#issue_2026-05-25-mimalloc-worse-than-jemalloc) — code-architecture

### 基准测试
- [mimalloc 替换 jemalloc 后内存峰值反而更高，回退到系统默认分配器](domains/code-architecture.md#issue_2026-05-25-mimalloc-worse-than-jemalloc) — code-architecture

### hardcoded Chinese
- [Login 面板硬编码中文字符串未走 i18n，切换英文后仍显示中文](domains/tui.md#issue_2026-05-26-login-panel-hardcoded-chinese-no-i18n) — tui

### login panel
- [Login 面板硬编码中文字符串未走 i18n，切换英文后仍显示中文](domains/tui.md#issue_2026-05-26-login-panel-hardcoded-chinese-no-i18n) — tui

### LcRegistry
- [Login 面板硬编码中文字符串未走 i18n，切换英文后仍显示中文](domains/tui.md#issue_2026-05-26-login-panel-hardcoded-chinese-no-i18n) — tui


### Anthropic 400
- [SkillPreload 触发 Anthropic 400 Bad Request：tool_result 缺少配对 tool_use](domains/agent.md#issue_2026-05-26-skillpreload-anthropic-400-tool-result-orphan) — agent

### AtomicBool
- [Micro Compact 重复触发，每轮工具调用后都显示"自动清理"通知](domains/compact.md#issue_2026-05-23-micro-compact-repeated-triggering) — compact

### Ctrl+C interrupt
- [Ctrl+C 中断后继续对话时 agent 丢失当前轮次上下文](domains/agent.md#issue_2026-05-26-ctrl-c-interrupt-causes-agent-amnesia) — agent
- [Ctrl+C 无法中断同步 SubAgent，需等待其自然结束后父 Agent 才被中断](domains/agent.md#issue_2026-05-25-ctrl-c-cannot-interrupt-sync-subagent) — agent

### LLM language drift
- [系统提示词缺少语言指示段落，AI 多轮对话后漂移至英文](domains/system-prompt.md#issue_2026-05-27-system-prompt-missing-language-instruction) — system-prompt

### Language instruction
- [系统提示词缺少语言指示段落，AI 多轮对话后漂移至英文](domains/system-prompt.md#issue_2026-05-27-system-prompt-missing-language-instruction) — system-prompt

### MCP transport
- [Windows 下 Command::new() 无法执行 .cmd 脚本，需统一跨平台 spawn 封装](domains/mcp.md#issue_2026-05-27-cross-platform-spawn-wrapper) — mcp

### SubAgent language drift
- [语言段落注入导致 SubAgent 语言漂移和缓存隔离失效](domains/agent.md#issue_2026-05-27-language-injection-subagent-drift-cache-isolation) — agent

### Windows cmd
- [Windows 下 Command::new() 无法执行 .cmd 脚本，需统一跨平台 spawn 封装](domains/mcp.md#issue_2026-05-27-cross-platform-spawn-wrapper) — mcp

### Windows paste
- [Windows 输入框粘贴多行内容被截断为单行发送](domains/tui.md#issue_2026-05-26-windows-paste-multiline-truncated) — tui

### add_message vs prepend_message
- [SkillPreload 触发 Anthropic 400 Bad Request：tool_result 缺少配对 tool_use](domains/agent.md#issue_2026-05-26-skillpreload-anthropic-400-tool-result-orphan) — agent

### agent amnesia
- [Ctrl+C 中断后继续对话时 agent 丢失当前轮次上下文](domains/agent.md#issue_2026-05-26-ctrl-c-interrupt-causes-agent-amnesia) — agent

### agent lifecycle
- [Compact 后 Resubmit 缺少 Loading Spinner](domains/tui.md#issue_2026-05-25-compact-resubmit-missing-loading-spinner) — tui

### bracketed paste
- [Windows 输入框粘贴多行内容被截断为单行发送](domains/tui.md#issue_2026-05-26-windows-paste-multiline-truncated) — tui

### cache isolation
- [语言段落注入导致 SubAgent 语言漂移和缓存隔离失效](domains/agent.md#issue_2026-05-27-language-injection-subagent-drift-cache-isolation) — agent

### cancel propagation
- [Ctrl+C 无法中断同步 SubAgent，需等待其自然结束后父 Agent 才被中断](domains/agent.md#issue_2026-05-25-ctrl-c-cannot-interrupt-sync-subagent) — agent

### cancel token
- [Ctrl+C 无法中断同步 SubAgent，需等待其自然结束后父 Agent 才被中断](domains/agent.md#issue_2026-05-25-ctrl-c-cannot-interrupt-sync-subagent) — agent

### cancelled state
- [Ctrl+C 中断后继续对话时 agent 丢失当前轮次上下文](domains/agent.md#issue_2026-05-26-ctrl-c-interrupt-causes-agent-amnesia) — agent

### compact resubmit
- [Compact 后 Resubmit 缺少 Loading Spinner](domains/tui.md#issue_2026-05-25-compact-resubmit-missing-loading-spinner) — tui

### cross-platform
- [Windows 输入框粘贴多行内容被截断为单行发送](domains/tui.md#issue_2026-05-26-windows-paste-multiline-truncated) — tui

### cross-platform spawn
- [Windows 下 Command::new() 无法执行 .cmd 脚本，需统一跨平台 spawn 封装](domains/mcp.md#issue_2026-05-27-cross-platform-spawn-wrapper) — mcp

### frozen_language
- [语言段落注入导致 SubAgent 语言漂移和缓存隔离失效](domains/agent.md#issue_2026-05-27-language-injection-subagent-drift-cache-isolation) — agent
- [系统提示词缺少语言指示段落，AI 多轮对话后漂移至英文](domains/system-prompt.md#issue_2026-05-27-system-prompt-missing-language-instruction) — system-prompt

### history truncation
- [Ctrl+C 中断后继续对话时 agent 丢失当前轮次上下文](domains/agent.md#issue_2026-05-26-ctrl-c-interrupt-causes-agent-amnesia) — agent

### hooks executor
- [Windows 下 Command::new() 无法执行 .cmd 脚本，需统一跨平台 spawn 封装](domains/mcp.md#issue_2026-05-27-cross-platform-spawn-wrapper) — mcp

### last_idx fallback
- [语言段落注入导致 SubAgent 语言漂移和缓存隔离失效](domains/agent.md#issue_2026-05-27-language-injection-subagent-drift-cache-isolation) — agent

### layout jitter
- [思考内容只显示最后一行导致自动换行布局抖动](domains/tui.md#issue_2026-05-23-thinking-tail-single-line-layout-jitter) — tui

### loading spinner
- [Compact 后 Resubmit 缺少 Loading Spinner](domains/tui.md#issue_2026-05-25-compact-resubmit-missing-loading-spinner) — tui

### micro compact
- [Micro Compact 重复触发，每轮工具调用后都显示"自动清理"通知](domains/compact.md#issue_2026-05-23-micro-compact-repeated-triggering) — compact

### mouse drag
- [消息区域滚动条滑块位置与鼠标可拖拽位置不对齐](domains/tui.md#issue_2026-05-27-message-area-scrollbar-thumb-misaligned) — tui

### multiline input
- [Windows 输入框粘贴多行内容被截断为单行发送](domains/tui.md#issue_2026-05-26-windows-paste-multiline-truncated) — tui

### once-per-prompt guard
- [Micro Compact 重复触发，每轮工具调用后都显示"自动清理"通知](domains/compact.md#issue_2026-05-23-micro-compact-repeated-triggering) — compact

### prepended_ids
- [SkillPreload 触发 Anthropic 400 Bad Request：tool_result 缺少配对 tool_use](domains/agent.md#issue_2026-05-26-skillpreload-anthropic-400-tool-result-orphan) — agent

### ratatui formula
- [消息区域滚动条滑块位置与鼠标可拖拽位置不对齐](domains/tui.md#issue_2026-05-27-message-area-scrollbar-thumb-misaligned) — tui

### repeated triggering
- [Micro Compact 重复触发，每轮工具调用后都显示"自动清理"通知](domains/compact.md#issue_2026-05-23-micro-compact-repeated-triggering) — compact

### scrollbar alignment
- [消息区域滚动条滑块位置与鼠标可拖拽位置不对齐](domains/tui.md#issue_2026-05-27-message-area-scrollbar-thumb-misaligned) — tui

### shell_command
- [Windows 下 Command::new() 无法执行 .cmd 脚本，需统一跨平台 spawn 封装](domains/mcp.md#issue_2026-05-27-cross-platform-spawn-wrapper) — mcp

### single-line wrap
- [思考内容只显示最后一行导致自动换行布局抖动](domains/tui.md#issue_2026-05-23-thinking-tail-single-line-layout-jitter) — tui

### sync SubAgent
- [Ctrl+C 无法中断同步 SubAgent，需等待其自然结束后父 Agent 才被中断](domains/agent.md#issue_2026-05-25-ctrl-c-cannot-interrupt-sync-subagent) — agent

### system prompt
- [系统提示词缺少语言指示段落，AI 多轮对话后漂移至英文](domains/system-prompt.md#issue_2026-05-27-system-prompt-missing-language-instruction) — system-prompt

### tail_lines
- [思考内容只显示最后一行导致自动换行布局抖动](domains/tui.md#issue_2026-05-23-thinking-tail-single-line-layout-jitter) — tui

### thinking display
- [思考内容只显示最后一行导致自动换行布局抖动](domains/tui.md#issue_2026-05-23-thinking-tail-single-line-layout-jitter) — tui

### thumb geometry
- [消息区域滚动条滑块位置与鼠标可拖拽位置不对齐](domains/tui.md#issue_2026-05-27-message-area-scrollbar-thumb-misaligned) — tui

### tool_result orphan
- [SkillPreload 触发 Anthropic 400 Bad Request：tool_result 缺少配对 tool_use](domains/agent.md#issue_2026-05-26-skillpreload-anthropic-400-tool-result-orphan) — agent

### SSE UTF-8 截断
- [SSE 流式解析跨 chunk UTF-8 截断产生乱码（U+FFFD）](domains/agent.md#issue_2026-05-29-sse-utf8-truncation-mojibake) — agent

### from_utf8_lossy
- [SSE 流式解析跨 chunk UTF-8 截断产生乱码（U+FFFD）](domains/agent.md#issue_2026-05-29-sse-utf8-truncation-mojibake) — agent

### pending_bytes
- [SSE 流式解析跨 chunk UTF-8 截断产生乱码（U+FFFD）](domains/agent.md#issue_2026-05-29-sse-utf8-truncation-mojibake) — agent

### CJK 乱码
- [SSE 流式解析跨 chunk UTF-8 截断产生乱码（U+FFFD）](domains/agent.md#issue_2026-05-29-sse-utf8-truncation-mojibake) — agent

### push_done 缺失
- [Immediate 命令（/compact、/clear）执行后 TUI 永久卡在 loading 状态](domains/agent.md#issue_2026-05-29-immediate-command-missing-push-done) — agent

### Immediate 命令
- [Immediate 命令（/compact、/clear）执行后 TUI 永久卡在 loading 状态](domains/agent.md#issue_2026-05-29-immediate-command-missing-push-done) — agent

### 并发 prompt 竞争
- [Immediate 命令（/compact、/clear）执行后 TUI 永久卡在 loading 状态](domains/agent.md#issue_2026-05-29-immediate-command-missing-push-done) — agent

### JSON 格式不一致
- [/compact 显示"未知命令"——AvailableCommandsUpdate 通知 JSON 格式不匹配被静默丢弃](domains/agent.md#issue_2026-05-29-available-commands-update-format-mismatch) — agent

### SessionNotification
- [/compact 显示"未知命令"——AvailableCommandsUpdate 通知 JSON 格式不匹配被静默丢弃](domains/agent.md#issue_2026-05-29-available-commands-update-format-mismatch) — agent

### serde tag 字段名
- [ACP 大重构后所有流式事件静默丢失——字段名 "type" vs "sessionUpdate" 不匹配](domains/agent.md#issue_2026-05-29-acp-session-update-field-name-mismatch) — agent

### sessionUpdate vs type
- [ACP 大重构后所有流式事件静默丢失——字段名 "type" vs "sessionUpdate" 不匹配](domains/agent.md#issue_2026-05-29-acp-session-update-field-name-mismatch) — agent

### 事件静默丢失
- [ACP 大重构后所有流式事件静默丢失——字段名 "type" vs "sessionUpdate" 不匹配](domains/agent.md#issue_2026-05-29-acp-session-update-field-name-mismatch) — agent

### /clear session 泄漏
- [/clear 后 ACP Server 端 history 未清理，新会话延续旧上下文](domains/agent.md#issue_2026-05-29-clear-keeps-acp-server-history) — agent

### reset_session
- [/clear 后 ACP Server 端 history 未清理，新会话延续旧上下文](domains/agent.md#issue_2026-05-29-clear-keeps-acp-server-history) — agent

### ACP session 状态不一致
- [/clear 后 ACP Server 端 history 未清理，新会话延续旧上下文](domains/agent.md#issue_2026-05-29-clear-keeps-acp-server-history) — agent

### prompt_complete
- [统一 Token Usage 传递：引入 prompt_complete 事件替代双路径冗余](domains/agent.md#issue_2026-05-29-unify-token-usage-prompt-complete) — agent

### token usage 双路径
- [统一 Token Usage 传递：引入 prompt_complete 事件替代双路径冗余](domains/agent.md#issue_2026-05-29-unify-token-usage-prompt-complete) — agent

### ToolEnd 工具名
- [ToolEnd 事件经 ACP bridge 后工具名丢失，显示为空字符串](domains/agent.md#issue_2026-05-29-tool-end-name-lost-in-acp-bridge) — agent

### ToolCallUpdate title
- [ToolEnd 事件经 ACP bridge 后工具名丢失，显示为空字符串](domains/agent.md#issue_2026-05-29-tool-end-name-lost-in-acp-bridge) — agent

### ACP event mapping
- [ToolEnd 事件经 ACP bridge 后工具名丢失，显示为空字符串](domains/agent.md#issue_2026-05-29-tool-end-name-lost-in-acp-bridge) — agent

### 插件依赖 CC
- [Peri 插件系统依赖 Claude Code 目录结构，未安装 CC 时安装/卸载不可用](domains/plugin.md#issue_2026-05-29-wsl-plugin-install-marketplace-uninstall-fail) — plugin

### marketplace git clone
- [Peri 插件系统依赖 Claude Code 目录结构，未安装 CC 时安装/卸载不可用](domains/plugin.md#issue_2026-05-29-wsl-plugin-install-marketplace-uninstall-fail) — plugin

## 更新记录

- 2026-05-29: 归档 8 个 issue（agent 7 + plugin 1），新增 22 个关键词索引
- 2026-05-27: 归档 16 个 issue（agent 4 + system-prompt 2 + compact 1 + tui 4 + mcp 1），新增 42 个关键词索引
- 2026-05-26: 归档 6 个 issue（agent 3 + compact 1 + code-architecture 1 + tui 1），新增 22 个关键词索引
- 2026-05-25: 归档 4 个 issue，新增关键词索引
- 2026-05-13: 首次创建，归档 22 个 issue，提取 14 条领域认知
- 2026-05-14: 第二次归档，归档 12 个 issue，提取 8 条领域认知（agent 2 + message-pipeline 2 + system-prompt 4）
- 2026-05-15: 第三次归档，归档 8 个 issue，提取 7 条领域认知（agent 3 + code-architecture 2 + message-pipeline 2 + tui 1）
- 2026-05-16: 第四次归档，归档 11 个 issue，提取 7 条领域认知（agent 6 + tui 1）
- 2026-05-16: 第五次归档，归档 13 个 issue，提取 11 条领域认知（tui 10 + message-pipeline 1 + cli 1 + tools 1）
- 2026-05-18: 归档 11 个 issue，新增 36 个新关键词条目（agent/tui/message-pipeline/acp/langfuse/code-architecture/plugin）
- 2026-05-17: 归档 12 个 issue，新增 ACP 领域，22 个新关键词条目
- 2026-05-17: 补充归档 2 个 Fixed issue（LLM 适配器模块化 + 后台任务竞态）
- 2026-05-18: 归档 11 个 Fixed issue（agent 1 + tui 4 + acp 1 + langfuse 1 + message-pipeline 1 + plugin 1 + code-architecture 2），13 个新关键词条目
- 2026-05-20: 归档 7 个 issue，新增 27 个关键词索引
- 2026-05-20: 归档 1 个 issue，新增 4 个关键词索引（session恢复/System消息过滤/messages_to_view_models/SystemNote泄漏）
- 2026-05-20: 归档 5 个 issue（compact 2, message-pipeline 1, system-prompt 1, tui 1），新增 24 个关键词索引
- 2026-05-24: 归档 15 个 issue（tui 4 + agent 2 + langfuse 2 + system-prompt 1 + cli 1 + acp 4 + code-architecture 1），新增 29 个关键词索引
