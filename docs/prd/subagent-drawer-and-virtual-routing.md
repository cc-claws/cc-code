# PRD: 后台多子 Agent 抽屉式管理与虚拟视口路由系统

> **文档版本**：v2.2 (后台唯一模式：删除前台同步路径、抽屉游标行改用纯前景色、补充 §4.16 fork 同线)  
> **基线代码**：`origin/main` (`cc-tui`, `cc-acp`, `cc-middlewares`, `cc-agent`)；真机联调基线 `feat/subagent-drawer-routing`
> **关联原型**：`docs/subagent-mock.html`（已通过浏览器验证并对齐真实终端截图）  
> **关联历史**：`docs/superpowers/specs/2026-05-26-background-agent-bar-design.md`

---

## 1. 业务背景与架构痛点分析

在复杂研发场景中，主 Agent 经常需要分发多个独立的子任务（例如：并行执行测试、安全审查、多平台信息抓取、耗时构建等）。当前 Peri 在处理多子 Agent（SubAgent）并行与后台执行时，存在如下阻碍上线的致命缺陷：

| 痛点分类 | 现状表现 | 根因剖析 |
|---|---|---|
| **信息过载与卡片挤压** | 试图在主对话流中以内嵌卡片（`SubAgentGroup`）展示子 Agent，多 Agent 并发时工具调用互相插队，时间线混乱。 | 单时间线容器无法承载并发树状流。 |
| **事件通道双轨竞争** | 界面上子任务的目标描述（`task_preview`）经常变为空白，且工具调用出现双重记录。 | ACP `session/update` 与内部 `ExecutorEvent` 互相竞争，补丁逻辑跳过了状态初始化。 |
| **思考/对话严重串味** | 后台子 Agent 的思考过程（`AiReasoning`）污染主会话；在并发下 `last_mut()` 盲猜导致工具状态张冠李戴；代码甚至直接跳过 `AssistantBubble`，导致用户看不到子 Agent 的回复。 | 缺乏统一的 `source_agent_id` 路由键，单栈假设在并发下彻底失效。 |
| **缺乏下钻与定向干预** | 用户无法查看某个具体子任务的完整工作流；无法向后台运行中的子 Agent 追问或下发补充指令。 | 缺乏**视口路由（Viewport Routing）**与**子线程通信通道**。 |

---

## 2. 交互全景图与视觉规范（对齐 6 张终端实录）

系统核心形态为：**主视口高度折叠摘要 + 状态栏下方常驻抽屉栏 + 虚拟视口路由切换**。

```
┌────────────────────────────────────────────────────────────────────────┐
│ [主消息视口 (Main Viewport)]                                           │
│  • 现在并行启动 2 个子 Agent，分别等待 60 秒后查询武汉和随州的天气情况。 │
│                                                                        │
│  • 2 background agents launched (↓ to manage)                          │
│    ├── 查询武汉天气                                                     │
│    └── 查询随州天气                                                     │
│                                                                        │
│  ✱ Hatching... (11s · ↓ 905 tokens · thought for 8s)                   │
├────────────────────────────────────────────────────────────────────────┤
│ > [输入框]                                            [ 查询随州天气 ]  │  ← 聚焦子视口时亮青色上边框 + 专属 Tag
├────────────────────────────────────────────────────────────────────────┤
│ [Sonnet 4.6] ▓▓▒░░░░░░░ 6% | peri git:(main*) | tok: 180k              │
│ Enter to view · x to stop · ctrl+x ctrl+k to stop all agents           │  ← 动态快捷键提示行
├────────────────────────────────────────────────────────────────────────┤
│  ◯ main                                                                │
│  ◯ general-purpose   查询武汉天气                        32s · ↓ 49   │
│> ● general-purpose   查询随州天气                        32s · ↓ 33   │  ← 抽屉管理栏 (> 选框, ● 当前视口)
└────────────────────────────────────────────────────────────────────────┘
```

> **指标口径**：抽屉右侧只出现**单个箭头**，方向即 token 流向（`↓` = Agent 回复产出 / `↑` = 用户提问输入），
> 详见 §4.12；`◯/●` 与 `>` 的字形、配色与列宽规格详见 §4.11。

### 2.1 状态流转四阶段

1. **分发态（Launch）**：
   * 主时间线输出树形节点：`• N background agents launched (↓ to manage)`，用 `├──` / `└──` 列出任务简述。
   * 抽屉栏立即展开显示所有后台 Agent，状态栏提示 `(↓ to manage)`。
2. **抽屉导航态（Drawer Nav）**：
   * 在空输入框按 **`↓`**，焦点下移进入抽屉，游标所在行**只通过前景色**变亮白（游标 `> ` + 整行文字白），**不设任何底色**（真机定稿，见 §4.11）。
   * **当前查看的视口**保留实心圆点 `●`，**当前选中的行**带有 `> ` 游标。两者解耦。
   * 快捷键提示行随游标位置切换文案（`drawer_hint()`，唯一判据是「游标所指行是否就是当前查看的视口」）：
     * 游标停在**当前查看的那一行**：`↑/↓ to select · ctrl+x ctrl+k to stop all agents`（此时 `Enter`/`x` 无意义，不提示）；
     * 游标移到**其他行**：`Enter to view · x to stop · ctrl+x ctrl+k to stop all agents`。
3. **虚拟视口下钻态（Virtual Viewport Routing）**：
   * 按 **`Enter to view`**，主消息视口立即呈现该子 Agent 的独立消息上下文（含完整的独立思考、工具运行进度 `Bash(sleep 60) └ Running... (25s)`、独立 Spinner）。
   * 输入框顶部分隔线变成**亮青色线条（Glow Line）**，右上角浮现专属标签 `[ 查询随州天气 ]`。
   * 输入框占位符变为 `> Message @<agent-type>...`，支持向子任务定向发话。
4. **完成与回流态（Callback & Sync）**：
   * 子任务完成时在主视口打印完成行：`• Agent "xxx" finished · 1m 32s`。
   * 主模型自动接管，将子 Agent 的产出综合输出报告；抽屉栏该任务状态更新，剩余任务继续运行。

---

## 3. 核心架构重构设计

### 3.1 数据模型重构（Thread-Scoped Session Pool）

