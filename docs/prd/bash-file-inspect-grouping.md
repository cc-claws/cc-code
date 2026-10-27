# PRD：文件查询类 Bash 并入只读工具组

> 状态：待评审
> 日期：2026-10-27
> 影响 crate：`cc-middlewares`（工具 schema）、`cc-tui`（VM / 聚合 / 渲染）

## 1. 背景与目标

### 1.1 现象

Agent 排查问题时大量用 Bash 做文件查看（`cat` / `ls` / `grep` / `head` / `find` / `sed -n`），
每次调用在消息区独立占一块，普通（非详细）模式下表现为连续刷屏：

```text
● Bash(ls /c/Users/adim/.cargo/registry/cache/*/ | grep -iE '^(dhat|pprof)' | head)
  ⎿  Error: Exit code 2
     === registry src dirs ===
● Bash(cd D:/code/peri && grep -n 'mimalloc' cc-tui/Cargo.toml; echo '=== backtrace ===')
  ⎿  72-
     73-[target.'cfg(not(target_os = "windows"))'.dependencies]
     … +16 lines (ctrl+o to expand)
● Bash(ls /d/cargo/registry/src/*/ | head -3; echo '=== cached crates ===')
  ⎿  /d/cargo/registry/src/mirrors.tuna...
```

而同样属于"只读查看"的 `Read` / `Grep` / `Glob` 早已被折叠成一行计数摘要，不会刷屏。
差异不是语义造成的，而是**聚合判定只看工具名白名单**（见 §2）。

### 1.2 目标体验

| 场景 | 现在 | 目标 |
|------|------|------|
| 连续文件查询 Bash | 每条一块，刷屏 | 与其他只读工具并成一组，普通模式只显示一行计数 |
| **单条**文件查询 Bash | 命令 + 3 行输出 | **折成一行计数**（与 `Read` 同构，与参考实现一致），`Ctrl+O` 展开 |
| 构建 / 测试 / git / 安装等真执行命令 | 单独一块 | **完全不变** |
| 运行中的文件查询 Bash | 命令 + 实时输出 + `Ctrl+B` 提示 | **也入组**（计数含在跑的这条，零闪烁）；代价是不再显示独立块与实时输出（见 §5.4、§13.1） |
| 已转后台的 Bash | 单条 + 后台指示 | **不聚合**，保持单条（见 §5.4） |
| 查看被折叠的细节 | — | `Ctrl+O`（详细模式）逐条展开，`● Bash` + 命令 + 输出 |
| 出错的 Bash | 显示错误摘要 | 仍在组内显示错误摘要，不因折叠丢失报错 |

示例（目标形态）：

```text
# ① 前面有 thinking：计数并入 Thought 行（顺序固定为 read/listed/searched/ran shell/recalled）
  Thought for 19s, read 1 file, ran 2 shell commands (ctrl+o to expand)

● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s

# ② 前面没有 thinking：独立一行动作摘要（首字母大写），同样带展开提示
  Ran 2 shell commands (ctrl+o to expand)

● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s

# ③ 单条文件查询：折成一行，不保留输出（与 Read 一致）
  Ran 1 shell command (ctrl+o to expand)
```

> Thought 行的 `(ctrl+o to expand)` 是既有行为（`cc-tui/src/ui/message_render/mod.rs:1556`
> 在非详细模式下无条件拼接）；独立摘要行目前没有该提示，本次一并补上（R6）。

### 1.3 不做什么

- 不改「构建/测试/git/安装」等命令的展示形态（它们不是文件查询）。
- 不做命令文本启发式识别（参考实现的做法见 §3.2，本期不采用）。
- 不给**已完成**的组附加 hint（参考实现的 `⎿` 只在进行中渲染；已冻结的行不补提示，见 §13.3）。
- 不改 SubAgent 抽屉内的工具列表（该处不聚合，见 §6）。
- 不改 `cc-widgets` 的 `ToolCallWidget` 体系（不在活跃渲染路径，见 §5.7 排除项）。

## 2. 现状与代码证据

聚合机制已存在，与本次需求直接相关的现状：

| 事实 | 证据 |
|------|------|
| 相邻只读工具被合并为 `ToolCallGroup` | `cc-tui/src/ui/message_view/aggregate.rs:13` `aggregate_tail_tool_groups` |
| 归类只看工具名，只认 4 个工具 | `cc-tui/src/ui/message_view/tools.rs:95` `ToolCategory::from_tool_name`（`Glob`/`Grep`/`Read`/`AskUserQuestion`） |
| Bash 返回 `None`，**永不进组** | 同上 `_ => None` |
| 聚合组默认折叠，折叠态只显示一行动作摘要 | `cc-tui/src/ui/message_render/mod.rs:2679`（组渲染入口）、`:2873` 起的折叠分支 |
| 计数摘要文案在此生成 | `cc-tui/src/app/message_pipeline/transform.rs:215` `readonly_action_summary_ext` |
| Bash 已有 `description` 参数，但 TUI 不消费 | 参数定义 `cc-middlewares/src/middleware/terminal.rs:195`；TUI 只取 `command` 作标题：`cc-tui/src/app/tool_display.rs:359` |
| `description` 只用于后台完成 / stall 通知 | `cc-agent/src/shell/mod.rs:106` `ShellRequest.description` |
| 「详细模式」是唯一模式开关 | `cc-tui/src/app/ui_state.rs:111` `detail_mode`，`Ctrl+O` 切换（`cc-tui/src/event/keyboard/shortcuts.rs:24-34`） |

结论：缺的不是"命令说明"，而是**一个可供机器判定的分类信号**。

## 3. 方案选型

### 3.1 选定：给 Bash 加可选布尔参数 `file_inspect`（模型声明）

- 模型在做纯文件查看时显式置 `true`。
- 缺省 `false` → 行为与今天完全一致；模型漏填只会退化成现状，**不会误折叠**。
- 判定精准、可测试、不依赖命令文本解析。

### 3.2 本期不采用（但已核实可行）

| 方案 | 不采用理由 |
|------|----------|
| TUI 侧按命令文本启发式识别 | 不是不可行——参考实现就是这么做的（§3.4）。但它要求真 shell 分词 + 三档命令白名单 + 重定向目标跳过 + 中性命令忽略，是一套需要长期维护的解析器；本期先用模型声明达到同样效果，启发式留作漏填兜底（§11） |
| 复用已有自由文本 `description` 做判定 | 自由文本无法可靠分类（"看下依赖" vs "跑下依赖检查" 语义接近） |
| 枚举细分为 `search` / `read` / `list` | 用户明确要求范围严格限定在文件查询，细分带来的摘要收益不抵模型判定成本（YAGNI） |
| 用命令首行 `# 注释` 当分类信号 | 注释是**给人看的标签**，不保证可枚举；参考实现也只把它当标签/进度 hint，不参与分类。作为标签能力记在 §11 |

### 3.3 范围界定（用户决策）

严格限定**文件内容的查看与列举**：`ls` / `cat` / `head` / `tail` / `find` / `wc` / `rg` / `grep` 读文件。
`git status` / `git log` / `git diff` / `cargo tree` 等**不纳入**，仍单条展示。

参考实现的分类白名单（作为范围对照，取值见 `BashTool.tsx:95/98/121`）：

- search：`find` `grep` `rg` `ag` `ack` `locate` `which` `whereis`
- read：`cat` `head` `tail` `less` `more` `wc` `stat` `file` `strings` `jq` `awk` `cut` `sort` `uniq` `tr`
- list：`ls` `tree` `du`
- 语义中性（不改变管道性质，不参与判定）：`echo` `printf` `true` `false` `:`

我们的范围比它窄（不含 `jq`/`awk` 等数据处理、不含 `which`/`locate`），且判定权交给模型声明。

### 3.4 与参考实现的对照（已核源码）

