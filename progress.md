# Design Review Progress

## 2026-05-07 第43轮：langfuse-client re-export 收紧 + acpx-g 重复代码消除

langfuse-client：ingestion_events_to_otel 改为 pub(crate)，移除 lib.rs 中 11 个未使用 OTLP 类型 re-export（OtelAttribute/Span/Status 等）。acpx-g：消除 executor.rs 和 loader.rs 中重复定义的 node_id/node_depends 函数（共 3 处重复），统一复用 runner/mod.rs 的 pub 版本。净减 46 行，编译零警告，161 测试全通过。

## 2026-05-07 第42轮：LLM adapter builder 方法测试补充

为 ChatAnthropic 补充 6 个测试：with_base_url 设置与空串处理、with_extended_thinking（budget 低于 1024 钳位、有效值透传）、without_cache 关闭缓存、new 默认值验证。为 ChatOpenAI 补充 4 个测试：with_base_url、with_reasoning_effort、new 默认值（base_url/reasoning_effort）、o3 系列 context_window。避免 from_env 环境变量测试的并发安全问题。rust-create-agent 测试总数从 254 增至 264。

## 2026-05-07 第41轮：widgets 死代码删除 + compact re-export 收紧

删除 perihelion-widgets/render_state.rs 中 TableBuilder::render 方法（58 行，被 render_with_wrap 替代）和 make_data_line 函数（57 行，仅被已删除的 render 调用），共净减 115 行 #[allow(dead_code)] 标记的死代码。移除 compact/mod.rs 中 group_messages_by_round、adjust_index_to_preserve_invariants、MessageRound 三项无外部调用者的 re-export。全量测试通过，编译零警告。

## 2026-05-07 第40轮：MCP 模块 API 可见性收紧 + 死代码清除

将 mcp 模块中 10 个内部函数从 pub 收紧为 pub(crate)或 cfg(test)：config.rs 6 个（load_from_path/load_global_config/server_config_hash/expand_env_vars/expand_server_config_with_context/expand_server_config）、transport.rs build_transport、tool_bridge.rs assemble_tool_pool、callback_server.rs parse_callback_url。删除 assemble_tool_pool 死代码（22 行）和 build_transport 包装函数（测试改用 try_from）。移除 mod.rs 中 TransportConfig/TransportError 等 5 项无用 re-export。净减 27 行，编译零警告。

## 2026-05-07 第39轮：plugin installer 测试补充 + loader 可见性收紧 + 警告消除

为 cleanup_orphaned_plugins 补充 6 个异步测试覆盖完整生命周期（无缓存、旧孤儿删除、近期保留、已安装保护、空目录清理、无标记不删除）；为 sanitize_plugin_id 补充 3 个测试、match_project_path 补充 6 个测试。将 loader 中 4 个 extract_* 函数从 pub 收紧为 pub(crate)，移除 mod.rs 中未使用 re-export。声明 integration feature 消除 unexpected_cfgs 警告。测试总数 478。

## 2026-05-07 第38轮：FilesystemThreadStore 测试补充 + 警告清理

为关键数据持久化组件 FilesystemThreadStore 补充 13 个测试：CRUD 生命周期（create/append/load_meta/update_meta/list/delete）、边界场景（空消息、不存在 thread）、extract_title 纯函数（文本提取、50字符截断）。清理 filesystem.rs 测试未使用 import、subagent 测试 cloned_ref_to_slice_refs。测试总数 1403。

消除全部 9 个 clippy 警告：filesystem.rs 两处 sort_by 改为 sort_by_key(Reverse)；installer.rs 移除未使用 StdDuration；loader.rs PluginCommand 移至测试模块 import、CommandFrontmatter 改为 pub；render_state.rs if let 替代 is_some+unwrap、match guard 替代嵌套 if；types.rs 移除未使用 Path。1390 测试通过。

将 `fetch_github` 改为构造 URL 后委托 `fetch_git`，消除约 50 行重复的 git clone/pull 逻辑；将 `try_load_cache`/`extract_name` 改为 pub 直接暴露，删除无意义的 `_wrapper` 包装方法；为 `parse_marketplace_input` 补充 10 个单元测试覆盖 GitHub shorthand/URL/SSH/本地路径/NPM 等格式。修复 clippy 警告：`map_identity`(installer.rs 3处)、`double_ended_iterator_last`(marketplace.rs/panel_ops.rs)、`collapsible_match`(event.rs)、`unnecessary_unwrap`(event.rs)、`redundant_closure`(event.rs)、`useless_conversion`(re_inject.rs/message_render.rs)。`InstallScope` 改为 derive Default。1390 测试全通过。

## 2026-05-02 第35轮：修复 CI 失败的 test_subagent_group_basic 测试

测试 `test_subagent_group_basic` 断言渲染快照中包含步数数字 "2"，但 SubAgentGroup 渲染不显示 total_steps，导致 CI 失败。移除了基于渲染输出的步数断言，保留内部状态的 total_steps 验证（已有 assert_eq!(*total_steps, 2)）。全量测试 293 通过，0 失败。

## 2026-05-02 第34轮：langfuse-client + compact 审查与测试补充

审查 langfuse-client（client/batcher/config/error/types，26 个测试）和 compact 模块（micro/full/invariant/config/re_inject，35+ 个测试）。两个模块代码质量高、测试充分。发现 TokenTracker::reset() 缺少测试、ContextBudget 零窗口边界未覆盖。补充 3 个测试（reset 归零验证、零窗口 should_warn、零窗口 usage_percent 除零行为）。

