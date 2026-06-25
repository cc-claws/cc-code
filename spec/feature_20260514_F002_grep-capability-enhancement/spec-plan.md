# Implementation Plan: 20260514_F002 - Grep 工具能力增强

**Issue**: `spec/issues/2026-05-14-grep-tool-capability-gap.md`

## 依赖关系

```
Task 1 (P0: 修复声明不实现参数)
  |
  +---> Task 2 (P1: 新增参数 -v/-F/-A/-B + output_mode 默认值)
  |       |
  |       +---> Task 3 (P2: files_without_matches + max_depth)
  |               |
  |               +---> Task 4 (测试 + description 更新)
  |
  Task 5 (grep crate multi_line 兼容) -- 独立，可与 Task 1-3 并行
```

---

## Task 1: P0 — 修复声明但不实现的参数

**文件**: `peri-middlewares/src/tools/filesystem/grep.rs`

### 1a. 实现 `multiline`

`GrepInput._multiline` 已声明但从未传递给 regex engine。

**变更**:

1. `GrepInput` 字段重命名：`_multiline` → `multiline`
2. `ParsedArgs` 新增字段：`multiline: bool`
3. `to_parsed_args()` 传递：`multiline: self.multiline`
4. `execute_search()` 构建 matcher 时：
   ```rust
   let mut builder = RegexMatcherBuilder::new();
   builder
       .case_insensitive(parsed.case_insensitive)
       .word(parsed.whole_word);
   if parsed.multiline {
       // multi_line: ^/$ 匹配行边界（非字符串边界）
       // dot_matches_new_line: . 匹配 \n
       builder.multi_line(true).dot_matches_new_line(true);
   }
   let matcher = builder.build(&parsed.pattern)?;
   ```
5. `execute_search()` 构建 searcher 时：
   ```rust
   if parsed.multiline {
       searcher_builder.multi_line(true);
   }
   ```
   > `Searcher.multi_line(true)` 启用跨行搜索模式（按 chunk 而非按行），与 `RegexMatcherBuilder.multi_line()` 配合才能正确工作。

6. `invoke()` 中解析 `multiline`：去掉 `_` 前缀
   ```rust
   multiline: input.get("multiline").and_then(|v| v.as_bool()).unwrap_or(false),
   ```

### 1b. 实现 `-n` 开关

当前硬编码 `line_number(true)`，参数存在但无效果。

**变更**:

1. `GrepInput` 字段重命名：`_line_number` → `line_number`
2. `ParsedArgs` 新增字段：`line_number: bool`
3. `to_parsed_args()` 传递：`line_number: self.line_number`
4. `execute_search()` 构建 searcher 时：
   ```rust
   searcher_builder.line_number(parsed.line_number);
   ```
5. `SearchSink.matched()` 中行号格式化适配：
   ```rust
   let line = if self.show_line_numbers {
       format!("{}:{}: {}", self.display_path, line_number, content)
   } else {
       format!("{}: {}", self.display_path, content)
   };
   ```
   需要在 `SearchSink` 中新增 `show_line_numbers: bool` 字段。
6. `invoke()` 中解析：去掉 `_` 前缀

### 1c. 暴露 `whole_word` (`-w`)

内部 `ParsedArgs.whole_word` 已存在，`RegexMatcherBuilder.word()` 已接线，但 `GrepInput` 未暴露。

**变更**:

1. `GrepInput` 新增字段：`whole_word: bool`（默认 `false`）
2. `to_parsed_args()` 改为：`whole_word: self.whole_word`（原来是硬编码 `false`）
3. `invoke()` 解析：
   ```rust
   whole_word: input.get("whole_word").and_then(|v| v.as_bool()).unwrap_or(false),
   ```
4. `parameters()` JSON schema 新增 `whole_word` 属性：
   ```json
   "whole_word": {
       "type": "boolean",
       "description": "Match whole words only (default: false)"
   }
   ```

