# `/export` 命令 — 导出对话到文件或剪贴板

## Status
- [ ] Phase 1: 实现消息渲染器（BaseMessage → 纯文本/Markdown/JSON）
- [ ] Phase 2: 实现 ExportCommand（TUI Command，打开 ExportPanel）
- [ ] Phase 3: 实现 ExportPanel UI（格式选择 + 文件名 + 确认）
- [ ] Phase 4: 文件写入 + 剪贴板复制
- [ ] Phase 5: 注册
- [ ] Phase 6: 单元测试

## Created
2026-06-27

## Severity
Feature — 开发效率工具

## Platform
全平台 (Windows / macOS / Linux)

## Problem

peri 缺少 `/export` 命令。用户无法将当前对话导出为文件或复制到剪贴板。常见需求场景：

- 导出对话记录分享给同事（不共享整个 session 文件）
- 保存 agent 的分析结果为 Markdown 文件
- 将对话内容粘贴到 Issue/PR 描述中
- 归档重要对话（比 session 文件更易读）

Claude Code（TS 版）已实现 `/export` 命令（`C:\Work\open-cladue\src\commands\export\`），但**该实现未完成**——依赖文件缺失（`ExportDialog`、`exportRenderer`、`slowOperations`），命令未注册到 `COMMANDS()` 数组。属于废弃的 scaffold 代码。

然而概念设计是完整的：**local-jsx 类型，渲染对话为纯文本，支持导出到文件（自动生成文件名）或通过 UI 确认文件名**。

## 参考实现 — Claude Code `/export`

**源码**：`C:\Work\open-cladue\src\commands\export/export.tsx`（~100 行，依赖缺失）

### 核心设计

```ts
const exportCommand = {
  type: 'local-jsx',
  name: 'export',
  description: 'Export the current conversation to a file or clipboard',
  argumentHint: '[filename]',
  load: () => import('./export.js'),
}
```

**两条执行路径**：

| 路径 | 触发方式 | 行为 |
|------|----------|------|
| **直接导出** | `/export myfile` | 渲染对话 → 写入 `cwd/myfile.txt` → 返回结果 |
| **交互式** | `/export`（无参数） | 渲染对话 → 显示 ExportDialog（文件名输入 + 确认） |

**自动文件名生成**：`<timestamp>-<first-prompt-kebab>.txt`
- 提取首条用户消息，取第一行，截断 50 字符
- sanitize 为 kebab-case：小写、去特殊字符、空格转连字符
- 示例：`2026-06-27-143052-how-to-fix-the-bug.txt`

**导出格式**：纯文本（`.txt`），通过 `renderMessagesToPlainText()` 渲染。

## 现有基础设施（可直接复用）

| 组件 | 位置 | 说明 |
|------|------|------|
| **BaseMessage 序列化** | `peri-agent/src/messages/message.rs` | `serde_json::to_string()` 已就绪，所有字段 derive Serialize/Deserialize |
| **SQLite 消息存储** | `peri-agent/src/thread/sqlite_store.rs` | `load_messages(thread_id)` 可按 session 加载全部消息 |
| **ThreadStore trait** | `peri-agent/src/thread/store.rs` | `load_messages()`、`load_meta()`、`list_threads()` |
| **Clipboard 复制** | `peri-tui/src/clipboard/copy.rs` | `copy_to_clipboard()` 多层 fallback（arboard → WSL PowerShell → tmux → OSC 52） |
| **ThreadBrowser** | `peri-tui/src/thread/browser.rs` | 已有会话列表 UI，可扩展导出操作 |

## Fix Proposal

### Phase 1: 消息渲染器

**新建** `peri-tui/src/export/renderer.rs`：

将 `Vec<BaseMessage>` 渲染为可读文本。支持 3 种格式。

```rust
use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};

pub enum ExportFormat {
    PlainText,   // .txt — 纯文本，适合分享
    Markdown,    // .md — 结构化 Markdown，适合文档
    Json,        // .json — 原始 JSON，适合程序处理
}

/// 将消息列表渲染为指定格式的字符串。
pub fn render_messages(messages: &[BaseMessage], format: ExportFormat) -> String {
    match format {
        ExportFormat::PlainText => render_plain_text(messages),
        ExportFormat::Markdown => render_markdown(messages),
        ExportFormat::Json => render_json(messages),
    }
}
```

**纯文本渲染**（`render_plain_text`）：

```
=== User ===
帮我分析一下这个函数的性能问题

=== Assistant ===
我来分析一下 `process_data` 函数的性能瓶颈...

[Tool: Read] src/processor.rs
[Tool Result] (128 lines)

