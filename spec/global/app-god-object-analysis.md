# App God Object 分析与分层方案

> **状态**: 调研完成，待实施
> **关联设计**: [组件化面板架构](./component-architecture-design.md)
> **日期**: 2025-05-08

## 1. 问题描述

当前 `App` 结构体承载了 34 个字段，`AppCore` 额外 39 个字段，`AgentComm` 22 个字段，`ChatSession` 7 个字段。总计 **102 个字段** 分布在 4 个结构体中，混合了 6 种以上职责：

- 会话管理（sessions, active）
- 应用配置（provider, model, peri_config）
- 外部服务（mcp_pool, thread_store, cron, plugins）
- 面板状态（13 个 Option&lt;Panel&gt;）
- UI 状态（scroll, selection, highlight timers）
- Agent 生命周期（event channel, cancel token, token tracking）

**核心问题**：

| 问题 | 影响 |
|------|------|
| 新增功能难以定位相关字段 | 34 个字段中找目标状态成本高 |
| 测试隔离困难 | config_path_override 等测试专用字段混入生产代码 |
| 借用冲突频繁 | `std::mem::take` 临时交换、`active` 临时替换 |
| 状态追踪复杂 | 无法快速判断"谁在什么时候修改了什么" |
| 方法爆炸 | App 上 90+ 个公开方法分布在 11 个 impl 块中 |

---

## 2. 字段分类统计

| 分类 | App | AppCore | AgentComm | ChatSession | 合计 |
|------|-----|---------|-----------|-------------|------|
| PanelManager | 6 | 7 | 0 | 0 | **13** |
| UiState | 5 | 20 | 0 | 0 | **25** |
| ServiceRegistry | 6 | 5 | 0 | 0 | **11** |
| AppConfig | 4 | 0 | 0 | 0 | **4** |
| SessionManager | 3 | 0 | 0 | 0 | **3** |
| AgentLifecycle | 0 | 0 | 22 | 0 | **22** |
| SessionMetadata | 0 | 0 | 0 | 5 | **5** |
| TestInfrastructure | 2 | 0 | 1 | 0 | **3** |
| **合计** | **26** | **32** | **23** | **5** | **86** |

---

## 3. 依赖关系分析

### 3.1 event.rs 依赖

event.rs（2486 行）依赖几乎所有字段：

- **读取+写入**: sessions, active, 所有 panel Option, loading, textarea, hint_cursor, history_index, text_selection, panel_selection, scroll_offset, pending_messages, quit_pending_since, highlight timers, permission_mode, peri_config
- **仅读取**: cwd, provider_name, model_name, session_areas, messages_area, textarea_area, panel_area

### 3.2 main_ui.rs 依赖

- **读取**: sessions, active, setup_wizard, 所有 panel, view_messages, textarea, loading, scroll state, selection state, highlight timers, provider/model for status bar, background_task_count, spinner_state, interaction_prompt
- **写入**: session_areas, messages_area, textarea_area, panel_area, panel_plain_lines

### 3.3 agent_ops.rs 依赖

- cwd, peri_config, provider_name, sessions, active, view_messages, textarea, loading, scroll, pending state, render pipeline, agent_rx, agent_state_messages, cancel_token, token tracking, retry, background tasks

### 3.4 panel_ops.rs 依赖

- peri_config, provider_name, model_name, cwd, sessions, active, config_path_override, 所有 panel options, panel selection state

---

## 4. 分层方案设计

### 重构前

```
App (34 fields, God Object)
├── sessions: Vec<ChatSession>
│   └── ChatSession (7 fields)
│       ├── core: AppCore (39 fields)
│       └── agent: AgentComm (22 fields)
├── 13 个 Option<Panel> 字段分散在 App + AppCore
└── 混合 UI/Config/Service/Panel/Session 状态
```

### 重构后

```
App (3 fields)
├── services: ServiceRegistry       ← 全局服务（13 字段）
├── session_mgr: SessionManager     ← 会话管理（3 字段）
└── panels: GlobalPanelManager      ← 全局面板（5 字段）

ChatSession (6 fields)
├── session_panels: SessionPanelManager  ← 会话级面板（5 字段）
├── ui: UiState                         ← UI 状态（17 字段）
├── messages: MessageState               ← 消息渲染（9 字段）
├── agent: AgentState                    ← Agent 通信（22 字段）
├── commands: CommandSystem              ← 命令系统（3 字段）
└── metadata: SessionMetadata            ← 会话元数据（5 字段）

AppCore → 消除
```

