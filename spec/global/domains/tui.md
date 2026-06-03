# TUI 领域

## 领域综述

TUI 领域负责交互式终端界面的实现，包括渲染引擎、事件处理、命令系统、面板管理和与 Agent 核心的集成。

核心职责：

- 双线程渲染：独立渲染线程计算 Markdown 解析（pulldown-cmark）和行包装，UI 线程只从 `RenderCache` 读取可见行，按需重绘
- 事件处理：crossterm 输入拦截、命令解析（`/` 前缀）、弹窗状态管理
- 命令系统：`/model`、`/history`、`/clear`、`/help`、`/compact`、`/config`、`/cost`、`/context`、`/memory`、`/mcp`、`/loop`、`/cron`、`/agents`、`/effort`、`/rename`、`/doctor`；Command trait 支持 alias 机制
- 多会话管理：SQLite 持久化，`/history` 面板按 cwd 过滤当前工作区对话
- 弹窗系统：HITL 审批弹窗、AskUser 问答弹窗（支持 header 短标签 + 选项 description + 动态高度计算）、Model/Agents/Thread/Relay 配置面板
- SubAgent 层级展示：SubAgentGroup 可折叠块，滑动窗口显示最近 4 步，显示格式 `Agent(type) #hash`，颜色区分状态（前台绿色、后台运行中黄色、错误红色）
- Skill 全文预加载：消息含 `#skill-name` 时通过 SkillPreloadMiddleware 将 skill 全文注入 agent state
- Setup Wizard：首次启动自动检测配置完整性，三步引导（Provider → API Key → Model Alias），原子写回 settings.json
- 配色系统 v1.1：橙色仅保留最高优先级交互（命令输入框），工具名三级分层（bash=ACCENT / 写操作=WARNING / 只读=MUTED），配置面板边框 MUTED 降噪
- App 结构体拆分：App 三字段（ServiceRegistry/SessionManager/PanelManager），ChatSession 六子模块（ui/messages/session_panels/agent/commands/metadata）
- 面板组件化：PanelKind/PanelState 枚举 + PanelComponent trait + PanelManager，新增面板只需实现 trait
- 配置系统补全：CLAUDE.local.md 支持、`@import` 外部文件引用、claudeMdExcludes glob 过滤、`$schema` passthrough
- Welcome Card：空消息时显示品牌 ASCII Art Logo + 功能亮点 + 命令提示，发送消息后自动消失
- Sticky Human Message Header：聊天区顶部固定显示最后一条 Human 消息（1-3 行截断），滚动时不随之移动

## 核心流程

### 渲染管道（双线程）

```
App (UI 线程)
  ↓ AgentEvent
render_tx.try_send(RenderEvent)
  ↓
RenderTask（渲染线程）
  ↓ markdown 解析 / 行包装
Arc<RwLock<RenderCache>>
  ↓ version 变化时
terminal.draw(main_ui::render)
```

### 事件处理循环

```
Event::Key → 命令前缀匹配（/）
           → 普通字符输入（loading 时缓冲 pending_messages）
           → Ctrl+V（剪贴板图片）
           → Del（删除最后一张附件）
           → Enter（loading 缓冲，非 loading 提交）
           → Tab/Shift+Tab/方向键（面板导航）

poll_agent() → AgentEvent → handle_agent_event → view_messages + render_tx
```

### 面板事件分发流程

```
Event::Key → PanelManager::dispatch_key(input, ctx)
  → active_panel.as_mut().unwrap().handle_key(input, ctx)
  → EventResult（Consumed/NotConsumed/ClosePanel/OpenPanel）
  → ClosePanel 时自动关闭，OpenPanel 时切换面板
  → 消除 15 层 if-else 链和 28 处 unwrap()
```

### SubAgent 渲染流程

```
SubAgentStart → 创建 SubAgentGroup { is_running: true, is_background: bool, bg_hash: None }
  → 渲染: Agent(type) 绿色（前台）或黄色（后台）

SubAgentEnd（后台）→ 保持 is_running=true，解析 bg_hash
  → 渲染: Agent(type) #hash 黄色

BackgroundTaskCompleted → 更新 SubAgentGroup { is_running: false, final_result: Some(...) }
  → 渲染: Agent(type) #hash 绿色

颜色映射: is_error → ERROR（红）；is_running && is_background → WARNING（黄）；默认 → SAGE（绿）
```

### 多模态消息提交流程

```
Ctrl+V（剪贴板有图片）
  → arboard 读取 RGBA → png 编码 → base64
  → pending_attachments.push()
  → 渲染附件栏

submit_message(text)
  → mem::take(pending_attachments)
  → AgentInput::blocks([Text, Image, ...])
  → run_universal_agent(provider, agent_input)
  → pending_attachments 清空
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 渲染框架 | ratatui ≥0.30，主 UI 线程 + 独立渲染线程 |
| Markdown 渲染 | pulldown-cmark 0.12（CommonMark 规范，事件驱动，自制 ratatui 渲染器） |
| 渲染线程同步 | `parking_lot::RwLock<RenderCache>` + `tokio::sync::Notify` 零 sleep |
| Headless 测试 | `ratatui::backend::TestBackend`，`#[cfg(test)]` 隔离 |
| 剪贴板 | `arboard` crate，跨平台，macOS/Linux/Windows |
| 图片编码 | `png` crate（RGBA→PNG）+ `base64` crate |
| 命令解析 | 前缀唯一匹配（`/` 开头），`default_registry()` 注册 |
| 模型别名 | Opus/Sonnet/Haiku 三档，`ModelAliasMap` 独立绑定 Provider+Model |
| 输入缓冲 | `pending_messages: Vec<String>`，Done/Error 时合并发送 |
| 弹窗滚动 | `scroll_offset: u16`，`ensure_cursor_visible()`，80% 高度上限 |
| SubAgent 展示 | SubAgentGroup ViewModel；滑动窗口 4 条；RenderEvent::UpdateLastMessage 原地更新 |
| 远程控制配置 | RelayPanel View/Edit 模式；RemoteControlConfig 持久化到 ~/.peri/settings.json |
| 环境变量注入 | AppConfig.env HashMap，main() 最先调用 inject_env_from_settings()，进程环境变量优先 |
| 文件组织 | app/ 拆分 8 子文件；ui/ 拆分 popups/、panels/ 子目录；pub(super) 可见性 |
| App 结构体 | App 三字段（ServiceRegistry/SessionManager/PanelManager），ChatSession 六子模块（ui/messages/session_panels/agent/commands/metadata） |
| 面板组件化 | PanelKind/PanelState 枚举 + PanelComponent trait + PanelManager，双实例（session/global），PanelContext 解耦借用 |
| SubAgent 显示 | 格式 `Agent(type) #hash`，颜色映射（ERROR/WARNING/SAGE），is_background + bg_hash 字段 |
| 配置系统 | CLAUDE.local.md 支持、`@import` 外部引用（深度上限 3）、claudeMdExcludes glob 过滤、`$schema` passthrough |
| TUI 命令 | `/effort` 切换推理力度、`/rename` 设置会话标题、`/doctor` 健康检查 |
| 配色方案 | v1.1 降噪：橙色仅用于输入框，工具名 bash=ACCENT/写操作=WARNING/只读=MUTED，面板边框 MUTED |
| Setup Wizard | 三步引导（Provider → API Key → Model Alias），save_setup() 原子写回 settings.json |
| Welcome Card | 空消息时 ASCII Art Logo + 功能亮点，窄屏降级为文字标题 |
| Sticky Header | 最后一条 Human 消息 1-3 行截断固定在聊天区顶部 |
| 历史过滤 | /history 面板按 cwd 过滤 ThreadMeta，标题含工作区路径 |
| 定时任务面板 | /loop 注册（cron 表达式 + prompt），/cron 面板管理（导航/删除/切换启用） |
| /compact 迁移 | 执行后创建新 Thread 保留旧历史，新 Thread 以摘要 System 消息开头 |
| AskUser 高度 | wrapped_line_count 动态换行计算，visible_height 替代硬编码滚动区域 |
| 弹窗滚动 | `scroll_offset: u16`，`ensure_cursor_visible(visible_height)`，Tab 切换重置 scroll_offset |