| 维度 | 参考实现 | 本 PRD |
|------|---------|--------|
| 分类来源 | Bash 工具自带 `isSearchOrReadCommand()`：shell 分词 + 运算符处理 + 三档白名单，管道中任一命令不在白名单即整条不折叠（`BashTool.tsx:164-233`） | 模型声明 `file_inspect` |
| 参与聚合的 Bash | search/read/list 三类 + 非查询类 Bash（后者单独计数 `Ran N bash commands`） | 仅 `file_inspect=true` 的文件查询，普通 Bash 不进组 |
| 单条是否折叠 | **折叠**：`flushGroup()` 只看组是否为空，没有条数门槛（`collapseReadSearch.ts:843`）；全文唯一的 `>= 2` 在 `:1170`，那是状态栏活动摘要 | 同左（R4） |
| 折叠行附加信息 | 仅"进行中"渲染 hint 第二行（`⎿ $ <命令>`，命令首行 `# 注释` 优先，`commandAsHint` 兜底截断）：`CollapsedReadSearchContent.tsx:492`。**完成后只剩计数 + `(ctrl+o to expand)`** | 同左：`⎿` 只在活跃组显示，内容优先最近一条工具参数，无工具时思考尾部（≤10 行），≥2s 出现（§13.3） |
| 运行中进度 | 折叠行追加 `(12s · 40 lines)`，超过 2 秒才显示（`CollapsedReadSearchContent.tsx:263-290`） | 运行中也入组，但**不显示**进度后缀（无实时输出，与参考实现取舍一致） |
| 展开 | `ctrl+o` / `--verbose` 逐条渲染，每条带 1 行结果摘要（`CollapsedReadSearchContent.tsx:195-232`） | 同左，且 Bash 条目复用退出码/stderr 头处理（§5.6） |

> 结论：**"单条也折叠"是参考实现的既有行为**，不是本次设计引入的退化；我们跟随它，
> 运行中同样入组（§13.1 修订了早期的"未落定不聚合"）；已转后台的仍保持单条，以保住 `Ctrl+B` 后的后台状态显示。

## 4. 需求清单

| 编号 | 需求 |
|------|------|
| R1 | Bash 工具新增可选布尔参数 `file_inspect`，非必填，缺省 `false`，不参与执行逻辑 |
| R2 | 工具描述（`description()`）补一句引导，提升模型置位命中率；**不改系统提示词**（避免缓存前缀失效） |
| R3 | `ToolBlock` VM 携带 `file_inspect`，三条构造路径（流式 / reconcile / 历史恢复）取值一致 |
| R4 | 聚合判定改为「工具名 + 标记」，`Bash && file_inspect` 可并入只读组 |
| R5 | 计数摘要新增 `ran N shell command(s)`，与 `read` / `listed` / `searched` 并列 |
| R6 | 渲染：折叠态不显示命令与输出；详细态逐条显示命令与输出，退出码等格式与单条 Bash 一致；**独立摘要行补 `(ctrl+o to expand)` 提示** |
| R7 | 历史恢复（`/resume`、fork、reconcile）与流式路径产出的 VM 必须等价，不得出现"重连后组散了" |
| R8 | **Bash 聚合门槛（§13.1 修订）**：`file_inspect = true` 即可入组，**含运行中**；仅排除 `shell_backgrounded == true` 与带 `BackgroundTaskStarted` 标记的两类。失败工具（`is_error` 或非零退出码）**仍入组**，折叠态给截断错误摘要 |
| R9 | 单条文件查询 Bash 也折叠（与参考实现一致，无条数门槛），完成后只显示计数行 + 展开提示 |
| R10 | **计数与聚合共用同一判定**：`ran N shell commands` 只在聚合组内条目上计数；`ToolBlock` 分支的计数逻辑不得统计 Bash。否则运行中的 Bash 会既出现在摘要里、又整块显示 |

## 5. 详细设计

### 5.1 工具参数（`cc-middlewares`）

`cc-middlewares/src/middleware/terminal.rs:177` `parameters()` 增加：

```json
"file_inspect": {
  "type": "boolean",
  "description": "Set to true when this command only inspects files and their contents (ls/cat/head/tail/find/wc/rg/grep over files) and changes nothing on disk. Also set it for a single such command — the frontend collapses them into one summary line. Leave it false for build, test, git, install, or any command that modifies files or drives an external process."
}
```

- 不加入 `required`（现为 `["command"]`）。
- `BashTool::invoke` 的入参解析（`terminal.rs:213` 起）**不读取**该字段，纯展示标记。
- **禁止把 `file_inspect` 接进任何权限 / 审批 / hook / 遥测判断**：它是模型自证的展示字段，
  一旦被 HITL 语义门或权限逻辑消费，模型就能靠置 `true` 影响安全决策。
- `description()`（`terminal.rs:173`）追加一句：做文件查看（含单条）时请置 `file_inspect = true`，以便前端折叠展示。
- 判定标准写"是否只读查看文件"而不是"输出要不要给人看"——后者会让模型对单条命令犹豫，
  而单条同样会折叠（R9）。

### 5.2 判定入口（`cc-tui`）

`cc-tui/src/ui/message_view/tools.rs` 新增（与 `ToolCategory` 同处，避免把判定逻辑塞进纯展示模块 `app/tool_display.rs`）：

```rust
/// 文件查看类 Bash（模型显式声明 file_inspect=true 才成立）
pub fn is_file_inspect(tool_name: &str, input: &serde_json::Value) -> bool {
    tool_name == "Bash" && input.get("file_inspect").and_then(|v| v.as_bool()) == Some(true)
}
```

`cc-tui/src/ui/message_view/tools.rs:79` 的 `ToolCategory` 新增变体 `Inspect`，并新增第二入口（`from_tool_name` 在 `:95`，保留给纯名字场景）：

```rust
pub fn from_tool_block(tool_name: &str, file_inspect: bool) -> Option<Self> {
    if tool_name == "Bash" && file_inspect {
        return Some(Self::Inspect);
    }
    Self::from_tool_name(tool_name)
}
```

聚合与渲染只用 `category` 判断"是否 AskUser 类、是否同类可并"，新增变体不影响既有分支
（`aggregate.rs:40` 的 `(cat == AskUser) == (entry_cat == AskUser)`、渲染层 `mod.rs:2688` 的 AskUser 特例）。

### 5.3 标记如何到达渲染层

`ToolBlock`（`cc-tui/src/ui/message_view/mod.rs:129`）新增字段 `file_inspect: bool`，三条构造路径全部填充：

| 路径 | 位置 | input 来源 |
|------|------|-----------|
| 实时流式（ToolStart） | `cc-tui/src/app/message_pipeline/transform.rs:181` `build_tool_start_vm` | `ToolStart.input` |
| reconcile（已完成工具） | `cc-tui/src/app/message_pipeline/reconcile.rs:225` | `completed_tools[].input` |
| 历史恢复（`/resume`、fork） | `cc-tui/src/ui/message_view/mod.rs:954` | `prev_ai_tool_calls` 中的同 id `arguments` |

**必须同步 `Hash` 实现**：`cc-tui/src/ui/message_view/mod.rs:439` 的 `ToolBlock` hash 分支加入
`file_inspect.hash(state)`。否则标记不同、其余字段相同的 VM 会命中同一 hash 被复用，
出现"刚触发折叠又弹回单条"的抖动。

`ToolEntry`（`tools.rs:108`，derive `Hash`）新增 `file_inspect: bool`：聚合后的计数与渲染只读 entries，
不再持有原始 input。

### 5.4 聚合规则

`cc-tui/src/ui/message_view/aggregate.rs:24`（外层解构）与 `:29`（内层解构）各多取 `file_inspect`，
判定从 `ToolCategory::from_tool_name(tn)`（`:25` / `:38`）换成 `ToolCategory::from_tool_block(tn, file_inspect)`。

规则保持现状语义，仅扩大参与者集合：

- 仍然**只在相邻**工具块之间聚合；`file_inspect = false` 的 Bash 与今天一样打断分组。
- **聚合门槛（R8，§13.1 已修订）**：`Bash` 满足 `file_inspect = true` 即可入组，**包含运行中的**；
  只保留两条排除：
  - `shell_backgrounded == true`（已转后台，需单条展示后台状态）
  - `BackgroundTaskStarted::parse(content).is_some()`
  
  失败的工具（`is_error` 或 Bash 非零退出码）**不剔除**，仍留在组内，
  折叠态由 `error_summary_lines` 给出截断摘要，完整内容按 `Ctrl+O` 查看。
  早期"运行中/带 timeout 不入组"的方案已废弃：它让计数比参考实现少 1，并产生"块闪一下再收起"。
  
  代价（已接受）：运行中的文件查询 Bash 不再有独立块与实时输出，`Ctrl+B` 的前台提示也不再显示
  （按键仍生效）。这是与参考实现对齐同一取舍。顺序依赖：`reconcile.rs` 先跑 `apply_shell_runtime`
  写入 `shell_backgrounded`，再跑 `aggregate_tool_groups`，所以聚合时读到的后台态是准确的。

  **历史恢复路径**：`messages_to_view_models` 不跑 `apply_shell_runtime`，恢复出来的
  `shell_backgrounded` 恒为 `false`，唯一有效门槛是内容里的 `BackgroundTaskStarted` 标记。
