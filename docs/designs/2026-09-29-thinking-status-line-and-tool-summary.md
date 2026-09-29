# 设计：Thinking 状态行 + 工具动作汇总（对齐 Claude Code 非详细模式）

> 状态：**编码完成（四轮审计通过，待提 PR）**
> 日期：2026-09-29
> 分支：`feat/thinking-status-line`
> 决策来源：AskUserQuestion（2026-09-29）
> 说明：文中所有「ASCII 绘制」均为**评审示意稿**，用来看观感、对规格，不保证与最终渲染逐字符一致；实现以代码为准。

---

## 1. 背景与诉求

在非详细模式（默认，未按 `Ctrl+O`）下，peri 与 Claude Code 的输出观感差异明显：

1. **工具逐行刷屏**：一个排查任务会连续调用几十个 `Bash` / `Read`，peri 每条占一行；Claude Code 把它们折叠成一句计数。
2. **缺「思考状态」表达**：Claude Code 运行中的 spinner 行有第三字段（`· thinking` / `· thought for 10s` / `· still thinking` / `· thinking more`），peri 没有。
3. **消息区思考行用字符数**：peri 是 `∴ Thought for 3629 chars`，Claude Code 是 `Thought for 16s, searched for 1 pattern`（秒 + 动作计数）。

---

## 2. 需求理解

### 2.1 拆成两个独立改动点

| 编号 | 位置 | 内容 | 代码锚点 |
|------|------|------|----------|
| **改动 A** | 运行中 spinner 行 | 追加第三字段，随思考状态动态切换 | `peri-tui/src/ui/main_ui/message_area.rs:51-75` |
| **改动 B** | 消息区思考行 | `∴ Thought for N chars` → `Thought for Ns, <动作计数>` | `peri-tui/src/ui/message_render.rs:1015-1066` |

两处**互相独立**，可分别验证、分别回滚。

### 2.2 改动 A 的规格：第三字段是「四态状态机」

不是固定文案，是随阶段切换的状态。用户提供的 Claude Code 截图呈现了 4 种形态：

| # | 状态 | 触发条件 | 尾字段 | 样例 |
|---|------|----------|--------|------|
| ① | 默认 / 刚进入思考 | 开始一次新的思考段 | `· thinking` | `Onioning… (5s · thinking)` |
| ② | 思考后 | 本次思考段结束 | `· thought for {N}s` | `Onioning… (13s · ↓ 2.1k tokens · thought for 10s)` |
| ③ | 持续思考 | **同一段**思考持续超阈（**默认 10s，未证实**） | `· still thinking` | `Nucleating… (13m 43s · ↓ 95.3k tokens · still thinking)` |
| ④ | 更多思考 | 本轮**已产出过**又再次思考（**思考不连续**） | `· thinking more` | `Composing… (3m 30s · ↓ 14.7k tokens · thinking more)` |

**❌ 已作废的推断（保留以示教训）**：早期版本曾写「③ 与 ④ 的时长是反的（13m43s vs 3m30s），所以不是纯时长阈值」。**该论据无效**——两张截图**无法确认来自同一会话**，跨会话比较时长没有意义，已删除此推理。

**判定规则**：

- **③ `still thinking`——单段时长触发**：**同一段**思考未被中断且持续超阈时显示。
  - ⚠️ **阈值未证实**：初版误设 60s。用户实测观察「约 10s 就出现」。现有截图样本**互相矛盾**（见下方证据表），无法确定精确值。
  - **实现决策：做成可调常量**，默认 **10s**，后续实测校准。
- **④ `thinking more`——连续性触发**：本轮思考**被打断过**（中间夹了工具调用或文本产出）后**再次**进入思考，显示 `thinking more`。
- **代码可判定性**：段边界可由 `AiReasoning` 段之间是否夹杂 `ToolStart` / `AssistantChunk` 观测；单段时长由 `Instant` 计算。**两个条件都可在 TUI 层实现，不碰事件语义。**
- ⚠️ **区分**：`still thinking` 看**单段绝对时长**，`thinking more` 看**是否为第 2+ 段**——两个维度独立。

**⚠️ 两套「时间阈值」的关系（易混，务必分清）**：

| 阈值体系 | 控制什么 | 值 | 见 |
|----------|----------|-----|-----|
| **状态词切换** | `still thinking` 何时取代 `thinking` | 10s（默认，未证实） | 本节 |
| **verb/状态词配色** | 颜色何时升温 | 5s / 15s / 30s | §2.7 |

两者**当前设为独立**：颜色档位只影响**颜色**，不改变状态词；状态词只影响**文案**，不改变颜色。二者恰好都在 5~10s 附近起步，但**是两条独立规则**。（若后续实测发现 Claude 二者联动，再合并——见 §6 开放问题。）

**⚠️ 关于阈值的矛盾证据（如实记录）**：spinner 显示的是**整轮总耗时**，而 `still thinking` 可能看**当前这一段思考**的时长——两者不是一回事，故下方样本不能直接互证：