废除 TUI 当前脆弱的单栈 `subagent_stack` 数组，引入真正的**多线程会话池**：

```rust
// cc-tui/src/app/chat_session.rs

/// 后台子 Agent 运行期会话句柄
pub struct BackgroundAgentSession {
    pub instance_id: String,            // 全局唯一 UUID (对应 ThreadStore thread_id)
    pub agent_type: String,             // 如 "general-purpose", "explore"
    pub task_preview: String,           // 初始 Prompt 摘要 (首行或截取 40 字符)
    pub current_activity: String,       // 实时动态行为描述 (如 "正在等待 60 秒后查天气")
    pub started_at: std::time::Instant, // 启动时间
    pub tokens_used: usize,             // 累计 token
    pub status: SubagentRunStatus,      // Running | AwaitingApproval | Completed | Stopped | Failed
    pub final_result: Option<String>,   // 最终输出摘要
    pub pipeline: MessagePipeline,      // 该子 Agent 专属的独立渲染管线 (拥有独立的 view_messages)
    pub pending_inputs: Vec<String>,    // 用户通过 Message @agent 发送的待处理指令队列
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentRunStatus {
    Running,
    AwaitingApproval,
    Completed,
    Stopped,
    Failed,
}

pub struct ChatSession {
    pub messages: SessionMessages,              // 主会话管线
    pub bg_agents: Vec<BackgroundAgentSession>, // 后台 Agent 运行池
    pub active_viewport: Option<String>,        // None = main 视口, Some(id) = 子视口
    pub bg_drawer_cursor: Option<usize>,        // 抽屉栏键盘游标 (0=main, 1..=N=bg_agents)
}
```

#### 3.1.1 实现现状：双结构职责边界（`background_agents` / `subagent_sessions`）

落地的数据模型把上面的单池草图拆成**两层**，职责必须严格区分，混用会导致"抽屉有行但点进去空白"或"句柄丢失"：

| 结构 | 类型 | 职责 | 关键约束 |
|---|---|---|---|
| `background_agents` | `Vec<RunningBgAgent>`（`agent_name` / `instance_id` / `started_at` / `total_steps`） | **抽屉行**：行顺序、行数、抽屉是否存在的唯一依据 | 只有 `is_background = true` 才写入（§4.15）；去重靠 `instance_id`；抽屉为空时 `bg_bar_height() = 0` |
| `subagent_sessions` | `HashMap<String, BackgroundAgentSession>` | **会话句柄池**：`pipeline` / `view_messages` / `AgentComm` / `SpinnerState` / `pending_inputs` / `token_metrics` | 前台后台**都**写入；`Enter` 下钻、状态栏切换、`send_subagent_input` 全部读它 |

对照关系（PRD 草图的字段以落地命名为准）：

| 草图 | 落地 |
|---|---|
| `bg_agents: Vec<BackgroundAgentSession>` | `background_agents: Vec<RunningBgAgent>` + `subagent_sessions: HashMap<String, BackgroundAgentSession>` |
| `active_viewport: Option<String>` | `focused_instance_id: Option<String>` |
| `bg_drawer_cursor: Option<usize>` | `ui.bg_bar_cursor: Option<usize>`（0 = main，1..=N = `background_agents[N-1]`） |
| `tokens_used: usize` | 由 `AgentComm::session_token_tracker` 派生（`tokens_used()`），抽屉指标另走 `token_metrics()`（§4.12） |

**唯一判据**：抽屉是否处于键盘导航态由 `ChatSession::drawer_nav_active()`（`ui.bg_bar_cursor.is_some() && !background_agents.is_empty()`）决定，用于 `drawer_hint()`、输入框变暗与 `bar_focus` 按键拦截；后台 Agent 全部结束后必须清空 `bg_bar_cursor`，否则第三行会卡在 `↑/↓ to select…` 且继续吞键。

### 3.2 虚拟视口路由机制（Virtual Viewport Routing）

在渲染层彻底解耦数据源，`render_thread.rs` 统一通过视口路由获取消息列表（**落地命名**：`active_viewport` → `focused_instance_id`，`bg_agents` → `subagent_sessions`，见 §3.1.1）：

```rust
impl App {
    /// 获取当前应呈现给用户的消息视图列表
    pub fn current_view_messages(&self) -> &[MessageViewModel] {
        let session = self.session_mgr.current();
        match &session.active_viewport {
            None => &session.messages.view_messages,
            Some(target_id) => {
                session.bg_agents
                    .iter()
                    .find(|a| &a.instance_id == target_id)
                    .map(|a| a.pipeline.view_messages.as_slice())
                    .unwrap_or(&session.messages.view_messages)
            }
        }
    }
}
```

### 3.3 事件调度链契约（解决双轨竞争与串味）

彻底统一事件路由，严禁未带 ID 的内部事件泄漏到主会话：

```
[SubAgentTool 运行时]
        │
        ├── 1. 启动: 发射 ExecutorEvent::SubagentStarted {
        │       instance_id, agent_type, task_preview, is_background
        │   }
        │
        ├── 2. 过程: 内部所有增量事件必须显式包装 source_agent_id
        │       ├── TextChunk { chunk, source_agent_id: Some(id) }
        │       ├── AiReasoning { text, source_agent_id: Some(id) }   ← 关键补全！
        │       ├── ToolStart { name, ..., source_agent_id: Some(id) }
        │       └── ToolEnd { output, ..., source_agent_id: Some(id) }
        │
        └── 3. 结束: 发射 ExecutorEvent::SubagentStopped {
                instance_id, result, is_error, elapsed_ms
            }
```

**TUI 路由准则**：
* 若 `source_agent_id == Some(target_id)`：**严格直接投递给 `bg_agents[target_id].pipeline` 消费**，严禁进入主会话！
* 若 `source_agent_id == None`：归属主会话消费。

---

## 4. 关键工程边界与技术实现细节（评审重点）

### 4.1 定向消息发送机制（`Message @agent`）

当用户处于子视口（`active_viewport = Some(id)`）并在输入框输入文字敲回车时：

1. **状态判定**：
   * 如果该子 Agent 处于 `Completed` / `Stopped` / `Failed` 状态：输入框只读锁定，提示 `Task ended (read-only)`。
   * 如果该子 Agent 处于 `Running` 状态：允许输入。