从代码来看，主要问题有两点：
1. ...

=== User ===
能给出优化方案吗？

=== Assistant ===
优化方案如下：
...
```

规则：
- System 消息跳过（不导出系统提示词）
- Tool 调用渲染为 `[Tool: <name>] <truncated_input>`（输入截断 200 字符）
- Tool 结果渲染为 `[Tool Result] (<N> lines)`（不导出完整输出，避免膨胀）
- Reasoning/Unknown block 跳过
- 消息间用空行分隔

**Markdown 渲染**（`render_markdown`）：

```markdown
# Conversation Export

**Session**: <thread_id>
**Date**: <created_at>
**Messages**: <count>

---

## User

帮我分析一下这个函数的性能问题

## Assistant

我来分析一下 `process_data` 函数的性能瓶颈...

<details><summary>Tool: Read src/processor.rs</summary>

(tool output truncated, 128 lines)

</details>

从代码来看，主要问题有两点：
1. ...

---

## User

能给出优化方案吗？

## Assistant

优化方案如下：
...
```

规则：
- YAML frontmatter（session 元数据）
- 每个 turn 用 `---` 分隔
- Tool 调用折叠在 `<details>` 中
- 保留 Markdown 格式（代码块、列表等）

**JSON 渲染**（`render_json`）：

直接 `serde_json::to_string_pretty(messages)`，最简单。

### Phase 2: ExportCommand（TUI Command）

**新建** `peri-tui/src/command/session/export_cmd.rs`：

```rust
//! `/export` 命令 — 导出对话到文件或剪贴板。
//!
//! 用法：
//!   /export              → 交互式选择格式 + 文件名
//!   /export report.md    → 直接导出为 Markdown 文件
//!   /export --clipboard  → 复制纯文本到剪贴板

use crate::app::App;
use crate::command::{Command, CommandResult};

pub struct ExportCommand;

impl Command for ExportCommand {
    fn name(&self) -> &str { "export" }
    fn aliases(&self) -> Vec<&str> { vec!["save"] }
    fn description(&self) -> &str { "Export the current conversation to a file or clipboard" }

    fn execute(&self, app: &mut App, args: &str) -> CommandResult {
        let args = args.trim();

        if args == "--clipboard" || args == "-c" {
            // 直接导出到剪贴板
            export_to_clipboard(app, ExportFormat::PlainText)
        } else if !args.is_empty() {
            // 直接导出到文件（从参数推断格式）
            let format = infer_format_from_filename(args);
            export_to_file(app, args, format)
        } else {
            // 交互式：打开 ExportPanel
            app.open_panel(PanelKind::Export);
            CommandResult::Ok
        }
    }
}
```

### Phase 3: ExportPanel UI

**新建** `peri-tui/src/ui/panels/export_panel.rs`：

```rust
pub struct ExportPanel {
    step: ExportStep,
    selected_format: usize,     // 0=PlainText, 1=Markdown, 2=Json
    filename_input: String,     // 可编辑文件名
    cursor_pos: usize,
    status: ExportStatus,
}

enum ExportStep {
    FormatSelect,    // 选择导出格式
    FilenameEdit,    // 编辑文件名
    Confirm,         // 确认导出
    Done(String),    // 完成，显示结果消息
}

enum ExportStatus {
    Idle,
    Exporting,
    Success(String),   // 文件路径
    Error(String),     // 错误信息
}
```

**UI 布局**：

```
┌─ Export Conversation ────────────────────────────┐
│                                                    │
│  Format:                                           │
│    ◉ Plain Text (.txt)                             │
│    ○ Markdown  (.md)                               │
│    ○ JSON      (.json)                             │
│                                                    │
│  Filename:                                         │
│    2026-06-27-143052-review-results.md             │
│                                                    │
│  [Export to File]  [Copy to Clipboard]  [Cancel]  │
│                                                    │
│  ✓ Exported to: /home/user/project/2026-06-27...   │
└───────────────────────────────────────────────────┘
```

**快捷键**：

| 键 | FormatSelect | FilenameEdit | Confirm |
|----|-------------|--------------|---------|
| `↑/↓` | 切换格式 | — | — |
| `Tab` | 跳到文件名 | 跳到按钮 | 跳到格式 |
| `Enter` | — | 跳到按钮 | 执行选中按钮 |
| `←/→` | — | — | 切换按钮 |
| `Esc` | 关闭面板 | 返回格式选择 | 关闭面板 |

**按钮导航**：用 `←/→` 在 3 个按钮间移动焦点，`Enter` 执行。

**自动文件名**：

```rust
fn generate_default_filename(messages: &[BaseMessage], format: ExportFormat) -> String {
    let timestamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
    let prompt_hint = extract_first_prompt_hint(messages);  // 首条用户消息，≤50 字符
    let ext = match format {
        ExportFormat::PlainText => "txt",
        ExportFormat::Markdown => "md",
        ExportFormat::Json => "json",
    };

    if prompt_hint.is_empty() {
        format!("conversation-{timestamp}.{ext}")
    } else {
        let sanitized = sanitize_filename(&prompt_hint);
        format!("{timestamp}-{sanitized}.{ext}")
    }
}