| 样本 | 总耗时 | 尾字段 | 与「10s 阈值」的关系 |
|------|--------|--------|---------------------|
| `Catapulting…` | 17s | `thinking` | ⚠️ **看似矛盾**（17s > 10s 却非 still）。若该轮工具占用大部分时间、单段思考 < 10s，则仍自洽 |
| 用户实测观察 | ~10s | `still thinking` | 用户系统提示的观察 |
| `Nucleating…` | 13m 43s | `still thinking` | 与任何合理阈值都不冲突 |

→ **结论**：证据不足以定死阈值，故实现为可调常量。

### 2.3 改动 B 的规格

Claude Code 真实样本（来自用户提供的运行日志与执行截图）：

```text
Thought for 16s, searched for 1 pattern (ctrl+o to expand)
Thought for 9s, searched for 2 patterns, read 2 files (ctrl+o to expand)
Thought for 5s (ctrl+o to expand)          ← 无工具调用时退化为纯时长

Listed 1 directory (ctrl+o to expand)      ← 无思考、纯动作时独立成行
Read 1 file, listed 1 directory (ctrl+o to expand)
Reading 1 file… (ctrl+o to expand)         ← 进行中（现在分词 + …）
```

规则：
- 时长用**秒**（`16s`），不是 peri 现行的字符数；
- 动作按类别计数，**逗号分隔**，**单复数变化**：
  - `1 pattern` / `2 patterns`
  - `1 file` / `2 files`
  - `1 directory` / `2 directories`
- 无工具时只保留 `Thought for {N}s`；
- **动作行可无思考前缀**：纯工具调用（无 reasoning）时直接输出 `{actions}`；
- **进行时/完成时两态**：进行中 `Reading 1 file…`，完成 `Read 1 file`；
- **失败必显**：只读工具折叠为计数后**不显示内容**；但 `is_error=true` 时**必须显示错误行**（对齐 Claude `⎿ Error: ...`）。判定依据是 `is_error` 标记，**不是**内容字符串是否以 `Error:` 开头——真实数据中存在 `is_error=false` 但内容为 `Error: File not found` 的行（框架未标错），此类仍折叠。

### 2.4 工具展示分层（重要——修正早期推断）

Claude Code 的**工具明细展示分三层**，并非全部折叠成计数：

| 工具类别 | 展示形式 | 样例 |
|----------|----------|------|
| **只读**（Read / Glob / Grep） | **折叠为计数**（并入动作行） | `Read 1 file, listed 1 directory` |
| **Bash** | **显示实际命令** + **输出摘要**（前几行）+ 截断提示 | `● Bash(git status --short && echo "=== branch ===" …)`<br>`  ⎿ ?? conversation-…md`<br>`    ?? weather_demo.php`<br>`    === branch ===`<br>`    ... +4 lines (ctrl+o to expand)` |
| **写入**（Write / Edit） | **显示路径 + diff 预览** | `Write(docs\designs\…md)`<br>`  ⎿ Wrote 278 lines to …`<br>`     1  # 设计：…`<br>`     ... +271 lines` |

> ⚠️ **修正 1**：早期推断曾认为 Bash 折叠成 `ran N shell commands` —— **错误**。Bash 展示实际命令。
> ⚠️ **修正 2（本轮新增）**：早期版本还写过「Bash 展示与 peri **已对齐**」——**同样错误**。经用户确认 + 代码核对：
> - **peri 非详细模式不展示 Bash 结果**（工具完成后 `collapsed=true`，`message_render.rs:1165` 的 `if !state.collapsed` 直接跳过输出；且折叠摘要仅 `Read` 有，`:1211`）。
> - Claude Code **展示**输出摘要 + 截断提示 `... +N lines (ctrl+o to expand)`。
> - 因此 **Bash 输出展示是新的 gap**，而非已对齐项。
> ⚠️ **修正 3**：peri 的截断措辞是 `... (N more lines)`（`message_render.rs:1187`），Claude Code 是 `... +N lines (ctrl+o to expand)`，**措辞不一致**。→ **已裁决（2026-09-29）**：保留 peri 措辞，追加 `(ctrl+o to expand)` 引导语。

### 2.5 第四态 `thinking more` 的实锤与重新解释

执行截图确认第四态真实存在：

```text
✳ Newspapering… (40s · ↓ 1.5k tokens · thinking more)
```

**重新定性**：曾经据此 + 另一张 13m43s 的 `still thinking` 得出「`thinking more` 与时长无关」——**但两张图非同会话，比较无效，该推论已撤回**。

当前采纳的假设（见 §2.2）：
- `thinking more` 的判定变量是**思考连续性**（是否被打断后又思考），而非时长；
- 40s 这个样本只能说明「短时间也会出现 `thinking more`」，**不能说明与时长无关**。

### 2.6 决策记录（AskUserQuestion 2026-09-29）