### 1d. 从 JSON schema 移除已无用的 `_` 前缀字段

`-n` 和 `multiline` 在 `parameters()` 中的声明保持不变（字段名不含 `_` 前缀），只是内部实现从"忽略"变为"生效"。

**验证**: `cargo build -p peri-middlewares` 编译通过。

---

## Task 2: P1 — 新增高频参数 + output_mode 默认值

**文件**: `peri-middlewares/src/tools/filesystem/grep.rs`

### 2a. 添加 `-v` / `invert_match`

`grep-searcher` crate 原生支持 `SearcherBuilder::invert_match(true)`。

**变更**:

1. `GrepInput` 新增：`invert_match: bool`（默认 `false`）
2. `ParsedArgs` 新增：`invert_match: bool`
3. `execute_search()` 构建 searcher 时：
   ```rust
   searcher_builder.invert_match(parsed.invert_match);
   ```
4. `parameters()` 新增 JSON schema 属性
5. `GREP_DESCRIPTION` 更新说明

### 2b. 添加 `-F` / `fixed_strings`

`RegexMatcherBuilder::fixed_strings(true)` 原生支持。

**变更**:

1. `GrepInput` 新增：`fixed_strings: bool`（默认 `false`）
2. `ParsedArgs` 新增：`fixed_strings: bool`
3. `execute_search()` 构建 matcher 时：
   ```rust
   builder.fixed_strings(parsed.fixed_strings);
   ```
4. `parameters()` 新增 JSON schema 属性

### 2c. 分离 `-A`/`-B` 上下文控制

当前仅支持对称 `-C`。`SearcherBuilder` 已有独立的 `before_context()` / `after_context()`。

**变更**:

1. `GrepInput` 新增：`before_context: Option<usize>`, `after_context: Option<usize>`
2. `ParsedArgs` 修改：`context_lines` → `before_context: usize`, `after_context: usize`
3. `to_parsed_args()` 逻辑：
   ```rust
   // -C 作为对称上下文的简写
   let (before, after) = if self.before_context.is_some() || self.after_context.is_some() {
       (self.before_context.unwrap_or(0), self.after_context.unwrap_or(0))
   } else {
       let c = self.context.unwrap_or(0);
       (c, c)
   };
   ```
4. `execute_search()` 构建 searcher 时：
   ```rust
   if parsed.before_context > 0 {
       searcher_builder.before_context(parsed.before_context);
   }
   if parsed.after_context > 0 {
       searcher_builder.after_context(parsed.after_context);
   }
   ```
5. `SearchSink.context_lines` → `after_context: usize`（context 回调只输出 after 行）
6. `parameters()` 新增 JSON schema 属性

### 2d. `output_mode` 默认值

**变更**:

1. `GrepInput.output_mode` 改为 `Option<String>`：`output_mode: Option<String>`
2. `to_parsed_args()` 中：
   ```rust
   let mode_str = self.output_mode.as_deref().unwrap_or("content");
   ```
3. `invoke()` 中：
   ```rust
   let output_mode = input.get("output_mode").and_then(|v| v.as_str()).map(|s| s.to_string());
   // 移除 output_mode 缺失时的错误返回
   ```
4. `parameters()` 中 `required` 数组改为 `["pattern"]`
5. `GREP_DESCRIPTION` 中 "Always provide pattern and output_mode parameters" → "Always provide pattern parameter"

**验证**: `cargo build -p peri-middlewares` 编译通过。

---

## Task 3: P2 — files_without_matches + max_depth

**文件**: `peri-middlewares/src/tools/filesystem/grep.rs`

### 3a. 添加 `files_without_matches` 输出模式（`-L`）

**变更**:

1. `OutputMode` 新增变体：`FilesWithoutMatch`
2. `to_parsed_args()` 新增匹配分支：`"files_without_matches" => OutputMode::FilesWithoutMatch`
3. `SearchSink.matched()` 中 `FilesWithoutMatch` 分支：设置 `has_match.set(true)` 并继续搜索（不 early return，需确认文件无匹配）
4. 并行闭包中搜索完成后：
   ```rust
   if parsed.output_mode == OutputMode::FilesWithoutMatch && !sink.has_match.get() {
       let mut r = results.lock().unwrap();
       r.push(display_path.clone());
   }
   ```
5. `parameters()` 中 `output_mode` enum 新增 `"files_without_matches"`

### 3b. 添加 `--max-depth`

`WalkBuilder::max_depth(Some(depth))` 原生支持。

**变更**:

1. `GrepInput` 新增：`max_depth: Option<usize>`
2. `execute_search()` 中 WalkBuilder 配置：
   ```rust
   if let Some(depth) = parsed.max_depth {
       builder.max_depth(Some(depth));
   }
   ```
   > `max_depth` 需要加到 `ParsedArgs` 或直接从 `GrepInput` 传入 `execute_search`。
3. `parameters()` 新增 JSON schema 属性

**验证**: `cargo build -p peri-middlewares` 编译通过。

---

## Task 4: 测试 + description 更新

**文件**: `peri-middlewares/src/tools/filesystem/grep.rs`

### 4a. 新增单元测试

| 测试名 | 场景 |
|--------|------|
| `test_grep_multiline` | `multiline=true` 时 `pattern="foo.*bar"` 跨行匹配 |
| `test_grep_line_number_off` | `line_number=false` 时输出不含行号 |
| `test_grep_whole_word` | `whole_word=true` 时 `pattern="test"` 不匹配 `testing` |
| `test_grep_invert_match` | `invert_match=true` 时输出不匹配的行 |
| `test_grep_fixed_strings` | `fixed_strings=true` 时搜索 `[ERROR]` 无需转义 |
| `test_grep_asymmetric_context` | `before_context=2, after_context=0` 仅显示前 2 行 |
| `test_grep_files_without_matches` | 输出无匹配的文件列表 |
| `test_grep_output_mode_default` | 不传 `output_mode` 时默认为 `"content"` |

### 4b. 更新 GREP_DESCRIPTION

更新工具描述，反映新增参数：
- 添加 `-v`、`-F`、`-w`、`-A`、`-B`、`whole_word`、`invert_match`、`fixed_strings` 的使用说明
- 更新 `output_mode` 说明：默认 `"content"`，新增 `"files_without_matches"`
- 添加 `multiline` 说明
- 添加 `max_depth` 说明

**验证**: `cargo test -p peri-middlewares --lib -- grep` 全部通过。

---

## Task 5: grep crate multi_line 兼容性验证

`SearcherBuilder::multi_line(true)` 改变搜索模式（按 chunk 而非按行），需验证：

1. `invert_match` + `multi_line` 组合是否正确
2. `context_lines` + `multi_line` 组合是否正确
3. 大文件性能是否可接受（multi_line 模式可能更慢）

**验证方式**: 手动测试 + 检查 grep-searcher 源码中 `multi_line` 的 context 回调行为。

---

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| `multi_line` 改变搜索语义，可能影响现有调用 | 默认 `false`，仅显式开启时生效 |
| `invert_match` + `multi_line` + context 组合可能有边界情况 | Task 5 专项验证 |
| `output_mode` 从必填改为可选，可能影响已有 prompt | LLM 不传 output_mode 时默认 "content"，行为不变 |
| `files_without_matches` 需搜索所有文件，无 early termination | 这是 `-L` 的语义要求，可接受；大目录受 15s 超时保护 |

## 无新增外部依赖

所有变更使用现有 `grep 0.4`、`ignore 0.4`、`regex 1` crate API。

---

## 关键文件清单

| 文件 | 变更类型 |
|------|----------|
| `peri-middlewares/src/tools/filesystem/grep.rs` | 修改 GrepInput/ParsedArgs/SearchSink/execute_search/parameters/description；新增测试 |
