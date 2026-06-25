# Skill Preload 主 Agent 消息历史注入 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让主 Agent 的 `SkillPreloadMiddleware` 在用户消息包含 `/skill-name` 时自动检测并以 fake Read 工具调用注入 skill 全文到消息历史。

**Architecture:** 采用 Strategy B（中间件自检测）——不修改 ACP 协议或 `execute_prompt` 签名。在 `SkillPreloadMiddleware.before_agent` 中，当 `skill_names` 为空时，从 state 的最后一条 Human 消息中提取 `/xxx` token，与可用 skill 列表匹配后注入。SubAgent 路径不受影响（仍使用显式 `skill_names`）。

**Tech Stack:** Rust, `peri-middlewares` crate, 现有 `list_skills` + `parse_skill_names_from_input` 逻辑。

---

## File Structure

| 文件 | 职责 | 操作 |
|------|------|------|
| `peri-middlewares/src/subagent/skill_preload.rs` | 核心中间件，新增自检测逻辑 | 修改 |
| `peri-middlewares/src/subagent/skill_preload_test.rs` | 测试 | 修改 |
| `peri-tui/src/app/agent_submit.rs` | 移除死代码 `parse_skill_names_from_input` | 修改 |
| `peri-tui/src/app/agent_submit_test.rs` | 移除对应测试 | 修改 |

---

### Task 1: 提取 skill 名称解析为公共函数

**Files:**
- Modify: `peri-middlewares/src/subagent/skill_preload.rs`

**背景：** `peri-tui/src/app/agent_submit.rs` 中有 `parse_skill_names_from_input` 函数（当前是死代码）。我们需要把这个逻辑移到 `skill_preload.rs` 中作为公共函数，供中间件内部使用。同时保留 TUI 侧的死代码移除到 Task 3。

- [ ] **Step 1: 在 `skill_preload.rs` 中添加 `extract_skill_names_from_text` 函数**

在 `SkillPreloadMiddleware` 结构体定义之前（`use` 语句之后），添加公共函数：

```rust
/// 从文本中提取 `/skill-name` 模式的 skill 名称
///
/// 支持格式：
/// - `/skill-name` — 单个 skill
/// - `/skill-a /skill-b` — 多个 skill（空格分隔）
/// - 消息中任意位置出现即可（不限于行首）
///
/// 仅匹配由 `/` 开头、后跟 `[a-zA-Z0-9_-]` 的 token。
pub fn extract_skill_names_from_text(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let name = word.strip_prefix('/')?;
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}
```

- [ ] **Step 2: 运行现有测试确保无回归**

Run: `cargo test -p peri-middlewares --lib -- skill_preload`
Expected: 所有现有测试 PASS（6 个）

---

### Task 2: 修改 `before_agent` 实现自检测

**Files:**
- Modify: `peri-middlewares/src/subagent/skill_preload.rs`

**核心逻辑：** 当 `self.skill_names` 为空时，从 state 的最后一条 Human 消息中提取 skill 名称；非空时保持原有行为（SubAgent 路径）。

- [ ] **Step 1: 写失败测试——主 Agent 自检测单个 skill**

在 `skill_preload_test.rs` 末尾添加：

```rust
#[tokio::test]
async fn test_auto_detect_skill_from_human_message() {
    // Arrange: 模拟主 Agent 场景——skill_names 为空，但 state 中有包含 /skill-name 的 Human 消息
    let dir = tempdir().unwrap();
    let skills_dir = dir.path().join(".claude").join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();
    write_skill(&skills_dir, "diagnose", "诊断技能");

    let mw = SkillPreloadMiddleware::new(vec![], dir.path().to_str().unwrap());
    let mut state = AgentState::new(dir.path().to_str().unwrap());
    // 模拟 executor 添加用户消息
    state.add_message(BaseMessage::human("帮我用 /diagnose 调试一下"));

    // Act
    mw.before_agent(&mut state).await.unwrap();

    // Assert: 应自动检测并注入 Ai + Tool = 2 条消息
    assert_eq!(state.messages().len(), 3, "应注入 2 条消息（Ai + Tool），加上原始 Human 消息共 3 条");
    // 第一条是原始 Human 消息
    assert!(matches!(&state.messages()[0], BaseMessage::Human { .. }), "第一条应为 Human");
    assert!(
        matches!(&state.messages()[1], BaseMessage::Ai { .. }),
        "第二条应为 Ai（fake Read）"
    );
    assert!(
        matches!(&state.messages()[2], BaseMessage::Tool { .. }),
        "第三条应为 Tool（skill 内容）"
    );
    let tool_content = state.messages()[2].content();
    assert!(
        tool_content.contains("Skill content for diagnose"),
        "Tool 结果应包含 skill 全文"
    );
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p peri-middlewares --lib -- test_auto_detect_skill_from_human_message`
Expected: FAIL（当前 `skill_names` 为空直接 early return）

