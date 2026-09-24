# TUI 消息区滚动条拖拽手感异常（点击跳转与拖拽换算不一致、滑块缩为 1 格、hover 无轨道）

**状态**：Open
**优先级**：中
**创建日期**：2026-09-23
**GitHub Issue**：#232（https://github.com/cc-claws/cc-code/issues/232）
**关联**：#230（GitHub）／`spec/issues/2026-09-23-tui-mouse-scroll-stutter-large-content.md`
**分支**：`fix/hover-scrollbar-input`

---

⚠️ **2026-09-23 定性说明（请先读）**

本文档区分两类内容：

**A. 代码事实（可独立验证，建议作为修复依据）**

1. 点击轨道跳转与拖拽使用两套不同的换算分母（`bar_area.height - 1` vs `bar_area.height - thumb_area.height`）
2. 消息区滚动条未设置 `viewport_content_length`（面板路径 `peri-widgets/src/scrollable.rs` 设置了）
3. 渲染流程中存在 `if metrics.is_none() { dragging = false }`，即渲染函数修改事件状态

**B. 影响推断（未在真实环境验证）**

- 「点击轨道后滑块不落在鼠标行」「长内容下滑块被压成 1 格」「拖拽过程中可能被中断」等结论，均由 agent 依据 ratatui 源码公式推导得出，**从未在真实终端中观测过**

**关键澄清**

用户的原始反馈只有一句：「滚动条拖拽有点怪、不丝滑」。本文档「症状详情」中列出的具体症状（偏移行数、错位不自我纠正、拖动中断、滑块高度、命中难度、无轨道）**全部是 agent 推算，不是用户描述**；且用户所说的「不丝滑」偏向流畅度，与本文档的准确性/尺寸类结论**是否为同一问题尚未确认**。

**建议**：以 A 类为修复依据；B 类结论需在真实 TUI 中复现确认后再采信。

---

## 问题描述

消息区右侧滚动条（hover 唤出的 `█` 滑块）用鼠标拖拽时手感"怪、不丝滑"。核心表现不是掉帧，而是**滑块与鼠标指针之间始终存在一段固定偏移**：点击轨道空白处后滑块跳到鼠标上方/下方若干行，随后整段拖拽过程都保持这个错位，滑块不像"黏在鼠标下"。此外长内容下滑块缩小到只剩 1 个字符高，几乎看不见也难抓。滚轮路径（#230 主症状）本身不在本文档范围。

## 症状详情（⚠️ 以下均为 agent 推算，非用户描述）

| # | 现象 | 观察条件 |
|---|------|----------|
| 1 | 点击轨道空白处，滑块不落在鼠标所在行，偏移约 3~7 行 | 内容刚超一两屏时最明显 |
| 2 | 点击轨道跳转后继续拖拽，滑块与鼠标之间的错位全程不变、不会自我纠正 | 必现（只要起点有偏移） |
| 3 | 拖拽过程中画面"抽一下"或鼠标仍按着但已无法拖动 | agent 正在输出（spinner 行增减）或弹窗出现时 |
| 4 | 长历史（数千行）时滑块只有 1 个字符高，看不出进度 | `max_scroll` 大于约 5 倍视口高度 |
| 5 | 很难精确抓住滑块，宽屏下尤其明显 | 终端宽度越大越明显 |
| 6 | 鼠标静止时完全看不到滚动条（连轨道都没有），无法预判可拖区域 | 常态 |

## 复现条件

- **复现频率**：现象 2 必现；现象 1 在中等内容长度下必现；现象 3 偶发（依赖 spinner/弹窗时机）
- **触发步骤**：
  1. 在 TUI 中制造一段刚超过一两屏的消息内容（不是几千行的极端情况）
  2. 把鼠标移到消息区最右侧唤出滚动条
  3. 点击滑块以外的轨道空白处，观察滑块落点是否在鼠标所在行
  4. 保持左键按住，上下拖动，观察滑块与鼠标之间是否始终错开固定距离
  5. 换一段数千行的长内容，观察滑块高度
- **环境**：Windows（ConPTY）/ Windows Terminal；与内容长度强相关，与终端型号无关

## 涉及文件