| 决策点 | 结论 |
|--------|------|
| 第三字段文案语言 | **照搬英文**固定输出：`thinking` / `thought for Ns` / `still thinking` / `thinking more` |
| 实现范围 | **完整四态**（不做削减版） |
| 改动 B（消息区汇总） | **一起做** |
| `still thinking` 阈值 | **默认 10s，做成可调常量**（用户实测观察约 10s 出现；证据不足未定死，见 §2.2 证据表） |
| 模式适用范围 | **两模式共用**——不改 `detail_mode` 分支。spinner 第三字段与消息区计数行在详细/非详细模式下**均生效**（对齐 Claude Code：其 spinner 亦不分模式） |
| Bash 输出摘要 | **纳入本次实现**（用户确认）。非详细模式展示输出**前 3 行**；截断提示**保留 peri 现有措辞** `... (N more lines)`，**其后追加** `(ctrl+o to expand)` 引导语（最终：`... (N more lines) (ctrl+o to expand)`，2026-09-29 用户二次裁决） |
| spinner verb 行为 | **方案 A：整轮固定一个动词**（用户选定，理由：符合人类视觉习惯）。agent 开始回复时选定一个动词（如 `Thinking`），思考/工具/输出全程不变；工具信息只在消息区展示。原 peri 行为（每步切 verb 为工具名）**改为固定**。 |
| spinner 配色 | 见 §2.7（颜色随时间四档变化；仅 verb + 状态词变色，其余 MUTED 灰） |

### 2.7 spinner 配色规则（时间驱动）

**观察来源**：用户对比 Claude Code 得出——颜色**由时间驱动**（非状态字眼），且**只有两个元素变色**。

#### 变色范围（严格限定）

| 元素 | 变色？ | 说明 |
|------|--------|------|
| `verb`（`Thinking…`） | ✅ | 主变色对象 |
| 状态词（`thinking` / `thinking more` / `still thinking`） | ✅ | 与 verb **同步**同档位 |
| `elapsed`（`2m 21s`） | ❌ | 始终 `MUTED #999999` |
| `tokens`（`↓ 2.6k tokens`） | ❌ | 始终 `MUTED` |
| `thought for Ns` | ❌ | 始终 `MUTED` |

> ⚠️ 早期版本曾把**整行**（含 elapsed/tokens）一起变色 —— **错误**。用户明确纠正：只有 verb 与状态词有色，其余 MUTED 灰。

#### 四档时间阈值（本设计选定）

| 档 | 触发 | 色值 | 颜色 | 体验含义 |
|----|------|------|------|----------|
| ① 默认 | `< 5s` | `#D77757` | 系统橙（peri `ACCENT`） | 正常，无感 |
| ② 变亮 | `≥ 5s` | `#EB9F7F` | 亮橙（用户实测观察值） | 「开始等了」 |
| ③ 浅黄 | `≥ 15s` | `#FFD966` | 浅黄 | 「明显在等」 |
| ④ 终黄 | `≥ 30s` | `#FFC107` | 琥珀黄（peri `WARNING`） | 「是不是卡住了」，此后不再变 |

**阈值设计依据**（产品视角论证）：

1. **5s** —— 用户对 Claude 的**实测观察锚点**，非凭空设定，保留。
2. **15s** —— Nielsen「10s 注意力边界」之后：用户开始明显感知等待、可能分心，但仍在容忍范围。
3. **30s** —— 「无反馈即怀疑卡死」的经典心理阈值（超过 30s 无进度信号，用户倾向认为进程挂起）。
4. **间隔递增（+10、+15）** —— 人对时长的感知遵循 **Weber-Fechner 对数律**：等 5s 后再等 10s、再 15s，**主观"每档等待感"才大致相等**。若用等距（如 5/10/15），后段会显得变色过频、干扰阅读。
5. **四档封顶** —— 已覆盖「无感 → 注意 → 烦躁 → 怀疑」完整情绪曲线；更多档位会让颜色跳变比情绪变化更快，反成噪音。

> ⚠️ **阈值中只有 5s 有实测依据**，15s / 30s 为产品推导值。**实现为集中可调常量**（如 `SPINNER_HEAT_*_SECS`），后续实测校准无需改逻辑。

#### 实现约束

- **工具执行段不重置档位**：进入工具执行时沿用上一段思考的档位，避免颜色来回跳（无实测依据，为平滑体验的主动选择）。
- 色值优先复用 peri `theme` 常量（`ACCENT` / `WARNING`）；亮橙、浅黄为新增，需在 `theme.rs` 补常量。

---

### 2.8 【新增】连续多轮「思考 + 只读工具」合并为一行

**需求来源**：用户真实 TUI 测试发现——一个回合内多轮「思考→只读工具」被**逐条**渲染成多行：

```text
∴ Thought for 147 chars, read 1 file (ctrl+o to expand)
∴ Thought for 257 chars, read 1 file (ctrl+o to expand)
∴ Thought for 6073 chars, read 1 file (ctrl+o to expand)
∴ Thought for 10025 chars (ctrl+o to expand)
∴ Thought for 6713 chars, searched for 1 pattern (ctrl+o to expand)
...（共 9 条）
```

**期望**（对齐 Claude Code）——**合并为一行**：

```text
Thought for 32s, searched for 1 pattern, read 4 files (ctrl+o to expand)
```

#### 合并规则（本设计选定）