- [ ] **Step 3: 实现 `before_agent` 自检测逻辑**

将 `before_agent` 方法替换为：

```rust
async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
    // 确定要预加载的 skill 名称列表
    let skill_names = if !self.skill_names.is_empty() {
        // SubAgent 路径：使用构造时传入的显式列表
        self.skill_names.clone()
    } else {
        // 主 Agent 路径：从最后一条 Human 消息中自动检测 /skill-name token
        let last_human = state
            .messages()
            .iter()
            .rev()
            .find(|m| matches!(m, BaseMessage::Human { .. }));
        match last_human {
            Some(msg) => extract_skill_names_from_text(&msg.text_content()),
            None => return Ok(()),
        }
    };

    if skill_names.is_empty() {
        return Ok(());
    }

    let dirs = self.resolve_dirs();
    let names_lower: Vec<String> = skill_names.iter().map(|s| s.to_lowercase()).collect();

    // 在 blocking 线程中扫描目录 + 读取文件内容
    let skill_contents = tokio::task::spawn_blocking(move || {
        let all_skills = list_skills(&dirs);
        all_skills
            .into_iter()
            .filter(|s| names_lower.contains(&s.name.to_lowercase()))
            .filter_map(|s| {
                let content = std::fs::read_to_string(&s.path).ok()?;
                Some((s.path.to_string_lossy().to_string(), content))
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| peri_agent::error::AgentError::MiddlewareError {
        middleware: "SkillPreloadMiddleware".to_string(),
        reason: format!("spawn_blocking 失败: {e}"),
    })?;

    if skill_contents.is_empty() {
        return Ok(());
    }

    // Generate tool_call_ids: call_{uuid hex without hyphens, 32 chars}
    let call_ids: Vec<String> = (0..skill_contents.len())
        .map(|_| format!("call_{}", uuid::Uuid::new_v4().simple()))
        .collect();

    // 构造 Ai 消息的 ToolUse ContentBlock 列表
    let tool_use_blocks: Vec<ContentBlock> = skill_contents
        .iter()
        .zip(call_ids.iter())
        .map(|((path, _), id)| {
            ContentBlock::tool_use(id.clone(), "Read", serde_json::json!({ "path": path }))
        })
        .collect();

    // 追加 Ai 消息（ai_from_blocks 自动双写 tool_calls）
    state.add_message(BaseMessage::ai_from_blocks(tool_use_blocks));

    // 追加 Tool 结果消息
    for (id, (_, content)) in call_ids.iter().zip(skill_contents.iter()) {
        state.add_message(BaseMessage::tool_result(id.clone(), content.clone()));
    }

    Ok(())
}
```

- [ ] **Step 4: 运行所有 skill_preload 测试确认通过**

Run: `cargo test -p peri-middlewares --lib -- skill_preload`
Expected: 所有测试 PASS（7 个，含新增 1 个）

---

### Task 3: 补充边界测试

**Files:**
- Modify: `peri-middlewares/src/subagent/skill_preload_test.rs`

- [ ] **Step 1: 添加多个 skill 自动检测测试**