### Layer 1: ServiceRegistry

```rust
pub struct ServiceRegistry {
    pub peri_config: Option<PeriConfig>,
    pub cwd: String,
    pub provider_name: String,
    pub model_name: String,
    pub permission_mode: Arc<SharedPermissionMode>,
    pub thread_store: Arc<dyn ThreadStore>,
    pub mcp_pool: Option<Arc<McpClientPool>>,
    pub mcp_init_rx: Option<watch::Receiver<McpInitStatus>>,
    pub bg_event_tx: mpsc::Sender<AgentEvent>,
    pub bg_event_rx: Option<mpsc::Receiver<AgentEvent>>,
    pub cron: CronState,
    pub plugin_data: Option<PluginLoadResult>,
    pub config_path_override: Option<PathBuf>,
    pub claude_settings_override: Option<PathBuf>,
}
```

### Layer 2: SessionManager

```rust
pub struct SessionManager {
    pub sessions: Vec<ChatSession>,
    pub active: usize,
    pub session_areas: Vec<Rect>,
}
```

### Layer 3: GlobalPanelManager

```rust
pub enum GlobalActivePanel {
    None,
    SetupWizard(SetupWizardPanel),
    OAuth(OAuthPrompt),
    Mcp(McpPanel),
    Status(StatusPanel),
    Memory(MemoryPanel),
    Plugin(PluginPanel),
}

pub struct GlobalPanelManager {
    pub active: GlobalActivePanel,
    pub mode_highlight_until: Option<Instant>,
    pub model_highlight_until: Option<Instant>,
    pub mcp_ready_shown_until: Cell<Option<Instant>>,
    pub quit_pending_since: Option<Instant>,
}
```

### Layer 4: SessionPanelManager

```rust
pub enum SessionActivePanel {
    None,
    Model(ModelPanel),
    Login(LoginPanel),
    Agent(AgentPanel),
    Hooks(HooksPanel),
    Config(ConfigPanel),
    ThreadBrowser(ThreadBrowser),
}

pub struct SessionPanelManager {
    pub active: SessionActivePanel,
    pub panel_area: Option<Rect>,
    pub panel_plain_lines: Vec<String>,
    pub panel_scroll_offset: u16,
    pub panel_selection: PanelTextSelection,
}
```

### Layer 5: UiState

```rust
pub struct UiState {
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub pending_messages: Vec<String>,
    pub hint_cursor: Option<usize>,
    pub pending_attachments: Vec<PendingAttachment>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: Option<String>,
    pub scroll_offset: u16,
    pub scroll_follow: bool,
    pub show_tool_messages: bool,
    pub text_selection: TextSelection,
    pub messages_area: Option<Rect>,
    pub textarea_area: Option<Rect>,
    pub copy_message_until: Option<Instant>,
    pub copy_char_count: usize,
}
```

### Layer 6: MessageState

```rust
pub struct MessageState {
    pub view_messages: Vec<MessageViewModel>,
    pub round_start_vm_idx: usize,
    pub pipeline: MessagePipeline,
    pub last_human_message: Option<String>,
    pub last_submitted_text: Option<String>,
    pub pre_submit_state_len: usize,
    pub render_tx: mpsc::UnboundedSender<RenderEvent>,
    pub render_cache: Arc<RwLock<RenderCache>>,
    pub render_notify: Arc<Notify>,
    pub last_render_version: u64,
}
```

### Layer 7-9: AgentState / CommandSystem / SessionMetadata

保持现有字段不变，仅重组归属。

---

## 5. 借用检查器挑战

### 5.1 现有 Workaround

| Workaround | 位置 | 原因 |
|-----------|------|------|
| `std::mem::take` 临时交换 | event.rs:560 | command_registry dispatch 需要 &amp;mut App |
| 临时 active 交换 | main_ui.rs:71 | 渲染每列时临时切换 active index |
| 顺序面板关闭 | panel_ops.rs | 无集中面板管理，手动互斥 |