| 维度 | 规则 |
|------|------|
| **合并范围** | 同一回合内（两次用户输入之间）**连续**的「含 Reasoning 的 AssistantBubble + 紧随只读工具」序列 |
| **断开条件** | 遇到含**可见正文文本**的 AssistantBubble（`Text` block 非空）→ 断开；用户消息 → 断开 |
| **非只读工具**（Bash/Write/Edit） | **不断开**，但**不计入**计数（Bash 仍逐条显示命令，见 §2.4） |
| **秒数** | 各轮 `duration_ms` **累加**（`thought for` 语义＝总思考耗时，与 spinner 的 `· thought for Ns` 一致） |
| **计数** | 各轮只读工具计数**累加**（`read N files` / `listed N directories` / `searched for N patterns`，按类别合并后统一输出） |
| **展开内容** | 展开后显示**所有**被合并轮次的 reasoning（`ctrl+o to expand` 的语义） |
| **无 reasoning 的轮次** | 不参与合并（纯工具轮不产生 `Thought for` 行，见 §2.3） |

#### 与 §2.3 的关系

§2.3 定义**单轮**格式（`Thought for Ns, read 1 file`）；本节定义**多轮合并**——单轮是合并的**退化情形**（只有一轮时输出不变）。

#### 待确认项（已裁决，2026-09-29 第三轮审计）

1. **秒数是累加还是跨度**：✅ **已裁决：累加**（各轮思考耗时之和），实现与测试（`test_merge_multi_segment_thinking_accumulates_duration_and_counts`）均已落地。
2. **合并是否跨越 Bash**：✅ **已裁决：跨越**（Bash 不断开，仅不计数），实现与测试（`test_parallel_tools_read_counted_alongside_bash`）均已落地。

---

## 3. 渲染对照（ASCII 绘制）

### 3.1 运行中 Spinner 行

**peri 现状**（依据代码路径推导：`set_loading(true)` → `Responding` 模式；`AiReasoning` 事件**不切** spinner，故思考期间 verb 停在「回复」文案）：

```text
┌─ 中文界面 ──────────────────────────────────────────────┐
│  ✢ 正在生成回复… (33s · ↓ 2.6k tokens)                  │
└─────────────────────────────────────────────────────────┘
┌─ 英文界面 ──────────────────────────────────────────────┐
│  ✢ Generating response… (33s · ↓ 2.6k tokens)           │
└─────────────────────────────────────────────────────────┘
```

> 注意：peri **从未** `set` 过 `SpinnerMode::Thinking`（全仓库 grep 无命中），所以思考期间文案是「回复」语义 —— 这本身就是 gap 的一部分。

**peri 改后**（新增 `Thinking` 态；尾字段固定英文；四态切换）：

```text
① 刚进入思考
   ✢ 思考中… (5s · thinking)
   ✢ Thinking… (5s · thinking)                    ← 英文界面

② 本次思考段结束
   ✢ 思考中… (13s · ↓ 2.1k tokens · thought for 10s)
   ✢ Thinking… (13s · ↓ 2.1k tokens · thought for 10s)

③ 单次思考持续超久
   ✢ 思考中… (13m 43s · ↓ 95.3k tokens · still thinking)
   ✢ Thinking… (13m 43s · ↓ 95.3k tokens · still thinking)

④ 本轮再次思考
   ✢ 思考中… (3m 30s · ↓ 14.7k tokens · thinking more)
   ✢ Thinking… (3m 30s · ↓ 14.7k tokens · thinking more)
```

**Claude Code 参照**（用户日志原样）：

```text
✻ Polishing… (46s · ↓ 6.8k tokens · thought for 18s)
✢ Catapulting… (17s · ↓ 1.6k tokens · thinking)
✳ Nucleating… (13m 43s · ↓ 95.3k tokens · still thinking)
· Composing… (3m 30s · ↓ 14.7k tokens · thinking more)
```

**差异**：peri 缺 `· {尾字段}` 整段；且 verb 语义（「正在生成回复」）与阶段不符。

---

### 3.2 消息区思考 / 动作行

**peri 现状**（已从会话记录确认）：

```text
● 我先确认这个「工具刷屏」在 TUI 里是怎么渲染的，尤其是「非详细模式」到底控了什么。

  ∴ Thought for 341 chars (ctrl+o to expand)

● 这个日志里刷屏的是大量工具调用（Bash/Read/Edit 几十条）。

  ∴ Thought for 78 chars (ctrl+o to expand)
```

**peri 改后**：

```text
● 我先确认这个「工具刷屏」在 TUI 里是怎么渲染的，尤其是「非详细模式」到底控了什么。

  Thought for 16s, searched for 1 pattern (ctrl+o to expand)

● 这个日志里刷屏的是大量工具调用（Bash/Read/Edit 几十条）。

  Thought for 9s, searched for 2 patterns, read 2 files (ctrl+o to expand)
```

**另有「纯动作行」（无 reasoning，只有工具）**：

```text
Listed 1 directory (ctrl+o to expand)
Read 1 file, listed 1 directory (ctrl+o to expand)
```

**以及「进行时」形态**（思考已结束、工具执行中）：

```text
Reading 1 file… (ctrl+o to expand)
```

**Claude Code 参照**（用户日志原样）：

