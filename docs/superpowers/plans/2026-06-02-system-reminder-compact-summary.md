# Compact Summary `<system-reminder>` Wrapping + TUI Collapse

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wrap compact summary text in `<system-reminder>` tags so TUI renders it as a single-line hint instead of full markdown.

**Architecture:** 3-layer change — CompactMiddleware adds the XML wrapper, MessageViewModel detects it and sets a flag, message_render branches on the flag to render one line. No new ViewModel variant.

**Tech Stack:** Rust, ratatui

---

### Task 1: CompactMiddleware — wrap summary text

**Files:**
- Modify: `peri-middlewares/src/compact_middleware.rs:227-231`

- [ ] **Step 1: Wrap summary text in `<system-reminder>` tags**

In `do_full_compact()`, change the `summary_content` formatting from:

```rust
let summary_content = format!(
    "{}\n\n[上下文已压缩，请根据摘要继续工作]",
    compact_result.summary
);
```

To:

```rust
let summary_content = format!(
    "<system-reminder>\n{}\n\n[上下文已压缩，请根据摘要继续工作]\n</system-reminder>",
    compact_result.summary
);
```

`compact_result.summary` already contains the `此会话从之前的对话延续。以下是之前对话的摘要。\n\n` prefix from `postprocess_summary()` — no need to add it again.

- [ ] **Step 2: Build and verify compilation**

```bash
cargo build -p peri-middlewares
```

Expected: PASS

- [ ] **Step 3: Run existing compact middleware tests**

```bash
cargo test -p peri-middlewares --lib -- compact_middleware
```