2. **投递模式（Mailbox Queue）**：
   * 用户输入的文本不打断当前正在执行的工具（如正在 `sleep 60`），而是推入 `pending_inputs` 队列，并同步显示在子视口为用户气泡（`UserBubble`）。
   * 该子 Agent 在当前 ReAct 步结束、准备进入下一轮 `before_model` 时，中间件将 `pending_inputs` 自动作为新的 `HumanMessage` 注入子上下文。
3. **紧急打断（Steer）**：
   * 若用户需要立即打断当前工具，先按 `x` 键触发 `ToolInterrupt`，子 Agent 中断当前动作后立即消费输入框指令。

### 4.2 HITL 审批穿透与全局挂起（Pending Approvals）

当后台子 Agent 执行需要审批的高危工具（如 `Write` 或高危 `Bash`）时，**绝不能因用户停留在主视口而发生静默死锁**：

1. **抽屉栏指示**：
   * 该子 Agent 的状态点立即变为黄色呼吸脉冲：`● general-purpose [审批挂起: Bash "rm ..."]`。
2. **全局底栏浮层（`PendingThreadApprovals`）**：
   * 无论用户处于 `main` 视口还是其他子视口，状态栏下方立即浮出全局待审批提醒条：
     ```text
     [审批请求] 来自 子Agent "查询武汉天气": 请求执行 Bash(curl ...) [y:同意 / n:拒绝 / Enter:查看详情]
     ```
3. **处理逻辑**：
   * 用户可直接按下快捷键 `y` / `n` 快速响应；
   * 或按 `Enter` 自动切换路由至该子视口，查阅完整上下文后审批。

### 4.3 终端高度防护与自适应折叠

在矮终端（如 24 行标准终端）下，抽屉栏必须严格控制占用空间，防止主消息区被压没：

1. **最大高度约束**：
   * 抽屉栏最多渲染 `5` 行（1 行 main + 最多 4 个 Agent），超出 4 个的部分在末尾显示单行 `  …+N more`。
2. **极矮终端退化（终端总高度 < 18 行）**：
   * 抽屉栏自动折叠为单行紧凑模式：
     ```text
     [● main | ◯ 武汉天气 | ◯ 随州天气] (↓ 展开管理)
     ```

### 4.4 取消与停止语义（`x` 与 `Ctrl+X Ctrl+K`）

1. **单任务中断（`x` 键）**：
   * 仅针对抽屉栏光标选中的子任务，调用其持有的 `CancellationToken.cancel()`。
   * 如果正在运行子进程（Bash），跨平台强制终止进程树（Windows 使用 `taskkill //f //pid`，Unix 使用 `kill -9`）。
   * 状态标记为 `Stopped`，右侧提示变为 `✗ stopped`，3 秒后淡出或移入历史。
2. **全局中断（`Ctrl+X Ctrl+K`）**：
   * 级联取消所有 `bg_agents` 的 Token，立刻恢复系统至完全静止状态。

### 4.5 持久化与会话恢复（Resume 规范）

1. **存储落库**：
   * 每个子 Agent 在 `build_agent` 时生成独立的 `child_thread_id`，在 SQLite `threads` 表中以 `parent_id` 关联主会话。
   * 子 Agent 运行过程中的每条规范消息（`BaseMessage`）均实时落库到对应线程。
2. **恢复策略（`peri --resume`）**：
   * 主会话仅恢复树形摘要节点与已完成报告卡片。
   * 历史上的子线程作为归档线程保留，可在历史列表查看；抽屉栏默认不挂载已完成的历史任务，保持界面干净。

### 4.6 状态栏工具计数强隔离（严防污染主 Agent）

状态栏第二行用于直观展示主 Agent 的工具调用效率（如 `✓ Read ×53 | ✓ Grep ×31 | ✓ Bash ×7 | +2 more`）。**当前代码中只要触发 `ToolEnd` 就无条件向主会话的 `agent.session_tool_stats` 递增计数的行为是严重的架构污染 Bug**。

**重构规则**：
1. **严格来源校验（Strict Source Guard）**：
   * 在 `cc-tui/src/app/agent_ops/mod.rs` 处理 `ToolStart` 与 `ToolEnd` 时，必须严格判定 `source_agent_id`：
   ```rust
   // 只有主 Agent 本身的工具调用才累加到主状态栏统计
   if source_agent_id.is_none() {
       let count = *agent.session_tool_stats.entry(name.clone()).or_insert(0) + 1;
       agent.session_tool_stats.insert(name.clone(), count);
       agent.finish_recent_tool(&tool_call_id, Instant::now());
   } else if let Some(ref aid) = source_agent_id {
       // 子 Agent 的工具调用计入各自 BackgroundAgentSession 内部的独立 tool_stats
       if let Some(sub) = self.session_mgr.current_mut().find_bg_agent_mut(aid) {
           *sub.tool_stats.entry(name.clone()).or_insert(0) += 1;
       }
   }
   ```
2. **视口隔离展示**：
   * 当用户处于 `main` 视口时，状态栏第二行仅显示主 Agent 自身的工具统计；
   * 当用户通过抽屉切换进入某个子 Agent 视口时，状态栏第二行可自适应渲染该子 Agent 专属的工具统计，两者界限分明，互不干扰。

### 4.7 父 Agent 对子 Agent 的生命周期管控（超时由主 Agent 决策 + 接口取消能力）

在复杂软件工程中，不同子任务的耗时跨度极大：轻量查询或代码审查可能只需 60～180 秒；而全量编译（如 `cargo build --release`）、完整集成测试集跑批、大型仓库迁移分析等任务**天然会持续数十分钟甚至数小时**。

**核心设计原则**：**严禁框架底层写死统一的超时硬上限**！超时预算必须作为显式参数**由主 Agent（调度编排者）按任务实际体量灵活下发与决策**，同时配合可视化进度感知与主动叫停接口。

#### 1. 超时预算由主 Agent 显式下发（Caller-Driven Timeout）
* **参数暴露**：在核心 `Agent` 工具参数中暴露 `timeout_secs: Option<u64>`（`run_in_background` 已随前台模式一并退场，见 §4.15）：
  ```json
  {
    "subagent_type": "general-purpose",
    "prompt": "执行全仓库 release 构建并运行测试套件",
    "timeout_secs": 3600  // 主 Agent 根据任务规模自主评估下发（例如 1 小时）
    // 无 run_in_background：子 Agent 恒为后台模式（§4.15）
  }
  ```