- `peri-tui/src/event/mod.rs` —— 滚动条命中判定、按下、拖拽的换算逻辑（`point_in_hit_bar` / `handle_message_scrollbar_down` / `handle_message_scrollbar_drag` / `message_scrollbar_offset_for_row`）
- `peri-tui/src/ui/main_ui/message_area.rs` —— 消息区滚动条的渲染、`thumb_area` 反推、`dragging` 状态清理（`render_message_scrollbar` / `render_messages`）
- `peri-tui/src/app/ui_state.rs` —— `MessageScrollbarMetrics` 与滚动条相关 UI 状态
- `peri-tui/src/event/input_pump.rs` —— 输入泵的事件合并策略（只合并 `Moved`，不合并 `Drag`）
- `peri-tui/src/event/mouse_batch.rs` —— 鼠标事件批处理（`MAX_MOUSE_BATCH`、连续 Drag 合并）
- `peri-widgets/src/scrollable.rs` —— 统一的滚动条构造器与面板滚动条的对照组实现
- 第三方：`ratatui-widgets-0.3.0/src/scrollbar.rs`（`part_lengths` / `viewport_length` 决定滑块几何）

## 代码定位与证据

以下均为可独立复核的事实性定位（不含因果断言），确认度见文末审计表。

**E1. 两套换算分母不一致**

- 点击轨道：`peri-tui/src/event/mod.rs:350-360`，分母 `bar_area.height - 1`
- 拖拽：`peri-tui/src/event/mod.rs:332-345`，分母 `travel = bar_area.height - thumb_area.height`（`event/mod.rs:300-305` 捕获）
- 两者仅在 `thumb_area.height == 1` 时相等。

**E2. ratatui 滑块长度公式**

- `ratatui-widgets-0.3.0/src/scrollbar.rs:561-588`：`thumb_length ≈ track² / (max_scroll + track)`
- `ratatui-widgets-0.3.0/src/scrollbar.rs:617-625`：`viewport_content_length == 0` 时 `viewport_length` 退化为 `area.height`

**E3. 消息区路径未设置 `viewport_content_length`**

- `peri-tui/src/ui/main_ui/message_area.rs:245`：`ScrollbarState::new(max_scroll + 1).position(offset)`，无 `.viewport_content_length(...)`
- 对照组 `peri-widgets/src/scrollable.rs:162-168`（面板路径）：显式调用 `.viewport_content_length(viewport)`
- 全仓库 `viewport_content_length` 仅出现在 `peri-widgets/src/scrollable.rs:167`

**E4. 轨道高 30 时的实际数值**（按 E2 公式复算）

| `max_scroll` | thumb 高 | 拖拽分母 `travel` | 点击分母 | 点轨道行偏移 +10 得到 | 从顶拖到同一行得到 |
|---|---|---|---|---|---|
| 20 | 18 | 12 | 29 | 6 | 16 |
| 60 | 10 | 20 | 29 | 20 | 30 |
| 1000 | 1 | 29 | 29 | 相同 | 相同 |

**E5. 拖拽为「相对按下瞬间的增量」，无绝对对齐校正**

- `peri-tui/src/event/mod.rs:300-305`：`drag_origin = Some((row, ui.scroll_offset, travel))`
- `peri-tui/src/event/mod.rs:332-345`：`new_offset = start_offset ± distance`，`distance = |row - start_row| * max_offset / travel`

**E6. `max_offset` 含 spinner 行，逐帧可变；但拖拽换算的分子在按下时冻结**

- `peri-tui/src/ui/main_ui/message_area.rs:154-158`：`visual_total = cache.total_lines + spinner_extra`，`max_scroll = visual_total - visible_height`
- `peri-tui/src/ui/main_ui/message_area.rs:186`：`scrollbar_max_offset` 仅渲染时写入

**E7. 渲染阶段存在「metrics 为 None 即清空 dragging」的副作用**

- `peri-tui/src/ui/main_ui/message_area.rs:219-223`
- `metrics` 为 None 的条件含 `max_scroll == 0` 与 `!active`（`message_area.rs:234`）

**E8. 命中容差与 hover 区域**

- `peri-tui/src/event/mod.rs:263`：`MESSAGE_SCROLLBAR_HIT_PAD = 2`
- `peri-tui/src/ui/main_ui/message_area.rs:214`：hover 区为 `inner.right()-1`、宽 1 列

**E9. 非 hover 状态完全不渲染滚动条（含轨道）**