## Feature 附录

### 20260324_F001_tui-clipboard-image-paste

**摘要:** Ctrl+V 粘贴剪贴板图片作为多模态消息发送
**关键决策:**

- 依赖: arboard 3 + png 0.17 + base64 0.22
- 数据结构: `PendingAttachment { label, media_type, base64_data, size_bytes }`
- run_universal_agent 签名变更: `input: String` → `input: AgentInput`
- 附件栏 Layout: 6-slot，新增 `Constraint::Length(attachment_height)`
**归档:** [链接](../../archive/feature_20260324_F001_tui-clipboard-image-paste/)
**归档日期:** 2026-03-24

### 20260324_F001_compact-context-command

**摘要:** /compact 指令调用 LLM 将对话历史压缩为结构化摘要
**关键决策:**

- 独立压缩任务: `tokio::spawn compact_task`，不经过 ReAct 循环
- 消息格式化: [用户]/[助手]/[工具结果] 标签，跳过 System
- 摘要存储: `BaseMessage::system(summary)` 替换 agent_state_messages
- view_messages 保留最近 10 条，头部插入压缩提示
- 空历史保护: is_empty() → 直接返回，不进入 loading
- 失败保护: CompactError 不修改历史
**归档:** [链接](../../archive/feature_20260324_F001_compact-context-command/)
**归档日期:** 2026-03-24

### 20260323_F001_tui-render-perf

**摘要:** 双线程渲染架构：独立渲染线程 + 按需重绘，消除消息多时卡顿
**关键决策:**

- 渲染线程: `tokio::spawn RenderTask::run`，持有私有消息副本
- RenderCache: `lines: Vec<Line<'static>>` + `version: u64`
- AppendChunk 增量: 仅重新渲染最后一条 assistant 消息
- 按需重绘: `last_render_version` 比较，`needs_redraw` 标志
**归档:** [链接](../../archive/feature_20260323_F001_tui-render-perf/)
**归档日期:** 2026-03-24

### 20260323_F002_tui-headless-mode

**摘要:** Headless 测试模式：TestBackend + 渲染线程零 sleep 同步
**关键决策:**

- App::new_headless(): `TestBackend::new(w, h)` + `spawn_render_thread`
- push_agent_event() + process_pending_events(): 测试注入事件，复用 handle_agent_event
- wait_for_render(): `notify.notified().await`，零轮询
- snapshot() / contains(): 遍历 buffer cell 拼接纯文本
- 条件编译: `#[cfg(any(test, feature = "headless"))]`
**归档:** [链接](../../archive/feature_20260323_F002_tui-headless-mode/)
**归档日期:** 2026-03-24

### 20260323_F003_tui-status-panel

**摘要:** TODO 状态固定面板、工具调用颜色分层、路径参数缩短
**关键决策:**

- TODO 面板: 独立 Layout slot，`todo_height` 动态计算，颜色分类（黄/灰/白）
- 工具颜色分层: 工具名（颜色+BOLD）+ 参数（DarkGray）
- 路径缩短: `strip_cwd(prefix)`，bash 和 search_files_rg 除外
- App 状态变更: `todo_items: Vec<TodoItem>`，删除 `todo_message_index`
**归档:** [链接](../../archive/feature_20260323_F003_tui-status-panel/)
**归档日期:** 2026-03-24

### 20260323_F001_model-alias-provider-mapping

**摘要:** Opus/Sonnet/Haiku 三级别名映射，支持 /model <alias> 快捷切换
**关键决策:**

- 数据结构: `ModelAliasConfig { provider_id, model_id }` + `ModelAliasMap { opus, sonnet, haiku }`
- 向后兼容迁移: 检测旧 provider_id 字段，自动填充 opus 别名
- 空 model_id fallback: anthropic→claude-sonnet-4-6, 其他→gpt-4o
- /model <alias> 命令: 直接切换 active_alias，无需打开面板
**归档:** [链接](../../archive/feature_20260323_F001_model-alias-provider-mapping/)
**归档日期:** 2026-03-24

### 20260323_F005_tui-bug-fixes

**摘要:** 修复弹窗滚动/粘贴换行/loading 输入锁死三个 TUI bug
**关键决策:**

- 弹窗滚动: 所有面板 popup_height ≤ area.height * 4/5，`scroll_offset` + `ensure_cursor_visible`
- Bracketed Paste: EnableBracketedPaste + Event::Paste → textarea.insert_str
- Loading 缓冲: pending_messages + "已缓存 N 条" 标题，Done/Error 时合并发送
**归档:** [链接](../../archive/feature_20260323_F005_tui-bug-fixes/)
**归档日期:** 2026-03-24

### 20260322_F002_data-pipeline-unification

**摘要:** 实时流式与历史恢复统一工具调用参数显示，含 tool_call_id 匹配
**关键决策:**

- ToolStart 扩展: 增加 `tool_call_id: String` 字段
- prev_ai_tool_calls: 存储 `(id, name, input)` 三元组
- 统一格式化: `format_tool_call_display()` 被实时和历史共用
- 降级处理: 无匹配时使用 tool_call_id 作为工具名
**归档:** [链接](../../archive/feature_20260322_F002_data-pipeline-unification/)
**归档日期:** 2026-03-24

### 20260322_F001_message-render-refactor

**摘要:** MessageViewModel 中间层重构，tui-markdown 渲染，工具折叠
**关键决策:**

- ViewModel 变体: UserBubble / AssistantBubble / ToolBlock / SystemNote / TodoStatus
- Markdown 渲染: `tui-markdown` crate，`ensure_rendered()` dirty flag 降频
- 工具折叠: collapsed 状态，Tab 键切换，默认折叠
- ChatMessage 完全移除: 替换为 view_messages
**归档:** [链接](../../archive/feature_20260322_F001_message-render-refactor/)
**归档日期:** 2026-03-24

### feature_20260324_F001_ratatui-markdown-renderer

**摘要:** pulldown-cmark 替代 tui-markdown，自制 ratatui Markdown 渲染器
**关键决策:**

- pulldown-cmark 0.12（CommonMark 规范，事件驱动）替代 tui-markdown 0.3
- RenderState 累积行内 Span，事件驱动构建 Text<'static>
- dirty flag 全量重解析（10KB 约 30μs，帧预算 16.7ms 内可接受）
- parse_markdown / ensure_rendered 接口不变，message_render.rs 零改动
**归档:** [链接](../../archive/feature_20260324_F001_ratatui-markdown-renderer/)
**归档日期:** 2026-03-27

### feature_20260325_F002_large-file-refactor

**摘要:** app/mod.rs 和 main_ui.rs 大文件拆分为多子文件
**关键决策:**

- Rust 同模块多文件 impl 块，app/ 拆分为 8 个子文件（hitl_prompt/ask_user_prompt/agent_ops 等）
- ui/ 拆分为 popups/（hitl/ask_user/hints）和 panels/（model/thread_browser/agent）子目录
- 纯机械搬移，禁止顺手重构，pub use 重导出保持外部路径不变
- pub(super) 可见性约束，render() 为唯一对外入口
**归档:** [链接](../../archive/feature_20260325_F002_large-file-refactor/)
**归档日期:** 2026-03-27

### feature_20260326_F001_subagent-message-hierarchy

**摘要:** SubAgent 执行消息分层为可折叠块
**关键决策:**

- 纯 TUI 层感知（方案 A）：利用 launch_agent ToolStart/End 事件作为边界
- SubAgentGroup ViewModel：滑动窗口最多 4 条，total_steps 单独累计
- RenderEvent::UpdateLastMessage 原地更新，不触发全量重建
- 完成后 Enter 键折叠/展开，折叠态只显示摘要行
**归档:** [链接](../../archive/feature_20260326_F001_subagent-message-hierarchy/)
**归档日期:** 2026-03-27

### feature_20260326_F004_remote-control-panel

**摘要:** /relay 命令面板：TUI 内配置并持久化远程控制参数
**关键决策:**