- **无条数门槛**：单条 `Bash(inspect)` 也成组（与参考实现一致，§3.4）。
- 允许跨类别混组（`Read` + `Grep` + `Bash(inspect)` 可同组）。
- `AskUserQuestion` 与其他类别互斥的规则不变。
- 空 reasoning bubble 仍被跳过。

### 5.5 计数与文案

`readonly_action_summary_ext`（`transform.rs:215`）新增 `shell_cmds: usize` 参数，插入位置在 `searched` 之后：

| 计数 | 文案 | 来源工具 |
|------|------|----------|
| reads | `read N file(s)` | `Read` |
| globs | `listed N director(y/ies)` | `Glob` |
| greps | `searched for N pattern(s)` | `Grep` |
| **shell_cmds** | **`ran N shell command(s)`** | **`Bash` + `file_inspect`** |
| recalled / updated | `recalled N memory(ies)` / `updated N memory(ies)` | 记忆路径的 `Read` / `Write` / `Edit` |

> N 是 **Bash 调用次数**，不是文件数（一条 `find` 可能命中多个文件），故文案用 `shell commands`
> 而非 `files`。单数形态 `ran 1 shell command`。

**计数范围（R10）**：`ran N` 只在"组内条目"上累加（含运行中的那条，与参考实现一致）：

1. `transform.rs:309` 起的独立动作摘要 —— 已经遍历的是 `ToolCallGroup.tools`，直接按
   `entry.file_inspect && entry.tool_name == "Bash"` 累加即可。
2. `transform.rs:378` 起的思考段内统计 —— 只有 `ToolCallGroup` 分支加 shell 计数；
   **`ToolBlock` 分支（`transform.rs:393-408`）不得统计 Bash**，因为能留在顶层的 Bash 一定是
   "未落定 / 带 timeout / 未声明 file_inspect"的那些，统计它会造成"摘要说跑了 N 条、下面还整块显示"。
3. `transform.rs:514` `absorb_preceding_tool_summaries` —— 同样只遍历 group entries。

> 实现建议：把 5 元组（`reads, globs, greps, recalled, updated`）改成 `ActionCounts` struct 再插入
> `shell_cmds`。现在全是 `usize` 位置参数，插错位置编译器不会报错，只会静默把 `ran N` 显示成别的计数。

### 5.6 渲染

三种摘要形态（非详细模式）：

```text
# ① 段内前面有 thinking → 并入 Thought 行（既有拼接逻辑，自动生效）
  Thought for 19s, ran 2 shell commands (ctrl+o to expand)

# ② 段内前面没有 thinking → 独立动作摘要行，首字母大写（既有 uppercase_first_char）
  Ran 2 shell commands (ctrl+o to expand)

# ③ 单条文件查询 → 同样是②的形态（无条数门槛，R9）
  Ran 1 shell command (ctrl+o to expand)
```

- **Thought 行**：`mod.rs:1556-1571` 已实现 `{timing}{actions}{hint}`，`hint` 在非详细模式下恒为
  `" (ctrl+o to expand)"`，本次**无需改动**。
- **独立摘要行**：`mod.rs:2874-2883` 目前只输出 `uppercase_first_char(summary)`，需在非详细模式
  追加同一提示串。由于 `standalone_action` 仅在折叠态渲染（详细态走逐条分支，不渲染该字段），
  提示直接在渲染层拼接，不写进 `standalone_action` 字符串（保持 hash 与数据层干净）。
  该改动对所有只读工具组的独立摘要行生效（`Read` / `Grep` / `Glob` / `Bash`），属一致性收益。
- **折叠态不显示的命令与输出**：`standalone_action` 一行摘要 + 错误摘要，
  命令与输出都不渲染，与 `Read` / `Grep` 完全同构。单条形态下用户要看到输出需 `Ctrl+O`，
  这是与参考实现一致的既定取舍（§3.4）。
- **详细态（`Ctrl+O`）**：组内逐条渲染（`mod.rs:2753` 分支）已能显示 `● Bash` + `args_display`，
  但输出走的是通用分支（`mod.rs:2850` `else if !entry.content.is_empty()`），**缺少 Bash 专属的
  退出码 / stderr 头处理，也没有行数上限**。需补一条 Bash 分支：
  - 复用 `transform_bash_output_lines(content, parse_exit_code(content), &mut error_header)`
    （`mod.rs:1969`，与单条 Bash 同源），保留非零退出的错误头。
  - 行数上限沿用单条 Bash 的口径（`mod.rs:1951` 起）：详细模式不限行，非详细模式本就不渲染输出。
  - 与单条 Bash 的差异：组内**不做**输出折叠提示行（`… +N lines (ctrl+o to expand)`），
    因为组本身就在详细态下才展开。
  - 详细态下 `standalone_action` 字段仍由 transform 生成并参与 hash（数据层不变），
    只是渲染路径不读它；`hint` 为空、entries 逐条展开。

### 5.7 实现触点清单

| # | 文件:行 | 改动 |
|---|---------|------|
| 1 | `cc-middlewares/src/middleware/terminal.rs:177` | `parameters()` 增加 `file_inspect` |
| 2 | `cc-middlewares/src/middleware/terminal.rs:173` | `description()` 追加模型引导 |
| 3 | `cc-tui/src/ui/message_view/tools.rs` | 新增 `is_file_inspect()` |
| 4 | `cc-tui/src/ui/message_view/mod.rs:129` | `ToolBlock` 增加 `file_inspect` |
| 5 | `cc-tui/src/ui/message_view/mod.rs:439` | hash 分支加入该字段 |
| 5b | `cc-tui/src/ui/message_view/mod.rs:262-290` | **手写 `PartialEq` 的 `ToolBlock` 模式也要加 `file_inspect`**（`shell_backgrounded: a/b_backgrounded` 那一段），否则仅该字段不同的两个 VM 会被判为相等 |
| 6 | `cc-tui/src/ui/message_view/mod.rs:954` | 历史恢复构造填充 |
| 7 | `cc-tui/src/ui/message_view/tools.rs:79` | `ToolCategory::Inspect` + `from_tool_block()` |
| 8 | `cc-tui/src/ui/message_view/tools.rs:108` | `ToolEntry` 增加 `file_inspect` |
| 9 | `cc-tui/src/ui/message_view/aggregate.rs:5` | `aggregatable_category()`：`from_tool_block` + R8 新门槛（`!file_inspect` / 已转后台 / 后台标记才排除） |
| 10 | `cc-tui/src/app/message_pipeline/transform.rs:181` | `build_tool_start_vm` 填充 |
| 11 | `cc-tui/src/app/message_pipeline/transform.rs:215` | `readonly_action_summary_ext` 签名 + 文案 |
| 12 | `cc-tui/src/app/message_pipeline/transform.rs:309` | 计数：独立动作摘要 |
| 13 | `cc-tui/src/app/message_pipeline/transform.rs:377` | 计数：段内（两个 match 分支） |
| 14 | `cc-tui/src/app/message_pipeline/transform.rs:514` | 计数：`absorb_preceding_tool_summaries`（→ 6 元组） |
| 15 | `cc-tui/src/app/message_pipeline/reconcile.rs:225` | 已完成工具构造填充 |
| 16 | `cc-tui/src/ui/message_render/mod.rs:2850` | 详细态新增 Bash 输出分支 |
| 17 | `cc-tui/src/ui/message_render/mod.rs:2874` | 独立摘要行追加 `(ctrl+o to expand)` |
| 18 | `cc-tui/src/app/message_pipeline/mod.rs:829-833` | `tool_end_internal` 缺 pending 时 `input` 回落为 `Null`；`file_inspect` 必须再从 `current_ai_tool_calls` 的同 id arguments 取，否则流式折叠、reconcile 后散开（违反 R7） |
| 19 | `cc-tui/src/ui/render_layout/estimator.rs:164-180` | `ToolCallGroup` 的高度估算按 `rows(content)+3` 计；新增 Bash 输出分支后逐条核对滚动高度是否与实际一致 |
| 20 | `cc-tui/src/ui/message_view/mod.rs:1163` | `tool_block` 包装器（Agent Error / SubAgent 用）填 `file_inspect: false` |

