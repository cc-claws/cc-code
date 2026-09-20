# 对齐 OpenAI Codex 规范的动态终端标题（Status Surface）体系

**状态**：Fixed  
**优先级**：中  
**创建日期**：2026-09-20  
**解决日期**：2026-09-20  
**模块**：TUI / 终端交互 / 会话管理  
**GitHub Issue**：#162 (https://github.com/cc-claws/cc-code/issues/162)  

## 问题描述

当前 Peri (CC Code) 在终端窗口/标签页标题栏中仅显示机械固定的硬编码字符串（如 `✻ CC Code — Running` 与 `✴ CC Code — Done`）。

在真实工程实践中，开发者普遍会在 Windows Terminal、Ghostty、iTerm2、tmux 或 Kitty 等现代终端模拟器中同时开启多个会话标签（如：同时修复 Bug、重构模块、排查日志）。现有标题方案导致多 Tab 完全无法辨识当前窗口处于哪个项目、处理什么任务；且当 Agent 卡在 HITL 审批（如危险命令拦截）或提问交互（`AskUserQuestion`）时，切到其他窗口的开发者完全感知不到需要介入（无阻塞状态反馈）。

参考 **OpenAI Codex CLI (`codex-rs`)（PR #15860 及 Issue #31124, #35626, #36132）** 与 Claude Code 的工业级实现，终端标题被设计为与底部状态栏平级的 **Status Surface（状态表面）**，具备多段动态信息拼接、生命周期状态感知、会话主题同步与安全去重机制。

## 现状与缺陷详情

### 1. 标题内容机械写死，缺失工程上下文
- 当前代码（`peri-tui/src/main.rs:882` 与 `peri-tui/src/app/mod.rs:564`）：
  - 启动时写死：`SetTitle("✻ CC Code")`
  - 执行中写死：`SetTitle(format!("{} CC Code — Running", frame))`
  - 完成后写死：`SetTitle("✴ CC Code — Done")`
- 缺失核心上下文：没有当前所在工作区/项目名（`project_name`），没有会话主题（`thread_title`）。开 3~5 个标签页时全部显示相同的标题，无法区分。

### 2. 关键阻塞状态（Action Required）感知缺失
- 当触发高危工具调用（HITL 审批弹窗）、`AskUserQuestion` 选项交互、或执行计划确认时，TUI 等待用户输入，但外部终端标题依然保持普通状态甚至转为 Idle，切走窗口的开发者无法得知“当前会话已暂停并等待我批准”。
- 对比 Codex / Claude Code：进入等待输入或审批状态时，标题立即切为醒目的 `⚠️ [项目] | [主题] — Action Required`，即使窗口在后台也能一目了然。

### 3. OSC 0 写入机制粗暴，无去重且退出无清理
- 当前在主事件循环中，无论标题内容是否变化，每 200ms 无条件向 stdout 执行 `execute!(stdout, SetTitle(...))`，造成无谓的控制台 I/O 抖动。
- 退出或 panic 时，未向终端发送清空/还原标题指令，导致终端关闭或切回 shell 后标题栏依然残留 `✴ CC Code — Done`。
- 未对动态文本（如模型输出或会话名）进行控制字符与转义字符清洗，存在潜在的 ANSI 注入或显示破坏风险。

## 期望设计与对齐规范（对齐 OpenAI Codex `codex-rs`）

### 一、Status Surface 架构分层
将终端标题从简单的字符串拼接待办，提升为标准的状态表面抽象（可独立为 `peri-tui/src/terminal_title.rs` 或纳入 UI 状态管线）：

1. **`TerminalTitleItem` 组成项（支持默认组合与灵活扩展）**：
   - `Activity`：旋转 Spinner 动画帧（`⠋` / `✻` 等）
   - `ProjectName`：当前工作区目录名 / Git 仓库根目录名（如 `peri`）
   - `ThreadTitle`：当前会话主题短标题（由首轮 Prompt 提炼或 `/rename` 自定义）
   - `StatusKind`：生命周期状态指示器（`Running` / `Thinking` / `Action Required` / `Done`）

2. **`TerminalTitleStatusKind` 生命周期状态机**：
   - `Idle` / `Ready`：会话就绪，无活动任务
     - 格式：`{project_name} | {thread_title}`（若未命名则为 `{project_name} — CC Code`）
   - `Working`：工具执行中 / 派发中
     - 格式：`{spinner} {project_name} | {thread_title} — Running`
   - `Thinking`：LLM 思考/流式推理中
     - 格式：`✻ {project_name} | {thread_title} — Thinking`
   - `ActionRequired`：等待用户交互输入（**核心体验提升**）
     - 触发条件：`HitlDialog` 弹出等待批准、`AskUserQuestion` 激活、方案选择等待输入
     - 格式：`⚠️ {project_name} | {thread_title} — Action Required`
   - `Done`：任务完成响应
     - 格式：`✴ {project_name} | {thread_title} — Done`

