# PRD: Agent Command Center（跨项目 Agent 会话中心）

> 对标 OpenAI Codex CLI 的 **Agent Command Center**（源码 `codex-rs/tui/src/app/agents_overview_view.rs` + `app/agent_center/`）。
> 本 PRD 的技术结论均来自源码/快照核对，证据以 `文件:行` 标注。

## 实现状态

**P0 / P1 / P2 已实现，正在执行集成审计与发布验证。** 效果图（可交互 mock）见 `.tmp/agent-command-center-interactive.html`（本地文件，未入库，42 条脱敏样例数据）。

- **§5.3 全部按已定决议及建议默认执行**；下列“实施修订”解决原文冲突，并作为最终验收口径。
- 本 PRD 的技术结论分两类：**已核实**（带 `文件:行` 证据）与**待核实**（仅 UX 描述，无代码依据），后者实现时必须先核对

### 实施修订（2026-10-15，Issue #542）

- 本期交付全部 P0 / P1 / P2；`/tasks` 保留代码按 A4 在功能验收后独立清理 PR。
- **状态以 Q3 与验收用例为准**：历史 `done/cancelled/error` 均为 Inactive，历史 active 在 24h 内为 Ready，超期为 Inactive；只有本进程的根任务/子任务运行与待审批/待回答可分别提升为 Working/Needs you，Needs you 优先。删除 §4.3 中历史 error 同时归两桶的歧义。
- **门控按 peri 实际能力映射**：本地 in-memory ACP 为可用本进程；无 daemon、vim 或自定义 move_left 绑定的配置项，不引入虚构状态。要求焦点、根会话、空文本/空附件、无 setup/OAuth/交互/面板/补全/子任务控制前缀，并仅接受裸 Left Press。粘贴独立路由，不穿透页面到 composer。
- **计数与分页**：轻量全局索引提供全部根会话计数；SQLite 在 root 筛选后每页 LIMIT 10，再对当页消息 JOIN 聚合。过滤/搜索作用于已加载页，Tab 不请求存储；Show more 始终可达。filesystem 后端必须读取 index.json，但只对页内会话统计消息文件大小。分组计数等于当前过滤后已加载的组内行数。
- **切换不中断任务**：peri 单个根 ChatSession 不承载并行根任务；有运行 Agent/Shell、排队消息或未处理审批时拒绝跨会话切换和删除，当前行 Enter 直接返回原视口。`n` 使用选中 cwd 创建空白草稿，首次发送前不创建持久化会话行。目录缺失或 ACP load 失败保留当前上下文。`o` 复用当前项目 `/history`。
- **提示词稳定性优先**：新 ACP session 按目标 ThreadMeta.cwd 构建 frozen 数据；同一 session 再次 load 不重冻提示词。切换时使 UI 派生 cwd、路径渲染、@ 搜索、技能、分支缓存、hooks 和 MCP 与目标会话一致，不改变进程 cwd。
- **永久删除确认**：Backspace/Delete 打开确认，`y` 明确永久删除（不可恢复），Esc/n 取消；Enter 不默认触发删除。重命名与删除固定原 thread ID，防止列表状态变化导致写错会话。
- `/agents` 命令名让渡后不保留指向旧定义面板的同名 alias，避免入口冲突；定义面板使用 `/agent-design`。

## 1. 背景

现在 peri 里看「有哪些会话」只有两个入口，且**都按 cwd 过滤**：

| 入口 | 现状 | 问题 |
|---|---|---|
| `/tasks`（`cc-tui/src/app/panel_plugin.rs:441`） | `list_threads()` 之后 `filter(cwd)`，只列当前项目 | 跨项目看不到 |
| `/history`（`cc-tui/src/app/thread_ops.rs:459`） | 同上 | 跨项目看不到 |

而底层数据**本来就是全局的**：`ThreadStore::list_threads()` 是全局查询（`cc-agent/src/thread/sqlite_store.rs:498`，`WHERE hidden = 0 ORDER BY updated_at DESC`），库落在 `~/.cc-code/threads/threads.db`（单库、带 `cwd` 列）。**过滤器是调用方自己加的。**

同时 `/tasks` 还有三个已确认的缺陷：

1. 状态是写死的假值（`panel_plugin.rs:447` `status: "done"`），不读 `ThreadMeta.agent_status`
2. 详情页是空壳：`detail_messages` 全仓只有 `clear()`、**没有任何赋值点**，`Enter` 进去恒为 `(Loading messages...)`
3. 面板里混了 `Cron Tasks` tab，与独立的 `/cron` 功能重叠

多项目并行时缺一个统一入口，故做此页。

## 2. 目标 / 非目标

**目标**

1. 一个**跨全部项目**的会话列表，按项目分组，一行 = 一个 root 会话
2. 五档状态桶可过滤：`All / Needs you / Working / Ready / Inactive`
3. 右侧详情栏可读：`Model / Reasoning / Project / Branch / Last message / Prompt`
4. `Enter` 直接进入该会话（复用既有会话页与渲染管线）；**跨项目可进入** —— A1 已定采用**方案 B**（对齐 ACP 的 per-session cwd，见 §5.1）
5. `/tasks` 的入口由本页取代（A4）：`/tasks` 命令先屏蔽，代码本 PR 内保留、验收后单独清理

**非目标（本期不做）**

- archive / hide / pin（peri 无对应持久化字段；`hidden` 列预留，本期不写）
- `w` new worktree（peri 无 worktree 概念）
- 跨进程实时状态（见 §5.2 降级）
- 分组偏好持久化（Q5 决定不持久化）

## 3. 用户体验

### 3.1 入口

| 入口 | 说明 |
|---|---|
| `/agents` | **本页占用该命令**。peri 原 `/agents`（选 agent 定义，`cc-tui/src/command/panel/agents.rs:19`）**改名为 `/agent-design`**（A3 已定，别名同为 `agent-design`） |
| `←` | 全局快捷键，**仅在满足全部门控条件时生效**（§3.5）。P0 若未做键位，`/agents` 是唯一入口 |
| `/tasks` | **入口屏蔽**（A4 已定）：从命令注册表移除，命令面板与补全里不再出现；代码**本 PR 内保留**（不删除），验收通过后单独提清理 PR。同时**去掉它的 cwd 过滤**不再有意义（入口都已不可见） |