- RelayPanel View/Edit 两模式（参考 ModelPanel 设计）
- RemoteControlConfig 结构化替代 extra 字段（向后兼容 extra.relay_*）
- --remote-control 无参数时从配置读取；无 --remote-control 参数则不自动连接
- Token 脱敏显示（****last4****），存储在 ~/.peri/settings.json
**归档:** [链接](../../archive/feature_20260326_F004_remote-control-panel/)
**归档日期:** 2026-03-27

### feature_20260328_F001_skill-preload-on-send

**摘要:** TUI 发送含 #skill-name 消息时自动全文预加载对应 skill
**关键决策:**

- AgentRunConfig 新增 preload_skills: Vec<String>
- submit_message 用正则 `#([a-zA-Z0-9_-]+)` 解析 skill 名列表
- run_universal_agent 有 preload_skills 时插入 SkillPreloadMiddleware（紧随 SkillsMiddleware 之后）
- 空列表时 SkillPreloadMiddleware.before_agent early return，无额外开销
- 找不到的 skill 名静默跳过
**归档:** [链接](../../archive/feature_20260328_F001_skill-preload-on-send/)
**归档日期:** 2026-03-28

### feature_20260328_F003_test-coverage-improvement

**摘要:** 四高风险区域补充 55+ 单元测试提升覆盖率
**关键决策:**

- 文件系统工具测试: tempfile TempDir 隔离，6 个工具各 4-5 个测试（正常/边界/错误）
- Relay Server 测试: auth.rs 5 个 token 验证；client/mod.rs 7 个历史缓存（new_for_testing 绕过 WS）
- AskUserTool 测试: MockBroker mock broker，10 个测试覆盖参数解析和返回格式
- TUI 命令测试: StubCommand + headless App，8 个 dispatch/prefix 匹配测试
- 新增总数 ~56 个测试，工具实现层覆盖率 ~40%→~80%
**归档:** [链接](../../archive/feature_20260328_F003_test-coverage-improvement/)
**归档日期:** 2026-03-29

### feature_20260328_F004_settings-env-injection

**摘要:** settings.json env 字段替代 .env 注入环境变量
**关键决策:**

- AppConfig.env: Option<HashMap<String, String>>，serde default + skip_serializing_if
- inject_env_from_settings(): main() 最先调用，std::env::var(key).is_err() 判断不存在再 set_var
- 优先级: 进程环境变量 > settings.json env 字段
- 错误处理: 文件不存在/env 缺失/JSON 解析失败均静默跳过（不 panic）
- 移除 dotenvy 依赖
**归档:** [链接](../../archive/feature_20260328_F004_settings-env-injection/)
**归档日期:** 2026-03-29

### feature_20260326_F008_statusbar-msgcount-relay-flag

**摘要:** 状态栏显示消息计数，禁止 relay 隐式自动连接
**关键决策:**

- 消息数从 app.view_messages.len() 直接读取，无需新增事件或字段
- 无 --remote-control 参数时 try_connect_relay else 分支直接 return，不读配置
**归档:** [链接](../../archive/feature_20260326_F008_statusbar-msgcount-relay-flag/)
**归档日期:** 2026-03-27

### feature_20260408_F001_askuser-dialog-height

**摘要:** AskUser 弹窗高度计算修复，滚动可见高度动态化
**关键决策:**

- wrapped_line_count 辅助函数：根据弹窗内宽度和 unicode-width 计算文本实际换行行数
- active_panel_height 重写：question/options/description 均使用 wrapped_line_count 估算
- visible_height 字段：AskUserBatchPrompt 新增字段，渲染函数每帧回写 content_area.height
- Tab 切换重置：next_tab()/prev_tab() 中 scroll_offset 归零
**归档:** [链接](../../archive/feature_20260408_F001_askuser-dialog-height/)
**归档日期:** 2026-04-27

### feature_20260331_F001_history-workspace-tag

**摘要:** /history 面板按 cwd 过滤只显示当前工作区对话
**关键决策:**

- open_thread_browser() 中 threads.into_iter().filter(|t| t.cwd == cwd).collect() 过滤
- ThreadBrowser 面板标题包含 app.cwd 路径
- ThreadMeta 已有 cwd 字段，无需数据库变更
**归档:** [链接](../../archive/feature_20260331_F001_history-workspace-tag/)
**归档日期:** 2026-04-27

### feature_20260330_F005_tui-setup-wizard

**摘要:** 首次启动三步引导（Provider → API Key → Model Alias）
**关键决策:**

- 配置完整性检测：启动时检查 provider/model/api_key，任一缺失触发向导
- 三步向导：Provider 选择（Anthropic/OpenAI Compatible）→ API Key 输入 → Model Alias 配置
- save_setup() 原子写回：先写临时文件再 rename 到 settings.json
- 向导完成后自动重启 Agent 连接
**归档:** [链接](../../archive/feature_20260330_F005_tui-setup-wizard/)
**归档日期:** 2026-04-27

### feature_20260330_F002_tui-color-refresh

**摘要:** 配色系统 v1.1 降噪，橙色聚焦交互，工具名三级分层
**关键决策:**

- 橙色收缩：仅保留命令输入框边框和高亮，其他橙色元素降级为 MUTED/WARNING
- 工具名颜色分层：bash=ACCENT（橙色），write/edit/folder=WARNING（黄色），read/glob/search=MUTED（暗灰）
- 配置面板边框统一为 MUTED，HITL/AskUser 弹窗使用 WARNING
- 工具参数统一 DarkGray 显示
**归档:** [链接](../../archive/feature_20260330_F002_tui-color-refresh/)
**归档日期:** 2026-04-27

### feature_20260330_F001_sticky-human-message-header

**摘要:** 聊天区顶部固定最后一条 Human 消息摘要
**关键决策:**

- 渲染位置：固定在聊天区最顶部，不随消息列表滚动
- 显示规则：1-3 行截断（Str::from(msg).lines().take(3)），空消息/无人类消息时不显示
- 生命周期：发送新消息后更新，/clear 后消失，打开历史 Thread 自动恢复
- 实现位置：main_ui.rs 渲染函数中作为独立 Layout 块
**归档:** [链接](../../archive/feature_20260330_F001_sticky-human-message-header/)
**归档日期:** 2026-04-27

### feature_20260329_F004_app-refactor

**摘要:** App 结构体拆分为 AppCore/AgentComm/RelayState/LangfuseState
**关键决策:**

- 四子结构体：AppCore（UI 状态）、AgentComm（agent channel）、RelayState（relay 客户端）、LangfuseState（追踪配置）
- 共 37 字段拆分，App 持有所有子结构体
- 对外 API 通过 impl App 上的转发方法保持不变，调用方零改动
- Deref 代理：部分内部模块直接 Deref 到子结构体简化访问
**归档:** [链接](../../archive/feature_20260329_F004_app-refactor/)
**归档日期:** 2026-04-27

### feature_20260329_F003_compact-thread-migration

**摘要:** /compact 执行后创建新 Thread 保留旧历史
**关键决策:**

- 新建 Thread：compact 完成后 open_new_thread()，旧 Thread 保留在 SQLite 中
- 新 Thread 开头：插入摘要 System 消息，标记"此对话从历史压缩而来"
- Relay 同步：CompactDone 事件通知 Web 前端切换到新 Thread
- view_messages 重建：新 Thread 从空消息开始
**归档:** [链接](../../archive/feature_20260329_F003_compact-thread-migration/)
**归档日期:** 2026-04-27

### feature_20260329_F003_ui-display-fixes

**摘要:** 修复空消息欢迎页、长文本截断、子 Agent 空状态显示
**关键决策:**

- 空消息时不再显示空白聊天区，改为显示欢迎提示或品牌内容
- 长文本消息截断策略优化，避免单条消息占据整个可见区域
- 子 Agent 执行中无输出时显示"正在执行..."占位，而非空白
**归档:** [链接](../../archive/feature_20260329_F003_ui-display-fixes/)
**归档日期:** 2026-04-27

### feature_20260329_F001_tui-welcome-card

**摘要:** 空消息时显示品牌 ASCII Art Logo + 功能亮点
**关键决策:**