**编译波及面（必须逐个补齐，否则 `cargo check` 直接失败）**：

`ToolBlock` 新增字段后，**构造点 57 处**（`grep -rn "shell_backgrounded:" cc-tui/src` 得 61 处，
其中 4 处不是构造：`message_view/mod.rs:150` 字段定义、`:266`/`:281` 的 `PartialEq` 模式、
`streaming_output.rs:44` 的 match 模式）：

- 生产 4 处：`message_pipeline/reconcile.rs`、`message_pipeline/transform.rs`、
  `ui/message_view/mod.rs:969`、`:1194`（`tool_block_with_id`，SubAgent / Skill 卡片 / Agent Error 共用）
- 测试 53 处：`ui/message_render/message_render_test.rs`(36)、`message_pipeline/shell_runtime_test.rs`(8)、
  `app/shell_command_test.rs`(3)、`ui/render_thread_test.rs`(3)、`message_pipeline/streaming_output_test.rs`(1)、
  `ui/background_task_render_test.rs`(1)、`ui/headless_test.rs`(1)

`ToolEntry` 新增字段后，全部**构造点 12 处**：

- 生产 1 处：`cc-tui/src/ui/message_view/aggregate.rs:43`
- 测试 11 处：`ui/message_render/message_render_test.rs`(9)、
  `ui/render_layout/layout_test.rs`(1)、`ui/render_thread_test.rs`(1)

> 测试构造点必须用 `cargo clippy --all-targets` 才会暴露（不带 `--all-targets` 时测试代码不参与编译，
> 但 CI 会挂），详见 §9。

**排除项（已核，不在活跃路径 / 无需改）**：

- `cc-widgets/src/tool_call/collapse.rs:1` `READ_ONLY_TOOLS`：仅服务 `ToolCallWidget` / `ToolCallState`，
  cc-tui 无任何引用（只在 `cc-widgets` 自身测试与 examples 使用）。
- `cc-tui/src/ui/message_view/mod.rs:1167` `tool_block_with_id`：三个调用方——Skill 调用卡片
  （`mod.rs:685`）、SubAgent 内部工具（`cc-tui/src/app/message_pipeline/mod.rs:922`）、
  `tool_block` 包装器（`mod.rs:1163`，被 `app/agent_ops/lifecycle.rs:337` 用于合成 "Agent Error" 块）。
  三者都不参与聚合，填 `false` 即可。
- `cc-tui/src/app/message_pipeline/shell_runtime.rs:107`：仅解构，不构造。

### 5.8 展示规格（逐帧 Demo）

以下 ASCII 全部来自**实际渲染输出**：用临时测试构造同一段 `BaseMessage[]`，经
`MessagePipeline::messages_to_view_models` + `render_view_model(width = 110)` 导出，核对完已删除临时测试。
帧 2/3 是改动前的实测输出；帧 4/5/7 是目标形态（尚未实现，按同一渲染管线推导）。

渲染格式的硬规则（决定了下面的每一帧）：

- **Thought 行没有符号**：2 空格缩进，形如 `  Thought for 19s, read 1 file (ctrl+o to expand)`
  （`mod.rs:1571` 的 `format!("  {timing}{actions}{hint}")`）。早期测试里出现的 `∴`
  是终端宽度探测用的合成串（`terminal_backend/compatible_test.rs:431`），**不是应用输出**。
- **单条工具行与组内条目格式相同**：`● Bash(<参数>)`，参数在圆括号内、接在名字后面
  （组内：`mod.rs:2810` 的 `format!("({})", summary)`；单条：`mod.rs:1732` 起）。超宽按列宽截断、`…` 收尾。
- 工具输出：首行前缀 `  ⎿  `，续行悬挂缩进（`BranchPrefix::PREFIX` / `HANG`）。
- 独立动作摘要行：2 空格 + 首字母大写（`mod.rs:2878` 的 `uppercase_first_char`）。
- Thought 行与后续工具块之间有一个**空行**（实测）。

#### 帧 1：同一次 agent 执行的工具事件（输入侧）

```text
[agent] Thinking… 19s
  ├─ Bash   for c in heaptrack drmemory umdh wpr vmmap procdump curl; do printf '%-12s ' "$c"; …
  │         file_inspect: true                    → ok（7 行输出，退出码 0）
  ├─ Bash   ls /d/cargo/registry/src/*/ | head -3; echo '=== cached crates ==='
  │         file_inspect: true                    → ok（3 行输出）
  ├─ Bash   grep -n -A20 '[features]' /d/cargo/…/libmimalloc-sys-0.1.49/Cargo.toml
  │         file_inspect: true                    → Error: Exit code 2
  ├─ Read   D:/code/peri/cc-tui/src/command/core/heapdump.rs
  └─ Bash   cd D:/code/peri && cargo build -p cc-tui
            （未置 file_inspect）
```

#### 帧 2：现状（改动前，普通模式）——13 行，刷屏（实测）

```text
  Thought for 19s, read 1 file (ctrl+o to expand)

● Bash(for c in heaptrack drmemory umdh; do printf '%-12s ' "$c"; done)
  ⎿  heaptrack    (not found)
     drmemory     (not found)
     umdh         (not found)
● Bash(ls /d/cargo/registry/src/*/ | head -3)
  ⎿  /d/cargo/registry/src/mirrors.tuna.tsinghua.edu.cn-4dc01642fd091eda//
● Bash(grep -n -A20 '[features]' /d/cargo/registry/src/.../libmimalloc-sys-0.1.49/Cargo.to…)
  ⎿  Error: Exit code 2
     === mimalloc 0.1.52 features ===
● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s
```

两条容易画错的细节（实测纠正）：

1. **单条 `Read` 在普通模式下完全不显示**：它已经是「1 条目的 `ToolCallGroup`」，
   计数被吸进 Thought 行的 `read 1 file`（`standalone_action = None` → 折叠分支什么都不渲染）。
   所以现状里也没有 `● Read(...)` 这一块。
2. `read 1 file` 与本次要新增的 `ran 3 shell commands` 是**同一条摘要行的兄弟项**，
   不是新增一整行。

#### 帧 3：现状（改动前，详细模式 / `Ctrl+O`）——实测

```text
  Thought for 19s, read 1 file
  ⎿  先看看有哪些 profiling 工具，再查 mimalloc 的 feature 开关。

● Bash(for c in heaptrack drmemory umdh; do printf '%-12s ' "$c"; done)
  ⎿  heaptrack    (not found)
     drmemory     (not found)
     umdh         (not found)
● Bash(ls /d/cargo/registry/src/*/ | head -3)
  ⎿  /d/cargo/registry/src/mirrors.tuna.tsinghua.edu.cn-4dc01642fd091eda//
● Bash(grep -n -A20 '[features]' /d/cargo/registry/src/.../libmimalloc-sys-0.1.49/Cargo.toml)
  ⎿  Error: Exit code 2
     === mimalloc 0.1.52 features ===
● Read(cc-tui/src/command/core/heapdump.rs)
  ⎿  Read 120 lines
● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s
```

- 详细态才显示 Reasoning 正文，首行前缀 `  ⎿  `，与工具输出同一套分支前缀。
- 详细态 Thought 行**不带** `(ctrl+o to expand)`。
- 单条 `Read` 在详细态展开成块（`● Read(<path>)` + `⎿  Read 120 lines`）。

#### 帧 4：目标（改动后，普通模式）——4 行（含空行）

```text
  Thought for 19s, read 1 file, ran 3 shell commands (ctrl+o to expand)

● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s
```

- 三条 `file_inspect` Bash + 一次 `Read` 合成一个只读组，计数并入 Thought 行。
- 计数顺序固定：`read` → `listed` → `searched` → `ran N shell commands` → `recalled` → `updated`。
- 未置 `file_inspect` 的 `cargo build` 不受影响，单条行格式（带括号）保持原样。

#### 帧 5：目标（改动后，详细模式 / `Ctrl+O`）

```text
  Thought for 19s, read 1 file, ran 3 shell commands
  ⎿  先看看有哪些 profiling 工具，再查 mimalloc 的 feature 开关。

● Bash(for c in heaptrack drmemory umdh; do printf '%-12s ' "$c"; done)
  ⎿  heaptrack    (not found)
     drmemory     (not found)
     umdh         (not found)
● Bash(ls /d/cargo/registry/src/*/ | head -3)
  ⎿  /d/cargo/registry/src/mirrors.tuna.tsinghua.edu.cn-4dc01642fd091eda//
● Bash(grep -n -A20 '[features]' /d/cargo/registry/src/.../libmimalloc-sys-0.1.49/Cargo.toml)
  ⎿  Error: Exit code 2
     === mimalloc 0.1.52 features ===
● Read(cc-tui/src/command/core/heapdump.rs)
  ⎿  Read 120 lines
● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s
```