### 3.2 页面结构（逐字对齐 Codex 快照 `command_center__live_center_columns.snap`）

```
  Agent command center  Group: Project  g
   All 6   Needs you 2   Working 1   Ready 2   Inactive 1        tab/⇧tab filter
  ─────────────────────────────────────────────────────────────
      Tasks                    Status         Updated  │ Task details
  Pinned  2                                           │
  › ○ Current task            Ready              now  │ Current task
    ● Needs an answer         Needs input     2h ago  │ ○ Ready
                                                       │
  /tmp/project  4                                     │ Model: Unknown
    ○ A longer task title…    Ready            5m ago  │ Reasoning: Unknown
                                                       │
                                                       │ Project
                                                       │ /tmp/project
  ? help  esc back  ↑/↓ move  enter open  n new
```

三条硬性形制（**易错点，实现时必须照做**）：

1. **全页只有一条横线**，位于 tab 行下方（`agent_center/render.rs:52-84` 的 header 第 3 行）
2. **标题行是纯文本一行**，不画线；`Group: … g` 紧跟标题后（左对齐，不是右对齐）
3. **页脚上方没有横线**；`tab/⇧tab filter` 右对齐在 tab 行末尾

窄屏降级：`body.width >= 90` 才显示右侧详情栏（`[Min(46) | gap 3 | Length(38)]`），否则单栏。

**peri 的两处增量**（Codex 快照里没有，但 peri 需要）：

1. **当前会话标记（F2）**：当前 `current_thread_id` 对应的行加 `●` 与 `(current)` 后缀——首次打开时否则不知道「我」在哪
2. **其它项目计数提示（F3）**：按 recency 排序 + `PAGE_SIZE = 10` 时，首屏可能只有当前项目的行（mock 实测：42 条中 30 条属于 peri，需按 4 次 `Show more` 才翻出第二个项目）。建议在计数行尾部提示「其它项目 N 条 · 按 g 分组」，否则用户会以为只有本项目

### 3.3 键位表（对齐 `agents_overview_view.rs` 与 `keymap.rs:1949-1963`）

| 键 | 行为 | Codex 证据 |
|---|---|---|
| `↑` `↓` | 移动光标，**选中行始终滚入视口** | `move_selection` |
| `tab` / `⇧tab` | 循环五档过滤，重置滚动 | `agent_center/input.rs:92-107` |
| `g` | 分组切换 `Project → Status → Model`（**Codex 会持久化；peri 按 Q5 不持久化**，每次默认 `Project`） | `agents_overview_view.rs:681` |
| `/` | 搜索态切换（关闭时清空） | `:644` |
| `enter` | **光标在 `Show more` 上 → 加载下一页**；重命名中 → 提交；否则 → 进入该会话 | `activate()` `:361-397` |
| `o` | 打开 **resume 选择器**（独立列表，**与选中行无关**） | `:677` → `AppEvent::OpenResumePicker` |
| `n` | **起一个全新空会话**并进入其 Composer，不打断其他 agent、**不往列表插行**（**A2 已定：用「选中行的 cwd」**，对齐 Codex；提示行写出目标项目） | `:704` → `NewAgentsOverviewSession{cwd: 选中行.cwd}` |
| `r` | 内联重命名，预填当前名字（peri 已有 `update_title`，见 §4.2） | `:726` |
| `⌫` | **永久删除**（对齐 Codex），必须走**二次确认**浮层；确认按钮上要有「不可恢复」字样（D4 已定） | `:742-743`，`agents_overview_actions.rs:209-232` |
| `?` | 快捷键浮层（Navigate / Tasks / View 三组） | `agent_center/hints.rs:58-128` |
| `esc` | 中心里 = 退回你的会话；**会话页里 Esc 保持原语义（取消/中断）**，不返回本页（层级优先级见 D2） | `ListAction::Cancel` |

**Codex 的 `a`（archive）本期不做**（对应 peri 的非目标）：它的语义是「停止运行中的工作 + 归档，可从 resume 选择器恢复」（`agents_overview_actions.rs:204-208`），是**软**操作；peri 的 `hidden` 列留作将来实现 archive，本期不写。

**进入会话后本页关闭**（`agents_overview.rs:458-460` 置 `ViewCompletion::Accepted`），不存在「从会话 Esc 回到本页」；要再打开按 `←`。

### 3.4 详情栏

顺序：`Task details` / 标题 / `● 状态` / `Model:` / `Reasoning:` / `Project` / `Branch` / `Last message` / `Prompt`。
空值显示 `Unknown` / `（无）`；`Last message` 在读取中显示 `(加载中…)`（B3），并有 150ms 防抖。

**注意**：Model / Reasoning / Branch 三行在 P1 之前**必然全是 `Unknown`**（列还不存在），P0 就要按「缺失也占位」渲染，别做「有值才显示」——否则 P1 上线时布局会跳。

### 3.5 `←` 的门控（**10 条，缺一不可**）

`agents_navigation.rs:22-33` 的 `agents_navigation_available()`：

| # | 条件 | 证据 |
|---|---|---|
| 1 | 会话连在 **LocalDaemon** 上（Embedded / Remote 都不启用，且底部连提示都不显示） | `agents_navigation_tests.rs:5-47` |
| 2 | `←` 仍绑定在编辑器的 `move_left`（用户改键即失效） | `agents_navigation.rs:13-20` |
| 3 | `has_focus` | `:26` |
| 4 | `draft.input_enabled` | `:27` |
| 5 | `slash_commands_enabled()` | `:28` |
| 6 | **草稿为空**（非空时 `←` 是移动光标；测试：草稿 `"ab"` + `←` + `!` → `a!b`） | `:29`，测试 `:86-101` |
| 7 | `!is_in_paste_burst()` | `:30` |
| 8 | `!popup_active()` | `:31` |
| 9 | `!vim_operator_pending()` | `:32` |
| 10 | 键事件是 **`KeyEventKind::Press`** —— 按住连发的 `Repeat` **不算** | 测试 `:115-128` |