* **参数语义与默认行为**：
  * **主 Agent 显式下发**：按主 Agent 设定的数值执行精确倒计时（如轻量查询设 120s，大型构建设 3600s）；
  * **主 Agent 未指定（`None` / 省略）**：**默认无超时限制（No Timeout）**，允许长耗时任务平稳运行到底，生命周期由主 Agent 编排逻辑或人类手动控制；
  * **传 `0`**：显式声明不设超时。
* **超时到达处理**：
  * 仅当主 Agent 显式指定了 `timeout_secs` 且实际耗时超出该阈值时，才触发自动熔断保护；
  * 熔断时安全杀死衍生子进程并释放并发槽位，抽屉栏标为红色 `✗ timeout`；
  * 向主会话回流通知：`SubagentStopped { is_error: true, result: "Task timed out after {timeout_secs}s as configured by caller." }`。

#### 2. 超时预算的终端可视化呈现（对齐截图实录）
正如真实截图（图 2 / 图 3 / 图 5）中所展现的细节：
```text
• Bash(cargo test)
  └ Running... (18m · timeout 1h)
```
* 当主 Agent 下发了 `timeout_secs` 时，子视口的运行进度行必须实时渲染倒计时预算（如 `timeout 1m 10s`、`timeout 1h`）；
* 若主 Agent 未设超时，则仅展示已运行时长 `Running... (18m)`，不展示伪造的超时上限。这让用户在下钻到子视口时，对该任务的生命周期预期一目了然。

#### 3. 主 Agent（LLM 编排者）的主动查询与取消接口
主 Agent 作为任务的分发者，必须具备对后台任务的主动管理与干预能力。在 `Agent` 工具 schema 中增加指令枚举：

```json
{
  "action": "spawn | cancel | status",  // 默认为 "spawn"
  "task_id": "bg-019c...",              // 当 action 为 cancel 或 status 时必填
  "reason": "用户在主会话变更了需求，提前终止此子任务"
}
```

* **主动查询（`action: "status"`）**：
  * 主 Agent 可在后续轮次主动感知长时间运行的子 Agent 进度（已执行工具步数、当前活动、累计 Token 等），以决定是继续等待还是给用户答复。
* **主动叫停（`action: "cancel"`）**：
  * 当主 Agent 评估某个子任务不再需要、或者用户在主会话说“别跑测试了，直接看代码”时；
  * 主 Agent 能够自主调用 `Agent(action="cancel", task_id="bg-xxx")` 立即终止后台执行；
  * 系统级联取消该子任务并杀死底层进程，主时间线打印：
    `• Sub-agent "全量构建测试" cancelled by orchestrator`。

### 4.8 状态栏 HUD 三行排版与空格等宽对齐规范（Typography & Spacing Alignment）

终端 UI 最忌讳字符参差不齐、黏连或空格缺失。状态栏必须严格遵守**等宽三行对齐规范**：

```text
 [gemini-flash] ██░░░░░░░░ 19% | peri git:(main*) | tok: 13.9M (in: 194.5k, out: 877) | ⏱  29m26s
  ✓ Read ×65 | ✓ Grep ×34 | ✓ Bash ×7 | ✓ WebSearch ×6 | +3 more
  Bypass (Shift+Tab to cycle)  Cache 98% · MEM 102MB
```

#### 1. 左侧缩进基准对齐
* **首列空格对齐**：第 1 行、第 2 行、第 3 行的最左侧一律以固定的 ` `（1 个 ASCII 空格）起始缩进，禁止出现第 1 行顶格、第 2 行带空格导致的视觉凹凸；
* **分隔符标准格式**：所有段落间的分隔符统一使用带对称空格的 ` | `（前后各 1 空格），禁止手写拼接时偶发变为 `| ` 或 ` |`。

#### 2. Token 与耗时格式化规范（严防空格黏连与漏括号）
针对诸如 `(in: 194.5k, out: 877)` 和耗时 `⏱  29m26s`：
* **Token 字符串模版**必须标准化为常量格式：
  `tok: {total} (in: {in_count}, out: {out_count})`
  * 括号内冒号后必有 1 空格：`in: 194.5k`、`out: 877`；
  * 严防代码拼接中由于未做 trim 导致的 `in18s`、`in:194.5k,out:877` 等紧凑粘连问题；
* **耗时图标间距**：时钟 emoji `⏱` 后跟固定 2 个半角空格（`⏱  29m26s`），解决终端中双宽 Emoji 后面文字被吞字或紧贴的问题。

#### 3. 视口切换时 HUD 数据的动态自适应
当用户在抽屉栏按 `Enter` 切换进入某个子 Agent 视口时：
* **第 1 行模型与上下文**：显示该子 Agent 对应的模型名（如子 Agent 配置了特定模型）以及该子任务独立的 Context 消耗与耗时；
* **第 2 行工具统计**：**动态切换为该子任务专属的工具计数**（如 `✓ Bash ×2 | ✓ WebSearch ×1`），彻底隔绝主会话历史；
* **切回主会话（`main`）**：状态栏三行立即无缝切回展示主会话全局的累计指标。

---

### 4.9 抽屉栏等宽列排版与右对齐布局（Drawer Tabular Layout）

抽屉栏中每个后台任务的信息量大（状态圆点、Agent类型、任务动态预览、已执行时长、Token消耗）。必须采用**分栏表格布局（Tabular Columns）**，防止由于任务标题长短不一导致右侧指标像狗啃一样参差不齐：

```text
 ┌── 状态列 ──┬── Agent 类型列 ──┬────── 任务行为预览列 (弹性截断) ──────┬────── 右侧指标列 (固定右对齐) ──────┐
 │   > ●     │ general-purpose  │ 查询武汉天气                          │                      32s · ↓ 49 │
 │     ◯     │ general-purpose  │ 正在等待 60 秒后查天气                 │                      32s · ↓ 33 │
 └───────────┴──────────────────┴──────────────────────────────────────┴───────────────────────────────┘
```

