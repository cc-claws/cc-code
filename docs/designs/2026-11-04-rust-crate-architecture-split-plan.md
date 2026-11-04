# Peri 架构深度拆分与分阶段落地实施方案

> **设计日期**：2026-11-04  
> **审计基线**：最新代码仓（2026-11-04，971 个 Rust 源码文件，259,801 行总代码）  
> **涉及 Crate**：`cc-tui`, `cc-middlewares`, `cc-agent`, `cc-acp`, `cc-widgets`, `cc-tools` (新增), `cc-terminal` (新增), `cc-tui-core` (规划)  
> **文档定位**：大型 Rust Workspace 模块深度解耦、消除循环依赖与编译单元治理技术规范。

---

## 1. 现状度量与代码深度审计

### 1.1 最新全仓精确度量数据（2026-11-04 基线）

排除 `target/` 构建缓存与 `.claude/` 临时目录后，全仓 Rust 源码精确统计如下：

```
====================================================================================================
 Crate               总文件数   测试文件数     总行数 (Lines)   测试行 (Lines)   占比    主要职责
====================================================================================================
 cc-tui                458        178         137,199           59,984         52.8%   TUI 会话、状态、命令、渲染、终端
 cc-middlewares        227         98          57,668           27,548         22.2%   HITL、子Agent、Hook、Plugin、工具
 cc-agent              124         52          31,278           16,640         12.0%   ReAct 循环、LLM 适配、持久化
 cc-widgets             62         29          14,742            5,951          5.7%   通用终端 Widget 组件库
 cc-acp                 68         21          14,025            5,138          5.4%   ACP 协议服务层、Stdio/Mpsc 桥接
 langfuse-client        13          5           2,565            1,022          1.0%   Langfuse 遥测客户端（独立）
 cc-lsp                 19          6           2,324              460          0.9%   LSP 客户端库（独立）
----------------------------------------------------------------------------------------------------
 Total                 971        389         259,801          116,743        100.0%   单测代码占比：44.9%
====================================================================================================
```

### 1.2 核心瓶颈 Crate 内部模块构成深度剖析

#### 1. `cc-tui`（137,199 行，占比 52.8%）内部构成
- **`src/app/`（会话与应用状态机）**：173 文件，**51,967 行**（含 11 类 Panel 逻辑、消息管道、交互状态）
- **`src/ui/`（视图渲染与组件）**：115 文件，**50,862 行**（含 `message_view` 8,020 行、`main_ui`、Markdown 渲染、渲染线程）
- **`src/event/`（事件驱动与调度）**：31 文件，**7,743 行**（键盘、鼠标批处理、输入泵、重绘调度）
- **`src/terminal_backend/`（终端兼容层）**：13 文件，**3,809 行**（OSC 8、双缓冲区、Windows 视口兼容）
- **`src/command/`（Slash 命令系统）**：44 文件，**3,506 行**（20+ 条 slash 命令处理）
- **`src/acp_server/`（内嵌 ACP 服务运行时）**：8 文件，**2,960 行**（TUI 端内嵌 ACP Session 调度）
- **`src/clipboard/`（剪贴板跨平台层）**：9 文件，**2,009 行**（Linux lease / macOS / Windows / OSC 52）
- **顶层基础设施**：约 **11,000 行**（`conpty.rs`, `terminal_color.rs`, `terminal_title.rs`, `keyboard_modes.rs`, `shell_exec.rs`, `cli_args.rs` 等）

#### 2. `cc-middlewares`（57,668 行，占比 22.2%）内部构成
- **`hitl/`**：34 文件，9,203 行（审批门、安全校验）
- **`tools/`**：28 文件，7,651 行（基础文件工具、SkillTool、AskUserTool、Todo）
- **`subagent/`**：21 文件，7,339 行（子 Agent 派发、生命周期管理）
- **`hooks/`**：22 文件，7,146 行（Git/Pre-commit/Post-execute 钩子）
- **`plugin/`**：19 文件，6,186 行（市场、插件安装卸载、清单）
- **`mcp/`**：23 文件，4,823 行（Stdio/SSE 客户端连接池）
- **`middleware/`**：12 文件，2,272 行（含 `terminal.rs` 中声明的 `BashTool`，`web_fetch.rs`，`web_search.rs`）
- **`tool_search/`**：13 文件，1,954 行（延迟工具发现与执行）
- **`process/`**：5 文件，1,408 行（跨平台进程管道、作业对象管理）

---

## 2. 关键代码审计：原拆分方案的三大致命盲区与根因分析

对原有简易拆分方案进行编译器级依赖审计，发现以下不可逾越的致命缺陷。任何盲目的“一刀切”物理搬迁都会直接导致全仓循环依赖或大规模符号未解析。

### 2.1 致命盲区一：`cc-tools` 拆分导致 Crate 循环依赖（Circular Crate Dependency）

* **原方案设想**：将 `cc-middlewares/src/tools/**` 整体搬迁至 `cc-tools`，声称其“仅依赖 `cc-agent (BaseTool)`”。
* **实际代码审计事实**：
  1. **`tools/skill.rs` 深度耦合中间件**：持有 `Option<Arc<SubAgentTool>>`，直接引用 `crate::skills::invocation`、`crate::subagent::SubAgentTool` 和 `crate::tool_search::core_tools::TOOL_SKILL`；
  2. **`tools/ask_user_tool.rs` 强依赖中间件**：直接依赖 `crate::ask_user::ask_user_tool_definition`；
  3. **工具物理位置认知错位**：原方案声称将 `BashTool`、`WebFetchTool`、`WebSearchTool` 放入 `cc-tools`，但**这三个工具根本不在 `tools/` 目录下**！`BashTool` 定义在 `src/middleware/terminal.rs`，`WebFetchTool` 定义在 `src/middleware/web_fetch.rs`，`WebSearchTool` 定义在 `src/middleware/web_search.rs`。