- 渲染条件：view_messages.is_empty() 时显示，发送消息后自动消失
- 内容：ASCII Art Logo + 功能亮点列表 + 命令提示（/help /model /history 等）
- 窄屏降级：宽度不足时隐藏 ASCII Art，仅显示文字标题
- 实现方式：独立渲染函数，作为 main_ui.rs 中的一个渲染分支
**归档:** [链接](../../archive/feature_20260329_F001_tui-welcome-card/)
**归档日期:** 2026-04-27

### feature_20260501_F001_color-system-refactor

**摘要:** TUI 配色对齐 Claude Code Dark 主题，清理 28 处硬编码颜色
**关键决策:**

- 12 个主题常量 RGB 值从暖棕调更新为 Claude Code Dark 主题值
- ACCENT #D77757、TEXT #FFFFFF、MUTED #999999、ERROR #FF6B80、WARNING #FFC107
- 清理 17 个文件中 28 处硬编码 Color::White/DarkGray 等，统一引用 theme::*
- 保持现有命名体系和代码结构不变
**归档:** [链接](../../archive/feature_20260501_F001_color-system-refactor/)
**归档日期:** 2026-05-04

### feature_20260503_F001_cc-commands-alignment

**摘要:** 新增 /config /cost /context /memory 四个命令 + Command alias 机制
**关键决策:**

- 4 个新命令：/config(表单面板)、/cost(费用面板)、/context(上下文面板)、/memory(编辑 CLAUDE.md)
- Command trait 扩展 alias 机制，现有命令补充别名（/clear→/reset、/new）
- /config 表单面板：autocompact/语言/system prompt 覆盖
- /cost 和 /context 共用面板组件
**归档:** [链接](../../archive/feature_20260503_F001_cc-commands-alignment/)
**归档日期:** 2026-05-04

### feature_20260502_F002_mcp-management

**摘要:** MCP 连接池后台初始化 + /mcp 运行时管理面板
**关键决策:**

- spawn_mcp_init() 后台 task，不阻塞 TUI
- /mcp 面板三视图（Browse/Tools/Resources），支持重连和持久删除
- submit_message() 异步等待 MCP 就绪（30s）
**归档:** [链接](../../archive/feature_20260502_F002_mcp-management/)
**归档日期:** 2026-05-04

### feature_20260512_F001_subagent-display-colors

**摘要:** SubAgent 显示颜色区分前台/后台，格式改为 Agent(type)
**关键决策:**

- SubAgentGroup 新增 is_background 和 bg_hash 字段
- 后台 agent 运行中显示黄色（WARNING），完成后变绿色（SAGE）
- 后台 agent SubAgentEnd 不冻结，保持 is_running=true
- BackgroundTaskCompleted 更新 SubAgentGroup 而非创建 ToolBlock
- 显示格式从 ● agent_id 改为 Agent(type) #hash
**归档:** [链接](../../archive/feature_20260512_F001_subagent-display-colors/)
**归档日期:** 2026-05-13

### feature_20260510_F001_simple-compat-features

**摘要:** 4 项配置系统补全（CLAUDE.local.md/@import/excludes）+ 3 个 TUI 命令
**关键决策:**

- CLAUDE.local.md 追加到主文件内容末尾，不入库的个人配置
- @import 外部文件引用，递归解析深度上限 3，循环检测
- claudeMdExcludes glob 模式跳过特定路径的 CLAUDE.md
- /effort 命令调整推理力度，/rename 命令修改会话标题，/doctor 健康检查
**归档:** [链接](../../archive/feature_20260510_F001_simple-compat-features/)
**归档日期:** 2026-05-13

### feature_20260508_F002_app-layer-refactor

**摘要:** App 分层重构：ServiceRegistry/SessionManager/UiState/MessageState
**关键决策:**

- ServiceRegistry: 20 个服务字段（peri_config/cwd/thread_store/mcp_pool 等）
- SessionManager: sessions/active/session_areas 三字段，current()/current_mut() 辅助方法
- UiState: 18 个 UI 字段（textarea/loading/scroll_offset 等）
- MessageState: 9 个消息字段（view_messages/pipeline/render_tx 等）
- App 从 26 字段压缩到 3 个子结构体，消除 std::mem::take workaround
**归档:** [链接](../../archive/feature_20260508_F002_app-layer-refactor/)
**归档日期:** 2026-05-13

### feature_20260508_F001_panel-component-architecture

**摘要:** 面板组件化：PanelManager/PanelComponent trait 统一面板生命周期
**关键决策:**

- PanelKind 枚举穷举所有面板类型，PanelState 枚举存储面板实例
- PanelComponent trait 定义 handle_key/handle_paste/desired_height/render 接口
- PanelManager 统一管理打开/关闭/互斥，dispatch_key 事件分发
- PanelContext 解决 &mut panel + &mut app 借用冲突
- 消除 28 处 unwrap()，15 层 if-else 链简化为单次调用
**归档:** [链接](../../archive/feature_20260508_F001_panel-component-architecture/)
**归档日期:** 2026-05-13

---

## Issue 经验附录

### issue_2026-05-12-textarea-mouse-click-cursor-misposition-cjk

**摘要:** 输入框鼠标点击光标定位不准
**状态:** Fixed
**归档日期:** 2026-05-13
**关键词:** CJK 宽度, unicode-width, 鼠标定位, display_col_to_char_idx
**问题本质:** 三个偏移叠加：(1) CJK 字符占 2 列宽但 Jump 期望字符索引；(2) Block padding 偏移 2 列未计算 inner area；(3) 水平滚动偏移未考虑
**通用模式:** 终端 UI 中鼠标坐标是显示列（display column），而光标位置是字符索引（char index）。包含 CJK 字符时两者非线性关系，需要逐字符累加 unicode_width 转换。Block 的 padding/border 和水平滚动也需要纳入偏移计算
**涉及文件:** peri-tui/src/event.rs, peri-tui/src/app/mod.rs, peri-tui/src/ui/main_ui.rs
**CLAUDE.md 链接:** false

### issue_2026-05-14-streaming-resize-cpu-spike

**摘要:** 流式加载期间拖动窗口宽度，Resize 事件无节流导致 CPU 暴涨
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** Resize事件, 去抖/节流, 渲染线程, CPU暴涨
**问题本质:** 拖动 resize 时 crossterm 每帧发送 Resize 事件（60fps = 60次/秒），每个事件触发渲染线程全量重建（message_hashes.clear() + rebuild + build_wrap_map 换行计算）。流式期间叠加 100ms Rebuild 事件，渲染线程饱和。
**通用模式:** 渲染事件的发送端和接收端都需要节流/合并：发送端用 last_resize_width 去抖（仅在宽度实际变化时发送），接收端用 drain coalescing（合并积压事件）。单端优化不足以解决问题。
**技术决策:** last_resize_width 字段记录已发送宽度，Resize handler 中 try_recv() drain 合并所有积压事件
**涉及文件:** peri-tui/src/app/message_state.rs, peri-tui/src/ui/main_ui.rs, peri-tui/src/ui/render_thread.rs
**CLAUDE.md 链接:** false

### issue_2026-05-15-concurrent-subagent-display-delay

**摘要:** 并发前台SubAgent调用时UI感知延迟，SubAgentGroup卡片不可见
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 并发SubAgent, build_tail_vms, merge_frozen_subagents, has_snapshot_this_round
**问题本质:** has_snapshot=true时build_tail_vms只调用merge_frozen_subagents做替换，运行中SubAgent被跳过；merge_frozen_subagents只替换不追加
**通用模式:** 状态合并函数需同时处理替换和追加两种场景；frozen和running需分别对待
**架构影响:** SubAgentGroup的显示需要分frozen（已完成）和running（进行中）两条路径
**技术决策:** 在has_snapshot=true分支中追加subagent_stack中未frozen的运行中SubAgentGroup
**涉及文件:** peri-tui/src/app/message_pipeline.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/agent.rs
**CLAUDE.md 链接:** true

### issue_2026-05-16-setup-save-destroys-existing-config