1. **固定宽度列**：
   * 游标与状态指示：固定占用 `4` 显示列（包含 `>` 游标和 `● / ◯` 圆点，字形定稿见 §4.11）；
   * Agent 类型名：固定占用 `18` 显示列（按 `truncate_to_display_width` 左对齐补齐空格）；
2. **弹性中轴截断**：
   * 任务描述根据当前终端宽度弹性伸缩，超出可用宽度时使用字符级单行省略号（`…`）闭合，**绝对禁止换行**；
3. **右侧指标严格右对齐**：
   * 右侧的时长与 Token（如 `32s · ↓ 49`，口径见 §4.12）固定右对齐停靠在终端右边框内侧 1 列处，无论左侧任务文字多大多长，右侧指标始终保持一条垂线垂直对齐。

---

### 4.10 快捷键分级冲突与取消优先级（Keybinding Cascade）

在引入子视口切换后，用户按键的语义必须拥有明确的优先级判定：

1. **`Ctrl+C` 优先级链**：
   * **在子视口下按下 `Ctrl+C`**：**优先且仅中断当前子视口的子任务**，主会话与其他后台 Agent 继续保持运行，视口保持在当前子任务并展示中断状态；
   * **在主视口下按下 `Ctrl+C`**：中断主 Agent 当前轮次的生成；
   * **连按两次 `Ctrl+C`（500ms 内）**：全局级联中断，停止主会话与所有后台子 Agent。
2. **`Esc` 键层级回退**：
   * **在子视口输入框按 `Esc`**：立即退出当前子视口路由，安全平滑返回 `main` 主会话；
   * **在底栏抽屉导航按 `Esc`**：焦点移出抽屉，安全返回当前激活视口的输入框。
3. **并发文件写冲突防御提示**：
   * 多个子 Agent 共享本地文件系统。当两个子 Agent 在短时间内尝试修改同一个文件时（触发 Write/Edit 冲突或 Git 脏工作区冲突），子任务执行器必须立即返回明确错误，并在其子视口标红展示冲突文件名，禁止静默覆盖。

---

### 4.11 抽屉视觉规格（配色 / 字形 / 列宽 / 空行）

抽屉的观感必须与 `docs/subagent-mock.html` 原型逐项对齐，以下为**定稿规格**（实现见 `cc-tui/src/ui/main_ui/bg_agent_bar.rs` 与 `cc-tui/src/ui/theme.rs`）：

1. **字形（定稿，不得混用同形异码）**：
   * 游标：`> ` （选中行）/ `  `（两空格，保证所有行左侧等宽）；
   * 当前视口圆点：`●`（U+25CF 实心）；非当前视口：`◯`（U+25EF 空心）——**禁用 `○`（U+25CB）**，两者宽度不一致会导致行内错位；
   * 极矮终端折叠态沿用同一套字形。
2. **配色（每行按 `bright` 分档，只有键盘游标所在行亮白，其余统一灰）**：
   * 游标 `> ` 与游标行正文（类型名 / 预览 / 右侧指标）：纯白 `#FFFFFF`（`DRAWER_TEXT_ACTIVE`，类型名加粗）；
   * 其余行的类型名 / 预览 / 指标 / 空心圆点：统一灰 `#64748B`（`DRAWER_MUTED`）；
   * 实心圆点 `●`（当前视口）：近白 `#F1F5F9`（`DRAWER_CIRCLE_ON`）；
   * **禁止任何行底色**：游标行只靠亮白前景色 + 加粗表达，`DRAWER_SELECTED_BG` 已删除——真机反馈「↑/↓ 选中有背景色会让人以为已经进这个视口了」（`Enter` 才是进入）；同样**禁止 `Modifier::REVERSED`** 反色；
   * **禁用青色、禁用中间灰阶堆叠**：原型 `docs/subagent-mock.html` 里的 `#38BDF8` 青色游标在真机上被否决，`DRAWER_CURSOR` / `DRAWER_TEXT` / `DRAWER_PREVIEW` 三个常量已删除（真机反馈："不要出现青色，只有 MUTED 和亮白，你搞那么多颜色干嘛"）。此条**覆盖原型**；
     * 作用域**仅限抽屉本身**：子视口输入框那道亮青上边框（`Color::LightCyan`，§4.1/§4.2）**保留**——它是"当前是子 Agent 窗口还是主窗口"的唯一视觉标识（真机决策，明确不改）。
   * `accent` 只覆盖**圆点与右侧指标**，正文不染色；取值：待审批 `theme::WARNING`（琥珀）、**已完成 `theme::SAGE`（绿）**、失败/停止 `theme::ERROR`（红）——
     完成必须转绿，用户要能一眼看出"它结束了，我看完就可以走了"；
   * 第三行抽屉提示（`Enter to view · x to stop · ctrl+x ctrl+k to stop all agents`）用 `theme::MUTED`，与消息区 `Thought for … (ctrl+o to expand)` 同色——它是辅助说明，不是游标。
3. **列宽**：游标 `2` 列 + 圆点 `2` 列（合计前缀 `4` 列，极窄终端按 `left_width` 收缩，否则整行越界）+ 类型名 `≤18` 列（`truncate_to_display_width` 补齐）+ 弹性预览 + 右侧指标右对齐；
   * 行总宽 = 终端宽度，指标右对齐停靠在右边框内侧 `1` 列处；
   * 极窄终端下右侧指标宽度不得挤掉 4 列前缀（`right_width` 上限为 `usable - 4`）。
4. **空行**：抽屉上方保留 `1` 行空行（与状态栏分隔，避免"贴身"）——`bg_bar_height()` 已在内容行数基础上 `+1`；
5. **折叠态行数（以 `bg_bar_height()` 为准）**：
   * 无后台 Agent：`0` 行；
   * 终端总高 `< 18` 行：`2` 行（空行 + 单行紧凑摘要 `[◯ main | ◯ 任务A …] (↓ 展开管理)`）；
   * 其他：`(N + 1).min(5) + 1` 行（`N` = 后台 Agent 数，即 main + 最多 4 个 Agent = 最多 5 个内容行，再加 1 行空行）。

---

### 4.12 抽屉右下角 token 指标口径（单箭头，方向随流向）