* **致命后果**：若将 `tools/` 整体迁入 `cc-tools`，`cc-tools` 必须反向依赖 `cc-middlewares`（引入 skills/subagent/ask_user）；而 `cc-middlewares` 又必须依赖 `cc-tools`（消费基础工具）。**Cargo 将立即报错：`Cycle detected in crate dependencies`**。

### 2.2 致命盲区二：`cc-terminal` 拆分遗漏底层依赖导致未解析符号

* **原方案设想**：仅搬移 `terminal_backend` 与 `clipboard` 两个目录。
* **实际代码审计事实**：
  1. `terminal_backend/mod.rs` 与 `compatible.rs` 强依赖 `crate::terminal_color::ColorDepth`；
  2. `terminal_backend/viewport.rs` 直接调用了 `crate::terminal_title::clear_terminal_title()`、`crate::keyboard_modes::restore_keyboard_enhancement()` 以及 `crate::conpty::disable_mouse_tracking()`；
  3. 这些底层支撑文件（`terminal_color.rs`, `terminal_title.rs`, `keyboard_modes.rs`, `conpty.rs`）在原方案中全被遗漏在 `cc-tui`。
* **致命后果**：若仅迁走 `terminal_backend`，新 crate `cc-terminal` 将直接报出大量 `cannot find function/type in crate root` 错误。

### 2.3 致命盲区三：`cc-tui-core` 与 `cc-tui` 的“破环假设”完全破产（反向依赖闭环）

* **原方案设想**：声称 `app/` 与 `ui/` 唯一的耦合是 `message_view`，只要把 `message_view` 重新定性为 ViewModel 挪入 `cc-tui-core`，就能“彻底消灭 `app -> ui` 的反向箭头”。
* **实际代码审计事实**：
  Grep 检索证实，`cc-tui/src/app` 目录下有整整 **59 个文件**、共计 **198 处** 直接引用了 `crate::ui::*`！
  1. **`PanelComponent` 架构闭环**：11 个面板（MCP, Plugin, Login, Hooks, Memory, Config, Agent, Theme, Status 等）在 `app/` 中维护状态，但其 `PanelComponent::render` 方法（如 `mcp_panel/component.rs:160`）直接调用 `crate::ui::main_ui::panels::mcp::render_mcp_panel`，且该渲染函数又以 `app: &mut App` 为参数，形成了紧密双向闭环；
  2. **`App` 自身聚合了 UI 类型**：`App` 结构体持有 `crate::ui::tips::TipScheduler`、`crate::ui::tips::SpinnerTipState`、`crate::ui::theme`；
  3. **`UiState` 深度侵入 Ratatui 专有结构**：`UiState` 字段直接持有 `tui_textarea::TextArea<'static>`、`ratatui::layout::Rect`、`MessageScrollbarMetrics`、`ScreenSnapshot`；
  4. **渲染管道双向耦合**：`src/app/agent_render.rs` 与 `src/app/message_pipeline/` 频繁与 `crate::ui::render_thread`（57 处）和 `crate::ui::message_render`（4 处）进行事件同步与缓存交换。
* **致命后果**：下层的 `cc-tui-core` 根本无法在不引用上层 `cc-tui` 的情况下通过编译。若强行按原案拆分，将引发上百个 `unresolved import crate::ui`，所谓“首轮构建即可通过”无法成立。

---

## 3. 目标架构蓝图（四层 9 个 Crate）

针对真实依赖图谱，确立**严格单向流动、自底向上**的四层架构体系，彻底杜绝同层环形依赖与越层反向引用：

