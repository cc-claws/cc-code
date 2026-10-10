# 状态栏第二行「最近工具」显示改造 — 设计

- 日期：2026-10-09
- 分支：`feat/statusbar-recent-tools`
- 范围：`cc-tui` 状态栏第二行（activity 行）的左侧工具段
- 状态：设计已定，待实现

## 一、背景与问题

状态栏第二行的运行中工具段（`◐ Name : 摘要`）存在四个问题：

1. **快工具根本看不到**。`poll_agent` 每帧把 ACP 通知一次 drain 干净（`cc-tui/src/app/agent_ops/polling.rs:68-91`），
   Read/Glob 这类毫秒级工具的 ToolStart + ToolEnd 常落在同一帧，`◐ Read : x.rs` 一帧都没渲染过。
2. **只有 11 个工具带摘要**。`format_tool_args` 是白名单 match（`cc-tui/src/app/tool_display.rs:42-84`），
   其余工具 `_ => None` → 摘要是空串 → 只显示裸名字（Agent/TodoWrite/AskUserQuestion/MCP/Cron 都中招）。
3. **顺序是「老在左」**。`render_second_row` 取 `running_tools[len-2..]` 正序遍历
   （`cc-tui/src/ui/main_ui/status_bar.rs:113-119`），最新的排在最右。
4. **没有 `…` 收口**。整行超宽时 `Paragraph` 无 `.wrap()`，右端被静默裁掉
   （`render_truncated_line`，`status_bar.rs:749-767`）。

补充事实（**本次不改**，仅记录）：

- 消息区已有「按可用宽度动态分配 + `…` 收口」的实现：`header_args_width`（`message_render.rs:75-86`，取可用宽度的 16/19、下限 32 列）
  + `tool_args_header`（`:574-584`）。这是消息区的能力，状态栏没有。
- 消息区工具块按工具分色：`tool_color()`（`cc-tui/src/ui/message_view/tools.rs:123-132`）。状态栏不分色。
- `spec/prd-shell-status-indicator.md` 写消息区运行中指示器是「黄色闪烁」，实际实现是灰色（`cc-widgets/src/tool_call/display.rs:13-17`）。既有文档/实现不一致。

## 二、目标行为（已与用户逐条确认）

第二行**永远单行，绝不折行**。布局：

```
[ 最近工具组：≤2 条，最新在左 ] | [ 聚合统计：≤4 条 + more ]
```

| 规则 | 内容 |
|------|------|
| 条数 | 最近工具组最多 2 条 |
| 顺序 | **最新在最左**，旧的往右排 |
| 状态图标 | 运行中 `◐`；已完成（停留期内）`✓` |
| 颜色 | `◐` 统一黄、`✓` 统一绿、工具名统一青、摘要 dim。**不跟工具身份色走**（与消息区 `tool_color()` 的规则无关） |
| 摘要宽度 | 沿用路径语义 `truncate_tool_target`（`status_bar.rs:696-714`），上限从 `TOOL_TARGET_MAX_LEN` **20 改为 30**（`status_bar.rs:18`）。**首版值 30，后续按真实用户反馈再调**。实测（cap 30）：`sleep 15` → `sleep 15`；`src/ui/main_ui/status_bar.rs`（28）→ 完整；`tool_dispatch_test.rs:1580-1599`（31）→ `tool_dispatch_test.rs:1580-...`；`cd /d/code/peri && cargo test -p cc-tui`（39）→ `.../peri && cargo test -p cc-tui` |
| 上游截断 | 继承：args 先被截到 40 字符（`agent_ops/mod.rs:201-204`） |
| 最短可见 | 条目显示时长 = `max(实际执行时长, 300ms)`。快工具被兜底到 300ms，慢工具执行结束立刻消失（不额外停留） |
| 完成语义 | 工具结束**不生成带摘要的完成条目**；该条从左侧消失，右侧 `✓ Name ×N` 对应工具 +1（纯数字） |
| 摘要覆盖 | **所有工具都要有**：白名单补齐为「白名单 + 通用兜底」 |
| 超宽处理 | 整行超宽 → 用 `truncate_to_display_width`（`message_render.rs:550`）在行尾 `…` 收口。**不做优先级丢弃** |