```text
● 图里这行 ✻ Polishing… (46s · ↓ 6.8k tokens · thought for 18s) 我不能再凭猜断言…

  Thought for 16s, searched for 1 pattern (ctrl+o to expand)
```

**差异**：peri 用 `chars` + `∴` 前缀 + 无动作计数；Claude Code 用秒 + 动作计数、无前缀符号。

---

### 3.3 端到端整轮对比（示意）

**peri 现状**——**用户提供的真实 peri 日志**（非合成）。判定依据：`∴ Thought for N chars`（`chars` + `∴` 前缀）是 peri 独有格式，Claude Code 用秒 + 动作计数，两者不会混淆。

```text
❯ SF20260000000001 这单解析是解析item绑定还是catalog呀  为啥报错了啊

∴ Thought for 341 chars (ctrl+o to expand)

● Bash(pwd && ls -la)

∴ Thought for 105 chars (ctrl+o to expand)

∴ Thought for 78 chars (ctrl+o to expand)

● Bash(ls -la /d/code/acme_order/CLAUDE.md; wc -l /d/code/acme_order/CLAUDE.md)

∴ Thought for 153 chars (ctrl+o to expand)

● Bash(cat /d/code/acme_order/CLAUDE.md)

∴ Thought for 155 chars (ctrl+o to expand)

∴ Thought for 473 chars (ctrl+o to expand)

● Bash(cd /d/code/acme_order && grep -rn "hasBatchParseItemMap\|flushPareItem\|resetOrderProductForBatchParseItem" app --include=*.php | head -50)

∴ Thought for 290 chars (ctrl+o to expand)

∴ Thought for 82 chars (ctrl+o to expand)

● Bash(cd /d/code/acme_order && ls app/Http/Controllers/Logic/ | grep -i "ParseItem\|ItemMap" ; find app ...)

∴ Thought for 344 chars (ctrl+o to expand)

● Bash(cd /d/code/acme_order && grep -n "DB_HOST\|DB_PORT\|DB_DATABASE..." .env | head -40)

∴ Thought for 268 chars (ctrl+o to expand)

∴ Thought for 435 chars (ctrl+o to expand)

● Bash(cd /d/code/acme_order && which mysql php 2>&1; php -v 2>&1 | head -3)

● Write(D:\code\acme_order\storage\probes\tmp_sf20260924983025.php)

∴ Thought for 323 chars (ctrl+o to expand)

● Bash(cd /d/code/acme_order && php storage/probes/tmp_sf20260924983025.php 2>&1 | head -80)

  ...（持续数十轮 Bash / Read / Write，每轮 Thought 与工具各自成行）...
```

**三个刷屏来源**（从真实日志可数出）：
1. **`∴ Thought for N chars`**：每个思考段一行（含只有 78/82 chars 的极短思考）；
2. **`● Bash(...)`**：每条命令一行（真实日志中占绝大多数）；
3. **`● Write(...)`**：每次写探针脚本一行。

**peri 改后**——思考与只读工具各自收成一行，Bash 仍显示命令（对齐 Claude Code）：

```text
❯ SF20260000000001 这单解析是解析item绑定还是catalog呀  为啥报错了啊

  Thought for 4s (ctrl+o to expand)
● Bash(pwd && ls -la)
● Bash(ls -la /d/code/acme_order/CLAUDE.md; wc -l /d/code/acme_order/CLAUDE.md)
● Bash(cd /d/code/acme_order && grep -rn "hasBatchParseItemMap..." app --include=*.php | head -50)
● Bash(cat /d/code/acme_order/CLAUDE.md)
  Thought for 5s, read 1 file (ctrl+o to expand)
● Bash(cd /d/code/acme_order && ls app/Http/Controllers/Logic/ | grep -i "ParseItem\|ItemMap")
● Bash(cd /d/code/acme_order && grep -n "DB_HOST\|DB_PORT..." .env | head -40)
● Write(D:\code\acme_order\storage\probes\tmp_sf20260924983025.php)
  Thought for 6s, read 1 file (ctrl+o to expand)
● Bash(cd /d/code/acme_order && php storage/probes/tmp_sf20260924983025.php 2>&1 | head -80)

● 我先给你结论：这单走的是「绑定item管理」，不是 catalog。

✻ Cogitated for 15s · done 10:30
```

> 关键变化：**`∴ Thought for N chars` → `Thought for Ns[, 动作计数]`**（去 `∴` 前缀、chars 改秒、连续思考合并计数）；**多轮极短 Thought（78/82 chars 那种）合并为一行**，不再是每轮占一行。

> 注：改后只读工具（Read/Glob/Grep）折叠为计数（`read 1 file, listed 1 directory`）；**Bash 保持显示实际命令**，与 Claude Code 一致（见 §2.4）。具体词表见 §5。

---

### 3.4 最新 Claude Code 执行截图原始素材（用户提供，**目标规格**）

以下为 **Claude Code 最新版**（非 peri）的实际渲染，是本设计的**对齐目标**：

