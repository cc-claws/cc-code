# diff 渲染对齐 CC 改造 — 未提交改动审计

- 审计对象：工作区 `D:/code/peri` 的未提交改动（62 个已跟踪文件 + 5 个未跟踪文件，+1406 / −336 行）。
- 基线：本地 `main` = `09e006ed`（`chore: bump npm version to 0.6.164`），当时落后 `origin/main` 5 个提交。
- 审计时间：2026-10-20。
- 审计方式：源码审查（逐文件读 diff 与实现）+ `cargo check -p cc-widgets -p cc-tui --all-targets -j 2`。**未运行任何测试**。
- 审计结论：编译通过；发现 3 个实质问题（1 个可复现逻辑缺陷、1 个渲染正确性缺陷、1 个视觉一致性缺陷）+ 3 项流程/卫生问题。
- 后续状态（2026-10-20 补记）：P1-1、P1-2 已由工作区另一处改动修复并补了回归测试（`tool_display.rs`、`renderer.rs` 的 mtime 为 14:07–14:09，晚于本报告 14:05）；P2 已按 CC `color-diff` 源码口径修复（语法高亮与词级底色改为两个正交 pass）；语言选择逻辑已按 CC `detectLanguage` 对齐（文件名表 + 扩展名白名单 + shebang）；P2 的残留项（`render_new_file_code` 走跨行有状态高亮）已于同日完成对齐并补回归测试，见下文 P2 小节；三项流程问题中，分支已按建议建为 `feat/tool-diff-highlight-alignment`，`.tmp-themes/` 已清理，附带影响（聊天气泡代码块配色随之变化）仍未写入 PR 描述。

## 改动范围概览

