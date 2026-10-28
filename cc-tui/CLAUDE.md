# cc-tui

TUI 应用，纯 ACP client 前端。运行时仅通过 `cc-acp` 的 `MpscTransport`（in-memory channel pair）与 ACP Server 通信，不直接依赖 `cc-agent`/`cc-middlewares` 的运行时路径。

## 依赖说明

`Cargo.toml` 保留 `cc-agent`/`cc-middlewares` 作为**类型依赖**（UI 渲染所需的 `BaseMessage`/`ContentBlock` 等类型），运行时通信仅通过 `cc-acp`。

## 核心文件

| 文件 | 职责 |
|------|------|
| `src/acp_client/client.rs` | ACP client 封装，`AcpNotification` 变体定义 |
| `src/acp_server/requests.rs` | ACP 请求路由 |
| `src/app/agent.rs` | `ExecutorEvent → AgentEvent` 映射（`map_executor_event`） |
| `src/app/agent_ops/acp_bridge.rs` | `AcpNotification → AgentEvent` 桥接 |
| `src/app/agent_ops/lifecycle.rs` | Agent 生命周期处理 |
| `src/app/agent_submit.rs` | 用户输入提交入口 |
| `src/app/agent_compact.rs` | Compact 事件处理：pipeline 清理 + UI 通知 |
| `src/app/message_pipeline/mod.rs` | `MessagePipeline`：规范状态维护 + `messages_to_view_models()` |
| `src/ui/main_ui/mod.rs` | 主布局 |
| `src/i18n/` | 国际化模块（`LcRegistry` + Fluent） |

## ACP 数据流

```
TUI 输入 → AcpTuiClient.new_session() / .prompt()
         → MpscClientTransport.send_request/notification()
         → MpscServerTransport.recv() (ACP Server, tokio::spawn)
         → ExecutorEvent → TransportEventSink.push_event()
         → AcpTuiClient.pump_notifications() → AcpNotification::AgentEvent
         → agent_ops::acp_bridge::handle_acp_notification()
         → map_executor_event() → AgentEvent
         → handle_agent_event() → UI 更新
```

**[TRAP]** TUI 层数据必须通过 ACP 协议到达 ACP 层，禁止直连。所有 TUI → ACP Server 的状态变更必须通过 `acp_client` 的协议方法。TUI 本地清空状态（如 `new_thread()`）不等于 ACP Server 端状态同步——必须同时通过 ACP 协议通知 Server 侧。（详见 spec/global/domains/agent.md#issue_2026-05-29-clear-keeps-acp-server-history）

## 终端模式

交互模式先在正常屏中使用固定区域视口，从 shell 当前输出下方显示紧凑欢迎卡；`terminal_backend/viewport.rs` 的 `ViewportState` 记录真实起点、已占用行数与全屏锁定状态。首帧靠近底部时仅滚动必要行数；正常屏只绘制和清理应用拥有的区域，不能逐帧全屏 Clear。

内容超过既有占用高度时向下扩展；每次下方空间不足都原生滚动必要行数并上移 `origin_row`，只有 `max(required_rows, allocated_rows)` 达到整个宿主高度才锁定全屏，不能按起点以下的 `available_rows` 提前切换。全屏锁定是本次终端运行级状态，不写会话，不因折叠、`/clear`、恢复历史或切换子视口解除。全屏消息滚动由 TUI 自渲染滚动条接管，`base_visual` 恒为 0。切换前先等待目标宽度重排，再同步输出切屏、resize 与首帧，并合并重复清屏。

**[TRAP]** 外部编辑器挂起（`panel_memory.rs`）需要关闭鼠标捕获、记录当前终端模式；返回后恢复原模式与应用区域。正常屏编辑时不能无条件 `EnterAlternateScreen`，编辑器临时全屏也不能锁定应用全屏。

退出时逐行清除应用拥有的正常屏区域，光标回到该区域起点，仅输出 Session ID、Resume 和 shell 提示符，保留其上方 shell 历史。全屏绘制不覆盖正常屏区域记录；退出先恢复正常屏，再清除进入全屏前的应用区域。成功清理后消费记录，显式恢复与 Drop 重复调用不会擦掉退出提示。异常恢复同样通过统一终端恢复路径清理 raw mode、鼠标和光标。

此改动已随 v0.6.171 发布（PR #652，issue #650）；验收范围见 [PRD](../docs/prd/growing-terminal-viewport.md)。

## 消息渲染

所有消息更新通过统一 `RebuildAll` 路径触发（无增量更新）。`MessagePipeline` 维护规范状态，`build_tail_vms()` 构建尾部 VMs，`messages_to_view_models()` 是唯一转换入口。流式文本通过 16ms 间隔 + 自适应分块策略触发 RebuildAll。独立 `RenderThread` 处理渲染，通过 `RenderCache(RwLock)` 与 UI 线程同步。