- `peri-tui/src/ui/main_ui/message_area.rs:234`：`!active` 时 `return None`，不调用 `render_stateful_widget`
- `peri-widgets/src/scrollable.rs:14-18`：`track_symbol(None)`、`begin_symbol(None)`、`end_symbol(None)`

**E10. Drag 事件不参与 hover 合并**

- `peri-tui/src/event/input_pump.rs:31-40`：`coalesce_hover` 仅在事件为 `MouseEventKind::Moved` 且队尾同为 `Moved` 时替换
- `peri-tui/src/event/mouse_batch.rs:5,34-41`：`MAX_MOUSE_BATCH = 128`，连续 Drag 仅保留最后坐标

**E11. `thumb_area` 由扫描帧缓冲中的 `█` 反推**

- `peri-tui/src/ui/main_ui/message_area.rs:252-260`

## 测试覆盖缺口

`peri-tui/src/event/scrollbar_test.rs` 仅覆盖「点住真实滑块 → 拖动 → 回锚点 → 拖到顶/底」路径（断言"点击滑块不跳转"）。以下两条未覆盖：

1. 点击轨道空白跳转 → 继续拖动（对应 E1 + E5）
2. 拖动过程中 `max_scroll` / `needs_scrollbar` / `popup_active` 发生变化（对应 E6 + E7）

## 审计与确认度

本次审计只做了「静态代码核对 + 公式复算 + 行号复核」，**未在真实终端运行 TUI 观测**。下列分类以「能否被独立复核」为准。

### A 类：可 100% 确证（代码事实 + 可复算）

| 证据 | 确证方式 |
|------|----------|
| E1 两处分母不一致 | 读代码，`bar_area.height - 1` 与 `bar_area.height - thumb_area.height` 为不同表达式；仅在 `thumb_area.height == 1` 时相等 |
| E2 ratatui 滑块公式 | 读 `ratatui-widgets-0.3.0/src/scrollbar.rs` 源码 |
| E3 消息区未设 `viewport_content_length` | 全仓库 grep 该符号，仅 `peri-widgets/src/scrollable.rs:167`（面板路径）出现 |
| E4 数值表 | 按 E2 公式独立复算（H=30），三行数值与本文档一致 |
| E5 拖拽为相对增量、无绝对对齐校正 | 读代码，`new_offset = start_offset ± distance` |
| E6 `spinner_extra` 参与 `max_scroll`；`scrollbar_max_offset` 仅渲染时写入 | 读代码 |
| E8 命中容差为 2 列 | 读代码 |
| E9 非 hover 不渲染滚动条且无 track | 读代码，`!active → return None` + `track_symbol(None)` |
| E10 `Drag` 不参与 hover 合并、批处理上限 128 | 读代码 |
| E11 `thumb_area` 由帧缓冲 `█` 反推 | 读代码 |

### B 类：机制确证，触发条件/后果未实测

| 项 | 说明 |
|----|------|
| E7 的派生后果 | 「渲染阶段清空 `dragging`」机制在代码中确凿；但拖动过程中是否真被触发（需 `max_scroll` 归零或 `popup_active` 变 true）未实测 |
| E6 的派生后果 | 「拖动中换算比例变化」机制确凿；实际漂移幅度（取决于 spinner 行数变化与 `max_scroll` 基数）未实测 |
| E10 的派生后果 | Drag 排队机制确凿；「队列积压导致拖动滞后于鼠标」需打点统计队列长度，未实测 |

### C 类：仅由公式推算，未经屏幕观测

| 症状 | 说明 |
|------|------|
| 症状 1「偏移约 3~7 行」 | 由 E4 公式在轨道高 30 下推算（滑块落点与鼠标行相差约半个滑块高度量级），非真机测量 |
| 症状 5「很难抓住」 | 由 E8 的 3 列命中宽度 + 1 格滑块尺寸推断，非真机测量 |

**审计结论**：E1~E11 的**事实性描述可 100% 复核**；涉及「手感/程度」的主观描述（症状 1、5）为推算值，需真机验证；E6/E7/E10 的派生后果需实测确认后才能作为修复依据。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-23 | — | Open | agent | 创建 issue |
| 2026-09-23 | Open | Open | agent | 补充定性说明：区分「可验证的代码事实（A）」与「未经验证的影响推断（B）」，澄清症状详情均为 agent 推算而非用户描述；同步更新 GitHub #232 正文 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