抽屉右侧指标**只显示一个箭头**，绝不能并列两个方向——同一时刻只有一个方向在发生，并列会把"这一格在告诉你什么"丢掉：

| 状态 | 渲染 | 取值 |
|---|---|---|
| 默认（子 Agent 正在产出回复） | `↓ {count}` | 最近一次调用的 `output_tokens` |
| 用户向该子 Agent 提问后（`send_subagent_input`） | `↑ {count}` | 最近一次调用的 `input_tokens` |

**实现约束**（`cc-tui/src/app/subagent_session.rs`）：

1. `BackgroundAgentSession::token_metrics() -> Option<String>` 是唯一出口；`last_flow_up: bool` 决定方向（`false` = 下行 = 回复产出）。
2. 翻转时机：`AgentEvent::AssistantChunk`（子 Agent 再次产出回复）→ `last_flow_up = false`；`send_subagent_input`（用户提问）→ `last_flow_up = true`。
3. 数字缩写**必须复用状态栏的 `format_token_count`**（`k`/`M` 档 + 固定一位小数），抽屉与状态栏第 1 行同一套口径；`bg_agent_bar.rs` 不得再自建包装函数。
4. 无用量数据（`session_token_tracker.last_usage` 为 `None`）时返回 `None`，该行右侧**只显示耗时**（如 `32s`），不得出现空箭头或 `0`。
5. 已结束行不显示 token：`✓ completed` / `✗ stopped` / `✗ timeout` / `✗ failed` 优先于指标。

---

### 4.13 子会话与主路径同构维护 `AgentComm`（子视口状态栏一致性）

`BackgroundAgentSession::handle_event()` 必须与主 Agent 路径（`cc-tui/src/app/agent_ops/mod.rs`）**同构**维护 `AgentComm`，否则子视口的状态栏第 2 行为空：

1. `ToolStart` **必须同时**登记三处：`agent.running_tools.push(..)`、`agent.active_tool = Some(..)`、`agent.push_recent_tool(..)`（`running: true`）。只写 `active_tool` 是反例——状态栏第二行读的是 `running_tools` / `visible_recent_tools()`，子视口会呈现空列表。
2. `ToolEnd` 必须对称回收：`running_tools.retain(..)`、`finish_recent_tool(..)`，并把 `active_tool` 回退为 `running_tools.last()`。
3. 状态栏工具计数（`session_tool_stats`）按 §4.6 强隔离：只累加该子会话自己的计数，`ToolEnd` 计数必须以「`running_tool_times` 中确实存在该 `tool_call_id`」为前提（防重复计数）。
4. `current_activity`（抽屉中间列）与 `active_tool`（审批条）职责分离：前者是子 Agent 的**助手文本**（`activity_buffer` 累积 `AssistantChunk`，`ToolEnd` 复位），后者才是工具串；`ToolStart` **不得**用 `display(args)` 覆盖 `current_activity`。
5. **任务预览（`task_preview`）的来源**：启动摘要（`• N background agents launched`）、抽屉行的回退标题、状态栏标题都读它，必须优先取 Agent 工具显式传入的 `description`（schema 已声明 "used for UI display"），缺失时才退回 prompt 并**跳过 `【…】` / `[…]` 包裹的纯指令行**，上限 40 字符（`cc-middlewares/src/subagent/tool/mod.rs` 的 `subagent_task_preview()`）。
   **禁止**直接取 prompt 首行——模型习惯把语言约束写在第一行，会把注入给子 Agent 的指令变成任务标题（真机症状：两个后台子任务标题都显示成 `【必须全程用中文思考和回答】`）。

---

### 4.14 子会话生命周期与资源回收边界（`subagent_sessions` 无界增长风险）

这是当前实现的**已知技术债**，必须在排期内明确（否则长会话内存持续上涨）：

1. **现状**：`subagent_sessions: HashMap<String, BackgroundAgentSession>` 只有**写入**（`handle_subagent_start` / `handle_subagent_event` 的 `or_insert_with`）与整表清理（`App::clear_subagent_sessions()`）两条路径。子 Agent `Completed` / `Stopped` / `Failed` **不回收**。
2. **放大因素**：每个 `BackgroundAgentSession` 持有完整 `MessagePipeline` + `view_messages` + `AgentComm`（含工具历史），单个对象随子任务长度线性增长；`/clear` 之外没有回收时机。
3. **排期要求**：
   * 已完成条目在**离开抽屉且不在视口**后，仅保留「轻量归档摘要」入主时间线，`subagent_sessions` 条目可回收；
   * 回收前必须已被用户查看或已被摘要吸收，禁止静默丢弃用户尚未看过的子会话输出；
   * 任何回收都必须同步清理 `background_agents` 对应行、`focused_instance_id`（若指向被回收项则回退 `None`）与 `ui.bg_bar_cursor`，避免出现"抽屉幽灵行"。
4. **已落地：「用户在看」优先于回收（真机定稿）**：
   * 子 Agent 结束时，若 `focused_instance_id == 该 instance_id`，**视口与抽屉行原地保留**（行内 `✓ completed`、圆点与完成标记转绿），既不切回 main、也不摘行——结束瞬间抽走视口等于把用户正在读的内容抹掉（真机反馈："我要看啊，不应该等我走了你再清理吗"）；
   * 用户按 `Esc`（或 `Enter` 切到别的行）真正离开时，`switch_subagent_viewport` 才调用 `reclaim_finished_subagent` 一次性回收该**会话句柄 + 抽屉行**（活跃中的子 Agent 不受影响）；
   * 聚焦判据必须是「两个 id 都在场」：`child_thread_id` 与 `focused_instance_id` 同时为 `None` 时**不得**判为聚焦，否则无 id 的完成事件会被误判为"用户正在看"，从而留下幽灵行（`agent_events_bg.rs` 的 `was_focused` 用 `match (Some, Some)` 实现）。

---

### 4.15 只有后台模式（前台同步已废弃）

**子 Agent 一律后台执行，这才是唯一产品模式**（客户培训也按这个口径讲）：抽屉是子 Agent 的唯一呈现，主 Agent 的消息区永远不被子 Agent 的内部过程污染。前台同步模式（`run_in_background=false`）已从代码里删除，`run_in_background` 参数也从工具 schema 退场。