Expected: all existing tests PASS (model is None in all tests, so full_compact never executes)

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/compact_middleware.rs
git commit -m "feat: wrap compact summary in <system-reminder> tags"
```

---

### Task 2: MessageViewModel — detect `<system-reminder>` and set flag

**Files:**
- Modify: `peri-tui/src/ui/message_view/mod.rs`
- Modify: `peri-tui/src/ui/message_view/message_view_test.rs`

- [ ] **Step 1: Add `system_reminder` field to `UserBubble` variant**

In `message_view/mod.rs`, add the field (line 82-87):

```rust
UserBubble {
    #[allow(dead_code)]
    content: String,
    rendered: Text<'static>,
    content_hash: u64,
    system_reminder: bool,  // NEW
},
```

- [ ] **Step 2: Update `from_base_message_with_cwd` to detect and strip tags**

Replace lines 466-476 (the `BaseMessage::Human` match arm):

```rust
BaseMessage::Human { content, .. } => {
    let raw = content.text_content();
    let (display_text, system_reminder) = if raw.contains("<system-reminder>") {
        let cleaned = raw
            .replacen("<system-reminder>\n", "", 1)
            .replacen("\n</system-reminder>", "", 1)
            .trim()
            .to_string();
        (cleaned, true)
    } else {
        (raw, false)
    };
    let rendered = parse_markdown_default(&display_text);
    let mut vm = MessageViewModel::UserBubble {
        content: display_text,
        rendered,
        content_hash: 0,
        system_reminder,
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 3: Update `user()` constructor to set `system_reminder: false`**

In `user()` (line 747), add the field:

```rust
pub fn user(content: String) -> Self {
    let rendered = parse_markdown_default(&content);
    let mut vm = MessageViewModel::UserBubble {
        content,
        rendered,
        content_hash: 0,
        system_reminder: false,  // NEW
    };
    vm.recompute_hash();
    vm
}
```

- [ ] **Step 4: Update `PartialEq` for `UserBubble`**

Replace lines 169-173:

```rust
(
    MessageViewModel::UserBubble { content: a, system_reminder: a_sr, .. },
    MessageViewModel::UserBubble { content: b, system_reminder: b_sr, .. },
) => a == b && a_sr == b_sr,
```

- [ ] **Step 5: Update `Hash` for `UserBubble`**

Replace lines 273-276:

```rust
MessageViewModel::UserBubble { content, system_reminder, .. } => {
    0u8.hash(state);
    content.hash(state);
    system_reminder.hash(state);  // NEW
}
```

- [ ] **Step 6: Add unit test for tag detection**

In `message_view_test.rs`, append:

```rust
#[test]
fn test_human_message_with_system_reminder_detection() {
    let text = "<system-reminder>\n此会话从之前的对话延续。\n## Summary\nDone.\n[上下文已压缩，请根据摘要继续工作]\n</system-reminder>";
    let msg = BaseMessage::human(text);
    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::UserBubble { system_reminder, content, .. } => {
            assert!(system_reminder, "应识别为系统提醒");
            assert!(!content.contains("<system-reminder>"), "标签应被剥离: {}", content);
            assert!(!content.contains("</system-reminder>"), "结束标签应被剥离");
            assert!(content.contains("## Summary"), "正文应保留");
            assert!(content.contains("[上下文已压缩"), "续接指令应保留");
        }
        _ => panic!("应为 UserBubble"),
    }
}

#[test]
fn test_human_message_without_system_reminder() {
    let text = "普通用户消息";
    let msg = BaseMessage::human(text);
    let vm = MessageViewModel::from_base_message(&msg, &[]);
    match vm {
        MessageViewModel::UserBubble { system_reminder, .. } => {
            assert!(!system_reminder, "普通消息不应标记为系统提醒");
        }
        _ => panic!("应为 UserBubble"),
    }
}
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p peri-tui --lib -- message_view
```

Expected: 2 new tests + all existing tests PASS

- [ ] **Step 8: Verify full build compiles**

```bash
cargo build -p peri-tui
```

Expected: PASS (all pattern matches with `..` still compile)

- [ ] **Step 9: Commit**

```bash
git add peri-tui/src/ui/message_view/mod.rs peri-tui/src/ui/message_view/message_view_test.rs
git commit -m "feat: detect <system-reminder> in Human messages, set system_reminder flag"
```

---

### Task 3: message_render — render hint line for system reminders

**Files:**
- Modify: `peri-tui/src/ui/message_render.rs`
- Modify: `peri-tui/src/ui/message_render_test.rs`

- [ ] **Step 1: Add system_reminder rendering branch**

Replace the `MessageViewModel::UserBubble` match arm (lines 169-196):

```rust
MessageViewModel::UserBubble { rendered, system_reminder, .. } => {
    if *system_reminder {
        // 系统提醒：渲染一行简略提示
        let hint = Span::styled(
            "📋 上下文已压缩",
            Style::default().fg(theme::DIM).add_modifier(Modifier::ITALIC),
        );
        return vec![Line::from(hint)];
    }
    // 普通 UserBubble — 原有渲染逻辑不变
    let user_bg: Color = theme::USER_BG;
    let mut lines = Vec::with_capacity(rendered.lines.len() + 1);
    for (i, line) in rendered.lines.iter().enumerate() {
        if i == 0 {
            let mut spans = vec![Span::styled(
                "❯ ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
                    .bg(user_bg),
            )];
            for span in &line.spans {
                spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
            }
            lines.push(Line::from(spans));
        } else {
            let mut spans = vec![Span::styled("  ", Style::default().bg(user_bg))];
            for span in &line.spans {
                spans.push(span.clone().patch_style(Style::default().bg(user_bg)));
            }
            lines.push(Line::from(spans));
        }
    }
    lines
}
```

- [ ] **Step 2: Add rendering test**

In `message_render_test.rs`, append:

```rust
#[test]
fn test_render_system_reminder_user_bubble() {
    // 构造一个带 system_reminder 标记的 UserBubble
    let mut vm = MessageViewModel::user("irrelevant content".to_string());
    if let MessageViewModel::UserBubble { system_reminder, .. } = &mut vm {
        *system_reminder = true;
    }
    vm.recompute_hash();
    let lines = render_view_model(&vm, Some(1), 80, false);
    assert_eq!(lines.len(), 1, "系统提醒应只渲染一行");
    let text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
    assert!(text.contains("上下文已压缩"), "应显示压缩提示文字，实际: {}", text);
}

#[test]
fn test_render_normal_user_bubble_unchanged() {
    let vm = MessageViewModel::user("Hello World".to_string());
    let lines = render_view_model(&vm, Some(1), 80, false);
    // 普通用户消息应有 ❯ 前缀
    let first_text: String = lines[0].spans.iter().map(|s| s.content.clone()).collect();
    assert!(first_text.contains("❯"), "普通消息应有 ❯ 前缀");
    assert!(first_text.contains("Hello"), "应包含原始内容");
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p peri-tui --lib -- message_render
```

Expected: 2 new tests + all existing tests PASS

- [ ] **Step 4: Verify full build**

```bash
cargo build -p peri-tui
```

Expected: PASS

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p peri-tui -p peri-middlewares --lib
```

Expected: all tests PASS

- [ ] **Step 6: Check Clippy**

```bash
cargo clippy -p peri-tui -p peri-middlewares
```

Expected: no new warnings

- [ ] **Step 7: Commit**

```bash
git add peri-tui/src/ui/message_render.rs peri-tui/src/ui/message_render_test.rs
git commit -m "feat: render system-reminder UserBubble as single-line hint"
```

---

### Verification Checklist

After implementation, run the full pre-commit:

```bash
lefthook run pre-commit
```

Then manual smoke test: trigger a compact in TUI, verify the compact summary appears as `📋 上下文已压缩` instead of full markdown.