**摘要:** save_setup 覆盖已有配置文件导致数据永久丢失
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 配置覆盖, save-before-load, 数据丢失, 先写后读
**问题本质:** save-then-load 模式中，先覆盖文件再读取同一文件做 merge，merge 成为空操作，非 provider 字段永久丢失
**通用模式:** 需要合并已有配置时，必须先读取原始数据，合并后再写入。绝不能先写入再读取同一文件做合并——读到的是自己刚写入的数据
**架构影响:** IO 顺序错误（写→读同一路径）是静默数据丢失的高危模式，应优先在函数签名层面阻绝（如分离 `build_config` 纯函数和 `save_config` IO 函数）
**涉及文件:** peri-tui/src/app/setup_wizard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-form-edit-labels-hardcoded

**摘要:** Form Edit 字段标签硬编码英文，未使用 i18n
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** i18n 未使用, 硬编码标签, ProviderType, unicode-width
**问题本质:** i18n key 已在 FTL 文件中定义，但渲染函数使用硬编码字符串，ProviderType::label() 不接受 LcRegistry 参数
**通用模式:** 使用 `_lc` 前缀命名但实际未消费的 i18n 参数是代码坏味道；需要翻译的类型应在签名中要求 `&LcRegistry`
**技术决策:** 字段标签对齐使用 `unicode-width` crate 的 `pad_display_columns` 辅助函数
**涉及文件:** peri-tui/src/ui/main_ui/popups/setup_wizard.rs, peri-tui/src/app/setup_wizard.rs, peri-tui/locales/en/main.ftl, peri-tui/locales/zh-CN/main.ftl
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-browse-submit-no-feedback

**摘要:** Browse 模式 Submit 失败时无任何反馈
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 静默失败, 无反馈, 用户体验, 错误提示
**问题本质:** has_valid=false 时仅返回 Redraw，界面无变化、无错误消息，用户无法理解为何无法提交
**通用模式:** 每个用户操作必须有可见反馈——成功进入下一状态，失败显示原因。空 Redraw 是反模式
**涉及文件:** peri-tui/src/app/setup_wizard.rs, peri-tui/src/ui/main_ui/popups/setup_wizard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-ctrlc-blocked-cannot-exit

**摘要:** Ctrl+C 在 Setup Wizard 中完全被拦截——无法退出
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 事件拦截, Ctrl+C 拦截, 全局处理器, 退出流程
**问题本质:** Wizard 拦截块在 `handle_setup_wizard_key` 返回 None 后无条件返回 Redraw，后续全局 Ctrl+C 处理器永远无法到达
**通用模式:** 事件拦截块必须在调用业务处理器之前检查全局关键事件（Ctrl+C、quit），或将这些事件作为业务处理器必须处理的基本事件
**涉及文件:** peri-tui/src/event.rs, peri-tui/src/app/setup_wizard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-api-key-mask-byte-vs-char

**摘要:** API Key 遮罩使用字节长度而非字符数
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 字节 vs 字符, CJK 显示, chars().count(), len() 陷阱
**问题本质:** `"•".repeat(s.len())` 使用字节长度，CJK 字符每字符 3 字节导致遮罩数量膨胀
**通用模式:** 所有面向用户显示的字符串长度计算必须使用 `chars().count()`，仅内部 buffer 管理可用 `len()`
**涉及文件:** peri-tui/src/ui/main_ui/popups/setup_wizard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-active-provider-oob-panic

**摘要:** active_provider 越界无保护可导致 render panic
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 越界检查, 裸索引, .get(), 防御性编程
**问题本质:** `providers[active_provider]` 裸索引，active_provider 可能因异常状态越界导致 panic
**通用模式:** 用户管理/异步更新的索引始终用 `.get()` 并处理 None 回退，防御性代码成本极低但收益巨大
**涉及文件:** peri-tui/src/ui/main_ui/popups/setup_wizard.rs, peri-tui/src/app/setup_wizard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-provider-type-toggle-resets-data

**摘要:** Edit 模式 ProviderType 切换静默重置所有已编辑数据
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 数据丢失, 确认提示, 键过载, 导航键冲突
**问题本质:** ←/→ 键在 ProviderType 字段上做类型切换+数据重置，但在其他字段是光标移动——同一按键在不同上下文语义完全不同，导致误触即数据丢失
**通用模式:** 导航键（←/→）不应触发破坏性操作；破坏性状态变更需要确认提示；一个按键在一个表单中应保持语义一致
**涉及文件:** peri-tui/src/app/setup_wizard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-language-step-hardcoded-no-i18n

**摘要:** Language 步骤完全硬编码中英混合文本，忽略 i18n
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** i18n 忽略, 硬编码混合文本, _lc 参数, FTL 未使用
**问题本质:** FTL 已定义完整翻译 key，但渲染函数故意忽略 `_lc` 参数，所有文本硬编码为中英混合
**通用模式:** `_lc` 前缀命名暗示参数应被消费，实际忽略是代码坏味道。应为所有面向用户的字符串使用 i18n，不留硬编码回退
**涉及文件:** peri-tui/src/ui/main_ui/popups/setup_wizard.rs, peri-tui/locales/
**CLAUDE.md 链接:** false

### issue_2026-05-16-tool-args-display-truncation-too-short

**摘要:** 工具调用参数显示截断过短
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 多层截断, 显示阈值, format_tool_args, format_args_summary
**问题本质:** 两条截断链（`format_tool_args` 60→`format_args_summary` 40）叠加，最终截断过短失去可读性
**通用模式:** 多层格式化管道中，修改一层阈值时必须检查下游是否二次截断；阈值应设在最终消费端而非每层都截
**涉及文件:** peri-tui/src/app/tool_display.rs, peri-widgets/src/tool_call/mod.rs, peri-widgets/src/message_block/blocks.rs, peri-tui/src/ui/message_render.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-mod-zero-empty-options

**摘要:** Language 步骤空选项下取模 panic 风险
**状态:** Fixed
**归档日期:** 2026-05-16
**关键词:** 取模零除, debug_assert, 防御性编程
**问题本质:** `(cursor + len - 1) % len` 在 len=0 时 panic，当前 len 为编译期常量无实际风险
**通用模式:** 取模运算前加 `debug_assert!(!slice.is_empty())` 守卫——debug 构建捕获逻辑错误，release 无开销
**涉及文件:** peri-tui/src/app/setup_wizard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-i18n-language-not-in-setup

**摘要:** Setup 向导缺少语言配置步骤
**状态:** Fixed（实际已完成但 issue 文档未更新状态——issue 创建时 Fixed 但描述的问题已在代码中解决）
**归档日期:** 2026-05-17
**关键词:** setup wizard, language, i18n, SetupStep
**问题本质:** Setup 向导只有 Choose/Form/Done 三步，缺少 Language 步骤，导致初次运行时无法选择 zh-CN
**通用模式:** 首次运行向导应覆盖所有用户偏好（语言、provider、key、model），早期遗漏会在后续补丁中累积复杂度
**涉及文件:** peri-tui/src/app/setup_wizard.rs, peri-tui/src/ui/main_ui/popups/setup_wizard.rs, peri-tui/src/config/types.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-model-panel-1m-context-toggle

**摘要:** Model 面板添加 1M 上下文开关
**状态:** Fixed
**归档日期:** 2026-05-17
**关键词:** context_window, 1M context, model panel, ContextBudget
**问题本质:** 1M context 模型需要手动开启开关，compact 阈值才能以 1M 而非模型默认值为基准
**通用模式:** 上下文窗口覆盖需要在三个路径同步：(1) 模型切换时立即反映到 status line；(2) 消息提交前覆盖；(3) Agent 运行时 ContextWarning 计算前覆盖。单路径覆盖不足以保证一致性
**技术决策:** `agent.context_window` 在 `apply_and_close`、`agent_submit.rs`、`agent_ops.rs` 三处同步覆盖
**涉及文件:** peri-tui/src/config/types.rs, peri-tui/src/app/model_panel.rs, peri-tui/src/ui/main_ui/panels/model.rs, peri-tui/src/app/agent.rs, peri-tui/src/app/agent_submit.rs
**CLAUDE.md 链接:** false