输入、后台更新与动画共用 33ms 绘制期限，连续请求不能推迟尾帧。`prepare_viewport` 通过 `try_prepare_width` 非阻塞投递/收取宽度确认，未就绪时保留旧屏并继续处理输入；同一内容布局的滚动不再投递宽度屏障。内容请求使用独立 revision，历史/模式/视图替换另推进 epoch，避免误收旧确认；虚拟窗口实测高度仅在是否可容纳的边界改变时重新准备宽度。缓存构造及旧成品析构在共享写锁外完成。拖选后的左键释放仍等最后一帧呈现，再从快照提取复制内容。

**[TRAP]** Ephemeral VM（SystemNote/CacheWarning）依赖锚点机制：`ephemeral_notes: Vec<(usize, MessageViewModel)>` 记录插入时的 `view_messages.len()` 作为位置索引（非 MessageId）。RebuildAll 时通过 `(anchor - prefix_len).min(tail_len) + prefix_len` 计算插入位置。`retain()` 路径通过 `anchor >= prefix_len` 过滤过期锚点。新增 ephemeral VM 类型必须同步更新过滤逻辑。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild）

**[TRAP]** BaseMessage vs MessageViewModel 维度混淆：`completed_len_at_round_start` 是 BaseMessage 长度，`prefix_len` 是 VM 索引，两者非 1:1。`prefix_len` 必须用 `round_start_vm_idx`，`drain` 必须钳位。**禁止 Pipeline 内部返回 `RebuildAll`**——Pipeline 不拥有 `round_start_vm_idx`。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker）

**[INFO]** `MessageViewModel` 已不再包含 `message_id` 字段。SubAgentGroup 使用 `instance_id: Option<String>` 标识。

**[TRAP]** `SystemNote` 是双态 VM：`content`（普通模式）与 `detail_content: Option<String>`（详细模式）由 `detail_mode` 单选渲染。`detail_content` **必须**同时参与 `PartialEq` 与 `Hash`，否则 Ctrl+O 切换时 hash diff 判定未变、界面不刷新。子 Agent 启动摘要依赖此机制在极简树形与 Prompt 卡片间切换；解析卡片文本必须用 `str::strip_prefix`，禁止 `&line[4..]` 字节下标切片（`└` 占 3 字节会 panic）。卡片圆点用 `U+25CF`（对齐 `● Bash(...)`），不得用 `U+2022`。（详见 docs/prd/subagent-drawer-and-virtual-routing.md §4.17）

**[TRAP]** frozen_subagent_vms 按 agent_id + 位置匹配（先 instance_id 精确匹配，失败后按顺序 agent_id 匹配）。`begin_round()` 清空 frozen_vms 和 ephemeral_notes，但 `done()` 不清空 frozen_subagent_vms（允许 Done→下一轮之间消费）。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication）

**[TRAP]** 子 Agent 停止态有三处易错：① 停止终态**必须独立成行**（`was stopped by main Agent` / `stopped by user`），不得混入 `finished`/`failed`——否则用户分不清「跑完了」「出错了」「被停了」；来源由 `BackgroundAgentSession.stopped_by_user` 区分（仅用户按键路径置位）。② 抽屉停止行用**红色实心 `●`**（`theme::ERROR`），`drawer_line()` 的 `force_filled` 参数保证非当前视口时也渲染实心（`◯` 只用于「非当前视口的活跃行」）。③ 等待计数行 `✻ Waiting for N … to finish` 是 `ephemeral_notes`，**活跃数归零必须移除**（残留会让用户误以为还有任务在跑）；清理匹配用「固定前缀 + 固定后缀」，不要用裸 `contains("Waiting for")` 以免误伤其他系统消息。

## 主布局

单 Session 垂直切分（Sticky Header → Messages → Attachment Bar → Panel Area → Input → Status Bar → BG Agent Bar）。`ui/main_ui/viewport_layout.rs` 共用底部高度测量与内容所需行数：Status Bar 空闲 2 行、有活动 3 行；Input 最少 3 行、上限为宿主终端高度的 40%；面板按宿主尺寸计算。增长阶段底部区域紧随内容，全屏阶段固定到底部。宽度变化先等待 RenderCache 重排再判断全屏，不能把上一帧剩余留白计入新增内容。

### 界面组件

| 组件 | 文件 | 说明 |
|------|------|------|
| Welcome Card | `ui/welcome.rs` | 左上角紧凑卡，实际版本、cwd、/help 与无 Provider 时 /login；宽度足够时保留小型吉祥物 |
| Sticky Header | `ui/main_ui/sticky_header.rs` | 滚动时顶部固定显示最后 Human 消息摘要 |
| Attachment Bar | `ui/main_ui/attachment.rs` | 图片附件标签列表，Input 正上方 |
| Input Area | `edit_utils.rs` | `tui_textarea::TextArea` 封装，高度动态 |
| Hints 浮层 | `ui/main_ui/popups/hints.rs` | `/` 前缀命令匹配，输入框上方 |
| @提及弹窗 | `app/at_mention/mod.rs` | `@` 立即弹出当前目录候选（最多 15 项 / 10 行），关键词搜索 200ms 防抖 |
| BG Agent Bar | `ui/main_ui/bg_agent_bar.rs` | 后台 Agent 列表，8 色循环 |

### 弹窗系统