与帧 3（现状详细模式）**逐字一致**——这是设计目标：折叠态从 12 行压到 3 行，展开态信息一行不少。
组内条目与单条用同一套 `● Bash(...)` 括号格式（`mod.rs:2810` `format!("({})", summary)`）。

#### 帧 6：组内出错时，错误摘要不因折叠而丢失

```text
  Thought for 19s, read 1 file, ran 3 shell commands (ctrl+o to expand)
  ⎿  Error: Exit code 2
     === mimalloc 0.1.52 features ===

● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s
```

错误摘要走既有 `error_summary_lines`（`mod.rs:737`）：取工具返回内容前 400 字符、最多
`ERROR_SUMMARY_MAX_LINES` 行，首行 `⎿` 前缀、续行悬挂缩进、错误色。
**注意**：摘要本身不带工具归属，若同一组内有多条失败，几段错误会连续出现、分不清是哪条命令——
本期不处理（见 §11）。

#### 帧 7：运行中的 Bash 也入组；仅后台化的保持单条（R8 修订版）

```text
  Thought for 8s, read 1 file (ctrl+o to expand)

● Bash(cd D:/code/peri && cargo test -p cc-tui --lib -j 1)
  ⎿  Running… 12s · 40 lines
     test result: …
     ctrl+b to run in background
```

转后台后仍是单条 + 后台指示，不会被折进只读组——这是本设计与参考实现的差异点（§3.4）。

#### 帧 8：无 thinking 的独立摘要行（含单条）

```text
  Ran 3 shell commands (ctrl+o to expand)

● Bash(cd D:/code/peri && cargo build -p cc-tui)
  ⎿  Finished dev profile in 42s
```

单条时：

```text
  Ran 1 shell command (ctrl+o to expand)
```

独立摘要行**行首无符号**、2 空格缩进、首字母大写（`mod.rs:2878` 的 `uppercase_first_char`）——
与 `Read 1 file` 的实测形态一致（临时测试里把 `standalone_action` 设为
`"read 1 file, ran 3 shell commands"`，渲染结果正是 `  Read 1 file, ran 3 shell commands`）。
`(ctrl+o to expand)` 由本次新增（R6）。

#### 行数对比

| 场景 | 改动前（实测） | 改动后 |
|------|--------------|--------|
| 帧 1 的普通模式渲染 | 帧 2：13 行 | 帧 4：**4 行**（含错误摘要时帧 6：6 行） |
| 同一段 `Ctrl+O` 展开 | 帧 3：16 行 | 帧 5：16 行（信息量不变） |

## 6. 边界与降级

| 场景 | 行为 |
|------|------|
| 模型不填 `file_inspect` | 与今天一致，单条展示（安全退化） |
| 模型滥填（如给 `cargo build` 置 true） | 会被折叠，但 `Ctrl+O` 可完整展开；属可恢复的展示偏差 |
| 写入型命令（`cat > f`、`sed -i`） | 依赖模型判断不置 true；判定逻辑不做二次校验（N 为命令次数，不做 shell 解析） |
| **运行中的文件查询 Bash** | 已入组（§13.1）：只剩组行与计数，不再有独立块/实时输出 |
| **已转后台 / 后台任务启动态** | 不聚合：`Ctrl+B` 后的后台指示与完成通知保持原样 |
| **单条文件查询 Bash** | 折成一行计数（§3.4 与参考实现一致），输出需 `Ctrl+O` |
| 混合序列 `Bash(inspect)` → `Read` → `Bash(普通)` | 前两条并为一组，第三条单条；出现"组 / 单条 / 组"交替属正常 |
| **同批并行的多条 Bash** | 全部即时入组（不再按落定状态区分），因此没有"组 + 单条"混合，也没有组结构重排 |
| **带显式 timeout 的文件查询 Bash** | 入组（不再排除）。`ToolEntry` 不带 `timeout_ms`，因此该条的超时提示在组内不可见（已知取舍） |
| **恢复/重启后的后台 Bash** | `messages_to_view_models` 不跑 `apply_shell_runtime`，`shell_backgrounded` 恒 false；能否挡住只取决于内容里的 `BackgroundTaskStarted` 标记 |
| **组内 Bash 命令行里的路径不可点** | 组内链接命中区只覆盖 `tool_file_url` 支持的 Read/Write/Edit（`mod.rs:2788-2807`），Bash 命令行中的路径不产生可点链接；单条 Bash 同样不可点，行为一致 |
| 组内 Bash 执行失败 | 折叠态经 `error_summary_lines` 显示错误摘要；详细态显示带退出码的错误头 |
| SubAgent 抽屉内 | 不聚合，标记不传递，行为不变 |
| `-p` 打印模式、ACP stdio 路径 | 不经过 `MessageViewModel`，无影响 |

## 7. 风险

1. **hash 漏更新**：`ToolBlock` 新增字段不入 hash，会因 VM 复用出现折叠态回弹。
2. **计数分支遗漏**：三处计数任一遗漏，摘要会出现"有工具但没计数"或重复计数。
3. **详细态输出未走 Bash 分支**：若直接落在通用分支（`mod.rs:2850`），组内 Bash 既没有退出码/stderr 头，
   也会和单条 Bash 的处理口径分叉（§5.6 已定复用 `transform_bash_output_lines`）。
   详细态不限行数是既定口径，不算风险。
4. **模型提示词膨胀**：`description()` 只加一句，不新增系统提示词段落，不影响 prompt cache 前缀。
5. **后台态误判**：R8 只剩两条排除，若把 `shell_backgrounded` 或后台标记判漏，后台命令会被折进组，后台指示与完成通知消失（功能性回退，优先级最高）。
6. **帧 5「逐字一致」是承诺不是现状**：新增的 Bash 输出分支会重排错误输出（`transform_bash_output_lines` 会把退出码抽成
   错误头、把 stderr 分段），实现后必须用同一个临时测试复跑对比帧 3，不能只看"看着差不多"。
7. **计数口径与聚合口径漂移**：只要有一处把"未落定 / 带 timeout"的 Bash 计进 `ran N`，就会出现
   "摘要说跑了 2 条、下面还整块显示 1 条"的自相矛盾（R10）。判定谓词必须**只有一个来源**——
   建议实现成 `fn is_collapsible_bash(vm) -> bool`，聚合与计数都调它。

## 8. 测试计划

`cc-middlewares`（`terminal_test.rs`）：

- `test_bash_parameters_expose_file_inspect`：schema 含 `file_inspect` 且 `required` 仍为 `["command"]`。
- `test_bash_call_ignores_file_inspect`：带该字段调用时执行行为不变。

`cc-tui`：