```text
Thought for 6s (ctrl+o to expand)
● 先不写码。我先确认 docs 目录位置和既有文档格式，别放错地方。

Listed 1 directory (ctrl+o to expand)          ← 纯动作行（无 Thought 前缀）

● Bash(git check-ignore -v docs spec 2>&1; echo "=== .gitignore ==="; grep -nE "docs|spec" .gitignore | head)
  ⎿ === .gitignore ===                       ← Bash 显示命令 + 输出摘要

Thought for 3s (ctrl+o to expand)
● docs/ 与 spec/ 都在 git 里。看下既有文档的组织习惯，好对齐格式。

Read 1 file, listed 1 directory (ctrl+o to expand)   ← 只读工具折叠计数

● 明白了。我按 docs/designs/ 的既有约定（2026-05-29-fix-llm-error-amnesia.md 那种日期前缀 design 文档）来写。

Reading 1 file… (ctrl+o to expand)             ← 进行时态
  ⎿ docs\designs\2026-05-29-fix-llm-error-amnesia.md
✳ Newspapering… (40s · ↓ 1.5k tokens · thinking more)   ← thinking more 实锤
  ⎿ Tip: Say "fan out subagents" and Claude sends a team.
```

**与 peri 现实输出的对照**（peri 现状来自本会话真实记录）：

| 形态 | Claude Code（目标） | peri 现状（真实记录） |
|------|---------------------|------------------------|
| 思考行 | `Thought for 6s (ctrl+o to expand)` | `∴ Thought for 341 chars (ctrl+o to expand)` |
| 纯动作行 | `Listed 1 directory (ctrl+o to expand)` | 无（折叠态仅出错才显示） |
| 折叠计数 | `Read 1 file, listed 1 directory` | 无计数文案 |
| 进行时 | `Reading 1 file…` | 无 |
| Bash | `● Bash(...)` + `⎿ 输出前几行` + `... +N lines` | `● Bash(...)` 仅命令，**无输出摘要** |
| 运行中 spinner | `✳ {verb}… (40s · ↓ 1.5k tokens · thinking more)` | `✢ {verb}… (33s · ↓ 2.6k tokens)`（无第三字段） |

> ⚠️ **本版更正**：上一稿曾把上述 `Listed 1 directory` / `Reading 1 file…` **误认作 peri 已实现**。实际这些是 **Claude Code** 的输出，peri **尚未实现**。

---

## 4. 差异总表

| # | 维度 | Claude Code | peri 现状 | 归属改动 |
|---|------|-------------|-----------|----------|
| 1 | spinner 第三字段 | 有（四态） | 无 | A |
| 2 | spinner verb 语义 | 按阶段（Thinking/Searching…） | 「正在生成回复…」写死，思考期不切 | A |
| 3 | spinner 计时点 | 段级 | 仅**回合级**（`SpinnerState::start_time`） | A |
| 4 | 消息区思考行 | `Thought for Ns, <动作>` | `∴ Thought for N chars` | B |
| 5 | 只读工具折叠文案 | `read 1 file, listed 1 directory` | 折叠态仅出错才显示，无计数文案 | B |
| 6 | 只读工具进行时态 | `Reading 1 file…` | 无 | B |
| 7 | 纯动作行（无思考） | `Listed 1 directory` | 无（Bash/Read 各自成行） | B |
| 8 | **Bash 输出展示** | **显示输出前几行 + 截断提示** | **不显示**（折叠态仅 header） | B（新增） |
| 9 | 截断提示措辞 | `... +N lines (ctrl+o to expand)` | `... (N more lines)` → 已改为 `... (N more lines) (ctrl+o to expand)` | B（已裁决） |
| 10 | Write/Edit 展示 | 路径 + diff 预览行数 | **一致**（也有 diff） | 已对齐 |
| 11 | 回合结束行 | `✻ {verb} for {elapsed} · done HH:MM` | **已有**（`message_area.rs:100`） | 已对齐 |

**注意 #10/#11**：peri 在 **Write/Edit diff、回合结束行** 两处**本就已对齐**，别误改。
**注意 #8**：**Bash 输出展示 peri 不显示**——这是本轮新确认的 gap（此前误标"已对齐"，已纠正）。

---

## 5. 实现路径

**硬约束**（来自 `CLAUDE.md`）：
- ✅ **不改**系统提示词内容、**不改**消息顺序、**不改**事件语义（`ExecutorEvent` / `AgentEvent` 变体定义与映射）。
- ⚠️ **需改**：`agent_ops/mod.rs` 的事件 **handler 内部逻辑**（新增 spinner 状态切换）——这是「事件处理代码」，与「事件语义/管线结构」不同。下面措辞已区分。

### 改动 A

1. **建立 thinking 态**：在 `agent_ops/mod.rs` 的 `AiReasoning` 分支（`:388`，**当前只喂 pipeline、完全不碰 spinner**）新增 spinner 切换 —— 收到首条 `AiReasoning` 时进入 thinking；收到 `AssistantChunk`（`:286`）或 `ToolStart`（`:172`）时切出 thinking。
   > 📌 **前提事实**：`SpinnerMode::Thinking` **已定义但全仓库无任何一处 `set`**（已 grep 验证）。实现本改动的实际含义 = **首次把该模式接入**。
