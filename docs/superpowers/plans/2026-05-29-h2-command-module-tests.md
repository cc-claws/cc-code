# H2: ACP Slash Command 模块单元测试 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `peri-acp/src/session/command/` 模块添加完整的单元测试覆盖，包括 CommandRegistry 查找逻辑、ClearCommand、CompactCommand 和辅助函数。

**Architecture:** 新建 `command/mod_test.rs` 和 `command/compact_test.rs` 测试文件。使用 MockEventSink 验证事件推送，直接构造 CommandContext 测试 execute 路径。

**Tech Stack:** Rust, tokio async test, serde_json, mock 实现

---

## 文件结构

| 操作 | 文件路径 | 职责 |
|------|----------|------|
| 创建 | `peri-acp/src/session/command/mod_test.rs` | CommandRegistry + ClearCommand 测试 |
| 创建 | `peri-acp/src/session/command/compact_test.rs` | CompactCommand + 辅助函数测试 |
| 参考 | `peri-acp/src/session/command/mod.rs` | CommandRegistry, CommandKind, AgentCommand trait |
| 参考 | `peri-acp/src/session/command/clear.rs` | ClearCommand |
| 参考 | `peri-acp/src/session/command/compact.rs` | CompactCommand, extract_file_info, extract_skill_names |
| 参考 | `peri-acp/src/session/event_sink.rs` | EventSink trait 定义 |

---

### Task 1: 创建 MockEventSink 和 CommandRegistry 测试

**Files:**
- Create: `peri-acp/src/session/command/mod_test.rs`

- [ ] **Step 1: 读取 mod.rs 了解 CommandRegistry 和 AgentCommand trait 的完整定义**

Read `peri-acp/src/session/command/mod.rs` 确认 `find()` 的参数解析逻辑（`/` 前缀处理、别名匹配、参数分离）。

- [ ] **Step 2: 创建 mod_test.rs 文件，包含 MockCommand 和基础 CommandRegistry 测试**

```rust
use super::*;
use crate::session::event_sink::EventSink;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

// MockEventSink 用于测试命令执行时的事件推送
#[derive(Default)]
struct MockEventSink {
    events: Arc<Mutex<Vec<serde_json::Value>>>,
}

#[async_trait]
impl EventSink for MockEventSink {
    async fn push_event(&self, event: serde_json::Value) {
        self.events.lock().unwrap().push(event);
    }

    async fn push_done(&self) {}
}

impl MockEventSink {
    fn collected(&self) -> Vec<serde_json::Value> {
        self.events.lock().unwrap().clone()
    }
}

// MockCommand 用于测试 CommandRegistry 逻辑
struct MockCommand {
    name: &'static str,
    aliases: Vec<&'static str>,
    kind: CommandKind,
}

impl MockCommand {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            aliases: vec![],
            kind: CommandKind::Immediate,
        }
    }

    fn with_aliases(mut self, aliases: Vec<&'static str>) -> Self {
        self.aliases = aliases;
        self
    }

    fn with_kind(mut self, kind: CommandKind) -> Self {
        self.kind = kind;
        self
    }
}

#[async_trait]
impl AgentCommand for MockCommand {
    fn name(&self) -> &str { self.name }
    fn aliases(&self) -> Vec<&str> { self.aliases.clone() }
    fn description(&self) -> &str { "mock command" }
    fn kind(&self) -> CommandKind { self.kind }
    async fn execute(&self, _ctx: CommandContext) -> CommandResult {
        CommandResult {
            messages: vec![],
            stop_reason: crate::session::executor::PromptStopReason::EndTurn,
        }
    }
}

// === CommandRegistry 测试 ===

#[test]
fn test_registry_find_by_exact_name() {
    // 测试精确名称匹配
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(MockCommand::new("compact")));
    let (cmd, args) = reg.find("/compact").unwrap();
    assert_eq!(cmd.name(), "compact");
    assert_eq!(args, "");
}

#[test]
fn test_registry_find_by_alias() {
    // 测试别名匹配
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(MockCommand::new("compact").with_aliases(vec!["compress"])));
    let (cmd, args) = reg.find("/compress").unwrap();
    assert_eq!(cmd.name(), "compact");
    assert_eq!(args, "");
}

#[test]
fn test_registry_find_with_args() {
    // 测试带参数的命令查找
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(MockCommand::new("skill")));
    let (cmd, args) = reg.find("/skill tdd").unwrap();
    assert_eq!(cmd.name(), "skill");
    assert_eq!(args, "tdd");
}

#[test]
fn test_registry_find_returns_none_for_unknown() {
    // 测试未知命令返回 None
    let reg = CommandRegistry::new();
    assert!(reg.find("/unknown").is_none());
}

#[test]
fn test_registry_find_returns_none_for_empty_string() {
    // 测试空字符串返回 None
    let reg = CommandRegistry::new();
    assert!(reg.find("").is_none());
}

#[test]
fn test_registry_find_returns_none_for_double_slash() {
    // 测试双斜杠返回 None
    let reg = CommandRegistry::new();
    assert!(reg.find("//").is_none());
}

#[test]
fn test_registry_find_returns_none_for_slash_only() {
    // 测试仅斜杠返回 None
    let reg = CommandRegistry::new();
    assert!(reg.find("/").is_none());
}

#[test]
fn test_registry_list_returns_all_commands() {
    // 测试 list 返回所有注册命令
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(MockCommand::new("compact").with_aliases(vec!["compress"])));
    reg.register(Box::new(MockCommand::new("clear").with_aliases(vec!["cls"])));
    let list = reg.list();
    assert_eq!(list.len(), 2);
}

#[test]
fn test_registry_find_strips_slash_prefix() {
    // 测试 find 会自动去除 / 前缀
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(MockCommand::new("compact")));
    let (cmd, args) = reg.find("/compact").unwrap();
    assert_eq!(cmd.name(), "compact");
    assert_eq!(args, "");
}
```