| 模块 | 内容 |
|---|---|
| `cc-widgets/src/diff/renderer.rs` | diff 渲染接入 syntect 语法高亮；`RenderCacheKey` 增加 `syntax_theme_hash`；新增 `render_new_file_code`（新建文件渲染无底色代码） |
| `cc-widgets/src/markdown/highlight.rs` | `THEME_SET` 改为 `load_theme_set()`，内嵌 `Monokai Extended` / `Monokai Extended Light` 两份 tmTheme |
| `cc-tui/src/ui/theme/palettes.rs` | 四套色板的 `diff_added/removed/word` 换成 CC 的绿/红底值 |
| `cc-tui/src/ui/theme/mod.rs` | `syntax_theme_name` 从 `base16-ocean.*` 切到 `Monokai Extended*` |
| `cc-tui/src/app/tool_display.rs` | `build_diff_input` 集中化；Edit 改为按磁盘全文反推编辑前内容（含 mtime 缓存 + 正向校验）；`format_tool_name_with_input`：Edit → `Update`，`old_string` 为空 → `Create` |
| `cc-tui/src/ui/message_render/mod.rs` | Edit/Write 更新类 diff 出现时隐藏工具结果文本行；新建文件走 `render_new_file_code` |
| `cc-middlewares/src/tools/filesystem/mod.rs` | 新增 `display_rel_path`：剥离 `\\?\` verbatim 前缀并转相对 cwd |
| 20+ 文件各 1–2 行 | 注释/文案中 `Claude Code` → `CC` 的统一措辞（无行为变化） |

## 问题清单

### P1-1 `replace_all` 分支的反推旧全文没有有效校验（可复现逻辑缺陷）

位置：`cc-tui/src/app/tool_display.rs:141` `edit_full_file_diff`。

`replace_all = true` 时：

```rust
let old_full = new_full.replace(new_string, old_string);   // 反推
let verified = old_full.replace(old_string, new_string);   // 校验
```

反推与校验是**同一操作的逆运算**，当 `new_string` 在文件中存有未被本次编辑产生的独立出现时，两者仍然互相成立，校验无法发现反推错误。

可复现反例：

- 编辑 `old_string = "foo"`, `new_string = "bar"`, `replace_all = true`。
- 编辑后磁盘内容 `"bar bar"`（真实编辑前是 `"bar foo"`）。
- 反推 `"bar bar".replace("bar", "foo")` → `"foo foo"`；校验 `"foo foo".replace("foo", "bar")` → `"bar bar"`，**通过**。
- 结果：diff 显示第二个 `bar` 是新增，实际它原本就存在；行号与删除行也随之错位。

非 `replace_all` 分支用 `replace_first` + 校验，一旦命中错误位置校验会失败并回退片段 diff，因此该缺陷只影响 `replace_all = true`。

**修复状态：已修复（2026-10-20 14:09，由工作区另一处改动完成，非本报告撰写者）。**

实现改为 `replace_all = true` 时直接回退片段 diff，并在入口对空 `new_string` 短路（`tool_display.rs:148-158`），同时在注释中写明歧义成因。

> 说明：本报告最初建议的"统计替换次数是否对称"并不可行——`bar bar` 中 `matches("bar") == 2`、反推后 `matches("foo") == 2`，次数相等仍会漏判。只有在"编辑后 `new_string` 仅出现 1 次"时（此时必然 `k = 0`，不存在原生实例）才能安全反推为单次替换；当前实现选择了更保守的整段回退，代价是 `replace_all` 场景失去行号上下文，功能上回到改动前的片段 diff 行为，不算回归。

回归测试：`cc-tui/src/app/tool_display_test.rs:427` `test_build_diff_input_replace_all_ambiguous_uses_snippet`，用的正是本报告中的 `bar foo → bar bar` 反例。

### P1-2 渲染缓存 key 漏了 `file_path`，跨文件串用语法高亮结果

位置：`cc-widgets/src/diff/renderer.rs:48`（`RenderCacheKey`）、`:58`（`from_input`）、`:327`（`lang_from_path(&input.file_path)`）。

缓存 key 由 `old_content` / `new_content` 哈希、`flags`、`width`、`syntax_theme_hash` 组成，**不包含 `file_path`**；而本次新增的语法高亮语言完全由 `file_path` 的扩展名推导。两个不同文件内容相同（空内容片段、`replace_all` 前的相同片段、同一段代码被两个语言扩展名共用等）时，缓存命中会直接复用另一种语言的着色结果。

这是本次改动引入的新缺陷（此前渲染结果与 file_path 无关，key 不含它是正确的）。

**修复状态：已修复（2026-10-20 14:09，由工作区另一处改动完成）。**

`RenderCacheKey` 新增 `lang_hash` 字段（`renderer.rs:55`）：`markdown-highlight` 开启时 hash 由 `lang_from_path` 推导的语言 token，关闭时 hash 整个 `file_path`。两种情形都能区分内容相同但扩展名不同的文件。

### P2 同一 hunk 内语法高亮口径不一致

位置：`cc-widgets/src/diff/renderer.rs` 的 `Add` / `Remove` 分支。

原状：带 `word_diff` 的 `+` 行优先走 `render_word_diff_spans`（无语法高亮），只有纯新增行才走 `highlight_line_with_bg`，导致同一个修改 hunk 里 `+` 行一行带语法色、一行纯色。

**修复状态：已修复（2026-10-20，按 CC color-diff 口径）。**

CC 的实际设计（`D:/code/cc-best-src/packages/color-diff-napi/src/index.ts`，TS 实现）：

- `render()`（`:936-955`）：`marker === '-' ? [[defaultStyle, code]] : highlightLine(...)` —— **只有删除行跳过语法高亮**，新增行与上下文行一律高亮。
- `applyBackground()`（`:793-842`）：在已着色的 span 上叠加背景（整行 `lineBg`、词级变更区间 `wordBg`），`{ ...style, background }` **保留语法前景色**。语法高亮与词级 diff 是两个正交 pass，不是二选一。
- `highlightLine()`（`:546-563`）：`code = line + '\n'` 逐行独立高亮，highlight.js 每次调用无状态，注释自称 "syntect-parity" —— 与 peri 的逐行 `HighlightLines::new` 一致。
- `wordDiffStrings()`（`:629-661`）：变化比例超过 `CHANGE_THRESHOLD = 0.4` 放弃词级高亮；`dim` 模式不做词级。peri 的 `WORD_DIFF_CHANGE_THRESHOLD = 0.4` 与之对齐。
- `findAdjacentPairs()`（`:601-627`）：相邻 `-` 块与 `+` 块按顺序一对一配对。

peri 侧的修复：

- 新增 `word_diff_change_ranges()`：由 `WordDiff.segments` 顺序累加得出词级变更区间的字节偏移（`Removed` 段在 `+` 行不显示，不占偏移）。
- 新增 `apply_word_diff_bg()`：把落在变更区间内的 span 片段背景换成 `word_bg`，前景色保留语法高亮结果，对应 CC 的 `applyBackground`。
- `+` 分支改为"先 `highlight_line_with_bg` 再叠词级底色"；语言未识别或 feature 关闭时回退原 `render_word_diff_spans` 纯色渲染。
- `-` 分支保持不做语法高亮，与 CC 一致（原报告把它算作"部分行纯色"不够准确，删除行本就该是纯色）。

测试：`renderer_test.rs` 新增 `test_render_diff_word_diff_line_keeps_syntax_color`（变更片段同时具备语法前景色与词级底色）、`test_render_diff_word_diff_unknown_lang_falls_back`。

**修复状态：已修复（2026-10-20 后续处理）。**

核实方式：用临时探针渲染同一段 `/* ... */` 内容，`render_new_file_code` 三行均为注释色 `Rgb(117,113,94)`，而 diff 行第 2/3 行为正文色 + `*` 的粉色（`Rgb(248,248,242)` / `Rgb(249,38,114)`）——差异真实存在。CC 侧核对：`ColorFile.render`（`color-diff-napi/src/index.ts:966`）与 `ColorDiff.render`（`:942`）共用 `highlightLine`，后者内部 `cachedHljsAst` 按单行 code 缓存，两者都是逐行无状态，故 peri 属单边偏离。

实现：`renderer.rs:329` 新增 `highlight_line_spans()`（单行无状态、仅输出前景色），`highlight_line_with_bg()` 改为复用它再叠背景与宽度截断；`render_new_file_code` 改用逐行 `highlight_line_spans`，不再调用跨行有状态的 `highlight_code_block`（后者保留给聊天气泡的 Markdown 代码块，那里需要跨行状态）。

回归测试：`test_render_new_file_code_matches_diff_line_highlight`（`renderer_test.rs`），断言 preview 与 diff 逐行前景色序列完全相同，且块注释中间行不含注释色。

行为变化：新建文件预览里多行块注释的中间行不再显示注释色——与 CC `ColorFile` 的逐行高亮一致。

### 流程 1：改动全部压在 `main` 上且落后远程 5 个提交

62 个文件的改动没有任何分支承载，直接在 `main` 工作区累积，同时 `main` 落后 `origin/main` 5 个提交。按仓库规范（禁止在 `main`/`test` 上修改代码），当前状态既不能安全 `pull --rebase`，也无法直接开 PR。

建议：先 `git stash` 或 `git switch -c feat/tool-diff-highlight-alignment`（携带改动新建分支），再对 `origin/main` 对齐。

### 流程 2：`.tmp-themes/` 未跟踪且未忽略，与正式资源重复

`.tmp-themes/monokai-extended{,-light}.tmTheme` 与将要提交的 `cc-widgets/src/markdown/themes/monokai-extended{,-light}.tmTheme` 内容重复，前者是临时下载目录且未进 `.gitignore`。提交时容易误带。

建议：删除 `.tmp-themes/`，或加入 `.gitignore`。

### 流程 3：附带影响未声明

`cc-widgets/src/markdown/highlight.rs` 的 `THEME_SET` 是全局静态，注入 Monokai Extended 并改由 `theme::widgets_theme()` 指定后，**聊天气泡内 Markdown 代码块的语法配色也一并变化**，不只是 diff 视图。若本次目标仅为 diff 对齐 CC，这是非预期扩散，应在 PR 描述中说明或收敛影响面。

## 语言选择对齐 CC（2026-10-20 追加）

**触发**：发现 `docs/*.md` 这类文档在 Write/Edit 预览里被按代码着色。核查 CC 源码后确认 CC **同样**会对 md 上色（`language-registration.test.ts` 的 26 种语言白名单含 `markdown`，`detectLanguage` 把 `md` 映射为 markdown）（**2026-10-22 更正**：该判断不成立，CC 实际无可见着色，见下节），真正的差异在**语言选择逻辑**：CC 是「文件名表 + 扩展名白名单 + shebang 检测」，peri 原先只取扩展名并交给 syntect 的全量语法集。

CC 侧依据：

- `detectLanguage()`（`color-diff-napi/src/index.ts:423-452`）：先查 `FILENAME_LANGS`（按文件名与「首个点号前的 stem」），再 `hljs.getLanguage(ext)` 判断扩展名是否受支持，最后按首行 `#!` / `<?php` / `<?xml` 识别（先剥 UTF-8 BOM）。
- `FILENAME_LANGS`（`:415-421`）：`Dockerfile`→dockerfile、`Makefile`→makefile、`Rakefile`/`Gemfile`→ruby、`CMakeLists`→cmake。
- 语言白名单（`src/__tests__/language-registration.test.ts`）：bash、c、cmake、cpp、csharp、css、diff、dockerfile、go、graphql、java、javascript、json、kotlin、makefile、markdown、perl、php、python、ruby、rust、shell、sql、typescript、xml、yaml。
- `ColorDiff.render()` 与 `ColorFile.render()` 共用该函数。

peri 侧改动（`cc-widgets/src/diff/renderer.rs`）：

- 用 `detect_language(file_path, first_line)` 取代 `lang_from_path`，实现上述三段式判定；新增 `FILENAME_LANGS`、`EXTENSION_LANGS` 两张表。
- 扩展名 → syntect token 的映射修正了歧义项：`.h` 映射到 `c`（直接用 `h` 会被 syntect 解析为 Objective-C）。
- 三个调用点同步切换：渲染缓存 key 的语言指纹、diff 行高亮、`render_new_file_code`。缓存的 `first_line` 取自 `new_content`（为空时取 `old_content`），与渲染路径一致。
- `test_render_diff_unknown_lang_falls_back` 的样本从 `Makefile`（现已能被识别）改为 `notes.txt`。

**语法集覆盖差距（已解决，2026-10-20）**：CC 的 26 种语言里，TypeScript、Kotlin、GraphQL、Dockerfile、CMake 在 syntect 默认语法集中没有定义，曾回退纯文本。已引入 `two-face 0.5`（bat 0.26.1 的语法集），`SYNTAX_SET` 改为 `two_face::syntax::extra_newlines()`，实测 5 种语言 token 全部命中。代价是二进制增加约 0.6 MiB 语法资产与相应编译时间。

> 完整记录（探测方法、代价约束、内存回归要求）见 [code-highlight.md](../spec/global/domains/code-highlight.md#扩展语法集two-face)。

验证：`cargo test -p cc-widgets --lib --features markdown-highlight -- diff::` 31 passed；`cargo test -p cc-tui --lib -- message_render tool_display` 132 passed；`cargo clippy -p cc-widgets --all-targets --features markdown-highlight -- -D warnings` 通过。

### 2026-10-22 更正：CC 的 md 实际无可见着色

上节「CC 同样会对 md 上色」的判断不成立。用隔离目录复制 `packages/color-diff-napi/src/index.ts`、安装 `highlight.js` + `diff` 后实测：

- `detectLanguage('notes.md', null)` → `'md'`，语言识别确实命中；
- `hljs.highlight('# 标题\n\n普通段落文字\n\n**粗体**', { language: 'md' })` → `<span class="hljs-section">` / `<span class="hljs-strong">`；
- `MONOKAI_SCOPES` / `GITHUB_SCOPES` / `ANSI_SCOPES` 三张表均无 `section`、`strong` 键，`scopeColor` 回退 `theme.foreground`；
- `new ColorFile(content, 'notes.md').render(...)` 与 `new ColorFile(content, 'notes.txt').render(...)` 输出**逐字节相同**。

结论：CC 只做了「语言识别 + 调用 highlight.js」，主题里没有任何 Markdown scope 映射，所以 `.md` 与纯文本观感一致。

peri 侧对齐（2026-10-22）：从 `EXTENSION_LANGS` 移除 `md` / `markdown` / `mdown` / `mkdn`，`.md` 按未识别语言回退纯文本，同时作用于 Write 新建预览与 Edit diff。回归测试：`test_detect_language_by_extension`（`.md` → `None`）、`test_render_new_file_code_md_falls_back_to_plain_text`。

## 待确认（非缺陷）

- `cc-tui/src/ui/theme/palettes.rs`：`DEFAULT` 与 `DARK` 现已使用完全相同的 diff 色值；`diff_added_dimmed`（`4,71,0` / `92,2,0`）比主色 `diff_added`（`2,40,0` / `61,1,0`）更亮，与 "dimmed" 的语义方向相反。**2026-10-20 补充核实：这两个字段没有任何使用点**——`cc-tui/src/ui/theme/mod.rs:135` 的 `widgets_theme()` 只把 `p.diff_added` / `p.diff_added_word` 映射到 `CustomTheme` 的 `diff_add_bg` / `diff_add_word_bg`，`diff_added_dimmed` / `diff_removed_dimmed` 既不参与 diff 渲染也不参与其它渲染，取值亮暗不影响视觉，属未使用的调色板字段（保留或删除均可，与本次对齐无关）。
- `cc-tui/src/app/tool_display.rs:63` `EDIT_DIFF_CACHE`：流式渲染期间每帧都会对该工具的缓存放行一次 `fs::metadata` + 用完整 `old_string`/`new_string` 拼 key 字符串，命中后仍要付出这部分开销。规模不大时无感，但属于每帧路径上的同步 IO，值得留意。

## 已确认无问题的部分

- `display_rel_path`（`cc-middlewares/src/tools/filesystem/mod.rs:54`）的 `\\?\` / `\\?\UNC\` 剥离与相对化逻辑正确，并配有 2 个单元测试；`write.rs:97,113` 与 `edit.rs:210` 的使用点已同步改为展示用路径。
- `format_tool_name_with_input`（`tool_display.rs:46`）的 `Edit → Update` / 空 `old_string → Create` 与 CC `userFacingName(input)` 语义一致，`reconcile.rs` / `transform.rs` / `acp_bridge.rs` 三处调用点全部改成带 input 版本，无遗漏旧调用。
- `render_new_file_code`（`renderer.rs:546`）的空内容、行数截断、显示宽度截断（按 span 累加 `unicode-width`）处理正确，未使用 `&s[..n]` 这类 CJK 不安全切片。
- `message_render/mod.rs:1580` 的 `diff_will_render` 判定用 `!d.is_new_file`，保留新建文件的 `Wrote N lines` 结果行，与 CC 行为一致，无重复信息。
- 20+ 文件的 `Claude Code` → `CC` 措辞改动均为注释/文案，无逻辑变化。

## 验证记录

| 检查 | 结果 |
|---|---|
| `cargo check -p cc-widgets -p cc-tui --all-targets -j 2` | 通过（exit 0，审计时） |
| `cargo test -j 1 -p cc-widgets --lib -- diff::` | 20 passed / 0 failed（审计时） |
| `cargo test -j 1 -p cc-widgets --lib --features markdown-highlight -- diff::` | 33 passed / 0 failed（two-face 接入后） |
| `cargo test -j 1 -p cc-widgets --lib --features markdown-highlight` | 238 passed / 0 failed（全量） |
| `cargo test -j 1 -p cc-tui --lib -- tool_display` | 29 passed / 0 failed（含 P1-1 回归用例） |
| `cargo test -j 1 -p cc-tui --lib -- message_render tool_display` | 132 passed / 0 failed（P2 修复后） |
| `cargo clippy -p cc-widgets --all-targets --features markdown-highlight -- -D warnings` | 通过 |
| `cargo clippy -p cc-widgets --all-targets -- -D warnings`（默认 feature） | 未通过：`byte_cache.rs` 的 `clear/len/capacity/budget` dead_code（既有代码，与本次改动无关；CI 走 `--workspace`） |
| `cargo test -j 2 -p cc-widgets --lib --features markdown-highlight -- diff::`（P2 残留项修复后） | 34 passed / 0 failed |
| `cargo test -j 2 -p cc-widgets --lib -- diff::`（默认 feature，修复后） | 20 passed / 0 failed |
| `cargo test -j 2 -p cc-widgets --lib --features markdown-highlight`（全量，修复后） | 239 passed / 0 failed |
| `cargo test -p cc-middlewares --lib filesystem` | 未执行 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 未执行 |
| 实际 TUI 视觉验收（diff 高亮 / 主题切换 / Ctrl+O） | 未执行 |

新增测试规模（未运行，仅统计行数）：`renderer_test.rs` +327、`tool_display_test.rs` +153、`message_render_test.rs` +88、`highlight_test.rs` +121、`mod_test.rs`（filesystem）+14。

## CC 对齐差距复核（2026-10-20）

逐项对照 CC 源码（`D:/code/cc-best-src`）的 diff 渲染链路：`color-diff-napi/src/index.ts`（渲染核心）、`src/components/StructuredDiff.tsx` / `StructuredDiffList.tsx` / `FileEditToolUpdatedMessage.tsx` / `HighlightedCode.tsx`、`packages/builtin-tools/src/tools/FileWriteTool/UI.tsx`、`src/utils/diff.ts`。

### 已对齐（本次核实）

| 项目 | 依据 |
|---|---|
| diff 四色（addLine/addWord/deleteLine/deleteWord） | 四个主题与 CC `buildTheme` 完全一致（`palettes.rs:142-147` 等 vs `index.ts:303-360`） |
| 词级 diff 变化比例阈值 0.4 | `WORD_DIFF_CHANGE_THRESHOLD` vs `CHANGE_THRESHOLD` |
| 删除行不做语法高亮 | `index.ts:940-943` 仅 `+` 与 context 行高亮 |
| 上下文行数 3 | CC `CONTEXT_LINES = 3`；peri 用 similar `unified_diff()` 默认值 3 |
| 摘要行文案与样式 | `Added N line(s), removed M line(s)`；无新增时首词大写 `Removed`；普通色 + 数字加粗（`FileEditToolUpdatedMessage.tsx:29-46`） |
| 多 hunk 分隔 `...` | `StructuredDiffList.tsx:31-35`；peri 用 `theme.dim()` |
| 单行号 gutter 格式与宽度 | marker + space + 右对齐行号 + space，宽度 `maxDigits + 3`（`index.ts:751-781`） |
| Edit → `Update`、空 `old_string` → `Create` | `FileEditTool/UI.tsx:25-50` |
| 更新类 diff 隐藏工具结果文本行、create 保留 `Wrote N lines to <path>` | `message_render/mod.rs:1580`；`FileWriteTool/UI.tsx:58-61` |
| 新建文件预览 10 行上限 + `… +N line(s) (ctrl+o to expand)` | `MAX_LINES_TO_RENDER = 10`；peri 非详细模式写死 10 |
| diff 区不显示文件标题行 | CC 标题在工具头；peri `message_render/mod.rs:1752` 显式 `rendered.remove(0)` |
| 语言检测三段落（文件名表 + 扩展名白名单 + shebang） | `detectLanguage`（`index.ts:423-452`） |
| 语法高亮逐行无状态 | 两侧 preview 与 diff 共用单行高亮（本报告 P2 残留项） |

### 未对齐（按影响排序）

| # | 项目 | CC 行为 | peri 现状 |
|---|---|---|---|
| 1 | 超宽行 | `wrapText` **折行**：按显示宽度切成多行，续行行号位补空格，改动行每行 padding 到 width（`index.ts:688-749`） | `truncate_to_width` **单行截断**，超出部分丢失 |
| 2 | 新建文件预览行号 | `ColorFile` 输出 ` 行号 ` gutter，宽度 `lineCount 位数 + 2`，全屏模式下与内容分列（`index.ts:966-993`、`HighlightedCode.tsx:80-90`） | `render_new_file_code` 无行号、无 gutter |
| 3 | 词级 diff 配对规则 | `findAdjacentPairs` 取 `min(delCount, addCount)` 逐对配对，Add 少于 Remove 时前 n 对仍有词级高亮（`index.ts:601-627`） | `fill_word_diffs` 要求 Remove 块后**等量**连续 Add 行，Add 少时整块不配对，词级高亮整体丢失（`mod.rs:315-338`） |
| 4 | 行首 tab 显示 | diff 前 `convertLeadingTabsToSpaces`：行首每个 tab → **2 空格**（`utils/diff.ts:124-141`、`utils/file.ts:137-142`） | 保留原始 tab，宽度由终端决定 |
| 5 | Write 覆盖已有文件 | 文件已存在时用旧内容做真 diff（`FileWriteToolDiff.tsx:19-34`），结果类型为 update | `build_diff_input` 的 Write 分支恒定 `is_new_file: true`，覆盖已有文件也按全绿新建渲染、无摘要行 |
| 6 | 语法主题选择 | light → `GitHub`，ansi → `ansi`，dark → `Monokai Extended`（`index.ts:183-187`） | light 系列 → `Monokai Extended Light`，ansi → `Monokai Extended`（`cc-tui/src/ui/theme/mod.rs:160-166`） |
| 7 | 空内容占位 | `(No content)`（`FileWriteTool/UI.tsx:51`） | 输出空行列表，无占位 |
| 8 | 大 diff 限流 | `DIFF_TIMEOUT_MS = 5_000` 时间超时（`utils/diff.ts:10`） | `MAX_DIFF_SIZE_BYTES = 1 MB` 输入长度上限（`mod.rs:21`），两种口径不可换算 |
| 9 | 单 hunk 全上下文模式 | `getPatchFromContents(singleHunk)` 用 `context: 100_000` 合并为单 hunk（`utils/diff.ts:103`） | 无该模式，全文件 diff 仍只保留 3 行上下文 |
| 10 | 颜色降级 | 显式量化：`COLORTERM` 判定 truecolor，否则 `ansi256FromRgb` 映射 256 色，ansi 主题走索引色（`index.ts:96-128`） | 直接输出 RGB，交给终端/ratatui 自行降级 |
| 11 | dim 参数 | 存在（dim 时跳过词级 diff、行号前缀加 DIM、ansi 主题删除行 dim）；当前调用点均传 `false` | 无对应概念 |
| 12 | `Wrote N lines to <path>` 样式 | 数字与路径 BOLD（`FileWriteTool/UI.tsx:59-60`） | 工具结果纯文本，无加粗 |

> 第 1、2、5 条是用户可见的行为差异；第 3 条会造成部分词级高亮缺失；第 4、6、7、12 条为观感差异；第 8-11 条属机制口径不同，需先决定是否跟进。

### 2026-10-20 复核更正（#9 / #11）

逐项核对 CC 的实际调用点后，上表两条判定不成立：

- **#9 单 hunk 全上下文**：`singleHunk` 全仓仅 `src/hooks/useDiffInIDE.ts:177` 使用（IDE diff 视图，`editMode === 'single'`），终端渲染链路从不传该参数——不属 TUI 差距，无需跟进。
- **#11 `dim` 参数**：CC 所有生产调用点（`FileEditToolUpdatedMessage.tsx:69`、`FileEditToolDiff.tsx:44`、`FileWriteToolDiff.tsx:55`、`DiffDetailView.tsx:126`）均为 `dim={false}`，peri 无此参数行为等价，无需跟进。

#8（5s 超时 vs 1MB 上限）与 #10（显式 256 量化 vs 交终端降级）属保护机制与终端能力口径差异，不构成缺陷，暂不跟进。

## markdown 渲染对齐（2026-10-20 补记）

### 差异定位

用 CC 声明的 `marked@17` 复刻 `formatToken` 输出 27 个语法样本的逐行 golden，与 peri `parse_markdown` 逐行比对，确认 5 处真实差异：

| 场景 | CC | peri 修前 |
|---|---|---|
| 消息以标题开头 | 标题在首行 | 多一个前置空行（`flush_line` 空 spans 时插行） |
| 嵌套列表 | 每项独占一行 | 外层项与内层首项挤在同一行 |
| 软换行段落 | 保留换行（text token 含 `\n`） | 合并成空格 |
| 段落紧跟列表 / 代码块（源无空行） | 不留空行 | 强制插空行 |
| 引用块多段落 / 水平线之后 | 中间 1 空行 / hr 后无空行 | 无空行 / hr 后多 1 空行 |

### 修复

- `cc-widgets/src/markdown/render_state.rs`：`Start(Heading)` 仅在 spans 非空时 flush；`Start(Item)` 先结束外层行；`SoftBreak` 改为 `flush_line()`；移除段落/列表/代码块/引用块的无条件空行，改由 `begin_block()` 消费源文本标记；hr 后不留空行
- `cc-widgets/src/markdown/mod.rs`：解析改用 `into_offset_iter()`，`BlockGap` 按字节偏移判定块间空行（跳过 `>` 前缀，hr 之后抑制）
- `cc-widgets/src/markdown/highlight.rs` + `cc-tui/src/ui/theme/mod.rs`：语法主题按 CC `defaultSyntaxThemeName` 映射（light → GitHub、ansi → ansi、其余 → Monokai Extended）
- `cc-tui/src/app/panel_theme.rs` + `cc-tui/src/ui/render_thread.rs`：主题切换后全量重建 VM 并丢弃渲染缓存（修复"切主题只有边框变色"）

### 验证

`test_markdown_structure_matches_cc` 等 9 个断言逐语法对拍；cc-widgets 262 passed、cc-tui 1345 passed、两包 clippy 通过。剩余差异仅为符号（`•`/`▍`/长横线）、表格边框风格、图片 alt 显示，以及 CC 自身的 loose list 丢 bullet 与删除线吞内容。

## 限制

- 本次审计基于静态阅读，未做运行时验证；"可复现反例"是逻辑推演，未在 TUI 中实际构造验证。
- 未核对 CC 侧 `structuredPatch` / `userFacingName` 的当前实现版本，对齐结论以代码注释声称为依据。
- 未评估 Monokai Extended 在 256 色 / 无色终端下的降级表现。