**切换会话侧还有额外拦截**（不是「按了就一定切过去」）：

| 拦截 | 行为 | 证据 |
|---|---|---|
| 有未处理的权限请求 | 拒绝切换 | `session_lifecycle.rs:1341` `reject_pending_permission_root_switch()` |
| Windows 沙箱设置未完成 | 拒绝并提示 `Finish Windows sandbox setup before switching threads.` | `:562`、`:1333` |
| 线程不可用 / 离线 | 拒绝 | `agents_overview.rs:453-456`、`agents_overview_new.rs` `reconnect.offline` |

### 3.6 会话页第三行（沿用既有 status_bar 结构）

```
Bypass (Shift+Tab to cycle) · ← 6 agents  Cache 100% · MEM 100MB          <右侧快捷键>
```

要点：

1. **`← N agents` 的 N = 当前项目（当前会话所在 `cwd`）的会话数**——即在 peri 项目里就数 peri 有多少个对话。（口径由实机确认；可下载的 Codex 版本 `7c152d4` 里还只有 `← for agents`，不带计数，故以实机为准。）
2. `Cache` / `MEM` **属于左侧块**——`status_bar.rs:620-655` 是 `left_spans.push(...)`，不是右对齐；右侧留给快捷键提示。
3. 权限模式的 `(Shift+Tab to cycle)` 用 peri 既有 `Shift+Tab` 循环；`← N agents` 同样只在 §3.5 门控全通过时显示。

## 4. 技术方案

### 4.1 架构决策：**新增全页视图路径，不走面板**

Codex 是 `AgentsOverviewView` 实现 `BottomPaneView`、由 `show_bottom_pane_view()` 直接接管底部面板（`agents_overview.rs:151`），`desired_height() = 24`。

peri 若走 `PanelKind`：① 面板渲染在 `chunks[3]` 带子里、**高度上限 60% 屏高**（`ui/main_ui/mod.rs:592`）；② 要改 `PanelManager` 约 **15 处穷举 match**（`kind/priority/scope/render/desired_height/...` + `keyboard/panels.rs` 两个清单）。

故新增一条**全页视图**路径，只加两个 hook：

| 挂点 | 位置 | 做法 |
|---|---|---|
| 渲染 | `cc-tui/src/ui/main_ui/mod.rs:32` `render()` 入口 | 命中即整页渲染并 `return`，`chunks` 布局（消息区 / 输入框 / 状态栏）全部不参与 |
| 按键 | `cc-tui/src/event/keyboard.rs:96` `handle_key_event` | 在 Stage 1 之前插专属 stage，命中即 `return`，不穿透到 textarea 与面板链 |

### 4.2 数据来源

| 列 | 来源 | 现状 |
|---|---|---|
| 标题 | `ThreadMeta.title` | ✅ 已有 |
| 分组键 | `ThreadMeta.cwd` | ✅ 已有 |
| Updated | `ThreadMeta.updated_at`（`list_threads` 已按它降序） | ✅ 已有 |
| 状态 | `ThreadMeta.agent_status` + 本进程内存态 | ⚠️ 见 §5 |
| Model | 待新增 | ❌ 无字段 |
| Reasoning | 待新增 | ❌ 无字段 |
| Branch | `ThreadMeta` **不存**；`git_branch_cache`（`service_registry.rs:85`）只是**当前 cwd 的进程级缓存**，拿不到别的项目 | ❌ 需新增独立列（**Q4 已决**，见 §10） |
| Last message | 需 `load_messages()` 取末条 | ❌ |
| Prompt | root 取首条 Human 消息 | ⚠️ 需读消息 |

**关联能力（已存在于 `ThreadStore`，本页直接复用，不需要新 API）**

| 需求 | 现有 API | 证据 |
|---|---|---|
| `r` 重命名 | `update_title(id, title)` | `store.rs:36`（trait 默认实现 + `sqlite_store.rs:526` 覆盖） |
| `⌫` 永久删除（D4 已定，对齐 Codex） | `delete_thread(id)` → `DELETE FROM threads`（messages 靠 FK CASCADE）**现成可用，零改动** | `sqlite_store.rs:515-524` |
| ~~软删 / archive~~ | `update_meta` 可写 `hidden`，但**本期不用**（`hidden` 列留给未来的 archive） | `store.rs:27`；`hidden` 列 `sqlite_store.rs:178` |
| 状态写入 | `update_thread_status(id, status)` | `store.rs:74`（当前只被子 agent 调用，**C3 已定本期修**） |

取数：去掉 `filter(cwd)`，直接用 `list_threads()`；**只列 root**（`parent_thread_id.is_none()`，子 agent 不单独占行）。`list_threads()` 自带的 `WHERE t.hidden = 0`（`sqlite_store.rs:500`）保持不动——本期没有写入 `hidden = 1` 的路径，条件恒真。

### 4.3 状态映射

Codex 由服务端算（`app-server/src/thread_status.rs:440-469`）：

| 服务端状态 | 分组 | 行内文案 |
|---|---|---|
| `Active` + `WaitingOnApproval` / `WaitingOnUserInput` | Needs you | `Needs input` ● 红 |
| `Active` | Working | `Working` ● 绿 |
| `Idle` | Ready | `Ready` ○ 青 |
| `SystemError` | Needs you | `Error` ! 红 |
| `NotLoaded` | Inactive | `Inactive` ○ 暗 |

root 的分组 = root 与**全部 child 的分组取 `min()`**（`agents_overview.rs:1225-1233`）。

peri 侧对应物：`cc-middlewares/src/subagent/background.rs:7` 的
`BackgroundTaskStatus { Running, AwaitingApproval, Completed, Stopped, Failed }`
与上表**几乎一一对应**，尤其 `AwaitingApproval` 就是 `Needs you`。

**peri 的实际判定（Q3 已拍板）**