fn extract_first_prompt_hint(messages: &[BaseMessage]) -> String {
    messages.iter()
        .find_map(|m| match m {
            BaseMessage::Human { content, .. } => Some(content),
            _ => None,
        })
        .and_then(|c| match c {
            MessageContent::Text(text) => Some(text.as_ref()),
            MessageContent::Blocks(blocks) => blocks.iter().find_map(|b| match b {
                ContentBlock::Text(t) => Some(t.text.as_str()),
                _ => None,
            }),
            _ => None,
        })
        .map(|text| {
            let first_line = text.lines().next().unwrap_or("");
            let truncated: String = first_line.chars().take(50).collect();
            truncated
        })
        .unwrap_or_default()
}

fn sanitize_filename(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == ' ' { c } else { '-' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .collect::<String>()
}
```

### Phase 4: 文件写入 + 剪贴板复制

**文件写入**：

```rust
fn export_to_file(app: &App, filename: &str, format: ExportFormat) -> CommandResult {
    let session = app.session_mgr.current();
    let thread_id = match &session.current_thread_id {
        Some(id) => id.clone(),
        None => return CommandResult::Error("No active session".into()),
    };

    // 从 SQLite 加载完整消息（比内存中的更可靠，包含已 compact 的历史）
    let messages = match app.thread_store.load_messages(&thread_id) {
        Ok(m) => m,
        Err(e) => return CommandResult::Error(format!("Failed to load messages: {e}")),
    };

    let content = render_messages(&messages, format);
    let path = Path::new(&session.metadata.cwd).join(filename);

    match std::fs::write(&path, &content) {
        Ok(_) => CommandResult::Ok,
        Err(e) => CommandResult::Error(format!("Failed to write file: {e}")),
    }
}
```

**路径安全**：复用 `validate_and_resolve()`（CLAUDE.md Sync 模块中的路径穿越防护）确保文件名不包含 `../` 等穿越路径。

**剪贴板复制**：

```rust
fn export_to_clipboard(app: &App, format: ExportFormat) -> CommandResult {
    let session = app.session_mgr.current();
    let thread_id = match &session.current_thread_id {
        Some(id) => id.clone(),
        None => return CommandResult::Error("No active session".into()),
    };

    let messages = match app.thread_store.load_messages(&thread_id) {
        Ok(m) => m,
        Err(e) => return CommandResult::Error(format!("Failed to load messages: {e}")),
    };

    let content = render_messages(&messages, format);

    match crate::clipboard::copy_to_clipboard(&content) {
        Ok(_) => CommandResult::Ok,
        Err(e) => CommandResult::Error(format!("Failed to copy: {e}")),
    }
}
```

### Phase 5: 注册

**TUI Command 注册**：`peri-tui/src/command/mod.rs` 的 `default_registry()` 中添加 `ExportCommand`。

**PanelKind 注册**：`peri-tui/src/app/panel_manager.rs` 的 `PanelKind` enum 添加 `Export`，`PanelState` 添加 `Export(ExportPanel)`。

### Phase 6: 单元测试

```rust
// renderer 测试
#[test]
fn test_render_plain_text_skips_system_messages() {
    let messages = vec![
        BaseMessage::system("You are helpful"),
        BaseMessage::human("hello"),
        BaseMessage::ai("hi there"),
    ];
    let text = render_messages(&messages, ExportFormat::PlainText);
    assert!(!text.contains("You are helpful"), "应跳过 System 消息");
    assert!(text.contains("hello"), "应包含 Human 消息");
    assert!(text.contains("hi there"), "应包含 Ai 消息");
}

#[test]
fn test_render_markdown_contains_frontmatter() {
    let messages = vec![BaseMessage::human("test")];
    let md = render_messages(&messages, ExportFormat::Markdown);
    assert!(md.starts_with("---"), "Markdown 应以 frontmatter 开头");
    assert!(md.contains("# Conversation Export"), "应有标题");
}

#[test]
fn test_render_json_is_valid_json() {
    let messages = vec![BaseMessage::human("test")];
    let json = render_messages(&messages, ExportFormat::Json);
    assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok(), "应为合法 JSON");
}