2. **新增计时点**（`peri-widgets/src/spinner/mod.rs`）：
   - `thinking_started_at: Instant`（当前思考段起点）
   - `last_thought_ms: u64`（上一段思考耗时）
   - `thinking_round: u32`（本轮第几次思考）
3. **尾字段状态机**：渲染时（`message_area.rs:74` 后）按「是否在思考 / 段是否结束 / 段时长 / 轮次」追加 `· {尾字段}`。
4. **verb 行为改为固定**（方案 A）：移除 `ToolStart` 处的 `set_verb`（`:213-216`）与 `AssistantChunk` 处的 `set_mode_with_label(Responding)`（`:293-300`）造成的逐段换词；整轮保持一次性选定的动词。
5. **配色四档**（§2.7）：`verb` 与状态词随「当前思考段耗时」升温；`elapsed` / `tokens` / `thought for Ns` 保持 `MUTED`。色值在 `theme.rs` 增常量。

### 改动 B

1. **跨 VM 统计**：原因 `Reasoning` block 在 `AssistantBubble` 内部，而工具计数在相邻的 `ToolBlock` / `ToolCallGroup` VM 外部。建议在 `messages_to_view_models()` + `aggregate_tool_groups()` **之后**做一次后处理，扫描「AssistantBubble(Reasoning) → 相邻只读工具组」并注入计数。
2. **动作词表**（仅**只读工具**折叠为计数；Bash / Write / Edit **不折叠**）：
   | 类别 | 覆盖工具 | 文案模板（推断） |
   |------|----------|------------------|
   | Search | Grep | `searched for {n} pattern(s)` |
   | Glob | Glob | `listed {n} director(y/ies)` |
   | Read | Read | `read {n} file(s)` |

   > Bash / Write / Edit **不进计数词表** —— 它们各自展示命令 / diff（见 §2.4）。这与 peri 现有行为部分一致（peri 也不折叠 Bash/Write/Edit），**差异只在文案与进行时态**。
3. **时长来源**：改动 B 的秒数同样依赖 §A-2 的计时点；历史恢复（restore）后无秒数时退化策略见 §6。

### 「工具刷屏」的连带修复

**重新定性**（依据执行截图 + 用户确认）：Claude Code 里 **Bash 并不折叠**，它逐条显示实际命令。所以「几十条 Bash 刷屏」在 Claude Code 里**同样存在**——这不是 peri 的缺陷，而是设计如此（Bash 命令本身是有效信息）。

peri 与 Claude Code 在 Bash 上的差异（**修正后**）：
1. **输出展示**：Claude Code 显示输出前几行 + `... +N lines`；**peri 非详细模式不显示 Bash 输出**（用户确认）；
2. **折叠文案**：peri 的只读工具聚合后无计数文案（`ToolCallGroup` 折叠态仅出错时显示），Claude Code 有 `read 1 file, listed 1 directory`；
3. **进行时态**：Claude Code 有 `Reading 1 file…` 进行中形态，peri 无。

因此**不需要**把 Bash 纳入聚合去「消除刷屏」——方向与 Claude Code 相反。真正要做的是改动 B 的**计数文案 + 进行时态 + Bash 输出摘要**。

---

## 6. 开放问题（剩余待定）

1. ~~**`still thinking` 阈值**~~ —— 默认 10s 可调（未证实，见 §2.2）。
2. **`thinking more` 判定**：假设为「本轮思考**被打断过**后的第 2+ 段」，**未证实**，需实测确认。
3. ~~**两态优先级**~~ —— **已定：`still thinking` 优先**（一段思考既被打断过又超阈时，显示 `still thinking`）。
4. **动作词表确切措辞**：`Read 1 file, listed 1 directory` / `searched for 1 pattern` 已实锤；**单复数边界**（0/1/N）待更多实例确认。
5. **restore 后的秒数**：历史恢复时无 thinking 计时数据，`Thought for Ns` 退化为 `chars` 还是隐藏？（Claude 行为未知。）
6. **verb 是否语义化**：是否把 `Glob/Grep/Read/Bash` 映射到 `Searching…/Reading…` 等语义 verb？（本次决策未覆盖。）
7. **进行时态范围**：`Reading 1 file…` 这类进行中形态，是否所有只读工具都要？（本次暂不纳入，待定。）
8. ~~**Bash 输出摘要策略**~~ —— **已定**：显示前 3 行，保留 peri 措辞 `... (N more lines)` 并追加 `(ctrl+o to expand)` 引导语（2026-09-29 二次裁决）。
9. **配色阈值校准**：5s 有实测依据，15s / 30s 为产品推导；实现为可调常量，实测后校准。
10. **两套时间阈值是否应联动**：状态词切换（10s）与配色（5/15/30s）当前独立。若实测发现 Claude 二者联动，需合并（见 §2.2 末）。
11. **浅黄色值**：`#FFD966` 为暂定，未对照 Claude 实测。

---

## 7. 附：关键代码锚点速查

> 以下锚点已于 2026-09-29 **逐条 `sed` 核验**，与实际代码一致。