### issue_2026-05-16-setup-polish-series

**摘要:** Setup Wizard 波兰系列（8 个 UI 小修）
**状态:** Closed（全部 8 个）
**归档日期:** 2026-05-17
**关键词:** setup wizard polish, paste newline, char vs byte, Ctrl 修饰符, form validation, empty state, env_get, CODEX migrate, needs_setup
**问题本质:** Setup 向导实现初版后的 8 个边界情况修复，涵盖粘贴、CJK、键盘、校验、空状态、环境变量、迁移、完整性检测
**子问题:**
- `paste-strips-all-newlines`: `text.replace('\n', "")` 把多行拼成一行 → 改为 `text.lines().next()`
- `char-vs-byte-index-confusion`: 3 处 `buf.len()` 字符索引边界检查 → 改为 `buf.chars().count()`
- `ctrl-left-right-not-filtered`: Ctrl+Left 在 ProviderType 字段误触类型切换 → 加 `ctrl: false` 守卫
- `edit-confirm-no-validation`: Enter 无条件返回 Browse 无字段校验 → 检查 provider_id/api_key/aliases 非空
- `empty-providers-no-hint`: 空 providers 时界面空白 → 渲染 `setup-no-providers` i18n 提示
- `env-get-silent-fail-on-non-string`: 非字符串 env 值静默返回空串 → match `is_string()` + `tracing::warn!`
- `migrate-codex-doc-vs-impl`: CODEX 前缀注释声称支持但实现缺失 → 添加 `("CODEX", ...)` 到 prefixes 数组
- `needs-setup-incomplete-check`: 仅检查 api_key 为空跳过 setup → 添加 `provider.id.trim().is_empty()` 检查
**通用模式:** UI 向导的边界情况修复是集中爆发的——初版必然有键盘、粘贴、校验、空状态四类遗漏。用一个 issue 追踪全系列比拆分成 8 个更高效
**涉及文件:** peri-tui/src/app/setup_wizard.rs, peri-tui/src/ui/main_ui/popups/setup_wizard.rs, peri-tui/src/app/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-split-session-command-hint-only-shows-active

**摘要:** 分屏模式下非活跃 Session 命令浮层显示异常
**状态:** Fixed
**归档日期:** 2026-05-18
**关键词:** std::mem::take, session index 竞态, 分屏, CommandRegistry, 命令浮层
**问题本质:** `/split` 命令在 `dispatch` 期间改变了 `app.session_mgr.active`，导致 `std::mem::take` 归还模式将 CommandRegistry 归还到错误的 session。叠加渲染层 `if is_active` 守卫导致非活跃 session 完全不显示命令浮层。
**通用模式:** 任何在 `dispatch` 期间可能改变 `app.session_mgr.active` 的命令都存在 session index 竞态风险。核心原则：在 take 前保存 index，归还时使用保存值。Hint 类渲染应无条件执行——数据隔离依赖 `render_session_column` 的临时 active 切换，视觉区分依赖 `is_active` 传参处理边框/光标。
**涉及文件:** peri-tui/src/event.rs, peri-tui/src/ui/main_ui.rs
**CLAUDE.md 链接:** true

### issue_2026-05-18-tui-dot-and-scrollbar-rendering

**摘要:** TUI 指示符号 ⏺ 与 ● 不统一，滚动条在部分终端有空隙
**状态:** Fixed
**归档日期:** 2026-05-18
**关键词:** 指示符号统一, 滚动条 track, box-drawing 字符, GPU 终端兼容
**问题本质:** ⏺ (U+23FA) 是录像按钮符号而非纯圆点，与 ● (U+25CF) 视觉不匹配。║ (U+2551) 在部分 GPU 终端的字体渲染中字符高度不足以完全填满字符格，导致滚动条 track 出现行列间空隙。
**通用模式:** 终端渲染符号选择应优先使用语义精确的字符（纯圆点用 ● 而非构图混合符号），滚动条 track 用 █ (FULL BLOCK) 避免 box-drawing 字符的跨终端间距问题。
**涉及文件:** peri-tui/src/ui/message_render.rs, peri-widgets/src/scrollable.rs
**CLAUDE.md 链接:** false

### issue_2026-05-17-panel-heavy-files

**摘要:** Panel 文件过度肥大：mcp_panel.rs + login_panel.rs + setup_wizard.rs
**状态:** Fixed
**归档日期:** 2026-05-18
**关键词:** 面板拆分, PanelComponent, state/ops/ui 三层分离
**问题本质:** 纯代码组织优化，无深度技术认知
**涉及文件:** peri-tui/src/app/mcp_panel.rs, peri-tui/src/app/setup_wizard.rs, peri-tui/src/app/login_panel.rs
**CLAUDE.md 链接:** false

### issue_2026-05-17-main-ui-heavy-file

**摘要:** peri-tui/src/ui/main_ui.rs 主 UI 布局逻辑集中（852 行）
**状态:** Fixed
**归档日期:** 2026-05-18
**关键词:** 主 UI 拆分, layout/event_handler 分离
**问题本质:** 纯代码组织优化，无深度技术认知
**涉及文件:** peri-tui/src/ui/main_ui.rs
**CLAUDE.md 链接:** false

### issue_2026-05-12-macos-option-backspace-scrolls-when-content-present
**摘要:** Mac 上 Option+Backspace 在有可滚动内容时触发滚动而非删除整行
**状态:** Fixed
**归档日期:** 2026-05-20
**关键词:** VS Code 终端, Option+Backspace, PageUp 映射, 词删除, TERM_PROGRAM
**问题本质:** VS Code 集成终端在 Mac 上将 Option+Backspace 映射为 PageUp 转义序列，crossterm 解释为 PageUp 事件，事件处理层未区分终端环境导致被无条件用于消息区域滚动
**通用模式:** 终端快捷键映射受终端模拟器环境（TERM_PROGRAM）影响——VS Code、iTerm2、Terminal.app 对相同物理按键可能产生不同转义序列；事件处理需兼顾语义意图（用户想删除词）和终端环境差异
**涉及文件:** peri-tui/src/event/keyboard.rs
**CLAUDE.md 链接:** false

### issue_2026-05-23-ask-user-overflow-and-description-missing

**摘要:** AskUser 弹窗内容溢出不可滚动且选项描述丢失
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** AskUser弹窗, Elicitation description, ScrollableArea, 面板高度
**问题本质:** TUI 弹窗组件（AskUser）的高度计算逻辑与内容实际渲染行数不匹配；ACP Elicitation JSON 中注入的 description 字段被反序列化时丢弃
**通用模式:** 弹窗/面板高度计算必须考虑动态内容（文本换行、选项描述），不能假设固定行高；跨层数据传递（JSON → struct）时枚举变体可能丢弃未知字段，需要专门的提取逻辑
**架构影响:** ScrollableArea 组件只有渲染没有交互是设计缺陷，需要 option_row_map 追踪真实渲染行号而非逻辑选项索引
**涉及文件:** peri-tui/src/ui/main_ui/mod.rs:310-376, peri-tui/src/ui/main_ui/popups/ask_user.rs:176-181, peri-tui/src/app/agent_ops_interaction.rs:86-104, peri-acp/src/broker/transport_broker.rs:258-299
**CLAUDE.md 链接:** false

### issue_2026-05-21-setup-wizard-settings-not-reloaded

**摘要:** Setup 向导完成后 ACP Server 配置未刷新，API key 未生效
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** Setup向导, Arc共享配置, RwLock同步, ACP Server
**问题本质:** App 层与 ACP Server 层持有独立的 Arc<RwLock<Config>>，Setup 只更新了 App 侧的 Arc，ACP Server 侧的 Arc 未同步
**通用模式:** 多个组件共享配置时，必须共享同一个 Arc 引用而非各自 clone；配置更新时必须遍历所有消费者确保同步
**架构影响:** 引入 ServiceRegistry 持有 ACP 共享 Arc 的模式，所有配置修改路径（8 条）统一调用 sync_peri_config_to_acp()
**涉及文件:** peri-tui/src/app/mod.rs:535-542, peri-tui/src/main.rs:601-617, peri-tui/src/event/keyboard.rs:91-98, peri-tui/src/acp_server/mod.rs:96-97
**CLAUDE.md 链接:** false

