**状态**：Open
**优先级**：中
**创建日期**：2026-10-28

## 问题描述

`cc-tui` 的 inline（growing）模式下，输入框逻辑行数增长（多行草稿）会把**整个终端视口**撑高并触发原生滚屏；用户把草稿删回单行后，视口高度不会回落，消息区留下大片空白，且不会自行恢复。

根因不是「忘了实现收缩」，而是**输入框行数被当成了与消息内容同级的单向增长源**：#650 的「增长不缩回」设计针对消息/回复（原生滚屏不可逆），但输入框是**可逆**的——它涨高的原因随时会被用户删掉。

> **触发路径澄清**：多行**粘贴**在支持 bracketed paste 的终端上会被折叠成 `[Pasted text #N +X lines]` 占位符（`cc-tui/src/app/paste_ops.rs:66-92`，`paste_line_count > 1` 即折叠），`textarea.lines()` 不增长，因此不触发本问题。真正触发的是输入框出现 ≥4 个逻辑行的场景：
> 1. 用户手动 `Enter` 输入多行草稿；
> 2. 草稿恢复 / 历史回填（`main_input_draft`）带来多行内容；
> 3. Windows 上不支持 bracketed paste 的终端（cmd.exe、旧 PowerShell）把多行粘贴退化为**逐字符 Key 事件**，而「模拟粘贴检测」（`cc-tui/src/event/mod.rs:203-285`）需要首字符后 1 ms 内有后续事件才会命中；慢终端下检测未命中时，换行就是真实 `Enter`，输入框真的变多行。

## 症状详情

| 维度 | 观察 |
|------|------|
| 触发场景 | inline 阶段（未锁定全屏）输入框出现多行草稿（手动多行 / 草稿恢复 / 慢终端粘贴），随后删回单行 |
| 实际表现 | 视口随输入框一起长高并上移起点；删回单行后视口保持高水位，消息区出现大片空白 |
| 期望表现 | 输入框涨高只在已有视口内借消息区的行，视口高度不变；删除后布局完全复原 |
| 附带损耗 | 空会话 + 较小终端（≤20 行）时 `required_rows` 可越过终端高度而**假性锁定全屏**，此后永久不可逆 |
| 复现频率 | 必现（每多 1 个逻辑行就多占 1 行视口高度） |

## 复现条件

- **复现频率**：必现
- **触发步骤（最稳定）**：
  1. 24 行终端里启动 `cc-code`（紧凑启动，未进入全屏）
  2. 在输入框里用 `Enter` 输入 5~8 个逻辑行的草稿（或恢复一个多行草稿）
  3. 观察视口向下展开、上方 shell 内容被滚走
  4. 连按退格/`Ctrl+U` 删空输入框
  5. 观察消息区残留的大片空白
- **Windows 粘贴路径**：在 cmd.exe / 旧 PowerShell（无 bracketed paste）粘贴多行文本；若「模拟粘贴检测」未命中，换行会变成真实 `Enter`，同样复现
- **环境**：`cc-tui` 交互模式；`-p` / headless 不涉及；逻辑与平台无关

## 根因

以 `origin/main`（ff45b1b）为准：

1. `cc-tui/src/ui/main_ui/viewport_layout.rs:71-96` `BottomHeights::measure()`
   ```rust
   input: (line_count.saturating_add(2).min(u16::MAX as usize) as u16)
       .min(terminal.height.saturating_mul(2) / 5)
       .max(3),
   ```
   输入框高度 = 逻辑行数 + 2 边框，上限 `终端高度 × 2/5`（24 行终端 ≈ 9 行），下限 3。上限按**宿主终端高度**算（`spec/global/domains/tui.md:29`「输入上限以宿主终端尺寸计算」的有意选择），因此 inline 视口只有 11 行时输入框也能长到 9 行。

2. `cc-tui/src/ui/main_ui/viewport_layout.rs:131-141` `required_rows()` = `bottom.total()`（含 input）+ `content_rows()`，于是输入框涨高直接推动视口增长。