1. `SubAgentTool::invoke` 直接走 `invoke_background`，唯一的前置校验顺序是「入参（缺 prompt / 缺 subagent_type / agent 定义不存在 / fork 缺父消息）→ 环境（本会话是否有 `BackgroundTaskRegistry`）」，保证模型拿到的是**它能处理的错误**，而不是「没有后台注册表」这类环境错误。
2. 结果回流：`BackgroundTaskResult` → `peri/subagent_lifecycle` + `AgentEvent::BackgroundTaskCompleted` → 主时间线完成行 + 自动续接（`pending_bg_continuation`）。
3. 并发上限 **3**（`BackgroundTaskRegistry::max_concurrent`），超出直接报错让模型自己排队等待。
4. `subagent_sessions` 条目对每个子 Agent 都存在，`Enter` 下钻与状态栏切换由此驱动；抽屉行只在后台任务存在期间出现。
5. 历史兼容：旧会话里已落库的内嵌 `SubAgentGroup`（前台同步时代的产物）仍要能正常渲染，渲染路径不得删除；但不再产生新的内嵌分组。

### 4.16 fork 与普通子 Agent 的关系（必须有且只有一条构建路径）

fork **不是另一种东西**，而是「带主 Agent 上下文的子 Agent」：事件、会话池、抽屉、子视口、时间线分组本来就共用（§4.15），构建/执行也必须共用。

1. **唯一入口**：`cc-middlewares/src/subagent/tool/build_agent.rs` 的 `build_agent_from_def(agent_def, agent_name, cwd, AgentBuildOptions)`。普通子 Agent 与 fork 都走它（`AgentBuildOptions` 只有 `fork` 与 `snapshot_at_message_id` 两个字段，因为取消策略恒为 independent）；**禁止再写第二套内联构建**（`execute_fork.rs` 曾经那份 ~100 行内联实现已删除）。
2. **fork 真正的特例只有「继承上下文」一条**：注入父级消息到子状态（`parent_messages`）、prompt 包一层 `build_fork_directive()`、子线程记录 `snapshot_at_message_id` 分叉点。
3. **权限继承不是 fork 专属**：所有子 Agent 都通过 `permission_config()` 复用父级 HITL 权限配置（派生的 `HumanInTheLoopMiddleware` 不持有审批 broker），fork 并不额外多一条权限通道。
4. **`fork = true` 的推导结果**（因为没有 agent.md 可依）：工具全量继承（`filter_tools(Empty, Empty)`，内部仍剔除 `Agent` 防递归）、系统提示不做覆写（复用主 Agent 人格）、中间件用 `SubAgentMiddlewareConfig::for_fork`（不带 skills）。
5. **其余一律共用**：子线程建档、`SubagentStarted/Stopped` 事件与 hooks、`task_preview`（§4.13 第 5 条）、max_iterations 与模型（取自 agent_def）、结果格式化、状态流转。
6. **漂移防御**：任何子 Agent 行为改动只改 fork 或只改普通路径，视为回归。已有用例守住：fork 启动事件的 `task_preview` 来自 `description`；fork 不套 agent.md 覆写、普通子 Agent 套（`tool_test.rs`）。

---

## 5. ACP 协议契约与接口定义

Peri 采用 TUI 与 ACP Server 解耦架构，以下为两端必须对齐的 JSON-RPC 协议项：

### 5.1 通知扩展：`_peri.sourceAgentId`
所有由子 Agent 产生的 `session/update`，必须在外层元数据携带来源标识：

```json
{
  "jsonrpc": "2.0",
  "method": "session/update",
  "params": {
    "sessionId": "root-session-uuid",
    "update": {
      "sessionUpdate": "agent_thought_chunk",
      "content": { "type": "text", "text": "正在查询实时天气接口..." }
    },
    "_peri": {
      "sourceAgentId": "child-thread-uuid-01"
    }
  }
}
```

### 5.2 子 Agent 生命周期通知：`peri/subagent_lifecycle`

```json
{
  "jsonrpc": "2.0",
  "method": "peri/subagent_lifecycle",
  "params": {
    "event": "started", // "started" | "progress" | "stopped"
    "instanceId": "child-thread-uuid-01",
    "agentType": "general-purpose",
    "taskPreview": "查询武汉天气",
    "currentActivity": "正在抓取页面",
    "elapsedMs": 15000,
    "tokensUsed": 49,
    "isError": false,
    "result": null
  }
}
```

#### 5.2.1 客户端注册硬约束（漏登记即整条抽屉静默失效）

服务端在 `cc-acp/src/session/event_sink.rs` 里用 `transport.send_notification("peri/subagent_lifecycle", ..)` 发出，**不带 `notifications/` 前缀**；而客户端 `cc-tui/src/acp_client/client.rs` 的默认匹配只认 `notifications/peri/` 前缀，未显式登记的方法会掉进 `AcpNotification::Other` 并被丢弃——表现是「服务端一直在发、TUI 里 `background_agents` 永远为空、底栏无抽屉、状态栏没有 `↓ to view`」。

1. `AcpTuiClient::is_peri_notification(method)` 是唯一登记点，新增任何 `peri/*` 通知**必须**在这里显式列出方法名（现有：`peri/subagent_lifecycle` + `notifications/peri/` 前缀）；
2. 客户端分发在 `cc-tui/src/app/agent_ops/acp_bridge.rs`：`AcpNotification::Peri { method == "peri/subagent_lifecycle" }` → `handle_subagent_lifecycle(params)` → `handle_subagent_start` / `handle_subagent_stopped`；
3. 该约束必须有测试兜底（`cc-tui/src/acp_client/client_test.rs` 断言方法名被认作 peri 通知），否则回归时只会表现为"UI 没反应"，极难定位；
4. 完整字段属两段合成：`started` / `stopped` 直接由 `lifecycle_payload()`（`cc-acp/src/session/subagent_protocol.rs`）从 `AgentEvent` 载荷映射；`currentActivity` / `tokensUsed` / `status` / `timeoutSecs` 由 `enrich_lifecycle()` 从 `BackgroundTaskRegistry` 快照补齐，**仅在后台注册表存在时**才有值。

### 5.3 停止指定子 Agent：`session/cancel` 扩展