### issue_2026-05-24-cancel-ineffective-during-streaming-and-tool-execution

**摘要:** Ctrl+C 在流式输出和工具执行中 UI 中断但底层请求未停止
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** Ctrl+C取消, cancel_sent_at, 流式中断, 事件竞态
**问题本质:** interrupt() 同时执行异步 cancel 和同步 UI 清理两条路径，UI 清理先于 cancel 生效导致用户以为已停止但实际未停止
**通用模式:** 异步系统中 UI 层中断应延迟到确认事件到达后再清理，不应立即强制清理；需要 timeout fallback 防止事件丢失导致永久 loading
**涉及文件:** peri-tui/src/app/mod.rs, peri-tui/src/app/agent_comm.rs, peri-tui/src/app/agent_ops/polling.rs, peri-tui/src/app/agent_ops/lifecycle.rs
**CLAUDE.md 链接:** false

### issue_2026-05-21-clear-command-doesnt-clear-live-context

**摘要:** /clear 命令只清 TUI 界面，不清 ACP Server 上下文
**状态:** Fixed
**归档日期:** 2026-05-24
**关键词:** /clear命令, ACP session/clear, 历史消息清理, SessionState
**问题本质:** TUI 层和 ACP Server 层有独立的会话状态（view_messages vs SessionState.history），/clear 只清了 TUI 侧
**通用模式:** 跨层状态清理必须通过协议请求（如 session/clear）而非仅清本地；所有层的状态重置必须在同一个请求中完成
**涉及文件:** peri-tui/src/command/core/clear.rs:19-21, peri-tui/src/app/thread_ops.rs:259-335, peri-tui/src/acp_server/mod.rs:39-52, peri-tui/src/acp_server/prompt.rs:88-155
**CLAUDE.md 链接:** false

### issue_2026-05-20-theme-markdown-color-decoupling

**摘要:** Markdown 与 Theme 颜色体系脱节，存在多处分叉硬编码
**状态:** Fixed
**归档日期:** 2026-05-25
**关键词:** Theme trait, 颜色解耦, MarkdownTheme, 适配器模式
**问题本质:** 三套独立颜色系统（DarkTheme / MarkdownTheme / 散落硬编码）互不联动，改 DarkTheme 不影响 Markdown 渲染
**通用模式:** widget 库通过 trait 暴露颜色接口，外部 adapter 桥接到库自身的 MarkdownTheme，消除分散硬编码
**技术决策:** `ThemeMarkdownAdapter<'a>` 包裹 `&dyn Theme` 实现 `MarkdownTheme` trait——零开销桥接
**涉及文件:** peri-widgets/src/markdown/mod.rs, peri-widgets/src/theme/mod.rs, peri-widgets/src/message_block/highlight.rs
**CLAUDE.md 链接:** false

### issue_2026-05-26-login-panel-hardcoded-chinese-no-i18n
**摘要:** Login 面板硬编码中文字符串未走 i18n，切换英文后仍显示中文
**状态:** Fixed
**归档日期:** 2026-05-26
**关键词:** i18n, hardcoded Chinese, LcRegistry, login panel
**问题本质:** 新增面板/组件时未遵循 i18n 规范，status_bar_hints 和渲染函数直接使用中文字面量，对应 FTL key 已存在但未被引用
**通用模式:** 新增 UI 文本必须使用 LcRegistry::tr()，禁止硬编码任何自然语言字符串。新增面板/组件的 checklist 应包含 i18n 检查项。
**涉及文件:** peri-tui/src/app/login_panel/component.rs, peri-tui/src/ui/main_ui/panels/login.rs, peri-tui/locales/en/main.ftl, peri-tui/locales/zh-CN/main.ftl
**CLAUDE.md 链接:** false

### issue_2026-05-25-compact-resubmit-missing-loading-spinner
**摘要:** Compact 后 Resubmit 缺少 Loading Spinner
**状态:** Closed
**归档日期:** 2026-05-27
**关键词:** loading spinner, compact resubmit, agent lifecycle
**问题本质:** compact 完成处理器调用 set_loading(false) 后，resubmit 阶段没有重新启用 spinner
**通用模式:** 自动化 resubmit/retry 路径需要与正常 agent 执行路径一致的生命周期状态管理
**涉及文件:** peri-tui/src/app/agent_compact.rs, peri-tui/src/app/agent_ops/lifecycle.rs
**CLAUDE.md 链接:** false

### issue_2026-05-23-thinking-tail-single-line-layout-jitter
**摘要:** 思考内容只显示最后一行导致自动换行布局抖动
**状态:** Verify
**归档日期:** 2026-05-27
**关键词:** thinking display, layout jitter, tail_lines, single-line wrap
**问题本质:** extract_tail_lines(text, 1) 只取最后 1 行，内容增长超出终端宽度时 ratatui 自动换行使渲染高度在 1↔2 行间跳变
**通用模式:** 流式内容渲染需固定区域高度或使用足够大的 tail_lines 值避免单行换行导致的布局变化
**涉及文件:** peri-tui/src/app/message_pipeline/reconcile.rs, peri-tui/src/ui/message_render.rs
**CLAUDE.md 链接:** false

### issue_2026-05-26-windows-paste-multiline-truncated
**摘要:** Windows 输入框粘贴多行内容被截断为单行发送
**状态:** Fixed
**归档日期:** 2026-05-27
**关键词:** Windows paste, bracketed paste, multiline input, cross-platform
**问题本质:** Windows 终端不支持 bracketed paste 协议，粘贴的多行内容被终端模拟为 Enter key event 触发 submit
**通用模式:** 跨平台输入处理必须考虑终端协议差异，`Event::Paste` 与 `Event::Key(Enter)` 是两条完全不同的处理路径
**涉及文件:** peri-tui/src/event/mod.rs, peri-tui/src/event/keyboard.rs, peri-tui/src/main.rs
**CLAUDE.md 链接:** false

### issue_2026-05-27-message-area-scrollbar-thumb-misaligned
**摘要:** 消息区域滚动条滑块位置与鼠标可拖拽位置不对齐
**状态:** Fixed
**归档日期:** 2026-05-27
**关键词:** scrollbar alignment, thumb geometry, mouse drag, ratatui formula
**问题本质:** 鼠标事件处理器使用简单线性公式，与 ratatui Scrollbar::part_lengths() 的 thumb 定位公式不一致
**通用模式:** UI 组件的鼠标交互必须复刻组件库自己的坐标计算公式，不能使用简化的线性近似
**涉及文件:** peri-tui/src/ui/main_ui/message_area.rs, peri-tui/src/event/mod.rs, peri-widgets/src/scrollable.rs
**CLAUDE.md 链接:** false

### issue_2026-05-30-render-event-unbounded-channel

**摘要:** RenderThread 事件通道使用 UnboundedChannel，极端情况下可能内存膨胀
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** 有界通道, 背压, 内存膨胀, 渲染线程
**问题本质:** 无界通道在极端场景（LLM 快速输出 + resize 风暴 + 大量 compact 事件）下事件积压导致内存无界增长
**通用模式:** 生产者-消费者场景中使用有界通道 + 背压防止内存无界增长；紧急事件可用 try_send + 覆盖策略
**涉及文件:** peri-tui/src/ui/render_thread.rs, peri-tui/src/app/message_pipeline/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-30-no-explicit-frame-rate-limit

**摘要:** TUI 渲染缺少显式帧率限制，loading 动画期间持续满帧重绘
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** 帧率限制, CPU 占用, loading 动画, 渲染节流
**问题本质:** loading 状态为 true 时每次事件循环都触发 terminal.draw()，无时间间隔检查
**通用模式:** 动画/loading 场景需要显式帧率限制（如 30 FPS），避免 CPU 空转
**涉及文件:** peri-tui/src/main.rs
**CLAUDE.md 链接:** false

### issue_2026-05-30-migrate-widgets-to-widgetref