统一通过 `InteractionPrompt` 枚举互斥管理。4 种弹窗：
- **HITL 审批**（`popups/hitl.rs`）：批量工具调用逐个审批
- **AskUser 问答**（`popups/ask_user.rs`）：Tab 栏切换 + 选项列表 + 自定义输入
- **OAuth 授权**（`popups/oauth.rs`）：URL 显示 + 浏览器打开
- **Setup Wizard**（`popups/setup_wizard.rs`）：首次配置向导

### 面板系统

15 种 `PanelKind`（分 Session/Global 作用域）：ModelPanel、LoginPanel、ConfigPanel、AgentPanel、HooksPanel、ThreadBrowser、CommandPalettePanel、ThemePanel、InputHistoryPanel（Session，9 种）；McpPanel、PluginPanel、CronPanel、StatusPanel、MemoryPanel、BackgroundTasksPanel（Global，6 种）。

互斥组（`MutexGroup`）：Settings（Model/Login/Config/CommandPalette/Theme）、Agent（Agent/Hooks）、Tools（MCP/Plugin/Cron/BackgroundTasks）、Info（Status/Memory）、Thread（ThreadBrowser 独占）。

`PanelManager` + `PanelComponent` trait（`panel_manager.rs`/`panel_component.rs`），新增面板只需定义变体 + 实现 trait。面板内禁止渲染提示行，由 `status_bar_hints()` 统一描述。

### Status Bar

2~3 行动态折叠布局（`ui/main_ui/status_bar.rs`，`has_hud_activity` 时展开第 2 行，否则折叠）：
- **第一行**（HUD Compact）：`[model] context | project git:(branch*) | stats`
- **第二行**（活动时展开）：运行中工具 / Agent / Todo activity
- **底行**（无活动时为第 2 行，有活动时为第 3 行）：权限模式（含 Shift+Tab 切换提示）与瞬时状态提示 → 右侧快捷键 hints 与 CPU/MEM 监控

瞬时提示用 `Instant` + Duration 控制消失；颜色分级用 `theme::ERROR`/`WARNING`/`SAGE`；面板 hints 通过 `PanelComponent::status_bar_hints()` trait 注入。

**[TRAP]** 状态栏配色是打磨定稿的固定值，**不参与主题化**——`ui/main_ui/status_bar.rs` 内经固定配色常量模块 `sb` 输出，渲染结果与主题系统引入之前逐字节一致。禁止把状态栏颜色改回 `theme::cur()` 调色板字段，否则观感会随 `/theme` 漂移（#523）。

### 消息区

空会话显示紧凑 Welcome Card；启动卡作为纯 UI 前缀跨首条消息与全屏切换保留，并随消息滚动。恢复已有历史不补插启动卡。卡片不写入消息历史、系统提示词或消息渲染缓存，其视觉高度单独参与测量、裁剪与滚动范围计算。消息列表 + 滚动条 + spinner 仍按视口裁剪渲染（`viewport_clip`）；非零起点保持真实坐标供鼠标、复制和链接命中。`MessageViewModel` 8 种变体：`UserBubble` / `AssistantBubble`（含 Text/Reasoning/ToolUse） / `ToolBlock` / `ShellCommand`（TUI 本地执行与后台化） / `SystemNote` / `CacheWarning` / `ToolCallGroup` / `SubAgentGroup`。

## i18n

`LcRegistry` 存储在 `ServiceRegistry.lc` 中，翻译资源通过 `include_str!` 编译时嵌入 `locales/{lang}/main.ftl`。

**[TRAP]** `FluentBundle::get_message` 返回的 `FluentMessage` 生命周期绑定在 bundle 上，`tr()` 方法必须返回 `String` 而非 `&str`。

Command trait 的 `description()` 接收 `&LcRegistry` 参数并返回 `String`。`CommandRegistry::match_prefix()` 和 `list()` 均需 `&LcRegistry`。

## 状态管理

**`ServiceRegistry` 与 `GlobalUiState`**：`App` 状态拆分为 `ServiceRegistry`（跨会话共享：config/MCP/cron/provider）和 `GlobalUiState`（纯 UI 临时状态：高亮计时器/弹窗）。面板 dispatch 宏（`with_global_panels!`/`with_session_panels!`）位于 `event/macros.rs`。

**`CommandRegistry::dispatch` 借用限制 [TRAP]**：`&self` + `&mut App` 冲突，用 `std::mem::take` + put-back 解决。dispatch 期间不可改变 `app.session_mgr` 的 session 实例。

## Compact 事件处理

**[TRAP]** `handle_compact_completed` 必须三步清理：① `pipeline.clear()` ② `pipeline.restore_completed(messages)` ③ `RebuildAll { prefix_len: 0 }`。缺少任一步都会导致旧消息残留或 system 消息泄漏。禁止在 TUI 层触发 auto-compact——所有触发判断在 executor 内部。（详见 spec/global/domains/compact.md#issue_2026-05-20-compact-command-not-triggering）

**[TRAP]** `restore_completed(messages)` 会把 system 消息放入 completed 列表。re_inject 产生的 System 消息不应渲染。`round_start_vm_idx` 和 `completed_len_at_round_start` 必须正确设置。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt）
