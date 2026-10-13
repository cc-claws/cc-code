# PRD: 后台多子 Agent 抽屉式管理与虚拟视口路由系统

> **文档版本**：v2.0 (评审完备版 / Ready for Engineering Review)  
> **基线代码**：`origin/main` (`cc-tui`, `cc-acp`, `cc-middlewares`, `cc-agent`)  
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
│  ○ main                                                                │
│  ○ general-purpose   查询武汉天气                   32s · ↓ 49 tokens │
│> • general-purpose   查询随州天气                   32s · ↓ 33 tokens │  ← 抽屉管理栏 (> 选框, • 当前视口)
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 状态流转四阶段

1. **分发态（Launch）**：
   * 主时间线输出树形节点：`• N background agents launched (↓ to manage)`，用 `├──` / `└──` 列出任务简述。
   * 抽屉栏立即展开显示所有后台 Agent，状态栏提示 `(↓ to manage)`。
2. **抽屉导航态（Drawer Nav）**：
   * 在空输入框按 **`↓`**，焦点下移进入抽屉，行首出现高亮青色箭头游标 `> `。
   * **当前查看的视口**保留实心圆点 `•`，**当前选中的行**带有 `> ` 游标。两者解耦。
   * 选到非当前视口时，快捷键提示行变为：`Enter to view · x to stop · ctrl+x ctrl+k to stop all agents`。
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

### 3.2 虚拟视口路由机制（Virtual Viewport Routing）

在渲染层彻底解耦数据源，`render_thread.rs` 统一通过 `active_viewport` 进行视口路由：

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
     [● main | ○ 武汉天气 | ○ 随州天气] (↓ 展开管理)
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
* **参数暴露**：在核心 `Agent` 工具参数中暴露 `timeout_secs: Option<u64>`：
  ```json
  {
    "subagent_type": "general-purpose",
    "prompt": "执行全仓库 release 构建并运行测试套件",
    "run_in_background": true,
    "timeout_secs": 3600  // 主 Agent 根据任务规模自主评估下发（例如 1 小时）
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
 │   > ●     │ general-purpose  │ 查询武汉天气                          │            32s · ↓ 49 tokens │
 │     ○     │ general-purpose  │ 正在等待 60 秒后查天气                 │            32s · ↓ 33 tokens │
 └───────────┴──────────────────┴──────────────────────────────────────┴───────────────────────────────┘
```

1. **固定宽度列**：
   * 游标与状态指示：固定占用 `4` 显示列（包含 `>` 游标和 `● / ○` 圆点）；
   * Agent 类型名：固定占用 `18` 显示列（按 `truncate_to_display_width` 左对齐补齐空格）；
2. **弹性中轴截断**：
   * 任务描述根据当前终端宽度弹性伸缩，超出可用宽度时使用字符级单行省略号（`…`）闭合，**绝对禁止换行**；
3. **右侧指标严格右对齐**：
   * 右侧的时长与 Token（如 `32s · ↓ 49 tokens`）固定右对齐停靠在终端右边框内侧 1 列处，无论左侧任务文字多大多长，右侧指标始终保持一条垂线垂直对齐。

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

### 7.2 终端人工验收核对单（Checklist）
- [ ] 空输入框按 `↓` 能精准进入底栏，`>` 箭头随 `↑/↓` 移动，选中行反色高亮。
- [ ] 按 `Enter` 切换进入子任务视口，输入框上边框变亮青色且右侧出现任务标签。
- [ ] 子任务视口内可完整看到其内部的 Thinking（支持 `Ctrl+O` 折叠）、Bash 倒计时和 Spinner。
- [ ] 在子任务视口中按 `↑` 选回 `main` 并按 `Enter`，可平滑返回主会话。
- [ ] 子任务结束时，主会话打印 `• Agent "xxx" finished · 1m 32s`，主模型完成报告输出。
- [ ] 终端窗口缩小时（24 行高度），底栏不超过 5 行，不发生界面崩溃或文字重叠。