| 桶 | 判定依据 | 数据来源 |
|---|---|---|
| Working | 本进程正在跑该会话（内存态，不看库） | 进程内会话状态 |
| Needs you | 本进程该会话有待回答/待审批，或任一子任务处于 `AwaitingApproval`；优先于 Working | 进程内会话/子任务状态 |
| Ready | `agent_status = "active"` **且** `updated_at` 在阈值（24h）内，且不在本进程运行 | `ThreadMeta` |
| Inactive | `agent_status ∈ {done, cancelled, error}`，或 `active` 但 `updated_at` 超阈值 | `ThreadMeta` |

**两条必须先在实现前接受的降级**：

1. 跨进程实时状态**不可得**（peri 无 daemon，见 §5.2 / C1）——只有本进程的会话能显示 Working / Needs you，其余只能按上面的启发式归类。
2. 根会话的 `agent_status` 现在**恒为 `"active"`**（`update_thread_status` 只被子 agent 调用）——**不修则所有历史会话都会显示成 Ready**，这是 C3 要求本期修的原因。

### 4.4 分页（**P0 渲染层切片，P1 真分页** —— Q6 已定）

Codex 是**真 RPC 分页**：`ThreadListParams{ limit: 10, sort_key: RecencyAt }`（`agents_overview_discovery.rs:170-190`），一次只取 10 条，`Show more` 再拉下一页。

peri 的 `ThreadStore::list_threads()`（`cc-agent/src/thread/sqlite_store.rs:498`）是**一次性全量返回**——整个 store 里唯一的 `LIMIT/OFFSET` 在 `load_context` 读消息处（`:551`）。

**决策**：

| 阶段 | 做法 | 效果 |
|---|---|---|
| **P0** | `Show more` 为**渲染层切片**（前 10 / 20 / 30 …），数据已在内存 | 交互与 Codex 一致，但**没有 DB/网络收益** |
| **P1** | trait 加 `list_threads_paged(limit, offset)`（`store.rs` + `sqlite_store.rs` + `filesystem.rs` 两后端），改真分页 | 与 Codex 等价 |

**P1 的性能点**：`THREAD_META_COLUMNS`（`sqlite_store.rs:22`）里每行都带一个 `(SELECT COALESCE(SUM(LENGTH(m.content)), 0) FROM messages m WHERE m.thread_id = t.id)` 子查询来算 `content_size`，跨全部项目全量拉取比现在按 cwd 过滤更重，**改真分页时必须一并把子查询改成 JOIN 聚合**（`LEFT JOIN messages GROUP BY`），否则每页仍在扫全表。（该常量已通过 `NULL as cached_context` 规避了 ~1MB/线程 的内存放大，`:20-21` 有注释。）

## 5. 已知缺口与降级（**必须在实现前接受**）

> 本节是「事实性缺口」（代码里确实没有的能力）；**「需要你拍板的选择题」在 §5.3**，别混读。

## 5.1 A1：跨项目 `enter` —— **已定采用方案 B（对齐 ACP）**（2026-10-15 复核推翻原判）

> **原判（已作废）**：曾把「peri 是一个进程 = 一个 cwd」列为【硬约束】，得出「跨项目只能看不能进」。
> **复核结论**：`cwd` 在 peri 里**本来就是 session / thread 的属性，不是进程的属性**。ACP 层早已把 cwd 参数化了（`AcpSession.cwd`、`new_session(cwd)`、`build_frozen_session_data(…, cwd)`），**系统提示词本来就是按传入的 cwd 构建的**。
> 原判的错误在于：把「TUI 侧没传线程自己的 cwd」误当成「架构不支持」。所以 Q10 不是架构决策，而是**一处参数传错的修复 + 若干 `services.cwd` 消费点跟随切换**。

**原判的依据（仍然成立，但只是 TUI 侧的欠债，不是硬约束）**

| 事实 | 证据 |
|---|---|
| `services.cwd` 只有声明，**全仓没有任何写入点** | `cc-tui/src/app/service_registry.rs:64`（`pub cwd: String`），全局搜索无赋值 |
| `ServiceRegistry` 是**跨会话共享**的 | CLAUDE.md「App 状态拆分为 `ServiceRegistry`（跨会话共享）和 `GlobalUiState`」 |
| `-c` 继续会话时**只找 `t.cwd == 当前进程 cwd` 的线程** | `cc-tui/src/main.rs:719` `threads.into_iter().find(\|t\| t.cwd == cwd)` |
| `open_thread()` **完全不碰 cwd** | `cc-tui/src/app/thread_ops.rs:177-227`：只 load 消息、设 `current_thread_id`、restore pipeline |
| ACP 的 session/list 里 `t.cwd` 只是**上报**，不是切换 | `cc-tui/src/acp_server/requests.rs:497-505` |

**推论（修正版）**：那两个「按 cwd 过滤」是**曾经的正确性要求**——`open_thread` 不会重建系统提示词，直接恢复一个别的 cwd 的会话会让它跑在错误的 cwd + 错误的系统提示词下。**但只要切线程时把 cwd 一起切过去，这个前提就消失了。**

**决议（A1，2026-10-15 拍板）：采用方案 B —— 对齐 ACP 的 per-session cwd**

| 方案 | 内容 | 结论 |
|---|---|---|
| A（保守） | 命令中心能看全部项目，但只有当前项目的行可以 `enter` | ❌ 不采用（本页核心卖点是跨项目续接，只读等于没做） |
| **B（对齐 ACP）** | **切线程时以该线程的 `cwd` 重建会话上下文** —— ACP 层本就如此，TUI 侧对齐即可 | ✅ **采用，排 P1** |

**方案 B 的证据与落点**（2026-10-15 追查）：