| 文件 | 行 | 用途 | 核验 |
|------|----|------|------|
| `peri-tui/src/ui/main_ui/message_area.rs` | 51-75 | 运行中 spinner 行拼接（`loading` 分支） | ✅ |
| `peri-tui/src/ui/main_ui/message_area.rs` | 99-102 | 回合结束总结行（已对齐，勿改） | ✅ |
| `peri-widgets/src/spinner/mod.rs` | 60-90 | `set_mode_with_label` 状态切换 | ✅ |
| `peri-widgets/src/spinner/mod.rs` | 115-117 | `elapsed_ms()` 回合计时 | ✅ |
| `peri-widgets/src/spinner/verb.rs` | 332-335 | `pick_summary_verb()` 随机完成动词 | ✅ |
| `peri-tui/src/app/agent_ops/mod.rs` | 388-399 | `AiReasoning` 处理（**确认：完全不碰 spinner**） | ✅ |
| `peri-tui/src/app/agent_ops/mod.rs` | 203-216 | `ToolStart` 处 `set_mode(ToolUse)` + `set_verb(工具名)`（**方案 A 要移除**） | ✅ |
| `peri-tui/src/app/agent_ops/mod.rs` | 286-300 | `AssistantChunk` 处 `set_mode_with_label(Responding)`（**方案 A 要移除**） | ✅ |
| `peri-tui/src/app/agent_ops/acp_bridge.rs` | 115-126 | `agent_thought_chunk` → `AiReasoning` | ✅ |
| `peri-tui/src/ui/message_render.rs` | 1015-1066 | `ContentBlockView::Reasoning` 渲染（`Thought for N chars`） | ✅ |
| `peri-tui/src/ui/message_render.rs` | 1165 | `if !state.collapsed`（Bash 输出被跳过处） | ✅ |
| `peri-tui/src/ui/message_render.rs` | 1211 | Read 折叠摘要（仅 Read，Bash 无） | ✅ |
| `peri-tui/src/ui/message_render.rs` | 1187 | 截断措辞 `... (N more lines)` | ✅ |
| `peri-tui/src/ui/message_view/tools.rs` | 22-31 | `ToolCategory::from_tool_name`（仅 Glob/Grep/Read/AskUser） | ✅ |
| `peri-tui/src/app/message_pipeline/transform.rs` | 63-99 | `messages_to_view_models`（改动 B 后处理挂点） | ✅ |
| `peri-tui/src/app/mod.rs` | 559-578 | `set_loading` 加载态切换 | ✅ |

**核验发现的 PRD↔代码 不一致（已修正）**：

| # | 原 PRD 表述 | 实际代码 | 处置 |
|---|-------------|----------|------|
| 1 | §5 标题「不碰事件管线」 | 改动 A **必须**改 `agent_ops/mod.rs` 的 handler 逻辑 | 已改正标题 + 拆清「语义不碰 / handler 要改」 |
| 2 | 未提 `SpinnerMode::Thinking` 现状 | 该枚举**已定义但全仓库从未 `set`** | 已补为改动 A 前提事实 |
| 3 | 改动 A 未提「方案 A 需移除现有 set_verb」 | `ToolStart`(:216)/`AssistantChunk`(:293) 会逐段换词 | 已补为改动 A 第 4 步 |

---

## 8. 实现回填（编码完成后的「实现 vs PRD」差异）

> 编码已完成（含审计修复）。本节记录实现相对 PRD 的**偏离与补齐**。

### 8.1 时长来源（偏离 PRD §5）

- **PRD 原述**：改动 B 的秒数「依赖 §A-2 的计时点」（spinner 侧）。
- **实际实现**：秒数在 **LLM 流式层独立计时**（`anthropic/stream.rs` / `openai/stream.rs` 记录 reasoning 段起止），
  写入 `ContentBlock::Reasoning.duration_ms` 并**随消息持久化**。
- **判定**：效果优于 PRD —— 使 `Thought for Ns` 在**历史恢复后仍可用**。PRD §6-5「restore 后无秒数」自动消解。

### 8.2 合并规则的补充（PRD §2.8）

实现时明确了两条 PRD 未写死的语义：

1. **含可见文本的 bubble 属「回答段」，不并入工具探索段**（段边界）。
2. **非只读工具（Bash/Write/Edit）不打断合并段**，仅跳过不计数。

### 8.3 未实现项（仍 open）

| 项 | 状态 | 说明 |
|----|------|------|
| 进行时态 `Reading 1 file…` | **未做** | PRD §6-7 明确「本次暂不纳入」 |
| 配色阈值校准（15s/30s） | 待实测 | 代码为可调常量，仅 5s 有依据 |
| `thinking more` 判定 | 待实测 | 「第 2+ 段」为假设 |

### 8.4 审计问题修复状态

首轮 P0-1（Anthropic `duration_ms` 链路断裂）与二轮 R1–R6 已修复；P1-2/3/5、P2-6 已补齐。
详见 `2026-09-29-thinking-status-line-code-audit.md` 对照。

### 8.5 §7 锚点行号

§7 核验表的行号为**编码前**快照；编码后行号已变动，如需精确定位以 grep 函数名为准。