## 2026-05-02 第33轮：perihelion-widgets 组件库测试补充

审查 perihelion-widgets 全部 11 个组件（list/scrollable/input_field/form/bordered_panel/tab_bar/checkbox/radio/tool_call/spinner/theme）。tool_call 模块仅 1 个测试，覆盖最低。为 format_indicator 补充 3 个状态测试；为 format_args_summary 补充 4 个截断测试；为 ToolCallState 补充 5 个测试（折叠/tick/result 分行/截断/状态相等）；为 ScrollState 补充 4 个边界测试。74 测试全通过。

## 2026-05-02 第31轮：核心框架 Code Review 与去重优化

审查 rust-create-agent 核心框架（executor/chain/LLM 适配层/state），合并 executor 中重复的 should_warn 调用和 pct 阈值判断；为 ChatAnthropic 显式声明 context_window；删除 grep.rs 中 115 行未使用的 parse_args 死代码；为 StopReason 补充 9 个单元测试。504 测试全通过。测试总数 293。

## 2026-05-02 第32轮：中间件层测试补充

审查 rust-agent-middlewares 全部模块（subagent/hitl/skills/cron/terminal/todo/agent_define/claude_agent_parser）。代码质量高、测试充分。为 format_agent_id 补充 5 个测试（kebab/snake/混合分隔符/单字/空串）；为 truncate_bytes 补充 6 个 UTF-8 安全测试（字符边界回退）；为 ToolsValue 补充 3 个解析格式测试。223 测试全通过。

## 2026-04-30 第30轮 — 第21轮：UX 打磨与 Bug 修复

| 轮 | 改动 | 测试 |
|---|---|---|
| 30 | Thread Browser 空列表添加引导提示 | 290 |
| 29 | Thread Browser 新建对话添加反馈消息 | 290 |
| 28 | 修复 Agent/Cron 面板描述截断的字节/字符混淆（.len→.chars().count） | 290 |
| 27 | Thread Browser 确认删除时面板高度不足导致提示被截断 | 290 |
| 26 | Thread Browser Ctrl+D 删除从立即执行改为两步确认 | 290 |
| 25 | Cron 面板关闭时清理 panel_selection/panel_area；Setup wizard 错误消息中文化 | 290 |
| 24 | Welcome Card 新增 Provider/Model 信息行；Thread Browser 对话列表追加消息数量标签 | 290 |
| 23 | Model/Login 面板操作成功反馈（切换模型、激活 Provider、保存） | 289 |
| 22 | Model 面板 Space 选中模型；Cron 删除增加确认步骤；面板粘贴事件统一拦截 | 287 |
| 21 | Cron 缓冲消息改为逐条发送，避免多个 cron 任务被合并为一条消息 | 833 |

## 2026-04-30 第20轮 — 第14轮：核心逻辑审查与优化

| 轮 | 改动 | 测试 |
|---|---|---|
| 20 | RetryableLLM 消除不可达死代码，BashTool 超时 clamp(1,300) | 833 |
| 19 | ContextBudget 事件链路：executor 新增 ContextWarning 事件发出 | 829 |
| 18 | LLM 适配层 context_window() 精确模型名推断（不再硬编码前缀匹配） | 826 |
| 17 | Anthropic Prompt Caching 改为在第一条 user 消息上加 cache_control（稳定缓存边界） | 823 |
| 16 | ContextBudget 定义层与执行层脱节修复：executor 改用 ContextBudget::should_warn() | 818 |
| 15 | SubAgent 消除二重文件解析冗余 I/O；新增 cancel 令牌传递链路支持 Ctrl+C 中断子 agent | 816 |
| 14 | HITL 批量审批：新增 before_tools_batch 钩子，多个敏感工具合并为一次审批弹窗 | 812 |

## 2026-04-29 第1轮 — 第13轮：初始 UX 全面审查

| 轮 | 改动 | 测试 |
|---|---|---|
| 13 | 清理 Tips 中引用不存在命令的提示（/rename 等 6 条），新增回归测试 | 252 |
| 12 | /compact 防重复触发；spinner 文字提示；micro-compact 消息中文化 | 786 |
| 11 | ToolBlock 错误结果 ERROR 红色高亮；/help 补全局快捷键提示 | 784 |
| 10 | 系统消息颜色按内容自动分级（错误红/警告橙/普通绿）；/compact 即时反馈 | 784 |
| 9 | 未配置 Provider 错误消息改为引导文案；状态栏显示任务运行时长 | 784 |
| 8 | 输入框占位提示(Alt+Enter换行)；命令前缀多匹配显示候选列表；状态栏快捷键提示 | 250 |
| 7 | Thread Browser 当前对话 ✓ 标识；ToolCallGroup 折叠展开提示；/help 补 Skills 说明 | 247 |
| 6 | Welcome Card 未配置引导；命令栏精简；工具运行中文字标签 | 247 |
| 5 | Cron 空列表引导和删除反馈；Login 编辑模式 Ctrl+V 提示和保存错误反馈 | 246 |
| 4 | Agent 面板空列表添加引导；Model 面板未配置时显示 /login 引导 | 244 |
| 3 | 全面排查单字母快捷键违规：HITL 改 Space+Enter，删除改 Ctrl+D，编辑改 Ctrl+N | 241 |
| 2 | Cron 面板 d 键删除修复；Thread Browser 删除后反馈消息 | 772 |
| 1 | Thread Browser/Login 面板删除功能缺失修复；Welcome Card 快捷键提示；配置保存错误反馈 | 772 |