3. `cc-tui/src/terminal_backend/viewport.rs:70`：
   ```rust
   let next_rows = self.allocated_rows.max(required_rows).max(1);
   ```
   `allocated_rows` 单调递增。删空输入后 `required_rows` 回落，但高水位被记住，`area.height` 不变 → 空出来的行被 `Constraint::Min(1)` 的消息区吸收，表现为留白。

4. 该行为已被测试与文档固化：
   - `cc-tui/src/ui/main_ui/viewport_layout_test.rs:71-96` `test_draft_grows_without_shrinking_and_preserves_cursor` 断言 `grown == first + 2` 且删除后 `plan.area.height == grown`
   - `docs/prd/growing-terminal-viewport.md:195` A03「输入多行草稿，再删除部分内容 → 按显示行数增长；删减不缩回」
   - `spec/global/domains/tui.md:29`、`TUI-STYLE.md:256` 同口径

5. 同机制的历史修复只解决了浮层：`spec/issues/2026-10-21-hint-popup-allocated-rows-blank-gap.md`。方案 1（拆分 total/persistent，commit 0c619369）失败在「浮层行数参与帧高 → 24 行终端算出 27 行 → 假性击穿全屏」；实测三条物理限制：原生滚屏不可逆、ratatui Fixed `resize` 缩小时不清理释放区、逻辑收缩后底部悬空。最终方案 3 让浮层完全不参与 `required_rows`。**输入框不能照搬方案 3**——它是必备持久区域，无法叠在消息区上绘制。

## 设计目标与不变量

| 编号 | 不变量 | 校验手段 |
|---|---|---|
| I1 | 视口高度只由「消息内容 + 固定底栏基线 + 欢迎卡」决定，**输入框行数不得改变视口高度** | 单测：同一 content 下 1 行与 200 行草稿的 `required_rows` 相等 |
| I2 | 任何一帧所有 `Constraint` 高度之和 ≤ `area.height`，禁止假性击穿全屏 | 单测：`area.height ∈ {4,6,24}` 时 `input + others ≤ area.height - 1` |
| I3 | 输入框渲染高度 = `preferred.clamp(3, 8)` → 再按 cap 与 `area.height - others - MIN_MSG` 取小，下限 3 | 单测矩阵 |
| I4 | `BottomHeights` 双语义：布局/消息排版用**真实**高度，增长判定用**基线**高度（input = 3） | 代码审查 + 单测 |
| I5 | `allocated_rows` 仍单调不减（保留滚屏不可逆的兜底，不改 `prepare()` 语义） | 回归：`viewport_test.rs` 全绿 |
| I6 | 消息区尽量保底 `MIN_MSG = 3` 行；不足时输入框退到 3 行，不得溢出帧 | 单测：`area.height` 极小 |
| I7 | **输入上限仍按宿主终端高度计算**（`tui.md:29`），不按当前紧凑 area 递归缩小；area 只做一次性防溢出钳制，且不回馈 `required_rows` | 代码审查：cap 的输入只有 `terminal.height` |

## 方案对比

| 方案 | 做法 | 结论 |
|---|---|---|
| A. 输入框不参与增长（推荐） | `required_rows` 按基线 3 行计，输入框在已分配视口内借消息区的行 | ✅ 视口零变化，删除后完全复原；不触碰滚屏/收缩的物理限制 |
| B. 输入框封顶 + 内部滚动（配合 A） | `cap = clamp(终端高度/3, 3, 8)`，超出部分由 textarea 内部滚动 | ✅ 解决 2/5 上限过大 + inline 小视口溢出 |
| C. 允许 `allocated_rows` 收缩 | 收缩 + 清理释放区 | ❌ 2026-10-21 issue 已实测否决：滚屏不可逆、Fixed `resize` 残留 |
| D. 输入框按 Overlay 绘制 | 复用浮层方案 3 | ❌ 输入框和状态栏是必备持久区域，语义不成立 |

## 工程设计

### 1. 单一来源的高度函数

