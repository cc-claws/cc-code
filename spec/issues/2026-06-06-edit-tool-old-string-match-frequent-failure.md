# Edit 工具 old_string 匹配频繁失败，Agent 被迫 fallback 到 Python 脚本

**状态**：Open
**优先级**：高
**创建日期**：2026-06-06
**类型**：Bug

## 问题描述

Agent 使用 Edit 工具编辑 CRLF 格式的文件时，`old_string` 精确匹配失败。根因是 Read 工具用 `content.split('\n')` 分割行，CRLF 文件的 `\r` 会留在行尾，但 LLM 提取 old_string 时可能去掉 `\r`，导致与 Edit 工具读取的原始 CRLF 内容不匹配。

## 根因分析（100% 确认）

### 问题链

```
文件 CRLF 格式
    ↓
Read 工具 split('\n') → 每行末尾保留 \r
    ↓
Read 输出包含 \r
    ↓
LLM 提取 old_string → 去掉 \r → LF 格式
    ↓
Edit 工具 read_to_string → CRLF 格式
    ↓
contains(LF) in CRLF → false → 匹配失败
```

### 代码问题

Read 工具（`read.rs:172`）：
```rust
let lines: Vec<&str> = content.split('\n').collect();
```

`split('\n')` 对 CRLF 文件会保留 `\r`：
- 输入：`"line1\r\nline2\r\n"`
- 输出：`["line1\r", "line2\r", ""]`

### 100% 复现方法

```python
# 1. 创建 CRLF 格式文件
with open('test_crlf.txt', 'wb') as f:
    f.write(b"line1\r\nline2\r\nline3\r\n")

# 2. Read 工具输出
# '     1\tline1\r\n     2\tline2\r\n     3\tline3\r\n     4\t'

# 3. LLM 提取 old_string（去掉 \r）
old_string = "line1\nline2\nline3\n"  # LF 格式

# 4. Edit 工具匹配
content = "line1\r\nline2\r\nline3\r\n"  # CRLF 格式
old_string in content  # False!
```

### 验证数据

| 文件 | 行尾符 | Edit 结果 |
|------|--------|----------|
| `ui_state.rs` | LF | 成功 |
| `input_field.rs` | LF | 成功 |
| `main.rs` | CRLF | 失败 |
| `mod.rs` | CRLF | 失败 |

## 症状详情

### 现象 1：CRLF 文件 Edit 失败

Agent 用 Read 读取 CRLF 文件，提取 old_string 传给 Edit，Edit 返回 `old_string not found`。

### 现象 2：LF 文件 Edit 成功

Agent 用 Read 读取 LF 文件，提取 old_string 传给 Edit，Edit 成功。

### 现象 3：错误提示显示"0 行不同"

`build_not_found_hint` 用 `trim()` 比较（忽略 `\r`），显示"0 行不同"，但 `contains()` 精确匹配失败。

## 复现条件

- **复现频率**：CRLF 文件必现
- **触发步骤**：
  1. Read 一个 CRLF 格式的文件
  2. LLM 提取 old_string 时去掉 `\r`
  3. Edit 匹配失败
- **环境**：Windows（`core.autocrlf = true` 时文件为 CRLF）

## 涉及文件

- `peri-middlewares/src/tools/filesystem/read.rs:172` —— `content.split('\n')` 对 CRLF 文件保留 `\r`
- `peri-middlewares/src/tools/filesystem/edit.rs` —— `content.contains(old_string)` 精确匹配

## 关联 Issue

- `spec/issues/2026-06-03-edit-tool-errors-invisible-and-retry-inefficient.md` —— Edit 工具 is_error 标记问题
- `spec/issues/2026-06-03-edit-tool-tab-indent-mismatch.md` —— Tab 缩进匹配失败

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-06 | — | Open | agent | 基于开发闪烁光标功能时的多次 Edit 失败创建 |
| 2026-06-06 | Open | Open | agent | 确认根因：CRLF 文件 split('\n') 保留 \r，LLM 去掉 \r 导致匹配失败 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