- `test_aggregate_bash_file_inspect_joins_read_group`：`Bash(file_inspect)` + `Read` → 单个 `ToolCallGroup`。
- `test_aggregate_bash_without_flag_stays_standalone`：`Bash(false)` 前后被正常工具包围时不进组、并打断分组。
- `test_aggregate_running_bash_joins_group`：`phase == Running` 的 `Bash(inspect)` **入组**（§13.1 修订后的行为）。
- `test_aggregate_backgrounded_bash_is_not_aggregated`：`shell_backgrounded == true` 或内容为后台启动标记时不入组（R8）。
- `test_aggregate_single_bash_inspect_still_groups`：单条 `Bash(inspect)` 也成组（R9，与参考实现一致）。
- `test_aggregate_group_entry_keeps_file_inspect`：组内 `ToolEntry.file_inspect` 正确。
- `test_readonly_action_summary_shell_command_count`：单/复数文案（`ran 1 shell command` / `ran 3 shell commands`）。
- `test_merge_thinking_counts_shell_commands_only_from_groups`：`ToolCallGroup` 条目计入；顶层 `ToolBlock` 形态的 Bash（未落定/带 timeout）**不计入**（R10）。
- `test_merge_thinking_does_not_count_running_bash`：`phase == Running && file_inspect` 的 Bash 不产生 `ran N`，同时仍整块显示（帧 7 形态）。
- `test_aggregate_failed_bash_stays_in_group`：`is_error` / 非零退出码的 Bash 仍在组内，折叠态给错误摘要。
- `test_render_bash_inspect_group_collapsed_snapshot` / `..._expanded_snapshot`：见下方"渲染核对方法"，作为常驻快照用例。
- `test_estimator_group_height_matches_rendered_rows`：组高度估算（`estimator.rs:164-180`）与实际渲染行数一致性。
- `test_tool_block_hash_includes_file_inspect`：仅该字段不同的两个 VM hash 不同。
- `test_restore_path_tool_block_matches_streaming`：同一 `Ai(tool_calls)` + `Tool` 消息恢复出的 VM 与流式构造等价。
- `test_restore_path_background_marker_not_aggregated`：恢复态里内容带 `BackgroundTaskStarted` 标记的 Bash 不入组（恢复路径 `shell_backgrounded` 恒 false，这是唯一有效门槛）。
- `test_tool_end_without_pending_keeps_file_inspect`：`tool_end_internal` 无 pending 时仍从 `current_ai_tool_calls` 取回 `file_inspect`（R7 的踩坑点）。
- `test_render_standalone_action_appends_expand_hint`：非详细模式下独立摘要行带 `(ctrl+o to expand)`，详细模式不带。
- `test_render_thought_line_appends_expand_hint_with_actions`：Thought 行形如 `Thought for 19s, ran 2 shell commands (ctrl+o to expand)`。
- 渲染回归：详细态组内 Bash 带退出码时输出错误头（复用 `bash_status_test` 口径）。
- 既有断言确认不被破坏：`message_render_test.rs:1302` 的 `contains("Read 1 file")` 在追加提示后仍成立；
  `message_pipeline_test.rs` 中 8 处直接断言 `standalone_action` 字符串的用例，因提示在渲染层拼接而保持绿。

**渲染核对方法（§5.8 的帧就是这么来的，实现后**落成常驻快照用例**而不是用完即删）**：

在 `cc-tui/src/ui/message_render/` 下加一个 `_test.rs`（按仓库规范 `#[path]` 挂载），做两件事：

1. `test_render_bash_inspect_group_collapsed_snapshot`：构造帧 1 的消息序列 →
   `MessagePipeline::messages_to_view_models` → 拼出文本，断言包含
   `  Thought for 19s, read 1 file, ran 3 shell commands (ctrl+o to expand)` 且**不包含** `● Bash(`；
2. `test_render_bash_inspect_group_expanded_snapshot`：同一序列 `detail_mode = true`，
   断言四条 `● Bash(`/`● Read(` 与错误头 `⎿  Error: Exit code 2` 都在。

这样帧 5「与帧 3 逐字一致」这条承诺才有回归保护（否则临时测试删掉后就没人守了）。

用例的构造套路（§5.8 的帧就是这么产出的）：

1. 用 `BaseMessage::ai_from_blocks(...)` 拼同一段对话（Reasoning + ToolUse ×N），再按 `tool_call_id`
   补 `BaseMessage::tool_result` / `tool_error`；
2. `MessagePipeline::messages_to_view_models(&msgs, cwd)` 得到 VM；
3. 逐个 `render_view_model(vm, Some(i), 110, detail_mode, 0)`，把每行 `spans` 的 `content` 拼成字符串；
4. 跑：`cargo test -p cc-tui --lib -j 1 <test_name> -- --nocapture`。

> `-j 1` 属于 cargo 参数，必须放在 `--` **之前**，否则报 `error: Unrecognized option: 'j'`（实测踩过）。

## 9. 验收标准

1. 一段连续文件查询 Bash（≥3 条）在普通模式下只占一行摘要——
   段内有 thinking 时为 `Thought for 19s, ran 2 shell commands (ctrl+o to expand)`，
   无 thinking 时为独立行 `Ran 2 shell commands (ctrl+o to expand)`；`Ctrl+O` 能逐条看到命令与输出。
2. 单条文件查询 Bash 折成 `Ran 1 shell command (ctrl+o to expand)`，`Ctrl+O` 能展开看到命令与输出。
3. `cargo build` / `cargo test` / `git commit` 等命令的展示形态与改动前像素级一致。
4. 运行中或已转后台的 Bash **不折叠**：实时输出、`Ctrl+B` 提示、后台完成态均与改动前一致；
   并且**不会被计入** `ran N`（不会出现"摘要说跑了、下面还整块显示"）。
5. `/resume` 恢复同一会话后，折叠形态与退出前的实时形态一致。
6. 组内 Bash 失败时，折叠态仍能看到错误摘要。
7. 验证口径（按仓库既有约定）：
   - `cargo clippy --workspace --all-targets`（**必须带 `--all-targets`**，否则测试代码里的构造点漏检，CI 会挂）
   - `cargo test -p cc-tui --lib -j 1` 与 `cargo test -p cc-middlewares --lib`，
     判定标准是"在同一基线上无新增失败"——`cc-tui` 全量 lib 测试存在浮动既有失败，需对比改动前基线，不能只看是否全绿
   - `cargo fmt --check` 只校验本次改动的文件（全仓存在大量既有格式差异）
   - 编译前确认磁盘余量：`cargo test -p cc-tui --lib` 需要约 10 GB 空闲（本机曾在 D 盘仅剩 1.6 GB 时
     报 `rustc-LLVM ERROR: IO failure on output stream: no space on device`；`target/debug/incremental`
     是可安全清空的纯缓存，实测 9.28 GB）

## 10. 分阶段落地

单阶段，无协议变更：

1. `cc-middlewares` 参数 + 描述（可独立合入，等价于 no-op）。
2. `cc-tui` 标记链路 + 聚合（含 R8 落定门槛）+ 计数 + 渲染。

## 11. 后续可选（本期不做）

- **命令启发式兜底**（模型漏填时自动识别）：参考实现的做法可直接借鉴——真 shell 分词、处理 `| && || ;`、
  跳过重定向目标（`>` `>>` `>&` 后的那个 token）、忽略语义中性命令（`echo`/`printf`/`true`/`false`/`:`）、
  三档白名单（search/read/list），管道中任一命令不在白名单即整条不折叠。
  见 `BashTool.tsx:164-233`（`isSearchOrReadBashCommand`）。
- **首行 `# 注释` 当工具行标签**：参考实现用 `extractBashCommentLabel`（`commentLabel.ts:1-13`）
  把命令首行的 `# 说明` 抽出来当非 verbose 模式下的工具行标签，也就是"让 agent 说明 bash 干啥"的零 schema 方案。
  落地位置是单条 Bash 的 `args_display` / 组内 `ToolEntry.args_display`，属独立需求。
- 折叠摘要附加被折叠命令的首条描述（如 `ran 3 shell commands · ls, grep, cat`）。
- 折叠态错误摘要带命令归属：目前 `error_summary_lines` 只输出内容，一组内多条失败时无法区分是哪条命令
  （§5.8 帧 4）。可考虑在摘要前加一行截断命令，需单独设计，避免把折叠态又撑高。

## 12. 实现记录（2026-10-27）

实现分支：`feat/bash-file-inspect-grouping`（worktree `.claude/worktrees/bash-file-inspect-grouping`，
未提交）。本节记录落地结果与设计的差异，评审时以本节为准补充 §5。

### 12.1 与本设计的差异

| 项 | 设计 | 实现 | 判断 |
|---|---|---|---|
| 落定谓词 | R8 四条 | 抽成 `aggregate.rs` 的 `aggregatable_category()`，四条全在 | 一致 |
| 计数来源 | R10：只从组内条目计 | 组内条目计 Bash，`ToolBlock` 分支不计 | 一致 |
| `PartialEq` | §5.7 只点名 hash | 手写 `PartialEq` 的三处比较也补了（`message_view/mod.rs:269/285/301`） | 比设计更严 |
| 高度估算 | §5.7 列为核对点 | 直接把 `transform_bash_output_lines` 接进 `estimator.rs`，按真实输出行算 | 超出设计，保留 |
| 空输出 | 未规定 | 组内详细态显示 `(No output)`（与单条同口径） | 合理补充 |
| 非零退出码着色 | 未规定 | `tool_entry_is_error()` 把 `exit code != 0` 纳入错误判定，与单条一致 | 合理补充 |
| **`is_reasoning_only()`** | 未涉及 | 收紧为「文本空 + 无耗时 + 无摘要」才可跳过 | **超范围的必要修复**，见 12.2 |

