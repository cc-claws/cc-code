# Input History Persistence — Implementation Plan

**Issue**: [2026-05-25-input-history-persistence.md](../../spec/issues/2026-05-25-input-history-persistence.md)
**Status**: Pending
**Date**: 2026-05-25

## Overview

当前输入历史（上下方向键回溯）仅存储在内存 `Vec<String>` 中，session 结束丢失。需要持久化到磁盘，按项目目录（cwd）隔离。

**Design**: JSON 文件存储，路径 `{cwd}/.peri/history.json`，上限 1000 条。

## Tasks

### Task 1: 创建持久化模块

**File**: `peri-tui/src/app/history_persistence.rs` (new)

**What**: 创建 `load_input_history(cwd: &str) -> Vec<String>` 和 `save_input_history(cwd: &str, history: &[String])` 函数。

**Details**:
- Path: `{cwd}/.peri/history.json`
- Format: JSON array of strings, newest first
- Load: 文件不存在返回空 Vec，JSON 解析失败返回空 Vec（不 crash）
- Save: 原子写入（先写 `.tmp` 再 `rename`），静默忽略 IO 错误
- Module declaration: 追加 `mod history_persistence;` 到 `modules_agent.inc`

### Task 2: UiState 构造函数接受 cwd

**Files**: 
- `peri-tui/src/app/ui_state.rs` — `UiState::new()` 签名改为 `fn new(textarea: TextArea<'static>, cwd: &str)`
- `peri-tui/src/app/chat_session.rs` — `ChatSession::new()` 传递 `cwd` 给 `UiState::new()`

**What**: `UiState::new()` 调用 `load_input_history(cwd)` 初始化 `input_history`，替代空 Vec。

### Task 3: push_input_history 保存到磁盘

**File**: `peri-tui/src/app/history_ops.rs`

**What**: `push_input_history()` 在 `truncate()` 之后调用 `save_input_history(cwd, &history)`。

**cwd 获取**: `self.services.cwd`（App 已有此字段）。

### Task 4: 提高上限

**File**: `peri-tui/src/app/history_ops.rs`

**What**: `truncate(200)` → `truncate(1000)`。

## Verification

- [ ] `cargo build -p peri-tui` 成功
- [ ] `cargo test` 全量通过
- [ ] `cargo clippy -p peri-tui` 无警告

## Files Changed

| File | Task | Change |
|------|------|--------|
| `peri-tui/src/app/history_persistence.rs` | 1 | 新建：load/save 函数 |
| `peri-tui/src/app/modules_agent.inc` | 1 | 追加 `mod history_persistence;` |
| `peri-tui/src/app/ui_state.rs` | 2 | `new()` 接受 cwd，调用 load |
| `peri-tui/src/app/chat_session.rs` | 2 | 传递 cwd 给 UiState |
| `peri-tui/src/app/history_ops.rs` | 3+4 | save + 提高上限 |