**摘要:** peri-widgets 组件未使用 WidgetRef，渲染路径存在不必要克隆
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** WidgetRef, 所有权, 克隆, ratatui, 渲染优化
**问题本质:** 标准 Widget trait 消费所有权，流式输出每 100ms 重绘导致频繁重建和克隆
**通用模式:** 高频渲染场景使用引用渲染模式（WidgetRef/unstable-widget-ref feature）避免所有权转移
**涉及文件:** peri-widgets/src/markdown/mod.rs, peri-tui/Cargo.toml
**CLAUDE.md 链接:** false

### issue_2026-05-31-interaction-popup-textarea-not-disabled

**摘要:** 交互弹窗激活时底部常驻输入框未失效
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** 弹窗, Paste 事件, IME, 事件路由, 终端光标
**问题本质:** Paste 和 Mouse 事件不走弹窗键盘拦截路径，导致输入泄漏到底层 textarea
**通用模式:** 事件系统中每类事件（Key/Paste/Mouse）都需独立检查弹窗/模态状态；终端 IME 预编辑窗口依赖可见光标作为锚点，不能简单隐藏
**架构影响:** 终端 IME 兼容性要求光标可见性与输入焦点解耦
**涉及文件:** peri-tui/src/event/mod.rs, peri-tui/src/ui/main_ui/mod.rs, peri-tui/src/event/keyboard/popups.rs
**CLAUDE.md 链接:** false

### issue_2026-05-30-table-holdback-during-streaming

**摘要:** 流式 Markdown 表格渲染缺少 holdback 机制，显示不完整列
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** 表格, 流式, holdback, Markdown 解析, 列对齐
**问题本质:** Markdown 表格在流式输出中列数不完整时被提前渲染，导致列错位和视觉闪烁
**通用模式:** 流式渲染中结构性内容（表格、列表、代码块）需要完整性检测后再提交；不完整行保持 holdback 状态
**涉及文件:** peri-tui/src/ui/markdown/mod.rs, peri-tui/src/app/message_pipeline/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-30-markdown-parse-lru-cache

**摘要:** TUI Markdown 解析缺少 LRU 缓存，每次渲染完整重解析
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** LRU 缓存, Markdown 解析, pulldown-cmark, 性能优化
**问题本质:** Markdown 解析无缓存，resize/RebuildAll 时重复解析相同内容造成 CPU 开销
**通用模式:** 纯计算 + 输入不变的场景使用缓存（key = content_hash + 上下文参数如 max_width）
**涉及文件:** peri-widgets/src/markdown/mod.rs, peri-tui/src/ui/render_thread.rs
**CLAUDE.md 链接:** false

### issue_2026-05-29-ctrl-c-priority-chain-clear-input

**摘要:** Ctrl+C 改为优先级链：清空输入框 → 中断 Agent → 退出
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** Ctrl+C, 优先级链, 中断, 事件处理, 交互设计
**问题本质:** Ctrl+C 行为缺少优先级层次，输入框有内容时直接中断 Agent 或进入 quit-pending
**通用模式:** 全局快捷键应设计优先级链（从局部到全局），避免误操作；shell 风格交互中 Ctrl+C 先清空输入行是用户预期
**涉及文件:** peri-tui/src/event/keyboard/normal_keys.rs, peri-tui/src/app/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-31-at-mention-blocking-glob-search

- **摘要:** @ mention 文件搜索性能差 + 多目录搜不到
- **状态:** Fixed
- **归档日期:** 2026-06-03
- **关键词:** at-mention 文件搜索, glob 性能, walkdir, 线程隔离
- **问题本质:** glob::glob() 深度优先遍历无法跳过大目录(node_modules/target)，MAX_GLOB_RESULTS 截断导致有效结果丢失；spawn_blocking 占用 tokio 线程池不释放
- **通用模式:** 文件系统遍历应用 walkdir + should_skip_dir 在目录层级过滤，而非 glob 后截断；CPU/内存密集搜索应放独立线程 + idle 自动退出，不占 tokio 线程池
- **技术决策:** 从 glob crate 迁移到 walkdir + should_skip_dir（对齐 GlobFilesTool），搜索从 spawn_blocking 改为 std::thread::spawn + mpsc + recv_timeout idle 退出
- **涉及文件:** peri-tui/src/app/at_mention/file_search.rs, peri-tui/src/app/at_mention/mod.rs, peri-tui/src/event/keyboard.rs

### issue_2026-06-02-rewind-loses-messages-esc-unresponsive

- **摘要:** Rewind 回退后前文消息全部丢失 + 双击 ESC 偶发无响应
- **状态:** Fixed
- **归档日期:** 2026-06-03
- **关键词:** Rewind 消息丢失, RebuildAll, 双击 ESC, rewind_pending_since
- **问题本质:** handle_rewind_completed 只把保留消息放入 pipeline.completed 但未触发 VM 转换，RebuildAll 的 tail_vms 只有 rewind 通知，保留消息永远不渲染；兜底分支无差别重置 rewind_pending_since 导致双击序列被中间事件中断
- **通用模式:** pipeline 操作后必须确保 completed 消息被渲染（通过 messages_to_view_models 或 StateSnapshot 触发）；双击/连续按键检测不应在兜底分支重置状态，应在明确的用户输入分支处理
- **架构影响:** rewind 与 compact 共享 pipeline 操作但 rewind 没有后续 agent 执行来触发 StateSnapshot，需自行处理渲染
- **涉及文件:** peri-tui/src/app/agent_compact.rs, peri-tui/src/event/keyboard/normal_keys.rs

### issue_2026-06-01-remove-split-multi-session

- **摘要:** 移除 /split 多 session 分屏功能
- **状态:** Fixed
- **归档日期:** 2026-06-03
- **关键词:** 多 session 分屏, SessionManager, 架构简化, /split 移除
- **问题本质:** TUI 层维护多 session 并发分屏功能增加 ~900 处 session_mgr 引用，但用户实际需求被 tmux 等工具覆盖，投入产出不成比例
- **通用模式:** 低使用率功能的大面积架构复杂度应及时清理；终端应用的多窗口需求应交给终端复用工具而非应用自身
- **架构影响:** SessionManager 保留但限制 len=1，多列布局改单列，删除 /split 命令和 Ctrl+N/P/W 快捷键
- **技术决策:** 完全移除 TUI 多 session 并发分屏，保留 ACP 层 SessionStore 的多 session 存储（用于 /history 恢复）
- **涉及文件:** peri-tui/src/command/session/split.rs, peri-tui/src/app/session_manager.rs, peri-tui/src/ui/main_ui/mod.rs

---

## 相关 Feature

- → [agent.md#20260322_F001_agent-storage-refactor](./agent.md#20260322_F001_agent-storage-refactor) — SQLite 持久化，TUI 消息渲染依赖此存储
- → [langfuse.md#feature_20260324_F001_langfuse-tui-monitoring](./langfuse.md#feature_20260324_F001_langfuse-tui-monitoring) — Langfuse 追踪集成在 TUI 的 app/agent.rs
- → [agent.md#feature_20260328_F001_ask-user-question-align](./agent.md#feature_20260328_F001_ask-user-question-align) — AskUser 弹窗展示更新（header + description），TUI 弹窗同步更新
- → [tui-widgets.md](./tui-widgets.md) — peri-widgets 独立 widget crate 和 Spinner/ToolCall/MessageBlock 组件
- → [code-highlight.md](./code-highlight.md) — syntect 代码高亮集成到 Markdown 渲染
- → [mouse-selection.md](./mouse-selection.md) — 鼠标拖拽文字选区和剪贴板复制
- → [skill-trigger.md](./skill-trigger.md) — Skills 触发键从 # 统一到 / 前缀
- → [hitl-permissions.md](./hitl-permissions.md) — 5 级权限模式 Shift+Tab 切换
- → [model-config.md](./model-config.md) — /login 面板 Provider CRUD
- → [message-pipeline.md](./message-pipeline.md) — MessagePipeline 统一消息管线
- → [compact.md](./compact.md) — Micro/Full Compact 策略增强