### 二、会话主题（Thread Title）自动提炼与即时同步
1. **自动提取短标题（Auto Thread Title）**：
   - 用户发送第一轮 Prompt 时，若当前会话未自定义名称，通过轻量提取策略（如取前 20 字符去换行、或由快速规则提炼 3~5 个词的名词短语，如 `fix-login-error`）作为当前会话的 `display_name`。
2. **即时联动更新**：
   - 当会话被自动命名、通过 `/rename` 命令改名、或在历史会话列表（`Ctrl+O` / Session Browser）切换时，立即触发终端标题更新。

### 三、底层写出安全与去重机制（Defensive Terminal I/O）
1. **内容去重（Deduplication）**：
   - 内部维护 `last_terminal_title: Arc<Mutex<Option<String>>>`；
   - 每次生成新标题字符串后，与上次写出的标题严格比对，**只有发生变化时才向 stdout 发射 OSC 0 转义序列**；动画旋转时仅在帧变化时写出。
2. **字符清洗（Sanitization）**：
   - 过滤字符中的控制字符（`\x00`~`\x1F`、`\x7F`）、换行回车符、以及不可见的双向文本（Bidi）字符，防止恶意/异常文本污染终端标题栏。
3. **优雅退出清理（Lifecycle Drop / Cleanup）**：
   - 应用退出（在 `main.rs` 恢复 raw mode 及 `LeaveAlternateScreen` 时）显式发射清空或还原指令，避免污染宿主 Shell。

## 涉及文件

- `peri-tui/src/terminal_title.rs`（新建：封装终端标题状态机、清洗函数与去重写出）
- `peri-tui/src/main.rs`（主事件循环动画驱动、启动与退出生命周期清理）
- `peri-tui/src/app/mod.rs`（`set_loading` 状态机联动，通知终端标题状态流转）
- `peri-tui/src/app/agent_ops_interaction.rs`（HITL 审批与提问弹窗激活/关闭时切换 `ActionRequired` 状态）
- `peri-tui/src/command/session/rename.rs`（重命名时即时刷新标题）
- `peri-acp/src/session/` 或 `peri-tui/src/session_mgr.rs`（首轮 Prompt 后的会话短标题提炼与事件通知）

## 验收标准

1. **多 Tab 辨识测试**：在不同目录下打开两个终端标签，窗口标题分别显示对应的目录名与会话主题，不再千篇一律显示 `CC Code`。
2. **HITL 审批感知测试**：故意触发需要审批的操作（如执行敏感 Bash），弹窗出现时，终端标题变为 `⚠️ <project> | <topic> — Action Required`；确认后恢复为 Running。
3. **I/O 节流验证**：在会话静止（Idle）状态下，终端不再向 stdout 频繁写入 OSC 转义序列。
4. **终端恢复测试**：正常退出（`/quit` 或 Ctrl+C）后，宿主终端标签恢复干净，不残留 `✴ CC Code — Done`。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-09-20 | — | Open | agent | 参照 OpenAI Codex (codex-rs) 规范创建动态终端标题需求 Issue |
| 2026-09-20 | Open | Fixed | agent | 完成动态终端标题（Status Surface）体系实现与单测验证 |

## 修复记录

- 新增 `peri-tui/src/terminal_title.rs` 模块及单元测试，抽象 `TerminalTitleItem` 与 `TerminalTitleStatusKind` 状态机。
- 深度对齐终端实践与用户感知习惯：
  - 彻底去除 `| project` 冗余后缀以及 `— Running`、`— Thinking`、`— Done` 等机械词汇。
  - 运行时纯净展示：`⠋ <主题>`（未命名时为 `⠋ <项目名>`）。
  - 完成响应时醒目展示：保留橙色菊花标志 `✴ <主题>`（未命名时为 `✴ <项目名>`），清晰指示 Agent 刚刚回复完成。
  - 空闲时纯净展示：`<主题>`（未命名时为 `<项目名>`）。
  - 交互阻塞时展示呼吸动画：`[ ! ] Action Required  <主题>`（`[ ! ]` 与 `[ . ]` 每秒交替呼吸闪烁）。
- 实现 `sanitize_title` 对齐 Codex 官方防御性清洗逻辑：空白符折叠（whitespace collapsing）、Trojan Source 完整 Bidi 过滤、240 字符硬截断。
- 引入 Graphemes 字形簇截断（项目名 24 字符、会话名 48 字符）。
- 实现 `extract_thread_title`，支持从用户首轮 Prompt 智能提炼会话短标题，并过滤 URL、Markdown 标记、代码块与 `<system-reminder>` 标签。
- 在 `SessionMetadata` 中增加 `thread_title` 字段，并在首轮发送、ACP 会话回填、`/rename`、历史会话切换及新建会话时实时同步。
- 在 `GlobalUiState` 中维护 `last_terminal_title` 缓存，仅在标题内容变动时发射 OSC 0 转义序列，彻底消除 Idle 态 I/O 抖动。
- 刷新间隔优化为 100ms（完全对齐 Codex `TERMINAL_TITLE_SPINNER_INTERVAL = 100ms`）。
- 在 TUI 退出清理（`LeaveAlternateScreen`）前显式调用 `clear_terminal_title` 清空还原标题栏。