### 12.2 超范围变更说明（须在提交信息中写明）

`is_reasoning_only()` 原先把所有"纯 reasoning"气泡都当作空思考跳过，聚合会把夹在只读工具之间的思考
整段丢弃（Thought 行只剩第一段的耗时与字符数）。加入 Bash 组后这条会被放大——一次探索常有多段思考
夹在工具之间——因此必须收紧。

影响面是**所有只读工具组**，不只是 Bash：`Read → Thought → Read` 现在保留思考正文与耗时，
组在思考处切分，再由 `merge_consecutive_thinking` 合成一行。
测试 `test_merge_file_inspect_preserves_reasoning_and_duration` 锁定该行为；既有断言零删除。

### 12.3 验证状态

| 项 | 状态 |
|---|---|
| `cargo check -p cc-tui --all-targets` | ✅ 零错误零警告（同时证明 57 处构造点全部补齐） |
| `bash_file_inspect_test.rs` 3 个精确快照 + pipeline/message_view/layout 新增用例 | ⏳ 未实跑 |
| `cargo clippy --workspace --all-targets` | ⏳ 未跑 |
| 真机 TUI 渲染 | ⏳ 未验证 |

> 未实跑原因：`cargo test` 需要重新 codegen 全量依赖，本机 D 盘余量不足（编译中从 6.2 GB 掉到 3.1 GB），
> 进程已主动中止以免写满磁盘。补跑：`cargo test -p cc-tui --lib -j 1 bash_file_inspect -- --test-threads=1`
> （`-j 1` 必须在 `--` 之前），磁盘需 ≥10 GB，`target/debug/incremental` 可安全清空。

### 12.4 遗留项

- 组内 Bash 输出走 `push_prefixed_text`，单条走 `push_visual_capped_output`；超宽行的折行/换行宽度
  可能不同，需真机目视核对。
- `absorb_preceding_tool_summaries` 仍是 6 元位置参数（未按 §5.5 建议改 struct），有计数测试兜底。
- AskUser 分支仍用 `t.is_error` 而非 `tool_entry_is_error`（AskUser 组不含 Bash，无实际影响）。
- 分支基线：本分支比 `main` 多 4 个无关提交（npm bump 0.6.195、`fix(widgets)` #703、docs maintenance
  merge、CHANGELOG 修正），开 PR 前需确认是否已在 `origin/main`，否则会被一起带入。

## 13. 真机验收后的行为修订（2026-10-27 晚）

以下条目**覆盖前文的 R8 与 §5.4/§5.6**，以本节为准。

### 13.1 聚合门槛（替代 R8）

运行中的文件查询 Bash **也并入组**，只保留两条排除：

- `shell_backgrounded == true`（已转后台，需要单条展示后台状态）
- 内容带 `BackgroundTaskStarted` 标记

理由：按 R8 的原设计，"运行中不入组"会让计数比参考实现少 1，并且出现"块先出现再消失"的闪烁；改为入组后计数与参考实现一致、零闪烁，代价是这类命令运行期不再显示独立块与实时输出（与参考实现相同）。

失败的工具（`is_error` 或 Bash 非零退出码）**仍然入组**：折叠态由 `error_summary_lines` 给出截断的错误摘要（首 400 字符、限行数），完整报错按 `Ctrl+O` 查看。曾经的"失败即剔除出组"方案会在模型反复传错参数时糊满一屏，已废弃。

### 13.2 思考行状态机（新增，前置未涉及）

消息区顶部那行由三部分状态驱动：

| 字段 | 含义 | 置位 / 清除 |
|---|---|---|
| `thinking_now` | 当前是否在思考 | `push_reasoning` 首个 chunk 置 true；正文到达（`freeze_group`）置 false |
| `group_ms` | 当前组内**事件间隔累计**（思考首 chunk / ToolStart / ToolEnd 打点，单段封顶 600s） | 每 `note_event` 累加；`freeze_group` 归零 |
| `finished_groups` | 已冻结的各组耗时 | `freeze_group` 时压入 |

渲染规则（非详细模式）：

- 活跃（`thinking_now || 有未结束的工具`）：`● Thinking for Ns, <现在时计数>… (ctrl+o to expand)`，圆点按**墙钟 500ms** 显隐闪烁，整行正常亮度；
- 冻结：`  Thought for Ns, <过去时计数> (ctrl+o to expand)`，暗色、无圆点/省略号/提示行；
- 每组独立计时（`finished_groups` 从后往前贴回各自的行），活跃组在此基础上叠加"距最近事件"的增量，因此会看到"46s → 4s"的回落。

### 13.3 `⎿` 提示行

- **只在活跃组渲染**（对齐参考实现 `fm = isActive && summary`）；
- 内容优先级：该组最近一条工具的参数（命令 / 路径 / pattern）→ 没有工具时回退思考尾部；
- 思考尾部最多 **10 行**（参考实现 `ak = 10`），工具参数单行；
- 活跃满 **2 秒**后才出现（参考实现同为 2 秒门槛）。

### 13.4 渲染缓存不变量

`normalize_active_thinking(vms, keep_live)` 在视图装配点（`apply_pipeline_action` 的 `RebuildAll` 分支）执行：

1. 整个视图最多保留**一行** active；
2. `keep_live == false` 时清空所有 active；
3. **改动 `active` 后必须 `recompute_hash()`** —— 否则渲染缓存按旧 hash 命中，界面会永久停在 `● Thinking …`（真机表现为滚动历史里积攒一串 Thinking）。

### 13.5 仍未对齐 / 已知差异

- 耗时数值：参考实现取消息自带的 `timestamp` 差之和，我们取事件到达时刻的差之和；公式同构但数值不会逐字相等；
- 偶发整段回答重复两遍（快照 + `ephemeral_notes` 各插一份），未定位、未修；
- `Ctrl+B` 对已入组的运行中命令：按键仍生效，但界面不再显示前台提示（与"放弃实时输出"同一取舍）。

### 13.6 计数归属：不再跨段吸收（新增）

早期 `merge_consecutive_thinking` 会把**紧邻 Thought 之前的工具组**的计数"吸收"到后面的 Thought 行上
（`absorb_preceding_tool_summaries`）。真机表现为：

```text
● Bash(find ...)                    ← 上一组的工具块，位置没动
  ⎿ ...

  Thinking for 28s, running 1 shell command…   ← 新思考行却吃掉了上面那条的计数
```

即**工具块排在思考行之前 + 计数归错行**。现已移除该函数：Thought 行只统计**自己段内**的工具，
顺序恢复为"思考行 → 它的工具"，与参考实现一致。工具若出现在思考气泡之前（模型先调工具再解释），
它们属于自己的一组，独立显示摘要行。

### 13.7 `file_inspect` 提示词的三次迭代（含实测数据）

| 版本 | 措辞 | 实测（新二进制会话） |
|---|---|---|
| v1 | "Set to true when this command only inspects files…"（说明式） | **0 / 64** 次填写，功能等于没上 |
| v2 | "MANDATORY / REQUIRED: every Bash call must decide…" | 填 79%（311/392），漏填 17，但**滥填 248**（`pwd; uname; cargo --version`、`git status` 都被标） |
| v3（当前） | "Set true only when **every executable** is a file lister/reader（白名单 + 中性命令例外）；不确定就留空" | 待测（fi33），目标：漏填 ≤20、滥填 ≤20 |

配套统计脚本：`scripts/inspect_usage_stats.py`

```bash
python scripts/inspect_usage_stats.py --days 3 --show-examples 3
# 输出：Bash 调用数 / 填 true 数 / 漏填数 / 滥填数 + 样例
```

判定白名单：`ls cat head tail find wc rg grep tree du stat file sed -n`（允许 `cd pwd echo printf` 中性），
任一可执行不在白名单即判为"不该标"。

> 注意：脚本建了但 `.gitignore:155` 的 `*.py` 规则会把它排除，提交时需要 `git add -f`
> 或在 `.gitignore` 加 `!scripts/*.py` 例外。

## 14. 测试 / 审计清单（可直接执行）

### 14.1 环境准备

```powershell
cd D:\code\peri\.claude\worktrees\bash-file-inspect-grouping\target\debug
.\cc-code-fi33.exe          # 当前最新产物
```

### 14.2 功能场景（每条给"期望"，不符合即为缺陷）