```rust
#[tokio::test]
async fn test_auto_detect_multiple_skills() {
    let dir = tempdir().unwrap();
    let skills_dir = dir.path().join(".claude").join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();
    write_skill(&skills_dir, "skill-a", "技能 A");
    write_skill(&skills_dir, "skill-b", "技能 B");

    let mw = SkillPreloadMiddleware::new(vec![], dir.path().to_str().unwrap());
    let mut state = AgentState::new(dir.path().to_str().unwrap());
    state.add_message(BaseMessage::human("/skill-a /skill-b 帮我看看"));

    mw.before_agent(&mut state).await.unwrap();

    // 1 Human + 1 Ai + 2 Tool = 4 条
    assert_eq!(state.messages().len(), 4, "2 个 skill 应注入 Ai + 2 Tool = 3 条，加 Human 共 4 条");
    assert_eq!(state.messages()[1].tool_calls().len(), 2, "Ai 消息应有 2 个 ToolUse");
}
```

- [ ] **Step 2: 添加不匹配 skill 时无注入测试**

```rust
#[tokio::test]
async fn test_auto_detect_no_matching_skill() {
    let dir = tempdir().unwrap();
    // 不创建任何 skill 文件

    let mw = SkillPreloadMiddleware::new(vec![], dir.path().to_str().unwrap());
    let mut state = AgentState::new(dir.path().to_str().unwrap());
    state.add_message(BaseMessage::human("/nonexistent-skill 不存在"));

    mw.before_agent(&mut state).await.unwrap();

    // 只有原始 Human 消息，无注入
    assert_eq!(state.messages().len(), 1, "找不到 skill 时不应注入任何消息");
}
```

- [ ] **Step 3: 添加无 Human 消息时 no-op 测试**

```rust
#[tokio::test]
async fn test_auto_detect_no_human_message() {
    let dir = tempdir().unwrap();
    let mw = SkillPreloadMiddleware::new(vec![], dir.path().to_str().unwrap());
    let mut state = AgentState::new(dir.path().to_str().unwrap());
    // 不添加任何消息

    mw.before_agent(&mut state).await.unwrap();

    assert_eq!(state.messages().len(), 0, "无 Human 消息时应 no-op");
}
```

- [ ] **Step 4: 添加 `extract_skill_names_from_text` 单元测试**

```rust
#[test]
fn test_extract_skill_names_basic() {
    let names = extract_skill_names_from_text("/diagnose");
    assert_eq!(names, vec!["diagnose"]);
}

#[test]
fn test_extract_skill_names_multiple() {
    let names = extract_skill_names_from_text("/diagnose /fix-issue /caveman");
    assert_eq!(names, vec!["diagnose", "fix-issue", "caveman"]);
}

#[test]
fn test_extract_skill_names_in_sentence() {
    let names = extract_skill_names_from_text("帮我用 /diagnose 调试一下这个问题");
    assert_eq!(names, vec!["diagnose"]);
}

#[test]
fn test_extract_skill_names_no_match() {
    let names = extract_skill_names_from_text("普通消息没有 skill");
    assert!(names.is_empty());
}

#[test]
fn test_extract_skill_names_slash_only() {
    let names = extract_skill_names_from_text("/");
    assert!(names.is_empty());
}

#[test]
fn test_extract_skill_names_rejects_special_chars() {
    // /foo/bar 不应匹配（包含 /）
    let names = extract_skill_names_from_text("/foo/bar");
    assert_eq!(names, vec!["foo"]); // 只匹配 /foo，/bar 单独处理
}

#[test]
fn test_extract_skill_names_command_not_matched() {
    // 已知的命令前缀如 /help、/compact 不应作为 skill 加载
    // 但当前设计不在此过滤——skill 查找时找不到就静默跳过
    // 如果 /help 不是 skill 名称，list_skills 不会返回它，结果为空
    let names = extract_skill_names_from_text("/help");
    assert_eq!(names, vec!["help"]); // 解析层不过滤，由 list_skills 匹配层决定
}
```

- [ ] **Step 5: 运行全部测试**

Run: `cargo test -p peri-middlewares --lib -- skill_preload`
Expected: 所有测试 PASS

---

### Task 4: 清理 TUI 侧死代码

**Files:**
- Modify: `peri-tui/src/app/agent_submit.rs`
- Modify: `peri-tui/src/app/agent_submit_test.rs`