```rust
pub(super) const INPUT_MIN_ROWS: u16 = 3;   // 1 行文本 + 上下边框
pub(super) const INPUT_MAX_ROWS: u16 = 8;   // 绝对上限
pub(super) const MIN_MESSAGE_ROWS: u16 = 3; // 消息区保底

/// 输入框渲染高度。cap 按宿主终端高度（I7），area 仅做一次性防溢出钳制。
pub(super) fn input_rows(preferred: u16, terminal_height: u16, area_height: u16, others: u16) -> u16 {
    let cap = (terminal_height / 3).clamp(INPUT_MIN_ROWS, INPUT_MAX_ROWS);
    let wanted = preferred.clamp(INPUT_MIN_ROWS, INPUT_MAX_ROWS).min(cap);
    // 软上限：给消息区留 MIN_MSG；硬上限：至少留 1 行，保证 I2
    let soft = area_height.saturating_sub(others).saturating_sub(MIN_MESSAGE_ROWS);
    let hard = area_height.saturating_sub(others).saturating_sub(1);
    wanted.min(soft.max(INPUT_MIN_ROWS)).min(hard.max(INPUT_MIN_ROWS))
}
```

预算输入只有 `preferred` / `terminal_height` / `area_height` / `others`。**不含 `content_rows`**：`content_rows` 源自渲染 worker 的排版结果，而 worker 的 `available_rows` 又由 `BottomHeights::total()` 反推（`viewport_layout.rs:211`），引入 content 会形成跨帧收敛环。

### 2. `BottomHeights` 双语义

```rust
impl BottomHeights {
    /// 布局/消息排版用：input 为真实可兑现高度。
    pub(super) fn measure(app: &mut App, terminal: Rect) -> Self { /* 现实现，input 走 input_rows */ }

    /// 增长判定用：input 取基线，输入框不得推动视口。
    pub(super) fn measure_for_growth(app: &mut App, terminal: Rect) -> Self {
        Self { input: INPUT_MIN_ROWS, ..Self::measure(app, terminal) }
    }
}
```

- `required_rows()` 改用 `measure_for_growth`（I1）。
- `mod.rs` 布局与 `try_prepare_width()` 的 `available_rows` 仍用 `measure`（真实），保证消息排版行数与实际布局一致（`viewport_layout.rs:211` 的 `available_rows = size.height - total()` 依赖此值）。
- `others` 需先于 `input` 计算：现有实现是结构体字面量一次性组装，要拆成局部变量后重排顺序（纯机械改动）。
- `measure` 需要 `&mut App`（`textarea.measure()` 带内部缓存），三处调用点（`main.rs:1294`、`viewport_layout.rs:81`、`viewport_layout.rs:159`、`mod.rs:67`）都要核对借用顺序。

### 3. 布局侧兜底（I2/I3）

`cc-tui/src/ui/main_ui/mod.rs:71,103`：

```rust
let others = heights.attachment + heights.panel + heights.queued + heights.status + heights.background;
let input_height = heights.input
    .min(area.height.saturating_sub(others).saturating_sub(1))   // 绝不溢出帧
    .max(INPUT_MIN_ROWS.min(area.height));
```

全屏与 inline 共用这一条：全屏时 `area.height == size.height`，等价于按屏幕钳制（顺带解决 `2/5` 上限 + 其他固定栏可能把消息区/状态栏压成 0 行的问题）。

### 4. cap 收紧

`(terminal.height / 3).clamp(3, 8)` 取代 `terminal.height * 2 / 5`：24 行终端 9 → 8 行，10 行终端 4 → 3 行。输入最大行数目前**没有任何配置项**（`AppConfig` 与 `ConfigPanel` 均无该字段），本次抽常量即可，不新增配置。

### 5. 输入框内部滚动与光标（P2）

`tui-textarea-2` 0.11 已内建自动滚动：渲染区域高度小于内容行数时，`render()` 会计算 `next_scroll_top` 把光标保持在视口内，并裁剪其余行，不 panic。因此**不需要自己实现滚动**。