#[test]
fn test_render_plain_text_tool_calls_truncated() {
    let messages = vec![
        BaseMessage::human("read file"),
        // 包含 tool_use 和 tool_result 的消息
    ];
    let text = render_messages(&messages, ExportFormat::PlainText);
    assert!(text.contains("[Tool:"), "应渲染 Tool 调用标签");
}

// filename 测试
#[test]
fn test_generate_default_filename_with_prompt() {
    let messages = vec![BaseMessage::human("How to fix the bug?")];
    let name = generate_default_filename(&messages, ExportFormat::Markdown);
    assert!(name.ends_with(".md"), "应有正确扩展名");
    assert!(name.contains("how-to-fix-the-bug"), "应包含 sanitized prompt");
}

#[test]
fn test_generate_default_filename_empty_prompt() {
    let messages = vec![];
    let name = generate_default_filename(&messages, ExportFormat::PlainText);
    assert!(name.starts_with("conversation-"), "无消息时用 conversation- 前缀");
}

#[test]
fn test_sanitize_filename_removes_special_chars() {
    assert_eq!(sanitize_filename("Hello World!"), "hello-world");
    assert_eq!(sanitize_filename("fix: bug #123"), "fix-bug-123");
    assert_eq!(sanitize_filename("  spaces  "), "spaces");
}

// format inference 测试
#[test]
fn test_infer_format_from_filename() {
    assert!(matches!(infer_format_from_filename("out.md"), ExportFormat::Markdown));
    assert!(matches!(infer_format_from_filename("out.json"), ExportFormat::Json));
    assert!(matches!(infer_format_from_filename("out.txt"), ExportFormat::PlainText));
    assert!(matches!(infer_format_from_filename("out"), ExportFormat::PlainText)); // 默认
}
```

## 使用方式

```
/export                    → 交互式面板：选格式 → 编辑文件名 → 确认导出
/export report.md          → 直接导出为 Markdown 到 cwd/report.md
/export data.json          → 直接导出为 JSON 到 cwd/data.json
/export --clipboard        → 复制纯文本到剪贴板
/export -c                 → 同上（别名）
/save report.md            → 同 /export（别名）
```

## 后续扩展（不在本次 scope）

| 扩展 | 说明 |
|------|------|
| **历史会话导出** | `/export <thread-id>` 从 ThreadBrowser 触发，导出非当前会话 |
| **HTML 导出** | 渲染为带 CSS 样式的 HTML，适合浏览器查看 |
| **Gist 上传** | Claude Code 的 `/share` 命令，上传到 GitHub Gist |
| **选择性导出** | 只导出某几轮对话（通过 Turn 选择器） |

## Affected Files

### 新建
| 文件 | 职责 |
|------|------|
| `peri-tui/src/export/mod.rs` | export 模块入口 |
| `peri-tui/src/export/renderer.rs` | `render_messages()` — PlainText/Markdown/JSON 三种格式渲染器 |
| `peri-tui/src/export/filename.rs` | `generate_default_filename()` + `sanitize_filename()` + `infer_format_from_filename()` |
| `peri-tui/src/command/session/export_cmd.rs` | `ExportCommand` TUI 命令 |
| `peri-tui/src/ui/panels/export_panel.rs` | `ExportPanel` — 格式选择 + 文件名编辑 + 确认，`PanelComponent` 实现 |

### 改造
| 文件 | 行号 | 改动 |
|------|------|------|
| `peri-tui/src/command/mod.rs` | `default_registry()` | 注册 `ExportCommand` |
| `peri-tui/src/app/panel_manager.rs` | L47-62, L139-153, L381-475 | 新增 `PanelKind::Export` + `PanelState::Export(ExportPanel)` + dispatch |

## 风险点

| 风险 | 影响 | 缓解 |
|------|------|------|
| 大会话导出（>10000 条消息） | 文件巨大、渲染耗时 | `render_messages()` 流式写入（`Write` trait），不一次性构建整个 String；或限制最大消息数 |
| 导出文件覆盖已有文件 | 数据丢失 | 写入前检查文件存在，提示用户确认（Panel 中显示警告） |
| 剪贴板内容过大（>1MB） | 部分平台剪贴板有限制 | 超过阈值时警告用户，建议改用文件导出 |
| Tool 输出中包含敏感信息（API key 等） | 安全风险 | Tool result 默认只导出 `(N lines)` 摘要，不导出完整内容 |
| `load_messages()` 加载历史消息阻塞 UI | 大会话卡顿 | 用 `tokio::spawn_blocking()` 包装 SQLite 读取 |