| 层 | 事实 | 证据 |
|---|---|---|
| ACP 会话本就自带 cwd | `AcpSession.cwd`，`build_session(session_id, thread_id, cwd)` 参数化 | `cc-acp/src/session/mod.rs:41,149-153` |
| `session/load` 协议已带 cwd | 客户端发 `{ sessionId, cwd, model }` | `cc-tui/src/acp_client/client.rs:275-278` |
| **系统提示词本就按传入 cwd 构建** | `build_system_prompt(None, **cwd**, ...)`、`is_git_repo` 亦按此 cwd | `cc-acp/src/session/frozen.rs:96-105` |
| **唯一堵点** | `open_thread()` 传的是 `services.cwd`（进程的），**不是 `meta.cwd`（线程自己的）** | `cc-tui/src/app/thread_ops.rs:234` |

即：**分层早就对了，只有 TUI 侧没把 cwd 换成"当前线程的 cwd"**。

**方案 B 的真实成本清单（2026-10-15 全仓枚举 `services.cwd`，共约 20 处消费点）**

| 类别 | 位置 | 不跟随的后果 |
|---|---|---|
| Agent 构建 | `agent_ops/subagent.rs:89,245`、`agent_submit.rs:254`、`agent_ops/polling.rs:247`、`agent_compact.rs:130`、`agent_shell_executor.rs:112` | **子 agent / 压缩 / shell 在错的项目里跑**（最严重） |
| 渲染 | `agent_ops/acp_bridge.rs:43`（`map_executor_event` 传 cwd）、`ui/message_view/mod.rs:1056` | 相对路径与文件链接显示错乱 |
| 状态栏 | `ui/main_ui/status_bar.rs:187,190` | 项目名/分支显示错项目 |
| `@` 提及 | `event/keyboard.rs:192` → `at_mention::ensure_cwd`（`at_mention/mod.rs:69-78`） | 搜索根还是旧项目，补全失效 |
| Hooks / MCP | `main.rs:860`（`settings.local` hooks）、`command/core/doctor.rs:73`（`.mcp.json`） | 加载错项目的 hook 与 MCP 配置 |
| 会话启动/续接 | `main.rs:715`（`-c` 匹配）、`:880`、`:1110`、`:1113` | 见下表「`-c` 语义」 |
| ACP 会话 | `acp_server/prompt.rs:98`（`state.cwd`） | 已被 ACP 层正确处理（无需改） |

**推荐做法：不要逐点改，把 `services.cwd` 降级为"派生值"**——保留一个真相源（当前会话的 cwd），进程启动时初始化为启动目录，打开某个线程后变为该线程的 `meta.cwd`。这样上表 20 处**一处都不用动**，且后续新增的消费点自动正确。逐点判断「跟线程还是跟进程」是**容易漏、且漏了就是隐性 bug** 的做法，只在无法引入真相源时才退而求其次。

**方案 B 的剩余风险（需求层，需在实现前接受）**

| 风险 | 说明 | 建议 |
|---|---|---|
| 切走再切回 = **两次重建会话上下文** | 系统提示词、auto-memory 目录、git 状态都要重算，会话页会有一帧「重载中」 | 切换时给明确的 loading 态，别静默 |
| `-c` / `--continue` 语义要不要跟着变 | 现在只匹配当前进程 cwd 的线程 | 建议**不改**，`-c` 保持「当前项目」语义，跨项目只从命令中心进 |
| 相对路径渲染 | 切到别的项目后，会话页里历史消息中的相对路径会以新 cwd 为基准显示 | 历史消息保留原始 cwd 记录（或直接显示绝对路径） |
| 真相源改造的回归面 | 上表近 20 处消费点会**同时**改变行为（hooks / `.mcp.json` 加载、doctor、subagent、状态栏、`@` 提及） | 需要一轮回归清单；这正是「不逐点改」的代价，但比漏点安全 |

**方案 A 与 B 的取舍建议**：B 的成本集中在「一次性把 cwd 从进程属性降级为线程属性」，收益是本页的核心卖点（跨项目续接）真正成立；A 可以让 P0 先落地，把 B 放到 P1，代价是 P0 的用户会看到「看得到进不去」，需要在 UI 上明确解释。

**`n`（新建会话）用哪个 cwd**：**A2 已定 —— 用「选中行的 cwd」**（对齐 Codex，覆盖原 Q9 的「当前 cwd」）。方案 B 落地后每个线程本就带自己的 cwd，所以在别的项目下起新会话是自然能力。

## 5.2 其它已知缺口

| 缺口 | 事实 | 本期处理 |
|---|---|---|
| **跨项目实时状态不可得** | `update_thread_status` **只对子 agent 线程调用**（`cc-middlewares/src/subagent/tool/execute_bg.rs:161/179`），根线程恒停在默认 `"active"`；且 peri 无 daemon，别的进程的运行态本进程看不见 | 降级：**只有本进程当前会话**能实时显示 `Working / Needs you`，其余按 `agent_status` + `updated_at` 判定（见 §4.3） |
| **状态栏不跟随选中行** | 状态栏三行读的全是**当前进程活动会话的内存态**：`AgentComm.session_token_tracker`（`agent_comm.rs:85`）、`AgentComm.recent_tools`（`:134`）、`ChatSession.todo_items / background_agents`（`chat_session.rs:38-39`） | 本页**不带状态栏**（整页自包含），不存在误读 |
| **Model / Reasoning 无数据源** | `ThreadMeta.config` 这个 JSON 快照字段**全仓从未被写入**（`ThreadMeta::new` 恒为 `None`） | P1 新增独立列（**Q8 已决**） |
| **Branch 无数据源** | `ThreadMeta` 不存分支；`git_branch_cache`（`service_registry.rs:85`）只是当前 cwd 的进程级缓存 | P1 新增独立列（**Q4 已决**），写入时机 = 会话创建/更新时对自身 cwd 探测 |
| **Last message 需读消息** | `ThreadMeta` 无 preview 字段（Codex 的 `Thread.preview` peri 没有） | 详情栏选中行**按需** `load_messages()` 取末条，不预取（Q7） |
| **重命名 / 删除的「现成用法」** | `update_title`（`store.rs:36`）与 `delete_thread`（硬删）都已实现，但 TUI 侧从未调用过；`hidden` 列仍在等 archive | 直接复用既有 API（§4.2「关联能力」表），不新增方法 |
| **`Show more` 在 P0 是假分页** | `list_threads()` 全量返回（`sqlite_store.rs:498`） | P0 渲染层切片；P1 改真分页（Q6，见 §4.4） |