`parse_skill_names_from_input` 函数和对应测试现在不再需要——逻辑已移入 `SkillPreloadMiddleware` 内部。

- [ ] **Step 1: 移除 `parse_skill_names_from_input` 函数及其 `#[allow(dead_code)]`**

从 `peri-tui/src/app/agent_submit.rs` 删除以下代码（行 1-21）：

```rust
// 删除整个函数：
/// 从用户输入中提取 /skill-name 模式的 skill 名称
///
/// 支持格式：
/// - `/skill-name` — 单个 skill
/// - `/skill-a /skill-b` — 多个 skill（空格分隔）
/// - 消息中任意位置出现即可（不限于行首）
#[allow(dead_code)]
fn parse_skill_names_from_input(input: &str) -> Vec<String> {
    let mut names = Vec::new();
    for word in input.split_whitespace() {
        if let Some(name) = word.strip_prefix('/') {
            if !name.is_empty() {
                names.push(name.to_string());
            }
        }
    }
    names
}
```

- [ ] **Step 2: 清空 `agent_submit_test.rs`（所有测试都是 `parse_skill_names_from_input` 的测试）**

由于整个文件都是被删函数的测试，清空为空模块占位：

```rust
// parse_skill_names_from_input 的测试已移至 peri-middlewares skill_preload_test.rs
// （逻辑现在由 SkillPreloadMiddleware 内部的 extract_skill_names_from_text 实现）
```

- [ ] **Step 3: 运行 TUI 编译和测试**

Run: `cargo build -p peri-tui && cargo test -p peri-tui --lib -- agent_submit`
Expected: 编译通过，测试通过（0 个测试——文件已清空）

- [ ] **Step 4: Commit**

```bash
git add peri-middlewares/src/subagent/skill_preload.rs peri-middlewares/src/subagent/skill_preload_test.rs peri-tui/src/app/agent_submit.rs peri-tui/src/app/agent_submit_test.rs
git commit -m "fix: SkillPreloadMiddleware 自动检测用户消息中的 /skill-name 并注入全文

主 Agent 路径的 SkillPreloadMiddleware 之前 skill_names 始终为空（硬编码 Vec::new()），
导致 /skill-name 触发的 skill 全文不会以 fake Read 工具调用注入到消息历史。

改为在 before_agent 中从最后一条 Human 消息自动提取 /xxx token，
与可用 skill 列表匹配后注入。SubAgent 路径不受影响（仍使用显式 skill_names）。

Fixes: spec/issues/2026-05-25-skill-preload-no-tool-calls-in-history.md"
```

---

## Self-Review

### Spec coverage

| 需求 | 覆盖任务 |
|------|----------|
| 主 Agent `/skill-name` 触发 skill 全文注入 | Task 2 |
| 多个 skill 同时注入 | Task 3 |
| 不存在的 skill 静默跳过 | Task 3（已有覆盖 + 新增） |
| SubAgent 路径不受影响 | Task 2（保留原有 `skill_names` 分支） |
| 移除 TUI 侧死代码 | Task 4 |

### Placeholder scan

无 TBD/TODO/placeholders。

### Type consistency

- `extract_skill_names_from_text` 返回 `Vec<String>` — 与 `self.skill_names` 类型一致
- `state.messages().iter().rev().find()` — 返回 `Option<&BaseMessage>`，`.text_content()` 返回 `&str`
- `BaseMessage::human()` / `BaseMessage::ai_from_blocks()` / `BaseMessage::tool_result()` — 与现有代码一致

### 注意事项

1. **`/help`、`/compact` 等命令名冲突**：`extract_skill_names_from_text` 解析出 `/help` 后，`list_skills` 找不到名为 `help` 的 skill 目录，静默跳过。不需要额外过滤逻辑。
2. **去重**：同一个 `/skill-name` 在消息中出现多次时，`list_skills` 过滤只会匹配一次（`all_skills` 中同名 skill 只出现一次），注入不会重复。
3. **大小写不敏感匹配**：已使用 `to_lowercase()` 比较，与原有行为一致。