缺的只有自绘光标：`cc-tui/src/ui/main_ui/mod.rs:427-457` 的 `draw_bar_cursor` 假设 `cell_y = inner_y + data_row`（逻辑行 1:1 映射），超出行数时会画到区外并被越界 `return` 吞掉（不会串行，但光标消失）。该 crate **不公开滚动偏移 getter**（`TextArea.viewport` / `screen_cursor` 均为 `pub(crate)`，`Scrolling` 只有相对位移 `Delta` / `Page*`）。可选路径：

- 方案 1（推荐）：不再自绘 `│`，改用 textarea 原生 cursor（`set_cursor_style` + 原生反色），由 crate 自己保证可见——改动最小，但需确认与现有细线光标视觉一致
- 方案 2：自管 `input_scroll_row`（内容变更/光标移动后按 `cursor().0` 与视口高 clamp），绘制时减偏移——保留现有视觉，需多一份派生状态

实现时还需注意：`event/keyboard.rs:267`、`agent_ops/lifecycle.rs:292` 的 `build_textarea` 会重建输入框，`set_min_rows/max_rows`、`wrap_mode` 等配置必须在该构造路径一并重放。

**软换行（`set_wrap_mode`）刻意不在本 issue 内**：默认关闭（现状为水平滚动），一旦启用，逻辑行与视觉行不再 1:1，`draw_bar_cursor`、高度测量、`event/mouse.rs:72`（假设 1 逻辑行 = 1 屏幕行）都要重做映射，属独立评估项。

## 实现步骤

| 阶段 | 内容 | 验收 |
|---|---|---|
| **P1（修 bug）** | `input_rows` + `measure_for_growth` + `required_rows` 切基线 + `mod.rs` 兜底 + 常量 | inline 多行草稿视口不变、删除零留白；`viewport_test.rs` 全绿 |
| **P2（体验）** | cap 收紧 + `set_min_rows/max_rows` 接入 + 自绘光标偏移（方案 1 或 2） | 超长草稿输入框封顶 8 行且光标可见可编辑 |
| **P3（可选）** | 软换行（`WrapMode::WordOrGlyph`）与视觉行映射 | 单独评估，不阻塞 P1/P2 |

P1 与 P2 分开提交，P1 可独立回滚。

## 测试矩阵

**新增单测**（`cc-tui/src/ui/main_ui/viewport_layout_test.rs`）
- [ ] 同一 content 下，1 行草稿与 200 行草稿的 `required_rows` 相等（I1）
- [ ] `preferred = 8`、`allocated_rows = others + 11` 时 input 取 8、消息区剩 3 行（I3/I6）
- [ ] `area.height ∈ {4,6,24}` 时 `input + others + 1 ≤ area.height`（I2，防假性击穿）
- [ ] 全屏（`fullscreen_latched = true`）下 input ≤ cap 且消息区 ≥ MIN_MSG
- [ ] 首帧 `allocated_rows = 0` 时 input 取 3（不出现 0 行输入框）
- [ ] 删除后 `plan.area.height` 与增长前一致（P1 契约）

**改写**（行为契约变更）
- [ ] `viewport_layout_test.rs:71-96` `test_draft_grows_without_shrinking_and_preserves_cursor` → `test_draft_borrows_message_rows_without_growing_viewport`：断言 `grown == first`、删除后 `plan.area.height == first`

**回归**
- [ ] `test_compact_idle_height_is_stable_and_input_follows_card`（空输入 input=3，`10..=12` 断言应保持）
- [ ] `test_first_message_keeps_welcome_and_real_message_coordinates`
- [ ] `test_hint_popup_expand_then_dismiss_leaves_no_blank_gap`
- [ ] `viewport_test.rs` 全部视口用例（`allocated_rows` 语义未改）

**测试夹具**：`App::new_headless(w, h)` + `ViewportState::growing(n)` + `TerminalOptions{viewport: Viewport::Fixed(plan.area)}`（`viewport_layout_test.rs:27-50`）；多行草稿用 `textarea.insert_str("a\nb\nc")` 或 `paste_text_into_textarea`（注意多行粘贴会被折叠，造草稿要用 `insert_str`）。