```
┌────────────────────────────────────────────────────────────────────────┐
│ 第 4 层：终端表现与交互层 (Presentation Layer)                         │
│   cc-tui (约 7.0 万行)                                                 │
│   - main.rs 入口、Event Pump (键盘/鼠标/调整大小事件)                   │
│   - Frame 渲染、Ratatui 组件绘制 (main_ui/*, markdown/*)                │
│   - 异步渲染工作线程 (render_thread/*) 与交互组件 (tui-textarea 绑定)  │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ (严格单向依赖: View 消费 Core)
┌───────────────────────────────────┴────────────────────────────────────┐
│ 第 3 层：会话引擎与协议服务层 (Session & Protocol Runtime)               │
│   cc-tui-core (约 4.5 万行, 规划演进)                                  │
│   - 纯净会话管理器 (SessionManager, ChatSession, SessionMetadata)       │
│   - Slash 命令系统 (CommandRegistry, CommandSystem)                    │
│   - 纯数据面板状态模型 (PanelState, 不含 render 逻辑)                  │
│   - 纯数据展示模型 (MessageViewModel, ContentBlockView, 零 UI 依赖)     │
│                                                                        │
│   cc-acp (约 1.7 万行, 吸收内嵌 acp_server)                            │
│   - ACP 协议通信通道 (MpscTransport / StdioTransport)                 │
│   - 会话执行运行时 (execute_prompt, SessionState, PromptLocks)         │
└──────────────────┬─────────────────────────────┬───────────────────────┘
                   │                             │
┌──────────────────┴─────────────────────────────┴───────────────────────┐
│ 第 2 层：领域控制流与工具能力层 (Domain & Capabilities)                 │
│   cc-middlewares (约 4.8 万行)                                         │
│   - HITL 安全审批门、子 Agent 协同状态机 (subagent/*)                   │
│   - 生命周期 Hooks、插件生态系统 (plugin/*)、MCP 协议连接池            │
│   - 中间件级复合工具 (SkillTool, AskUserTool, SubAgentTool, ExtraTools) │
│                                                                        │
│   cc-tools (新增, 约 0.8 万行, 纯叶子工具库)                           │
│   - 核心无状态文件工具 (Read, Write, Edit, Glob, Grep)                  │
│   - 独立 BashTool (从 terminal 中间件解耦下沉)                          │
│   - 工具输出过滤与落盘 (output_filter, output_persist, todo)            │
│   - 跨平台进程包装与作业对象 (process/*)                               │
└──────────────────┬─────────────────────────────┬───────────────────────┘
                   │                             │
┌──────────────────┴─────────────────────────────┴───────────────────────┐
│ 第 1 层：基础设施与通用协议层 (Infrastructure & Primitives)             │
│   cc-agent (约 3.1 万行: ReAct 循环, LLM 适配器, SQLite 线程持久化)     │
│   cc-widgets (约 1.5 万行: 终端通用组件库, 仅依赖 ratatui/pulldown)     │
│   cc-terminal (新增, 约 0.9 万行: 跨平台终端底层, PTY/ConPTY, 剪贴板, 色彩) │
│   cc-lsp (0.2 万行)  /  langfuse-client (0.3 万行)                      │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 4. 核心解耦规则与白名单划界

为确保拆分绝对零破坏、不产生任何环形依赖，明确以下核心边界隔离铁律：

### 4.1 `cc-tools` 下沉白名单与边界隔离（杜绝循环依赖）

| 工具 / 模块 | 归属 Crate | 划分依据与依赖特征 |
| :--- | :--- | :--- |
| **文件三剑客 (`read`, `write`, `edit`)** | `cc-tools` | 纯文件系统操作，仅依赖 `cc-agent` (BaseTool) 与 `tokio::fs` |
| **搜索两兄弟 (`glob`, `grep`, `rg_engine`)** | `cc-tools` | 依赖 `ignore`, `regex`, `unicode-width`，与中间件无耦合 |
| **进程执行 (`process/**`)** | `cc-tools` | 跨平台进程管道、Windows 作业对象、输出捕获，零业务依赖 |
| **通用 Bash 工具 (`bash_tool`)** | `cc-tools` | 将 `middleware/terminal.rs` 中的纯命令执行器提取为标准 `BashTool` |
| **输出管道 (`output_filter`, `output_persist`, `todo`)** | `cc-tools` | 工具输出截断、持久化落盘与待办事项管理，通用无状态 |
| **技能调用工具 (`SkillTool`)** | **`cc-middlewares` (保留)** | 强依赖 `skills::invocation` 与 `subagent`，禁止下沉 |
| **人工提问工具 (`AskUserTool`)** | **`cc-middlewares` (保留)** | 强依赖 `ask_user` 中间件与状态交互，禁止下沉 |
| **子 Agent 工具 (`SubAgentTool`)** | **`cc-middlewares` (保留)** | 强依赖子智能体状态机与 ReAct 生命周期，禁止下沉 |
| **延迟发现工具 (`SearchExtraTools`, `ExecuteExtraTool`)** | **`cc-middlewares` (保留)** | 强依赖 `tool_search` 动态注册表，禁止下沉 |

### 4.2 `cc-terminal` 完整终端底层打包清单（避免未解析符号）

将所有无业务依赖的终端控制、系统接口统一收敛到 `cc-terminal`：

1. **`backend/`**：`cc-tui/src/terminal_backend/**`（双缓冲区、高亮输出、视口兼容）；
2. **`clipboard/`**：`cc-tui/src/clipboard/**`（Linux X11 lease 持有、arboard、OSC 52、macOS stderr 抑制）；
3. **`conpty/`**：`cc-tui/src/conpty.rs` 与 `conpty_host.rs`（Windows 伪控制台原生 API 封装）；
4. **`color/`**：`cc-tui/src/terminal_color.rs`（终端色深探测 24-bit / 256 色 / ANSI）；
5. **`title/`**：`cc-tui/src/terminal_title.rs`（OSC 0 终端标题安全设置与清理）；
6. **`keyboard/`**：`cc-tui/src/keyboard_modes.rs`（Kitty / CSI u 键盘增强模式开启与恢复）；
7. **`osc8/`**：`cc-tui/src/ui/osc8.rs`（OSC 8 超链接转义序列生成与检测）。

### 4.3 `cc-tui` 面板与状态解耦方案（MVVM 重构规范）

为化解 `app/` 与 `ui/` 间的 59 处耦合，必须对 `PanelComponent` 实施“数据与渲染分离”重构：

```rust
// 改造前（紧耦合在 cc-tui/src/app/panel_component.rs）：
pub trait PanelComponent: Any {
    fn kind(&self) -> PanelKind;
    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult;
    // 致命耦合点：Trait 直接要求依赖 Frame 和 App 渲染！
    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect);
}

// 改造后（解耦设计，使得 Core 可独立编译）：
// 1. 在 cc-tui-core 中定义纯状态控制器：
pub trait PanelController: Any + Send + Sync {
    fn kind(&self) -> PanelKind;
    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult;
    fn desired_height(&self, screen_height: u16, screen_width: u16) -> u16;
}

// 2. 在 cc-tui (表现层) 中定义独立视图渲染器：
pub trait PanelViewRenderer {
    fn render_panel(&self, f: &mut Frame, panel: &dyn PanelController, app_view: &AppViewContext, area: Rect);
}
```

通过这一重构，面板的数据状态（`McpPanelState`, `PluginPanelState`）可以安全存放在 `cc-tui-core`，而 Ratatui 的 `Frame` 渲染逻辑完全留在 `cc-tui`。

---

## 5. 两阶段平滑落地路线图 (Two-Phase Implementation Roadmap)

为确保工程落地安全，采取“**Phase 1：低风险无环叶子下沉与运行时统一**”与“**Phase 2：MVVM 架构重构与 Core 抽取**”两阶段战略。

```
┌────────────────────────────────────────────────────────────────────────┐
│ Phase 1：纯叶子下沉与运行时整合 (零风险，立即可落地)                    │
│   1. 创建 cc-terminal，整合所有终端控制与剪贴板 (瘦身 9,000 行)           │
│   2. 创建 cc-tools，抽离文件基础工具与进程管道 (瘦身 8,000 行)          │
│   3. 将 cc-tui/src/acp_server 收敛入 cc-acp (瘦身 3,000 行)            │
│   => 检查点：全仓零类型断裂，cargo check --workspace 瞬间通过          │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ (平滑过渡验证，CI 全绿)
┌───────────────────────────────────┴────────────────────────────────────┐
│ Phase 2：MVVM 状态解耦与 Core 抽离 (架构深度治理)                      │
│   1. 实施 PanelController / PanelViewRenderer 数据与视图分离           │
│   2. 纯化 MessageViewModel 与 UiState 数据字段                         │
│   3. 正式确立 cc-tui-core，完成 App 与 UI 物理隔离                     │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 6. Phase 1 落地详细操作规程

### 步骤 1.1：声明 Workspace 新成员

修改根目录 `Cargo.toml`：

```toml
[workspace]
members = [
    "cc-agent",
    "cc-tools",         # 新增：内建核心工具与进程包装
    "cc-middlewares",
    "cc-terminal",          # 新增：终端后端与跨平台剪贴板
    "cc-widgets",
    "cc-acp",
    "cc-tui",           # 瘦身后继续承担 TUI 表现层
    "langfuse-client",
    "cc-lsp",
]
```

### 步骤 1.2：构建 `cc-terminal` 终端底层 Crate

1. **创建 crate 结构**：
   ```bash
   cargo new --lib cc-terminal
   ```
2. **配置 `cc-terminal/Cargo.toml` 依赖**：
   ```toml
   [dependencies]
   crossterm = { workspace = true }
   ratatui = { workspace = true }
   tokio = { workspace = true, features = ["sync", "time"] }
   arboard = { workspace = true }
   tracing = { workspace = true }
   unicode-width = { workspace = true }
   
   [target.'cfg(windows)'.dependencies]
   windows-sys = { workspace = true, features = [
       "Win32_System_Console",
       "Win32_Foundation",
       "Win32_Security",
   ] }
   ```
3. **搬移文件并建立目录**：
   - `cc-tui/src/terminal_backend/*` -> `cc-terminal/src/backend/*`
   - `cc-tui/src/clipboard/*` -> `cc-terminal/src/clipboard/*`
   - `cc-tui/src/conpty.rs`, `conpty_host.rs` -> `cc-terminal/src/conpty/*`
   - `cc-tui/src/terminal_color.rs` -> `cc-terminal/src/color.rs`
   - `cc-tui/src/terminal_title.rs` -> `cc-terminal/src/title.rs`
   - `cc-tui/src/keyboard_modes.rs` -> `cc-terminal/src/keyboard.rs`
   - `cc-tui/src/ui/osc8.rs` -> `cc-terminal/src/osc8.rs`
4. **在 `cc-tui/src/lib.rs` 建立垫片重导出**：
   ```rust
   pub use cc_terminal::backend as terminal_backend;
   pub use cc_terminal::clipboard;
   pub use cc_terminal::color as terminal_color;
   pub use cc_terminal::conpty;
   pub use cc_terminal::keyboard as keyboard_modes;
   pub use cc_terminal::title as terminal_title;
   ```
5. **验证检查点**：
   ```bash
   cargo check -p cc-terminal
   cargo check -p cc-tui
   ```

### 步骤 1.3：构建 `cc-tools` 核心工具 Crate

1. **创建 crate 结构**：
   ```bash
   cargo new --lib cc-tools
   ```
2. **配置 `cc-tools/Cargo.toml` 依赖**：
   ```toml
   [dependencies]
   cc-agent = { path = "../cc-agent" }
   async-trait = { workspace = true }
   tokio = { workspace = true, features = ["fs", "io-util", "process"] }
   serde = { workspace = true, features = ["derive"] }
   serde_json = { workspace = true }
   tracing = { workspace = true }
   regex = { workspace = true }
   ignore = { workspace = true }
   thiserror = { workspace = true }
   process-wrap = { workspace = true }
   ```
3. **物理搬迁白名单工具**：
   - `cc-middlewares/src/tools/filesystem/*` -> `cc-tools/src/filesystem/*`
   - `cc-middlewares/src/tools/output_*.rs` -> `cc-tools/src/output/*`
   - `cc-middlewares/src/tools/todo*.rs` -> `cc-tools/src/todo/*`
   - `cc-middlewares/src/process/*` -> `cc-tools/src/process/*`
   - 从 `cc-middlewares/src/middleware/terminal.rs` 抽离纯工具结构体至 `cc-tools/src/bash.rs`
4. **保留中间件级工具在 `cc-middlewares`**：
   - `tools/skill.rs`、`tools/ask_user_tool.rs` 留在 `cc-middlewares/src/tools/` 下。
5. **在 `cc-middlewares/src/lib.rs` 追加垫片重导出**：
   ```rust
   pub use cc_tools::filesystem;
   pub use cc_tools::process;
   pub use cc_tools::output;
   pub use cc_tools::bash;
   ```
6. **验证检查点**：
   ```bash
   cargo check -p cc-tools
   cargo check -p cc-middlewares
   ```

### 步骤 1.4：收敛 `acp_server` 至 `cc-acp`

将 `cc-tui/src/acp_server/*` 的会话执行机制完整合并进 `cc-acp` 的 `executor` 与 `session` 模块，使 `cc-tui` 完全只扮演通过 `MpscTransport` 消费事件的客户端，消除内嵌 ACP 服务的重复维护。

---

## 7. 验收标准与开发规范保障

### 7.1 严格静态检查与单测校验（对齐 CI）

所有重构改动必须在独立 Git Worktree 中进行，执行以下流水线校验：

```bash
# 1. 严格 Clippy 静态检查（全目标零警告，严格对齐 CI 要求）
cargo clippy --all-targets -- -D warnings

# 2. 单线程全量单测验收（避免 Windows 堆栈与端口竞争）
cargo test -p cc-terminal -j 1
cargo test -p cc-tools -j 1
cargo test -p cc-middlewares --lib -j 1
cargo test -p cc-tui --lib -j 1 -- headless
```

### 7.2 针对 Windows 平台与项目规范的防护约定

1. **环境与路径防护**：
   - 严禁在脚本或内联测试中将 MSYS 风格路径（`/d/...`, `/tmp/...`）传给 Windows 原生工具；
   - 路径统一使用正斜杠盘符格式（`D:/code/peri`）；
2. **测试命名与隔离规范**：
   - 新增测试函数名称严格执行英文 `snake_case`（如 `test_bash_tool_output_truncation`），严禁中文字符；
   - 所有单测严禁写入全局配置，统一通过测试隔离上下文执行；
3. **Git 提交与变更纪律**：
   - 搬迁文件必须使用 `git mv` 保持历史记录完整追踪；
   - 严格执行多行 Commit 规范与 `Co-Authored-By` 署名。

---

## 8. 重构收益对照基线（预估与实测）

| 核心指标 | 现状 (重构前) | Phase 1 完成后 | Phase 2 完成后 (最终态) |
| :--- | :--- | :--- | :--- |
| **最大单 Crate 源码行数** | `cc-tui`: **137,199 行** | `cc-tui`: **123,000 行** | `cc-tui`: **68,000 行**<br>`cc-tui-core`: **55,000 行** |
| **工具层编译耦合度** | 依赖全量中间件与状态机 | `cc-tools` 独立叶子编译 | `cc-tools` 独立叶子编译 |
| **底层终端接口变更波及** | 触发 `cc-tui` 13.7 万行重编 | 仅重编 `cc-terminal`（0.9 万行） | 仅重编 `cc-terminal`（0.9 万行） |
| **全仓冷启动构建时间** | 基准时间 (100%) | 预计缩短 **15% ~ 20%** | 预计缩短 **30% ~ 40%** |
| **日常 UI 调整增量编译** | 重新单态化并链接整仓 | 链接体积缩小 10% | 增量编译耗时预计缩短 **55%** |
| **协议纯洁度与分层** | TUI 内嵌服务、逻辑纠缠 | `cc-acp` 统一协议会话核心 | 严格单向 MVVM 数据流 |

---

## 9. 落地前代码审计修订（2026-11-04）

> 本章由实施分支的真实代码核对产生，用于修正 §6 操作规程中与仓库实际不符之处。**实施以本章为准。**

### 9.1 更正：§6.4 的循环依赖判断不成立

初步审计曾认为 `acp_server` 依赖 `crate::config::PeriConfig`、`crate::app::agent::LlmProvider` 会使 cc-acp 反向依赖 cc-tui。实测：

- `cc-acp/src/provider/config.rs:11` 定义 `PeriConfig`，`mod.rs:10` 导出；`mod.rs:37` 定义 `LlmProvider`；`store.rs:83` 定义 `save_to`。
- `cc-tui/src/config/mod.rs:2`、`cc-tui/src/app/provider.rs:2` 仅为 `pub use cc_acp::provider::...` 重导出。

故搬入 cc-acp 只需把 `cc-tui/src/acp_server/` 内的 `crate::config::*` / `crate::app::agent::LlmProvider` / `crate::acp_server::SessionState` 改为 `crate::provider::*` / `crate::session::...`，**无循环依赖**，§6.4 可执行。

### 9.2 §6.2 / §6.3 的 Cargo.toml 示例不可用

根 `[workspace.dependencies]` 仅含 §1 所列条目，**不含** `crossterm` / `ratatui` / `arboard` / `unicode-width` / `regex` / `ignore` / `process-wrap` / `windows-sys`。示例中的 `workspace = true` 会直接报 `workspace dependency not found`。新 crate 一律使用显式版本（与 cc-tui / cc-middlewares 现有声明一致），仅对 workspace 已有项使用 `workspace = true`。另 `process` 实际还需 `cc-agent`、`uuid`、`libc`、`process-wrap` / `windows`、`num_cpus`、`base64`，示例亦未列全。

### 9.3 §4.2 打包清单修订

- **`ui/osc8.rs` 不下沉**：`cc-tui/src/ui/osc8.rs:55` 依赖 `crate::ui::message_render::find_url_ranges`，`:144` 依赖 `crate::ui::render_thread::WrappedLineInfo`。把它放进第 1 层 cc-terminal 会造成基础设施层 → 表现层的反向依赖，与 §2 批评的错误同类。保留在 `cc-tui/src/ui/`。
- **补入 `shell_exec.rs`**：`terminal_backend/shell_console_test.rs:8` 依赖 `crate::shell_exec`，由 `terminal_backend/windows.rs:123` 以 `#[path]` 挂载。若不搬 `shell_exec.rs`，该 Windows 测试模块无法编译。`shell_exec.rs` 全部导出为 `pub`，cc-tui 侧垫片可无损覆盖；其 `cc_middlewares::process::*` 改为 `cc_tools::process::*`。

### 9.4 cc-terminal 内部模块保持同名（零路径改动的关键）

§6.2 建议把 `terminal_color.rs` 改名为 `color.rs`、`terminal_backend/` 改名为 `backend/`，并在 cc-tui 侧做 `pub use cc_terminal::backend as terminal_backend`。但待搬迁文件内部大量使用 `crate::terminal_color::depth()`、`crate::terminal_title::clear_terminal_title()`、`crate::keyboard_modes::restore_keyboard_enhancement()`、`crate::conpty::disable_mouse_tracking()` 等同 crate 路径。**改名会连带破坏这些内部引用。**

修订：cc-terminal 内保持与 cc-tui 完全相同的模块名与目录名（`terminal_backend/`、`clipboard/`、`conpty.rs`、`conpty_host.rs`、`terminal_color.rs`、`terminal_title.rs`、`keyboard_modes.rs`、`shell_exec.rs`），cc-tui 侧用同名 `pub use cc_terminal::X;` 垫片。这样 cc-tui 内 `crate::X` 引用与 cc-terminal 内部引用均无需改动。

### 9.5 §6.3 的 cc-tools 模块结构

`crate::tools::output_persist::*` 在 `filesystem/glob.rs`、`grep.rs` 中被引用。修订：cc-tools 提供 `pub mod tools { filesystem, output_filter, output_persist, todo }` 与 `pub mod process`，cc-middlewares 侧 `tools/mod.rs` 用 `pub use cc_tools::tools::{...}`、`lib.rs` 用 `pub use cc_tools::process;`。`ArcToolWrapper` / `BoxToolWrapper` / `ask_user_tool.rs` / `skill.rs` / `tools/mod_test.rs` 留在 cc-middlewares。

### 9.6 基线数据修正

实测（排除 `target/`）937 个 `.rs` 文件 / 254,030 行，非 §1.1 的 971 / 259,801。

### 9.7 Phase 2 实施边界

§4.3 的 `PanelController` / `PanelViewRenderer` 仅为接口草图。实际 `cc-tui/src/app` 有 59 个文件、198 处 `crate::ui::*` 引用，且 `UiState` 持有 `tui_textarea::TextArea<'static>`、`ratatui::layout::Rect` 等表现层类型。Phase 2 必须按「先抽纯数据 → 再切渲染回调 → 最后物理分 crate」三步推进，每一步单独 `cargo check --workspace` 验证，不可一次性物理搬迁。

### 9.8 Phase 2 分批落地规程

**首批（本轮已实施）**：建立 `cc-tui-core`，下沉零 UI / App 耦合的通用基建：

- `cc-tui/src/i18n/` → `cc-tui-core/src/i18n/`，`cc-tui/locales/` → `cc-tui-core/locales/`（同步迁移以保持 `include_str!("../../locales/...")` 相对路径）；
- `cc-tui/src/export/` → `cc-tui-core/src/export/`；
- `cc-tui/src/config/` → `cc-tui-core/src/config/`（`cc_acp::provider` 的重导出）；
- `cc-tui/src/lib.rs` 用 `pub use cc_tui_core::{config, export, i18n};` 垫片，全仓 `crate::i18n::*` / `crate::export::*` / `crate::config::*` 引用零改动。

**为何不能一次性完成物理隔离**（实测依据）：

1. `app → ui` 共 198 处引用，按目标分布：`main_ui::panels` 14、`message_view::MessageViewModel` 13、`main_ui::render` 13、`render_thread::SelectionAnchor` 12、`render_thread::RenderEvent` 14、`message_view::*` 40+、`render_thread::WrappedLineInfo` 6、`ui::theme` 6 等；
2. `PanelComponent::render(&mut self, f: &mut Frame, app: &mut App, area: Rect)`（`app/panel_component.rs:14`）直接要求 `Frame` 与 `App`；11 个面板的 render 调用 `ui::main_ui::panels::*::render_*_panel(app: &mut App, ...)`，形成双向闭环；
3. `App`（`app/mod.rs:166`）持有 `ui::tips::TipScheduler` / `SpinnerTipState` / `ui::theme`；`UiState`（`app/ui_state.rs:37`）持有 `tui_textarea::TextArea<'static>`、`ratatui::layout::Rect`；
4. `PanelContext`（`app/panel_manager.rs`）本身含 `App` 引用，故 §4.3 草图的 `PanelController::handle_key(ctx: &mut PanelContext)` 无法直接放进无 UI 依赖的 core——草图需先定义纯数据 `PanelContextView` 才能成立。

**后续批次**（每批独立验证：`cargo check --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、相关 crate 单测）：

| 批次 | 内容 | 消除的耦合 |
| :--- | :--- | :--- |
| A | 抽 `ui/message_view` 纯数据类型（`MessageViewModel`、`ContentBlockView`、`ToolEntry`、`HookRow`、`NoteTone`、`ToolPhase`）入 core，cc-tui 侧重导出垫片 | `app → ui::message_view` 约 40 处 |
| B | 抽 `ui/render_thread` 纯数据/事件类型（`RenderEvent`、`WrappedLineInfo`、`SelectionAnchor`、`DisplayFlags`、`VirtualLayoutSnapshot`）入 core | `app → ui::render_thread` 约 40 处 |
| C | 拆分 `PanelComponent`：core 定义 `PanelController`（kind / handle_key / desired_height，接受纯数据 `PanelContextView`），cc-tui 定义 `PanelViewRenderer`；改 11 个面板 component、`panel_manager` 分派，并把 `render_*_panel(&mut App, ...)` 改为消费 ViewModel | `app → ui::main_ui::panels` 14 处及 render 闭环 |
| D | 纯化 `UiState`（TextArea / Rect 改中性类型或留表现层）、`App` 的 ui 字段改注入，最后把 `app/` 纯状态部分物理搬入 core | 剩余 `app → ui` 引用 |

**验收与风险**：批次 C / D 涉及渲染与事件分发，现有单测覆盖有限，必须逐批小步提交、保留可回滚点，并补 TUI 人工冒烟（渲染、面板切换、滚动）。一次性物理搬迁会静默破坏渲染行为，故本轮不做。

### 9.9 命名决策

实施评审后把 `cc-term` 统一改名为 `cc-terminal`：`term` 在 Rust 生态虽是 terminal 的通用缩写（`termcolor` / `termion`），但有"术语/条款"歧义，且该 crate 实际承载终端后端、剪贴板、ConPTY、色彩、shell 执行，`cc-terminal` 语义更精确。改动覆盖目录名、package name、`cc_terminal::` 导入路径、workspace members、依赖路径与本文档全部引用。

### 9.10 本轮 Phase 2 实际落地（三批）与规程更正

**实际完成的三批**（每批均通过 `cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings` 及相关单测）：

| 批次 | 下沉到 cc-tui-core 的内容 |
| :--- | :--- |
| 1（§9.8 首批） | `i18n/`、`export/`、`config/`、`locales/` |
| 2 | `ui/theme/`（3,740 行）、`terminal_theme/`（353 行）、`app/tool_display.rs` + 测试（1,186 行） |
| 3 | `ui/message_view/`（3,303 行）、`ui/markdown/`（1,529 行）、`shell_history.rs`（133 行）、`shell_notification` 抽取、`text_width` |

结果：`cc-tui-core` 11,863 行 / 29 文件；`cc-tui` 由 121,624 行降至 111,122 行 / 377 文件。三批均采用**同名模块 + `pub use cc_tui_core::X;` 垫片**（`ui/mod.rs` 的 theme/markdown/message_view、`app/mod.rs` 的 tool_display、`lib.rs` 的 shell_history/terminal_theme），使全仓 `crate::ui::*` / `crate::app::tool_display::*` 路径零改动。单测：cc-tui-core 231 passed、cc-tui --lib 1548 passed。

**规程更正（重要）**：

1. §4.3 与 §9.8 批次 A 假设 `MessageViewModel` 是"零 UI 依赖的纯数据"，**实测不成立**：其变体直接持有 `ratatui::text::Text<'static>`、`ratatui::style::Color`、`cc_widgets::markdown::LinkHit`、`cc_widgets::DiffInput`、`TableHoldbackScanner`，构造逻辑还调用 `app::tool_display::*` 与 `app::shell_notification_display_text`。且 `ui/markdown` 与 `ui/message_view` 互相引用（`markdown::ensure_rendered_incremental(&mut ContentBlockView)`），不能单独搬。故批次 A 改为**整链下沉**，core 因而依赖 `ratatui` / `cc-widgets` / `cc-terminal`。
2. `shell_notification_display_text` 原在 `app/background_shell.rs`，其测试通过 `use super::*` 访问私有辅助函数。处理：抽 347–581 行到 `core/shell_notification.rs`，原处 `pub use cc_tui_core::shell_notification::*;`，相关 `pub(crate)` 项提升为 `pub`，测试零改动。
3. `ui/message_render::truncate_to_display_width` 抽到 `core/text_width`，cc-tui 侧 `pub(crate) use` 重导出保持路径。
4. `terminal_theme` 内部 `crate::terminal_color` 由 `pub use cc_terminal::terminal_color;` 垫片承接。

**仍未完成**（§5 Phase 2 的第 1、2 点及第 3 点的物理隔离，实测状态）：

- `PanelComponent::render(&mut Frame, &mut App, Rect)` 仍在 `cc-tui/src/app/panel_component.rs:51`；11 个面板的 `render_*_panel(&mut App, ...)` 双向闭环未拆；
- `UiState` 仍持 `tui_textarea::TextArea<'static>` 与 9 处 `ratatui::layout::Rect`；
- `app/`（173 文件）与 `ui/`（main_ui / render_thread / render_layout / message_render 等）**都还在 cc-tui**，`app → ui` 引用仍有 194 处；
- `cc-tui-core` 当前是"共享展示模型 + 通用基建"层，尚未成为文档设想的"纯净会话管理器"。

原因：把 `render_*_panel(&mut App)` 改为消费 ViewModel，需先重新设计 ViewModel 的字段集（这些渲染函数读取 App 的大量字段），属于渲染架构重写而非机械搬迁；一次性改动会静默破坏渲染与事件分发。批次 C / D 必须按 §9.8 逐批小步推进，并补 TUI 人工冒烟（渲染、面板切换、滚动）。

### 9.11 UiState 类型归属决策与落地（批次 D 第一步）

审计依据：`UiState` 由 `ChatSession::ui` 持有（`cc-tui/src/app/chat_session.rs:30`），而 `ChatSession` 是 §3 要下沉的会话类型；`UiState` 不拆，ChatSession 无法进 core。实测字段并不同质：

| 字段组 | 归属 | 理由 |
| :--- | :--- | :--- |
| 滚动/开关/输入历史/草稿/光标/toast/loading/纯数值标记（约 40 个）+ `PastedTextBlock` / `SteerForegroundGrace` + 纯逻辑方法 | `cc-tui-core::ui_state::UiStateCore` | 零 UI 依赖，属会话交互状态 |
| `DisplayFlags`（3 bool） | `cc-tui-core::display_flags` | 纯数据，随 UiStateCore 走 |
| `textarea: TextArea<'static>` | cc-tui 表现层 | 73 处调用点；换中性类型等于自研输入框并引入键盘/光标行为漂移 |
| 全部 `ratatui::Rect` 几何（11 处）、`MessageScrollbarMetrics`、`cc_widgets::ScrollbarMetrics` | cc-tui 表现层 | 每帧由 `terminal.draw()` 登记的命中几何，是渲染产物 |
| `ScreenSnapshot::from_buffer(&ratatui::Buffer)`、`text_selection` 全家、`at_mention`（含 popup 渲染） | cc-tui 表现层 | 直接吃 `ratatui::buffer::Buffer` |

**不采用**「全换中性类型进 core」（core 会依赖 ratatui + 行为漂移）或「整体留 cc-tui」（ChatSession 无法下沉）。

落地方式（`Deref`/`DerefMut` 委托，调用点零改动）：

```rust
// cc-tui
pub struct UiState {
    pub core: UiStateCore,          // 纯状态
    pub textarea: TextArea<'static>, // 控件
    // ... 其余 Rect / 选区 / 快照字段保持原名
}
impl Deref for UiState { type Target = UiStateCore; ... }
impl DerefMut for UiState { ... }
```

- `ui.scroll_offset` / `ui.detail_mode` 等约 40 个纯字段访问经 Deref 落到 `core`，全仓零改动；`ui.textarea` / `ui.messages_area` 等表现层字段仍是 `UiState` 直接字段，同样零改动；
- `ui/render_thread.rs` 的 `DisplayFlags` 改为 `pub use cc_tui_core::display_flags::DisplayFlags;`，两侧同一类型，`display()` / `apply_display()` 由 Deref 直接调用 core 实现；
- 仅 3 处「同一表达式内混用可变方法调用与 Deref 字段读取」触发 E0502，改为先取值再调用（`thread_ops.rs:40`、`event/mod.rs:406`）。

后续：`ChatSession` / `SessionMetadata` / `SessionManager` 的下沉仍依赖 `PanelComponent::render` 与 `render_*_panel(&mut App)` 的闭环拆解（批次 C），不在本次类型归属范围内。

### 9.12 真实收益评估（实测，替代 §8 的预估）

§8 的收益对照表是**方案阶段的预估**，其中「Phase 2 完成后 cc-tui 68,000 行 / cc-tui-core 55,000 行」的前提本轮未达成，故"冷启动 −30~40%、UI 增量 −55%"不能作为已实现收益引用。以下为本次改动后的实测数据。

**已拿到的收益（可量化）**

| 指标 | 基线 | 现状 | 说明 |
| :--- | :--- | :--- | :--- |
| 底层终端接口变更波及 | 触发 cc-tui 134,541 行重编 | 仅重编 cc-terminal 8,662 行 | §8 中最实在的一条，已达成 |
| 工具层编译耦合 | 工具与 hitl / subagent / hooks / plugin 同 crate | cc-tools 9,295 行独立叶子，仅依赖 cc-agent | 已达成 |
| 共享展示模型、通用基建、会话纯状态 | 全部混在 cc-tui | cc-tui-core 12,086 行 / 31 文件，零 cc-tui 依赖 | 已达成 |
| cc-tui 体积 | 134,541 行 / 444 文件 | 110,936 行 / 377 文件 | −23,605 行（−17.5%） |
| 依赖图 | 单 crate 内部纠缠 | 严格单向 DAG，零循环依赖 | 已达成 |
| 静态检查与单测 | — | `clippy -D warnings` 全绿；单测 cc-tui 1548 / cc-middlewares 1097 / cc-tools 245 / cc-terminal 187 / cc-tui-core 231，0 failed | 已达成 |

**未拿到的收益 / 需要澄清**

- **运行时收益为零**：本次全部是模块物理搬迁、垫片重导出与类型归属调整，未改变任何执行逻辑；功能、性能、内存占用不受影响。
- **编译时间**：未做严格 A/B。基线全量 `cargo check --workspace --all-targets` 为 1m23s；改动后增量 check 为 3~20s（增量与全量不可直接比较），不能据此宣称 −30~40% 或 −55%。
- **日常 UI 增量编译**：cc-tui 仍是 110,936 行的单 crate，改 `ui/` 仍触发整个 cc-tui 重编；文档设想的"只重编 68,000 行表现层"未达成。
- **实测瓶颈**：cc-tui lib test 二进制链接两次因 D 盘空间不足失败（每次清理 `target/debug/incremental` 约 12.8 GB 后通过），说明当前构建瓶颈在大二进制链接与磁盘，而非状态层编译量；把更多状态搬进 core 不解决该瓶颈。
- **§2.3 的 198 处 `app→ui` 引用需重新解读**：现状 194 处中约 50 处（`message_view` ~40、`theme` 6、`DisplayFlags` 3）经 `pub use cc_tui_core::*` 垫片已指向 core 类型，**物理上已解耦、无需改代码**；真实剩余耦合约 70 处（`render_thread` ~30、`main_ui::panels` / `main_ui::render` ~27、`tips` ~10），另有 16 个 `PanelComponent::render` 实现与 `App` 的 `TipScheduler` / `SpinnerTipState` 字段。

**批次 C / D 决策：暂缓**

剩余改造（拆分 `PanelComponent` 为 `PanelController` / `PanelViewRenderer`、把 `render_*_panel(&mut App)` 改为消费 ViewModel、`ChatSession` / `SessionManager` 物理下沉）收益为**纯编译期 + 架构纯洁度**，运行时为零；而要改动的是渲染、事件分发与鼠标命中路径，现有单测以 headless 快照为主、对交互行为覆盖有限，破坏后是用户可感知的功能回归。投入产出比不成立，故本轮收口。

**重启条件**：① UI 增量编译时间经实测成为开发瓶颈；② 确定替换 TUI 渲染后端；③ 需要为面板状态建立独立单测体系。届时按 §9.8 批次 C→D 逐批小步推进，每批独立 `clippy -D warnings` + 单测 + TUI 人工冒烟。