时序（`MIN_VISIBLE = 300ms`）：

```
t0      ToolStart Bash(sleep 15)   ◐ Bash : sleep 15                    | ✓ Bash ×21 | ...
t1      ToolStart Read             ◐ Read : tool_dispatch_… | ◐ Bash : sleep 15 | ...
t1+8ms  ToolEnd Read               同左（条目兜到 300ms），✓ Read ×13 立即 +1
t1+300ms 停留到点                   ◐ Bash : sleep 15                    | ✓ Read ×13 | ...
t16     ToolEnd Bash               （执行 >300ms，立即消失）             | ✓ Bash ×22 | ...
```

## 三、实现设计

### 3.1 数据结构

`cc-tui/src/app/agent_comm.rs` 新增（`running_tools` 保持原样，它同时驱动 terminal title 和 HUD 高度）：

```rust
/// 状态栏「最近工具」显示条目（仅用于第二行渲染）
pub struct RecentToolEntry {
    pub tool_call_id: String,
    pub display: String,
    pub args_summary: String,
    pub running: bool,
    /// ToolStart 时刻，用于计算最短可见时长
    pub started_at: Instant,
    /// 停留截止时刻：运行中为 None；结束后 = max(结束时刻, started_at + TOOL_MIN_VISIBLE_MS)
    pub visible_until: Option<Instant>,
}

// AgentComm
pub recent_tools: VecDeque<RecentToolEntry>,   // push_front，容量 2
```

常量与结构体同放 `agent_comm.rs`（`agent_ops` 与 `status_bar` 都要用）：
`RECENT_TOOLS_MAX_VISIBLE: usize = 2`、`TOOL_MIN_VISIBLE_MS: u64 = 300`。

### 3.2 事件写入点（`cc-tui/src/app/agent_ops/mod.rs`）

- `ToolStart`（现 `:188-241`）：`push_front(RecentToolEntry{running:true, ..})` 后 `truncate(RECENT_TOOLS_MAX_VISIBLE)`。
- `ToolEnd`（现 `:242-290`）：按 `tool_call_id` 命中后置 `running=false`，
  `visible_until = Some(max(now, started_at + TOOL_MIN_VISIBLE_MS))`。**不移动位置**。
- 会话切换/清理点同步清空：`thread_ops.rs:164`、`agent_ops/lifecycle.rs:32`、`lifecycle.rs:187`。
- 清空条件：清空 `running_tools` 的地方一律同步清 `recent_tools`。

### 3.3 渲染（`status_bar.rs`）

- `render_second_row`：把现在遍历 `running_tools` 的循环换成遍历 `recent_tools`（front→back 即最新→最旧）；
  渲染前过滤：`running == true` 或 `now < visible_until`。
- 条目形态：`◐/✓ + " " + display + " : " + truncate_tool_target(args_summary, 30)`；
  `args_summary` 为空时省略 ` : ` 部分（与现状一致）。
- `has_hud_activity`（`:56-63`）：增加 `|| !agent.recent_tools.is_empty()`（避免停留期内第二行被折叠掉）。
- `render_truncated_line`：第二行 left_spans 拼接后按 `area.width` 走 `truncate_to_display_width` + `…`。

### 3.4 摘要覆盖（`cc-tui/src/app/tool_display.rs`）

`format_tool_args` 保留现有 11 个分支，末尾 `_ =>` 改为通用兜底，字段优先级：

| 输入字段 | 结果 |
|----------|------|
| `AskUserQuestion` | `questions[0].question`（取不到则 `header`） |
| `Agent` | `description`（取不到则 `prompt`） |
| `TodoWrite` | `N 项任务`（`todos` 数组长度） |
| 其他（含 MCP / Cron 等） | 按 key 顺序取 `input` 里第一个非空字符串值 |

结果统一截 40 字符，与白名单分支一致。

### 3.5 不做（YAGNI）

- 不改 `truncate_tool_target` 的路径式截断语义，只把上限 20 → 30。
- 首版不按显示列宽（unicode-width）算摘要上限，仍按字符数；中文摘要的列宽问题留待反馈。
- 已知既有怪癖不改：`.../末段` 分支不受上限约束（cap 30 时最多输出 31 字符）。
- 不改消息区 `tool_color()` 的分色规则，不改消息区 `header_args_width` 分配。
- 不缩短 MCP 工具名（`mcp__context7__query-docs` → `McpContext7Query-docs` 现状保留）。
- 不做「超宽按优先级丢弃聚合段」的复杂策略。
- 不改第一行、第三行。