- [ ] **Step 3: 确保 mod.rs 中声明了 mod_test.rs 模块**

检查 `peri-acp/src/session/command/mod.rs` 中是否已有 `#[cfg(test)] mod mod_test;`。如果没有，需要添加。

- [ ] **Step 4: 运行测试验证通过**

Run: `cargo test -p peri-acp --lib -- session::command::mod_test`
Expected: ALL PASS

- [ ] **Step 5: 提交**

```bash
git add peri-acp/src/session/command/mod_test.rs peri-acp/src/session/command/mod.rs
git commit -m "test: add CommandRegistry unit tests (find/list/alias/edge cases)"
```

---

### Task 2: 添加 ClearCommand 测试

**Files:**
- Modify: `peri-acp/src/session/command/mod_test.rs`

- [ ] **Step 1: 读取 clear.rs 确认 execute 行为**

Read `peri-acp/src/session/command/clear.rs` 确认 `execute()` 发送的具体事件字段。

- [ ] **Step 2: 添加 ClearCommand 测试**

在 `mod_test.rs` 末尾追加：

```rust
// === ClearCommand 测试 ===

use super::clear::ClearCommand;

fn make_command_context(sink: Arc<MockEventSink>) -> CommandContext {
    CommandContext {
        session_id: "test-session".to_string(),
        history: vec![BaseMessage::human("hello"), BaseMessage::ai_text("hi")],
        cwd: "/tmp".to_string(),
        peri_config: Arc::new(PeriConfig::default()),
        compact_model: None,
        event_sink: sink,
        args: "".to_string(),
    }
}

#[tokio::test]
async fn test_clear_command_returns_empty_messages() {
    // 验证 ClearCommand 返回空消息列表
    let sink = Arc::new(MockEventSink::default());
    let ctx = make_command_context(sink.clone());
    let cmd = ClearCommand;
    let result = cmd.execute(ctx).await;
    assert!(result.messages.is_empty());
}

#[tokio::test]
async fn test_clear_command_returns_end_turn() {
    // 验证 stop_reason 为 EndTurn
    let sink = Arc::new(MockEventSink::default());
    let ctx = make_command_context(sink.clone());
    let cmd = ClearCommand;
    let result = cmd.execute(ctx).await;
    assert!(matches!(result.stop_reason, crate::session::executor::PromptStopReason::EndTurn));
}

#[tokio::test]
async fn test_clear_command_sends_compact_completed_event() {
    // 验证 ClearCommand 通过 event_sink 发送 CompactCompleted 事件
    let sink = Arc::new(MockEventSink::default());
    let ctx = make_command_context(sink.clone());
    let cmd = ClearCommand;
    cmd.execute(ctx).await;
    let events = sink.collected();
    assert!(!events.is_empty(), "应发送 CompactCompleted 事件");
}

#[tokio::test]
async fn test_clear_command_name_and_aliases() {
    // 验证命令名和别名
    let cmd = ClearCommand;
    assert_eq!(cmd.name(), "clear");
    assert!(cmd.aliases().contains(&"cls"));
    assert!(cmd.aliases().contains(&"reset"));
    assert_eq!(cmd.kind(), CommandKind::Immediate);
}
```

- [ ] **Step 3: 运行测试验证通过**

Run: `cargo test -p peri-acp --lib -- session::command::mod_test::test_clear`
Expected: ALL PASS

- [ ] **Step 4: 提交**

```bash
git add peri-acp/src/session/command/mod_test.rs
git commit -m "test: add ClearCommand unit tests (execute/name/aliases)"
```

---

### Task 3: 创建 CompactCommand 测试

**Files:**
- Create: `peri-acp/src/session/command/compact_test.rs`

- [ ] **Step 1: 读取 compact.rs 确认所有路径和辅助函数**

Read `peri-acp/src/session/command/compact.rs` 确认：
- `extract_file_info()` 的输入消息格式和输出
- `extract_skill_names()` 的输入消息格式和输出
- `execute()` 的 4 个代码路径

- [ ] **Step 2: 创建 compact_test.rs，包含辅助函数测试**