### 5.2 分层后的解决方案

- **方案 A**: 字段投影拆分借用 — Rust 允许同时可变借用不同字段
- **方案 B**: Accessor Trait — `trait HasServices { fn services(&amp;self) -&gt; &amp;ServiceRegistry; }`
- **方案 C**: 延迟变异队列 — 收集命令统一执行
- **方案 D**: 枚举路由消除 unwrap — `SessionActivePanel` 枚举 match 替代 12 个 Option

---

## 6. 迁移风险评估

### 高风险

| 变更 | 原因 | 缓解策略 |
|------|------|---------|
| event.rs 重写 | 980 行，每个分支假设 &amp;mut App 全访问 | 先引入 accessor trait，逐步替换 |
| poll_agent 重构 | 更新 20+ 字段响应 AgentEvent | 按 AgentEvent variant 逐个迁移 |
| submit_message 重构 | 触及 15+ 字段跨 App/AppCore/AgentComm | 创建 AgentBuilder 封装初始化 |
| AppCore 消除 | 所有 `session.core.*` 路径需更新 | 最后执行，全项目搜索替换 |

### 中风险

| 变更 | 缓解策略 |
|------|---------|
| main_ui.rs render | 传递 &amp;SessionManager + index |
| MessagePipeline | 传参而非自身借用 |
| 测试隔离 | 移入 ServiceRegistry 统一管理 |
| PanelManager 提取 | 与 component-architecture-design.md 合并执行 |

### 低风险

- ServiceRegistry 提取（大部分不可变）
- SessionManager 提取（已有良好封装）
- CommandSystem 提取（创建后不可变）
- SessionMetadata 提取（访问频率低）

---

## 7. 推荐重构顺序

| 阶段 | 内容 | 风险 | 预估 | 前置依赖 |
|------|------|------|------|---------|
| 1 | 提取 ServiceRegistry | 低 | 2 天 | 无 |
| 2 | 提取 SessionManager | 低 | 1 天 | 无 |
| 3 | 提取 PanelManager | 中 | 3 天 | 阶段 1, 2 |
| 4 | 提取 UiState | 中 | 2 天 | 阶段 2 |
| 5 | 提取 MessageState | 中 | 2 天 | 阶段 4 |
| 6 | 提取 AgentState | 高 | 3 天 | 阶段 5 |
| 7 | 提取 CommandSystem | 低 | 0.5 天 | 阶段 4 |
| 8 | 提取 SessionMetadata | 低 | 0.5 天 | 阶段 2 |
| 9 | 消除 AppCore | 高 | 3 天 | 阶段 4-8 |
| 10 | 消除 God Object | 高 | 2 天 | 阶段 9 |

**总预估**: 约 19 个工作日（~4 周），可增量交付。

---

## 8. 成功指标

| 指标 | 当前值 | 目标值 |
|------|--------|--------|
| App 字段数 | 34 | 3 |
| AppCore 字段数 | 39 | 0（消除） |
| ChatSession 字段数 | 7 | 6（结构化子模块） |
| 单层最大字段数 | 34 (App) | 22 (AgentState) |
| std::mem::take 次数 | 1 | 0 |
| 新增面板需改文件数 | 3+ | 1 |
| unwrap() 面板访问次数 | 28 | 0 |

---

## 9. 与面板组件化重构的关系

本文档的 **阶段 3（PanelManager 提取）** 与 [组件化面板架构设计](./component-architecture-design.md) 高度重叠。建议：

- **合并执行**：PanelManager 提取作为面板组件化的阶段 2
- **统一枚举**：`SessionActivePanel` 和 `GlobalActivePanel` 直接采用设计文档中的 `PanelKind` 枚举
- **先做面板**：面板组件化（阶段 1-3）可在 App 分层之前独立推进

推荐路线：

1. 先完成面板组件化（component-architecture-design.md 的 Phase 1-5）
2. 然后按本方案的阶段 1-2-4-5-6-7-8-9-10 顺序推进 App 分层
3. 面板组件化的 Phase 2（PanelManager）即为本方案的阶段 3