## 5.3 需求问题清单（A1–A4 / C3 / D4 已定，其余 15 条按建议默认执行）

> 2026-10-15 已拍板 6 条（标 ✅ 已定）；其余按「建议默认」执行，**若不同意任何一条直接说编号**。
> 这些不是「mock 怎么画」的问题，而是**做下去必须有个答案、且答案会改变范围 / 数据模型 / 交互**的问题。

**A. 范围与破坏性变更**

| # | 问题 / 结论 | 现状 / 证据 | 建议默认 |
|---|---|---|---|
| A1 | ✅ **已定**：采用方案 B（对齐 ACP 的 per-session cwd），排 P1 | §5.1 | — |
| A2 | ✅ **已定**：`n` 用**选中行的 cwd**（对齐 Codex），提示行写出目标项目 | Q9 原定「当前 cwd」；Codex 用「选中行 cwd」 | — |
| A3 | ✅ **已定**：原「选 agent 定义」改名 **`/agent-design`**，`/agents` 让给本页；CHANGELOG 标注，不保留冲突的同名 alias | §3.1 | — |
| A4 | ✅ **已定**：`/tasks` **入口屏蔽**（从命令注册表移除），代码本 PR 内**保留**，验收通过后单独提清理 PR | `/tasks` 按 cwd 过滤、状态写死 `"done"`、详情栏恒 `(Loading messages...)` | — |

**B. 详情栏的数据源**

| # | 问题 / 结论 | 现状 / 证据 | 建议默认 |
|---|---|---|---|
| B1 | Model / Reasoning / Branch 对**历史会话**没有值（新列从今天起才有），显示什么？ | `ThreadMeta.config` 恒 `NULL`；新列只能回填不到 | 缺失显示 `(unknown)`，**不隐藏行**（保持三列对齐） |
| B2 | Branch 是**创建时快照**还是**每次读取时重探**？ | `git_branch_cache` 只是当前 cwd 的进程级缓存 | 快照 + 会话更新时刷新（跨项目重探需进目录，成本高） |
| B3 | Last message 按需 `load_messages()` 的加载态与跨项目成本？ | `ThreadMeta` 无 preview；大库上单线程 load 可能几十 ms | 详情栏显示 `(加载中…)`，选中行防抖 150ms |

**C. 状态桶语义（peri 无 daemon，这是最大的语义缺口）**

| # | 问题 / 结论 | 现状 / 证据 | 建议默认 |
|---|---|---|---|
| C1 | 别的进程的运行态**永远看不到**，接受降级吗？还是增设第 6 档 `Unknown`？ | `BackgroundTaskStatus` 的 `AwaitingApproval` = Needs you，但只对本进程可见 | 接受降级，**不增设第 6 档**（Codex 也是 5 档），但详情栏用一行小字说明 |
| C2 | Inactive 的 **24h** 阈值做成配置项吗？ | Q3 已定「`agent_status` ∈ done/cancelled/error → Inactive；`active` 且不在本进程 → 看 `updated_at` 24h」 | 先硬编码 24h，留 TODO，不进配置 |
| C3 | ✅ **已定：本期修** —— 根会话在轮次开始/结束写 `agent_status` | 现状唯一调用点是子 agent（`execute_bg.rs:161/179`），根会话恒 `"active"` | 不修则所有关掉的会话都显示 Ready |

**D. 交互细节（容易实现错的地方）**

| # | 问题 / 结论 | 现状 / 证据 | 建议默认 |
|---|---|---|---|
| D1 | `←` 的 10 条门控里，「`AppServerTarget::LocalDaemon`」在 peri 对应什么？ | peri 无 daemon 概念；`←` 目前绑 `move_left` | 把该条映射为「本进程可用」，其余 9 条照搬 |
| D2 | `Esc` 的层级优先级（帮助 → 重命名 → 搜索 → 中心返回会话）需要一份明确顺序 | mock 里已修过一个「会话页吃掉帮助层 Esc」的 bug | 按上述顺序，帮助层**永远最先** |
| D3 | `o` resume 与选中行**无关**（对齐 Codex），但 peri 已有 `/history`——两者关系？ | `keymap.rs:1949-1963`：`o` → `OpenResumePicker` | `o` = 开 `/history` 的同一选择器；**本页内不做「resume 选中行」** |
| D4 | ✅ **已定：硬删（永久删除），对齐 Codex** | 确认浮层必须明确不可恢复，且要求显式确认；软归档本期不做 | 新增 `delete_thread_tree(id)` 删除根会话与隐藏子孙及消息，保留原 `delete_thread` 的单行语义；确认浮层固定显示「不可恢复」，仅 `y` 执行 |
| D5 | `r` 重命名写哪、是否同步 `/history`？ | `ThreadMeta.title` 可写 | 写 `title`，`/history` 自动同步（同一数据源） |

**E. 性能与迁移**

| # | 问题 / 结论 | 现状 / 证据 | 建议默认 |
|---|---|---|---|
| E1 | 去掉 cwd 过滤后是全量拉取，真分页本期做吗？ | Q6 已定做（`list_threads_paged`）；`THREAD_META_COLUMNS` 每行带 `SUM(LENGTH(content))` 子查询 | 确认本期做，**分页 + 子查询改为 JOIN 聚合** |
| E2 | sqlite 新增 3 列的老库迁移策略与版本号？ | 既有 `alter_columns` 列表（`sqlite_store.rs:174-191`）：`ALTER TABLE … ADD COLUMN` + 忽略 `duplicate column name` 实现幂等 | 沿用同一模式追加 3 条，**不加 schema 版本号**（与现状一致） |
| E3 | `/` 搜索搜什么范围？ | Codex 只过滤已加载列表 | 搜 `title`（不搜消息内容，避免跨库扫） |

**F. 信息架构（首次打开的第一印象）**

