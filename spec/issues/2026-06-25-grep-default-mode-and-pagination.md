# Grep 工具 P0 优化：默认模式 + max-columns + 分页提示

**状态**：Open
**优先级**：中（行为对齐，影响 token 开销）
**创建日期**：2026-06-25
**关联 Issue**：[#4](https://github.com/cc-claws/cc-code/issues/4)

## 问题描述

参考 [cc-claws/claude-code](https://github.com/cc-claws/claude-code) 上游 `src/tools/GrepTool/GrepTool.ts` 实现，cc-code 的 Grep 工具在 3 个关键行为上偏离上游设计，导致 token 开销偏高、模型对结果完整性缺乏感知。

## 与 Claude Code 上游的差异

| 维度 | cc-code 现状 | Claude Code 上游 | 影响 |
|------|--------------|------------------|------|
| default output_mode | `content`（输出匹配行） | `files_with_matches`（输出文件名） | 🔴 默认模式更费 token |
| `--max-columns` 限制 | 无 | `--max-columns 500` | 🔴 minified/base64 文件输出超长行撑爆 context |
| 截断提示 | `...(truncated at N lines)` | `[Showing results with pagination = limit: N, offset: M]` | 🟡 模型不知道可以分页 |

## 方案（P0 三项）

### (1) default output_mode 改为 `files_with_matches`

**修改**：`grep_args.rs:79`

```rust
// before
let mode_str = self.output_mode.as_deref().unwrap_or("content");
// after
let mode_str = self.output_mode.as_deref().unwrap_or("files_with_matches");
```

**文档同步**：`grep.rs` 的 `GREP_DESCRIPTION` 描述默认值改为 `files_with_matches`。

**兼容性**：现有调用如果不传 output_mode，输出会从匹配行变为文件名列表。这是**有意为之**的 token 节省，对齐 Claude Code。LLM 提示词里已说明默认值，影响可控。

### (2) max-columns 限制

grep crate 的 `SearcherBuilder` 无原生 max_columns 支持，在 `SearchSink.matched()` 里手动按字节长度过滤。

**修改**：
- `grep_format.rs`：`SearchSink` 加 `max_columns: usize` 字段；`matched()` 的 `OutputMode::Default` 分支检查 `mat.bytes().len() > max_columns` 时跳过（不 push 到 results，但仍计入 match_count 用于计数模式）。
- `grep.rs`：构造 SearchSink 时传入 `max_columns: 500`（常量，与 Claude Code 一致）。

**字节长度 vs 字符长度**：用 byte length 检查（简单、O(1)），CJK 双宽字符可能误判但偏差可接受（500 bytes 约等于 250 个 CJK 字符或 500 个 ASCII 字符，都远超常规代码行）。

**只影响 Default 模式**：FilesOnly/CountOnly/FilesWithoutMatch 模式不输出行内容，无需检查。

### (3) 截断时分页提示

**修改**：`grep.rs:243-249`

```rust
// before
if total >= head_limit && head_limit > 0 {
    let persist_hint = persist_truncated_output(&output);
    output.push_str(&format!("\n... (truncated at {} lines)", head_limit));
    output.push_str(&persist_hint);
}

// after
if total >= head_limit && head_limit > 0 {
    let persist_hint = persist_truncated_output(&output);
    let offset_val = offset.unwrap_or(0);
    output.push_str(&format!(
        "\n\n[Showing results with pagination = limit: {}, offset: {}]",
        head_limit, offset_val
    ));
    output.push_str(&persist_hint);
}
```

需要在 `execute_search` 函数签名里加 `offset: Option<usize>` 参数（或从外部传入）。

**模型友好**：明确告知「还有更多结果，可用更大 offset 分页」，避免模型误以为搜索已完整。

## 测试计划

### 单元测试（grep_test.rs）

新增 3 个测试用例：

1. `test_grep_default_output_mode_is_files_with_matches`：不传 output_mode，验证输出是文件名列表格式（`{path}` 而非 `{path}:{lineno}: {content}`）
2. `test_grep_max_columns_skips_long_lines`：构造一行 > 500 bytes 的内容，验证 Default 模式不输出该行
3. `test_grep_truncated_shows_pagination_hint`：head_limit=2，构造 3 行匹配，验证输出含 `[Showing results with pagination = limit: 2, offset: 0]`

### 现有测试回归

确保现有 `grep_test.rs` 中所有显式传 output_mode 的测试不受影响。

### 不验证（需 TUI 自测）

- 实际 LLM 调用时的 token 节省量（需真实环境观测）
- CJK 双宽字符在 max-columns 下的边界行为

## 不做（明确范围）

- ❌ P1/P2 项（VCS 排除、UNC 安全、Did you mean 提示、参数简化）—— 留待后续 PR
- ❌ 删除 fixed_strings/invert_match/whole_word（功能减弱，需单独讨论）
- ❌ 改底层 grep crate 为 rg 二进制（性能优势保留）
- ❌ 重构 grep.rs/grep_args.rs/grep_format.rs 文件拆分（结构合理）