```json
{
  "jsonrpc": "2.0",
  "method": "session/cancel",
  "params": {
    "sessionId": "root-session-uuid",
    "targetThreadId": "child-thread-uuid-01" // 可选，不传则取消整会话
  }
}
```

---

## 6. 改动清单与模块分工（研发排期索引）

| Crate | 关键文件 | 核心改动 | 预估工时 |
|---|---|---|---|
| **`cc-agent`** | `src/agent/events.rs` | 1. 为 `AiReasoning` 补充 `source_agent_id: Option<String>`；<br>2. 补充 `SubagentStarted` 完整载荷（带 `task_preview`）。 | 0.5 天 |
| **`cc-middlewares`** | `src/subagent/tool/` | 1. `build_agent.rs`：创建时注入 40 字 Prompt 前缀；<br>2. `execute_bg.rs`：增加 `timeout_secs` 超时包装；<br>3. `define.rs`：`Agent` 工具增加 `action: "cancel" \| "status"` 控制协议；<br>4. `SourceAgentIdHandler`：补齐对 `AiReasoning` 的 tag 包装。 | 1 天 |
| **`cc-acp`** | `src/session/executor/` | 1. `session/update` 桥接统一透传 `_peri.sourceAgentId`；<br>2. `session/cancel` 支持按 `targetThreadId` 级联取消。 | 1 天 |
| **`cc-tui`** | `src/app/chat_session.rs` | 重构数据模型，引入 `BackgroundAgentSession` 与 `active_viewport`。 | 1 天 |
| **`cc-tui`** | `src/app/agent_ops/` | 1. 实现事件多路分流：带 `source_agent_id` 的事件路由到专属管线；<br>2. 严格拦截 `ToolEnd`，禁止子 Agent 工具污染主状态栏 `session_tool_stats`。 | 1.5 天 |
| **`cc-tui`** | `src/ui/main_ui/bg_agent_bar.rs` | 完整重写抽屉栏（游标箭头、实/空心点、自适应高度、快捷键联动）。 | 1.5 天 |
| **`cc-tui`** | `src/ui/main_ui/input.rs` | 视口高亮青色边框、专属 Tag、Placeholder 联动渲染。 | 0.5 天 |
| **`cc-tui`** | `src/app/events.rs` | 键盘状态机：`ArrowDown` 进入抽屉、`Enter` 视口切换、`x` 单停。 | 1 天 |

---

## 7. 验收标准与测试规范

### 7.1 自动化 Headless 回归测试
1. **多 Agent 状态隔离断言**：
   * 并发启动 2 个 Mock 子 Agent 并同时吐出不同的 Reasoning 和 Tool 调用。
   * 断言：`sub1.pipeline.view_messages` 与 `sub2.pipeline.view_messages` 内容完全独立；主会话 `messages.view_messages` 仅有树状启动节点，无任何工具碎片。
2. **视口切换断言**：
   * 触发 `active_viewport = Some(sub1)`，断言 `current_view_messages()` 返回 sub1 内容，输入框 Tag 显示 sub1 任务名。
   * 切换回 `None`，断言立即恢复为主会话内容。
3. **单任务中断断言**：
   * 对 sub1 发送 `cancel`，断言 sub1 状态变为 `Stopped`，sub2 依然保持 `Running` 继续读秒。
4. **抽屉渲染断言**（`bg_agent_bar_test.rs` / `main_ui_test.rs`）：
   * 列宽：行总宽恒等于终端宽度，最后一列不越界，右侧指标右对齐（100 列断言）；
   * 字形与配色：只有键盘游标所在行亮白，其余统一灰；圆点为 `●`/`◯`（断言不得出现 `○` U+25CB）；抽屉不得出现青色或中间灰阶；
   * 完成态：正在查看的子 Agent 结束时视口内容与会话句柄都保留，抽屉行转绿（`theme::SAGE`）；用户离开后才回收会话与行；
   * token 指标：Agent 回复时为 `↓ {output}`、用户提问后翻转为 `↑ {input}`，同一行**不得**同时出现两个箭头或累计值（`test_drawer_metrics_follows_token_flow_with_single_arrow`）。
5. **通知登记断言**：`peri/subagent_lifecycle` 必须被 `is_peri_notification()` 认作 peri 通知（§5.2.1），否则整条抽屉静默失效。
6. **游标收敛断言**：后台 Agent 全部结束后 `ui.bg_bar_cursor` 必须为 `None`、第三行不再渲染抽屉导航提示、按键不被 `bar_focus` 吞掉。

### 7.2 终端人工验收核对单（Checklist）
- [ ] 空输入框按 `↓` 能精准进入底栏，`>` 箭头随 `↑/↓` 移动，游标行**只有白字、没有底色**（也不得有反色观感）——有底色会误以为已经进入视口。
- [ ] 抽屉只有**亮白**与**灰**两档：不出现青色游标，也没有多种灰阶堆叠；第三行提示与消息区 `Thought for … (ctrl+o to expand)` 同色（MUTED）。
- [ ] 抽屉右侧**只有一个箭头**：子 Agent 产出回复时 `↓ {n}`，向它提问后翻为 `↑ {n}`；无用量数据时只显示耗时。
- [ ] 抽屉中间列显示子 Agent 的**助手文本**（不是 `Bash(...)` 工具串），工具细节进视口看。
- [ ] 按 `Enter` 切换进入子任务视口，输入框上边框变亮青色且右侧出现任务标签。
- [ ] **视口内遇到子 Agent 结束**：画面内容不消失（行内转绿显示 `✓ completed`），继续读没问题；按 `Esc` 返回主会话时该行才收起、会话回收。
- [ ] 子任务视口内可完整看到其内部的 Thinking（支持 `Ctrl+O` 折叠）、Bash 倒计时和 Spinner，且状态栏第 2 行显示**该子任务专属**的工具计数（非空、非主会话数字）。
- [ ] 在子任务视口中按 `↑` 选回 `main` 并按 `Enter`，可平滑返回主会话。
- [ ] 子任务结束时，主会话打印 `• Agent "xxx" finished · 1m 32s`，主模型完成报告输出。
- [ ] 终端窗口缩小时（24 行高度），抽屉为 `6` 行（5 内容行 + 1 空行）；总高 `< 18` 行退化为 `2` 行紧凑态，不发生界面崩溃或文字重叠。