| # | 问题 / 结论 | 现状 / 证据 | 建议默认 |
|---|---|---|---|
| F1 | 项目分组名用 **full path** 还是 repo 名？ | mock 用 full path；同名 repo 会撞车 | full path（可折叠显示尾部） |
| F2 | 怎么标出「当前会话」？ | mock 用 `❯` 光标 + 状态点；真实实现需比对 `current_thread_id` | 当前会话行加 `●` 与 `(current)` 后缀 |
| F3 | 按 recency 排序时，**首屏可能只有当前项目**（真实数据里 30/42 条都是 peri） | 本 mock 实测：要按 4 次 `Show more` 才翻出第二个项目 | 计数行后加提示「其它项目 12 条 · 按 g 分组」，避免用户以为只有本项目 |

## 6. 分期

| 阶段 | 内容 | 前置决策 |
|---|---|---|
| **P0** | 全页视图骨架（渲染早退 + 按键 stage）、去掉 cwd 过滤、按 `cwd` 分组 + 组头计数、五档过滤与计数、`↑↓` 光标（含滚入视口）、渲染层分页切片、五档状态启发式、详情栏（Project / Prompt / Last message）、`?` 浮层、`/agents` 命令入口、`/tasks` 入口屏蔽（代码保留） | A4 |
| **P1** | ①数据层：`ThreadMeta` 新增**独立列** `model` / `reasoning_effort` / `branch`（迁移 + 列常量 + `row_to_meta` + 全部 `create_thread` 调用点写入），详情栏三行接上真实数据；②C3：根会话轮次开始/结束写 `agent_status`；③真分页 `list_threads_paged` + 子查询改 JOIN 聚合；④**方案 B 跨项目进入**：`open_thread()` 传 `meta.cwd` + `services.cwd` 降级为派生值（20 处消费点自动跟随）+ `n` 用选中行 cwd | A1、A2、C3、E1、E2、B1–B3 |
| **P2** | `←` 全局键与 10 条门控（**入口不能晚于 P0，P0 未做则命令是唯一入口**）、原 `/agents` → **`/agent-design`** 改名、删除确认浮层 / 重命名 / 搜索的实现口径、窄屏降级 | A3、D1–D5、E3 |
| **收尾（独立 PR）** | 删除 `/tasks` 的保留代码（A4 的第二半），清理 `#[allow(dead_code)]` | A4 |

## 7. 涉及文件

| 文件 | 改动 | 阶段 |
|---|---|---|
| 新增：`cc-tui/src/app/agent_center/` | 视图状态、渲染、键位、分组、选择 | P0 |
| `cc-tui/src/ui/main_ui/mod.rs:32` | 新增全页视图早退分支 | P0 |
| `cc-tui/src/event/keyboard.rs:96` | 新增专属 key stage（在 Stage 1 之前，命中即 return） | P0 |
| `cc-tui/locales/{zh-CN,en}/main.ftl` | 新增文案（两份都要加，**本仓无 en/zh 一致性测试**） | P0 |
| `cc-tui/src/app/panel_plugin.rs:404-424` + `cc-tui/src/command/panel/tasks.rs:14` | **A4（入口屏蔽）**：从命令注册表移除 `/tasks`；`open_tasks_panel()` 及其面板代码**本 PR 内保留**（不删、不重写），加 `#[allow(dead_code)]` 以免 lint 报错，验收通过后单独提清理 PR | P0 |
| `cc-agent/src/thread/types.rs` | **P1**：`ThreadMeta` 加 `model` / `reasoning_effort` / `branch` 独立列 | P1 |
| `cc-agent/src/thread/sqlite_store.rs:176+` | **P1**：`ALTER TABLE` 迁移（既有幂等模式）+ 列常量 + `row_to_meta` + `list_threads_paged` + 子查询改 JOIN 聚合 | P1 |
| `cc-agent/src/thread/store.rs:30` | **P1**：trait 加 `list_threads_paged(limit, offset)` | P1 |
| `cc-agent/src/thread/filesystem.rs` | **P1**：同步实现新 trait 方法 | P1 |
| 根会话状态写入点（`cc-acp/src/session/executor.rs::execute_prompt()` 首尾 **或** `cc-tui/src/app/agent_submit.rs`，实现时二选一定位） | **P1 / C3（已定修）**：轮次开始/结束写 `agent_status`（当前唯一调用点只有 `execute_bg.rs:161,179` 的子 agent） | P1 |
| `cc-tui/src/command/panel/agents.rs:19` | **P2**：让出 `/agents`，改名 **`/agent-design`**（A3 已定） | P2 |
| `cc-tui/src/app/thread_ops.rs:234` | **P1（方案 B，A1 已定）**：`open_thread()` 改传 `meta.cwd`；`:459`（`/history`）保持按 cwd 过滤不变 | P1 |
| `cc-tui/src/app/service_registry.rs:64` | **P1（方案 B，A1 已定）**：`cwd` 降级为「跟随当前会话的派生值」（见 §5.1 成本清单） | P1 |
| 文档：`CHANGELOG.md` / `README.md` + `README_ZH.md` / `TUI-STYLE.md` / `spec/global/features.md` | 命令让渡（`/agents` → `/agent-design`）与新增 `←` 快捷键必须同步（按 CLAUDE.md 文档地图；两版 README 特性表一一对应） | P2 |

**不改**：`PanelManager`（不新增 `PanelKind`，见 §4.1）。`/history`（保留单项目语义）。

## 8. 验收标准