## 四、边界与风险

- **SubAgent 工具调用**：现在没有按 `source_agent_id` 过滤，子代理的工具也会进入这两个列表。本次保持该行为不变。
- **并发**：同批并发工具按 ToolStart 到达顺序 push_front，最晚开始的在最左。
- **长工具被挤出窗口**：容量 2 是硬上限，正在运行的长工具可能被两个更新的工具挤出显示（不影响执行）。已与用户确认接受。
- **CMD 宽度**：`✓`/`×` 在 CP936 传统控制台下按 2 列物理推进，与 `…` 收口叠加时需按 unicode-width 计算（沿用 `truncate_to_display_width` 即满足）。

## 五、测试（已实现，命名全英文 `test_<对象>_<场景>`）

`cc-tui/src/app/agent_comm_test.rs`（`recent_tools` 生命周期，不依赖 App）：

1. `test_push_recent_tool_keeps_newest_first` — 最新在最前。
2. `test_push_recent_tool_truncates_to_max_visible` — 第 3 条挤掉最老。
3. `test_finish_recent_tool_holds_fast_tool_until_min_visible` — 8ms 结束的条目垫到 300ms。
4. `test_finish_recent_tool_removes_slow_tool_right_after_end` — 执行超 300ms 结束时立即不可见。
5. `test_visible_recent_tools_filters_expired_and_keeps_running` — 过期条目过滤、运行中保留。
6. `test_finish_recent_tool_ignores_unknown_id` — 未知 id 不改变状态。

`cc-tui/src/ui/main_ui/status_bar.rs`（渲染与截断）：

7. `test_render_recent_tool_segment_matches_codebuddy_hud` — `◐ Read : path` 形态与配色。
8. `test_render_recent_tool_segment_uses_completed_glyph_after_finish` — 完成后转绿色 `✓`。
9. `test_render_recent_tool_segment_omits_args_when_empty` — 无摘要时省略 ` : `。
10. `test_render_recent_tool_segment_colors_are_uniform_across_tools` — 不同工具颜色一致。
11. `test_truncate_tool_target_uses_char_count_cap` — 30 字符上限的截断结果。
12. `test_truncate_spans_to_width_adds_ellipsis_when_narrow` — 窄宽度行尾 `…` 且总宽不超限。
13. `test_truncate_spans_to_width_keeps_line_when_fits` — 放得下就不动。
14. `test_truncate_spans_to_width_counts_cjk_as_two_columns` — CJK 按 2 列计。

`cc-tui/src/ui/main_ui/main_ui_test.rs`（受停留影响的既有测试）：

15. `test_status_area_clears_long_agent_shell_text_after_tool_finishes` — 已更新：结束后停留期内仍显示摘要，停留过期后必须清干净。

## 六、涉及文件

| 文件 | 改动 |
|------|------|
| `cc-tui/src/app/agent_comm.rs` | 新增 `RecentToolEntry` + `recent_tools` 字段/初始化 + 生命周期方法；测试入口 |
| `cc-tui/src/app/agent_comm_test.rs` | 新增测试文件 |
| `cc-tui/src/app/agent_ops/mod.rs` | ToolStart push_front；ToolEnd 标记完成 + visible_until |
| `cc-tui/src/app/agent_ops/lifecycle.rs` | 清空点同步（2 处） |
| `cc-tui/src/app/thread_ops.rs` | 清空点同步 |
| `cc-tui/src/app/mod.rs` | 导出 `RecentToolEntry` |
| `cc-tui/src/app/tool_display.rs` | `format_tool_args` 通用兜底（AskUserQuestion / Agent / TodoWrite / 其他） |
| `cc-tui/src/ui/main_ui/status_bar.rs` | 渲染改读 `recent_tools`；停留过滤；`…` 收口；30 字符上限；测试 |
| `cc-tui/src/ui/main_ui/main_ui_test.rs` | 更新受停留影响的既有测试 |