| # | 操作 | 期望 |
|---|---|---|
| 1 | 让 agent 用 `ls` / `cat` / `grep` 查文件 | 普通模式只出现一行 `● Thinking for Ns, …` 或 `Thought for Ns, read/ran …`，**没有** `● Bash(...)` 块 |
| 2 | 按 `Ctrl+O` | 逐条展开 `● Bash(<命令>)` + 输出；退出码非 0 的行带 `Error: Exit code N` 头并标红 |
| 3 | 让 agent 跑 `cargo build` / `git commit` | **仍然单条** `● Bash(...)` + 输出，形态与改动前完全一致 |
| 4 | 让 agent 跑一条会失败的命令（`grep nope.txt`） | 折叠组下方出现**截断的错误摘要**（首 400 字符内），完整内容 `Ctrl+O` 可见；不再出现整块报错铺屏 |
| 5 | 中途观察 | **同时只有一行** `● Thinking`（圆点闪烁）；上一条已是暗色 `Thought for Ns`；正文出现后立刻由 `Thinking` 变 `Thought` |
| 6 | 一轮结束 | 屏幕上**没有任何** `● Thinking` 行 |
| 7 | 长命令运行中 | 该命令已并入组（不再有独立块/实时输出），计数含它自己 |
| 8 | `Ctrl+B` 转后台 | 该命令回到单条显示，带后台指示与完成通知 |
| 9 | 退出后 `-c` 继续会话 | 折叠形态与退出前一致 |
| 10 | 统计 | `python scripts/inspect_usage_stats.py --days 1`：漏填 ≤20、滥填 ≤20 |

### 14.3 单元测试与静态检查（已实测，2026-10-27 17:40）

| 检查 | 结果 |
|---|---|
| `cargo test -p cc-tui --lib -j 1` | **1721 passed / 0 failed / 11 ignored** ✅ |
| `cargo test -p cc-middlewares --lib -j 1` | 1263 passed / **1 failed** |
| `cargo clippy -p cc-tui -p cc-middlewares --all-targets` | **0 warning / 0 error** ✅ |

唯一的失败 `cc-middlewares` `mcp::config::tests::test_load_merged_config_full_no_plugins`：
断言 `config.mcp_servers.is_empty()`，而该测试读取**本机真实 MCP 配置**（develop 机上配了 MCP server），
CI 干净环境通过；与本特性无关。

**过程记录（重要）**：中途曾出现 57 个失败，全部是环境问题造成的假失败，不是代码缺陷：

- D 盘被主工作区 `target`（21.5 GB）与本 worktree `target`（12.6 GB）占满；
- C 盘 100% 满（0 字节），libtest 无法写临时文件，报
  `io error when listing tests: Os { code: 112 }`（112 = ERROR_DISK_FULL），
  导致 `event::*` / `ui::headless::*` / `clipboard::*` 等大批无关测试假失败；
- 处理：删除主工作区 `target`、清理 `%LOCALAPPDATA%\Temp`（2 GB）、
  并把 `TEMP`/`TMP` 指向 D 盘后重跑，即得上述全绿结果。

本轮按新语义更新的 4 个旧断言：

- `test_merge_removes_extra_reasoning_blocks_in_same_bubble`：耗时取最后一段（2000ms）
- `test_multi_round_thinking_merged_with_accumulated_stats`：收敛计数 + 最后一段（1000ms）
- `test_merge_thinking_counts_shell_commands_only_from_groups`：运行中/带 timeout 也入组（计数 3）
- `test_aggregate_bash_file_inspect_joins_group_unless_backgrounded`：仅后台态保持单条

本轮测试同时抓出并修复两个真 bug：

1. `absorb_preceding_tool_summaries` 跨段吸收 → 思考行排到工具块之后 + 计数归错行（§13.6，已删除）；
2. `build_streaming_bubble` 去重守卫误用 `!current_ai_finalized`（首个 ToolStart 即置位），
   导致此后思考气泡不再输出、与工具组无法合并、计数丢失；改为 `reasoning_flushed`
   （"快照已收纳"才算重复）。

### 14.3.1 验证用产物

| 产物 | 路径 | 说明 |
|---|---|---|
| `cc-code-verify.exe` | `target/debug/cc-code-verify.exe` | 17:40 构建，70,493,696 bytes，sha256 前缀 `44be61dd02613ea0` |

### 14.4 已知未修

1. 偶发"整段回答渲染两遍"（快照 + `ephemeral_notes` 各插一份），未能稳定复现；
2. 已入组的运行中命令按 `Ctrl+B` 后，按键生效但前台提示不可见（与"放弃实时输出"同一取舍）；
3. 耗时数值与参考实现同构但不等值（事件到达时刻 vs 消息 `timestamp`）。

### 13.8 跨轮合并与提示来源（2026-10-27 17:56）

真机暴露两个问题，已修：

1. **跨轮不合并**：每次重建只重建"本轮尾部"，`merge_consecutive_thinking` 只在同一次重建的 VM 列表内生效，
   于是出现"上一轮一条长时间组 + 本轮一条空 Thought"两行相邻。
   处理：装配点（`apply_pipeline_action` 的 `RebuildAll` 分支）在拼接前缀与尾部之后调用
   `retry_merge_from(vms, prefix_len - 1)`，从边界前一个 VM 起重跑一次合并。
   不动 prefix/tail 索引——那是我此前踩过"整段重复渲染"的地方。
2. **`⎿` 与单条块重复**：提示行取的是段内最后一条工具参数，但未入组的工具会自己单条渲染，
   导致同一条命令出现两次。处理：只从**入组条目**（`ToolCallGroup.tools`）取提示，
   顶层 `ToolBlock` 不再参与 hint。

验证：`cargo test -p cc-tui --lib` 仍为 **1721 passed / 0 failed / 11 ignored**。

已知仍存在（结构性问题，未在本轮处理）：

- 组只在"正文到达 / 本轮结束"时闭合，长时间连续调用工具时耗时会一路累加（真机出现 409s）；
  CC 同样累加间隔，但其组有更细的消息边界；
- 根治方案是引入真正的 `TurnGroup` VM（思考 + 连续工具 = 一个对象），替代事后拼接；
  本轮只做了边界重合并，属于渐进式修补。

### 13.9 折叠组成为一等公民（2026-10-27 18:45，重构）

前几节记录的问题（§13.6 计数归属、§13.8 跨轮不合并、409s 无限累加、⎿ 重复）都源自同一结构：
**思考行挂在 `AssistantBubble` 上，工具组由 `aggregate` 事后拼出，两者再用 `merge_consecutive_thinking` 缝合**。

本次把组提升为一等公民：

```rust
ToolCallGroup {
    category, tools, collapsed, standalone_action,
    thinking: Option<GroupThinking>,   // 新增：组自带的思考行
}

pub struct GroupThinking {
    pub text: String,              // 合并后的思考全文（详细模式展开）
    pub duration_ms: Option<u64>,  // 完成态耗时
    pub action_summary: Option<String>, // 计数摘要
    pub hint: Option<String>,      // ⎿ 提示（只来自入组条目）
    pub active: bool,              // 是否仍在进行
    pub active_since: Option<Instant>,  // 活跃走秒锚点
}
```

- merge 阶段：段内若有只读组，把思考数据挂到**组**上；无正文时**不再产出思考气泡**，一屏只剩一个 VM；
- 渲染：组自己画 `● Thinking for Ns, …` / `  Thought for Ns, …`，详细态还负责展开思考正文；
- 活跃态/耗时（`mark_live_thinking`）直接打到组上，不再改 bubble 的 Reasoning；
- 折叠态的 `standalone_action` 在有 `thinking` 时不再渲染，避免摘要重复。

验证：`cargo test -p cc-tui --lib` **1721 passed / 0 failed / 11 ignored**；`cargo clippy -p cc-tui --all-targets` **0 warning**。

产生的测试改动（结构变化导致，非行为回退）：

- 三个提取辅助函数（`reasoning_blocks_of` / `all_reasoning_meta` / `thought_titles`）同时支持 `group.thinking`；
- 两个渲染快照去掉"标题后的空行"——旧结构是 bubble 与组之间的分隔，合并成一个 VM 后不再存在。

仍未解决（下一步）：

- 跨轮合并目前靠 `retry_merge_from` 在装配点补合；组本身仍**不跨重建持久**，
  要彻底消除需把组写进 `completed` 历史表示（或 VM 层缓存）；
- 数值口径仍与参考实现不完全相同（事件到达时刻 vs 消息 `timestamp`）。