1. 跨项目**可见**：能同时看到 ≥3 个不同 `cwd` 的会话，按项目分组、组头带计数（full path 分组名）
2. 五档过滤计数与列表一致；切 tab 不发请求
3. `↑↓` 光标移动时选中行**始终在视口内**（长列表可滚动）
4. `Enter` 进入会话后本页关闭；`←`（满足 §3.5 全部条件时）能重新打开
5. **跨项目可进入（A1 = 方案 B）**：`Enter` 别的项目的行能真正切过去，且切换后系统提示词 / 相对路径 / `@` 提及 / 状态栏 / 子 agent 构建都按**该线程的 cwd**，不是启动目录
6. `o` 打开 resume 选择器（与选中行无关）；`n` 在**选中行的项目**下起新会话（不插行），提示行写出目标项目
7. 详情栏显示 `Model / Reasoning / Project / Branch / Last message / Prompt`；历史会话缺失值显示 `Unknown`，三行不塌陷
8. 全页**只有一条横线**，页脚上方无线（对齐 §3.2）
9. `?` 能开能关，`esc` 也能关（**任何视图下**；层级优先级见 D2）
10. 状态桶降级可解释：本进程以外的会话**不会**被显示成 Working/Needs you（C1），且关掉的会话不显示 Ready（C3 修好前此项必挂）
11. 删除为**永久删除**（D4 已定）：`⌫` 必须经二次确认浮层，确认文案含「不可恢复」；确认后该行与其消息一并消失
12. 当前会话行有可见标记（F2）；首屏只有当前项目时给出「其它项目 N 条」提示（F3）
13. `/tasks` 已从命令面板消失（A4），`/agent-design` 可用（A3），`/agents` 打开本页

## 9. 测试用例

- `test_center_lists_threads_across_all_cwds`：3 个不同 cwd 各 ≥1 会话，全部出现在列表中
- `test_center_groups_by_cwd_and_counts`：组头计数等于该组行数
- `test_center_filter_tabs_update_counts`：切到 `Working` 只留 Working 行，且计数行不变（计数是全量）
- `test_center_cursor_stays_visible_while_scrolling`：展开 >1 页后连续 `↓`，选中行不越出视口
- `test_center_show_more_loads_next_page`：`Show more` 上按 `enter` 后行数 +10
- `test_center_show_more_cursor_reachable_by_down`：连续 `↓` 能停在 `Show more` 上（对齐 Codex 的光标可达性）
- `test_center_status_bucket_maps_agent_status`：`done`/`cancelled`/`error` → Inactive；`active` 且 25h 前 → Inactive；`active` 且 1h 前 → Ready
- `test_center_detail_shows_unknown_for_legacy_threads`：无 `model`/`branch` 列值的旧会话不 panic、显示 `Unknown`
- `test_agents_key_requires_empty_draft`：草稿非空时 `←` 不打开本页
- `test_agents_key_ignores_key_repeat`：`KeyEventKind::Repeat` 不触发（§3.5 第 10 条）
- `test_agents_key_toggles_help_and_esc_closes`：`?` 开关、`esc` 关闭（**含会话页路径**）
- `test_enter_closes_center_and_left_reopens`：`enter` 后本页不在视图栈中，`←` 可重开
- `test_enter_on_other_project_row_switches_cwd`：别的项目的行 `Enter` 后当前会话的 cwd 变为该行的 cwd（A1 = 方案 B）
- `test_open_thread_uses_thread_cwd_not_process_cwd`：`open_thread(meta)` 后 `services.cwd`（或其派生值）等于 `meta.cwd`
- `test_n_creates_session_in_selected_row_cwd`：A2——`n` 起的空会话 cwd 等于选中行的 cwd
- `test_delete_thread_confirmed_removes_row_and_messages`：D4——确认后 `list_threads()` 不含该行、`load_meta()` 报错（真删）；未确认则不变
- `test_tasks_command_not_registered`：A4——命令注册表中不存在 `/tasks`，但 `/agent-design` 与 `/agents` 存在

## 10. 决策记录（2026-10-15 拍板；Q10 已作废重议）

| # | 问题 | 决策 |
|---|---|---|
| Q1 | `/agents` 让渡后，原「选 agent 定义」叫什么 | **修订（A3，2026-10-15）：`/agent-design`**（原拟 `/agent-def`，adim 定为「agent 设计」） |
| Q2 | `← N agents` 的 N 数什么 | **当前项目（当前会话所在 `cwd`）的会话数** —— 在 peri 项目里就数 peri 有多少个对话 |
| Q3 | `Inactive` 判定 | **两者结合**：`agent_status` 为 `done`/`cancelled`/`error` → Inactive；`active` 且不在本进程 → 再看 `updated_at` 超过阈值（24h）→ Inactive，否则 Ready |
| Q4 | `Branch` 来源 | **落库**（P1 加独立列），写入时机 = 会话创建/更新时对自身 `cwd` 探测 git |
| Q5 | 分组偏好是否持久化 | **不持久化**，只存内存，每次默认 `Project` |
| Q6 | 是否做真分页 | **顺手做真分页**：给 `ThreadStore` 加 `list_threads_paged(limit, offset)`（trait + sqlite/filesystem 两个后端） |
| Q7 | `Last message` 加载时机 | 选中行**按需** `load_messages()`，不预取 |
| Q8 | Model / Reasoning / Branch 的存储形态 | **独立列**（`model` / `reasoning_effort` / `branch`），不用 `config` JSON —— 因列表页要按 Model 分组，列比 JSON 划算 |
| Q9 | `n` 新建会话用哪个 `cwd` | **修订（A2，2026-10-15）：用「选中行的 cwd」**（对齐 Codex）。原「当前 cwd」的结论建立在已作废的「单进程单 cwd」判断上 |
| Q10 | 跨项目能否 `enter` 进入 | **已定（A1，2026-10-15）：采用方案 B** —— 对齐 ACP 的 per-session cwd。原【硬约束】判定作废（详见 §5.1）；落地排 P1 |

## 11. 决议索引

| 编号 | 决议 |
|---|---|
| A1 | 方案 B（对齐 ACP 的 per-session cwd），P1 落地 |
| A2 | `n` 用选中行的 cwd（对齐 Codex） |
| A3 | 原「选 agent 定义」改名 `/agent-design`，`/agents` 归本页 |
| A4 | `/tasks` 入口屏蔽、代码保留，验收后单独清理 |
| C3 | 本期修根会话 `agent_status` 写入 |
| D4 | 删除为永久删除 + 二次确认（对齐 Codex） |

其余 15 条（B1–B3 / C1–C2 / D1–D3、D5 / E1–E3 / F1–F3）按 §5.3 的建议默认执行。