```rust
use super::*;
use crate::session::command::mod_test::MockEventSink;
use peri_agent::message::BaseMessage;
use std::sync::Arc;

// === extract_file_info 测试 ===

#[test]
fn test_extract_file_info_解析文件消息() {
    // 验证能从 System 消息中提取文件路径和行数
    let msg = BaseMessage::system("[最近读取的文件: /tmp/test.rs]\nfn main() {}");
    let result = extract_file_info(&[msg]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, "/tmp/test.rs");
}

#[test]
fn test_extract_file_info_多条文件消息() {
    let msg1 = BaseMessage::system("[最近读取的文件: /a.rs]\ncontent1");
    let msg2 = BaseMessage::system("[最近读取的文件: /b.rs]\ncontent2");
    let result = extract_file_info(&[msg1, msg2]);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_extract_file_info_空消息列表() {
    let result = extract_file_info(&[]);
    assert!(result.is_empty());
}

#[test]
fn test_extract_file_info_非文件消息跳过() {
    let msg = BaseMessage::system("普通系统消息");
    let result = extract_file_info(&[msg]);
    assert!(result.is_empty());
}

// === extract_skill_names 测试 ===

#[test]
fn test_extract_skill_names_解析技能消息() {
    let msg = BaseMessage::system("[激活的 Skill 指令: tdd]");
    let result = extract_skill_names(&[msg]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], "tdd");
}

#[test]
fn test_extract_skill_names_多条技能消息() {
    let msg1 = BaseMessage::system("[激活的 Skill 指令: tdd]");
    let msg2 = BaseMessage::system("[激活的 Skill 指令: debug]");
    let result = extract_skill_names(&[msg1, msg2]);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_extract_skill_names_空消息列表() {
    let result = extract_skill_names(&[]);
    assert!(result.is_empty());
}

#[test]
fn test_extract_skill_names_非技能消息跳过() {
    let msg = BaseMessage::system("普通消息");
    let result = extract_skill_names(&[msg]);
    assert!(result.is_empty());
}
```

- [ ] **Step 3: 确保 compact.rs 中声明了 compact_test.rs 模块**

检查 `peri-acp/src/session/command/compact.rs` 中是否已有 `#[cfg(test)] mod compact_test;`。如果没有，需要添加。

- [ ] **Step 4: 运行辅助函数测试验证通过**

Run: `cargo test -p peri-acp --lib -- session::command::compact_test`
Expected: ALL PASS

- [ ] **Step 5: 提交**

```bash
git add peri-acp/src/session/command/compact_test.rs peri-acp/src/session/command/compact.rs
git commit -m "test: add extract_file_info and extract_skill_names unit tests"
```

---

### Task 4: 添加 CompactCommand execute 路径测试

**Files:**
- Modify: `peri-acp/src/session/command/compact_test.rs`

- [ ] **Step 1: 添加空历史路径测试**

在 `compact_test.rs` 末尾追加：

```rust
use super::super::compact::CompactCommand;
use super::super::mod_test::{MockEventSink, make_command_context};

#[tokio::test]
async fn test_compact_command_空历史返回原消息() {
    // 空历史 → 返回原始 history + 发送警告事件
    let sink = Arc::new(MockEventSink::default());
    let mut ctx = make_command_context(sink.clone());
    ctx.history = vec![];
    let cmd = CompactCommand;
    let result = cmd.execute(ctx).await;
    assert!(result.messages.is_empty());
}
```

- [ ] **Step 2: 添加无 model 路径测试**

```rust
#[tokio::test]
async fn test_compact_command_无model返回原消息() {
    // 无 compact_model → 返回原始 history + 发送错误事件
    let sink = Arc::new(MockEventSink::default());
    let ctx = make_command_context(sink.clone());
    // make_command_context 默认 compact_model: None
    let cmd = CompactCommand;
    let result = cmd.execute(ctx).await;
    assert_eq!(result.messages.len(), 2); // 原始 history
}
```

- [ ] **Step 3: 添加命令属性测试**

```rust
#[test]
fn test_compact_command_name_and_aliases() {
    let cmd = CompactCommand;
    assert_eq!(cmd.name(), "compact");
    assert!(cmd.aliases().contains(&"compress"));
    assert_eq!(cmd.kind(), CommandKind::Immediate);
}
```

- [ ] **Step 4: 运行所有 compact 测试验证通过**

Run: `cargo test -p peri-acp --lib -- session::command::compact_test`
Expected: ALL PASS

- [ ] **Step 5: 提交**

```bash
git add peri-acp/src/session/command/compact_test.rs
git commit -m "test: add CompactCommand execute path tests (empty history, no model)"
```

---

### Task 5: 运行全量测试确认无回归

- [ ] **Step 1: 运行 peri-acp 全量测试**

Run: `cargo test -p peri-acp --lib`
Expected: ALL PASS

- [ ] **Step 2: 运行 peri-agent 全量测试**

Run: `cargo test -p peri-agent --lib`
Expected: ALL PASS