**手测**（Windows PowerShell，24 行终端）
- [ ] `Enter` 输入 8 行草稿 → 视口不动、无滚屏、无留白
- [ ] 删空 → 布局完全复原
- [ ] 消息满屏后输入多行 → 消息区压到 3 行且可滚动，内容不丢
- [ ] 删空后继续对话 → 增长行为与改前一致
- [ ] 全屏态多行草稿 → 输入框 ≤ 8 行，状态栏不被挤掉
- [ ] cmd.exe（无 bracketed paste）粘贴多行 → 不再撑高视口

## 影响面与风险

- **行为变更**：PRD A03「按显示行数增长、删减不缩回」被「输入框不推动视口增长」取代，需同步改文档口径（见下）。
- **可感知代价**：inline 阶段消息区最少让到 3 行（不足时输入框退到 3 行）；超长草稿输入框只显示 3~8 行。
- **不受影响**：`allocated_rows` / `prepare()` 语义、全屏切换时机、退出清理、ACP 协议、会话持久化格式、消息渲染缓存、粘贴折叠行为。
- **回滚**：P1 单 commit `revert` 即恢复原行为，无数据迁移。
- **风险点**：`try_prepare_width` 的 `available_rows` 用真实 input 高度，而 `required_rows` 用基线口径——两者必须都经 `input_rows` 收口，否则消息排版行数与实际布局会出现 1~5 行偏差（消息少渲或末尾截断）。

## 文档同步

- `docs/prd/growing-terminal-viewport.md`：改写 A03；新增 R08「输入框高度封顶、超出内部滚动，不参与视口增长」；边界表补一行
- `spec/global/domains/tui.md:29`：补「输入框行数不参与视口增长判定」，保留「输入上限按宿主终端尺寸计算」的既有约定
- `TUI-STYLE.md:256`：同步输入框高度规则
- `spec/global/features.md` #650 条目：补一句
- `spec/issues/2026-10-28-input-draft-grows-inline-viewport.md`：按仓库惯例落本地 issue 副本

## 验收标准

- [ ] inline 阶段输入任意行数草稿，视口高度、`origin_row`、`allocated_rows` 均不变
- [ ] 删空草稿后布局完全复原，无空白残留，无需重启
- [ ] 任何一帧 `frame_rows ≤ area.height`（无假性全屏锁定）
- [ ] 全屏态输入框封顶，消息区/状态栏不被压成 0 行
- [ ] `cargo test -p cc-tui` 与 `cargo clippy -p cc-tui` 全绿
- [ ] PRD / `domains/tui.md` / `TUI-STYLE.md` / `features.md` 四处口径一致

## 关联

- 父需求：#650（紧凑启动与增长视口，已随 v0.6.171 发布）
- 同机制历史修复：`spec/issues/2026-10-21-hint-popup-allocated-rows-blank-gap.md`（浮层方案 3）
- 本文档基准：`origin/main` @ ff45b1b

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-28 | — | Open | agent | 用户反馈 inline 草稿撑高视口；代码核查确认根因并给出工程化设计 |
| 2026-10-28 | Open | Open | agent | 深挖后修正：多行粘贴会折叠成占位符，改以「输入框逻辑行数增长」为触发条件；cap 维持按宿主终端计算以符合 `tui.md:29` |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）



## 本次实现进度（2026-11-12）

- P1：增长判定使用输入基线 3 行；实际输入只借现有消息区，极小区域按剩余空间钳制，审批隐藏输入保持 0 行。保留 allocated_rows 与视口终端生命周期语义。
- 回归覆盖同内容单行/200 行草稿、首次分配、删空恢复、4/6/24 行区域与消息保底；保留欢迎卡与候选浮层测试。
- P2：输入封顶、原生内部滚动与光标可见单独实施，保留分阶段提交要求。
- 本机资源限制，仅做最小源码检查；完整 cargo test/clippy 与真实终端验收待 CI/人工验证，未宣称通过。
