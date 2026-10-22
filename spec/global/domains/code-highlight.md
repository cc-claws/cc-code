# 代码高亮 领域

## 领域综述

代码高亮领域负责 TUI 中 Markdown 多行代码块的语法高亮渲染，使用 syntect 实现基于语法分析的高精度着色。

核心职责：

- 通过 markdown-highlight feature flag 控制启用
- syntect default-onig，使用静态链接的 Oniguruma 正则引擎
- 延迟初始化 SyntaxSet/ThemeSet，仅加载一次
- 未识别语言回退到统一颜色

## 核心流程

### 代码高亮流程

```
Markdown 解析遇到代码块（```lang）
  → feature markdown-highlight 启用?
      是 → highlight_code_block(code, lang)
           → SyntaxSet::find_syntax_by_token(lang)
           → 找到 → syntect HighlightLines → ratatui Span 着色
           → 未找到 → 回退 theme.code() 统一颜色
      否 → 直接使用 theme.code() 统一颜色
  → 单行代码块不做语法高亮
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 高亮库 | syntect 5（default-onig，Oniguruma 静态链接） |
| 构建要求 | onig_sys 编译捆绑 C 源码，需 C 工具链；不启用 bindgen，无额外 libclang 要求 |
| Feature flag | markdown-highlight 控制，不影响默认构建 |
| 初始化 | once_cell::sync::Lazy 延迟加载 SyntaxSet/ThemeSet |
| 语法集 | two-face 0.5（bat 0.26.1 的扩展语法集），补齐 syntect 默认集缺失的 TypeScript / Kotlin / GraphQL / Dockerfile / CMake 等 |
| 主题 | Monokai Extended / Monokai Extended Light（内嵌 tmTheme，随 UI 明暗族切换） |
| 回退 | 未识别语言或无标签 → theme.code() 统一颜色 |
| 单行代码 | 不做语法高亮，保持 theme.code() |

## 语言识别规则（对齐 CC detectLanguage）

`cc-widgets/src/diff/renderer.rs` 的 `detect_language(file_path, first_line)` 对齐 Claude Code
`packages/color-diff-napi/src/index.ts:423` 的三段式判定，diff 渲染与新建文件预览共用：

1. **文件名表**：`Dockerfile` / `Makefile` / `Rakefile` / `Gemfile` / `CMakeLists`；按文件名与「首个点号前的 stem」匹配（`Makefile.bak`、`CMakeLists.txt` 均可命中）。
2. **扩展名白名单**：CC 注册的 26 种语言的扩展名别名。白名单外的扩展名一律回退纯文本（例：`.txt`、`.xyz`、`.toml`）。**markdown 例外**：`.md` / `.markdown` / `.mdown` / `.mkdn` 不启用高亮——CC 的 hljs markdown 只产出 `section` / `strong` scope，其主题 scope 表没有对应颜色，渲染结果与纯文本完全一致，因此 peri 直接把 markdown 从 `EXTENSION_LANGS` 移除，保持观感对齐。
3. **首行检测**：剥 UTF-8 BOM 后，按 `#!/bin/bash`（含 `/sh`）、`python`、`node`、`ruby`、`perl`、`<?php`、`<?xml` 识别。

映射到 syntect token 时修正歧义项：`.h` → `c`（直接用 `h` 会命中 Objective-C）。

Markdown 聊天内容里的代码块（```lang）仍按语言标签直接 `find_syntax_by_token`，不走上述文件名/首行规则。

## 扩展语法集（two-face）

历史背景：syntect 默认语法集（Sublime 默认包）缺少 CC 支持的 5 种语言——TypeScript、Kotlin、GraphQL、Dockerfile、CMake，这些文件曾回退纯文本。2026-10-20 起 `SYNTAX_SET` 改为 `two_face::syntax::extra_newlines()`（bat 0.26.1 的语法集），5 种语言全部补齐。

探测结果（two-face 接入后实测）：

```rust
for tok in ["ts","tsx","kt","kts","graphql","gql","dockerfile","Dockerfile","cmake","CMake"] {
    println!("{tok} => {:?}", SYNTAX_SET.find_syntax_by_token(tok).map(|s| &s.name));
}
// ts => TypeScript, tsx => TypeScriptReact, kt/kts => Kotlin,
// graphql/gql => GraphQL, dockerfile/Dockerfile => Dockerfile, cmake/CMake => CMake
```

代价与约束：

- 依赖：`two-face = { version = "0.5", default-features = false, features = ["syntect-onig"] }`，regex 引擎必须与 syntect 一致（onig）。
- 语法资产比 syntect 默认集大约多 0.6 MiB，会进入最终二进制；首次编译时间相应增加。
- 内存影响需按 `issue_2026-10-10_php-highlight-memory` 的方法回归（`/gc` 统计、全进程 WorkingSet 采样）。
- `detect_language` 的白名单仍按 CC 的 26 种语言生效（markdown 例外，见「语言识别规则」）：two-face 多出来的语法（TOML、Swift、Dart 等）不会被自动启用，扩展名仍需显式加入 `EXTENSION_LANGS`。

回归测试：`cc-widgets/src/diff/renderer_test.rs` 的 `test_render_diff_typescript_has_syntax_color`、`test_render_diff_dockerfile_has_syntax_color`、`test_detect_language_by_extension`（`.md` 断言为 `None`）、`test_render_new_file_code_md_falls_back_to_plain_text`。

## 语法主题选择（对齐 CC defaultSyntaxThemeName）

`cc-tui/src/ui/theme/mod.rs` 的 `syntax_theme_for()` 按 CC `defaultSyntaxThemeName`
（`color-diff-napi/src/index.ts:183-187`）映射：

| UI 主题 | syntect 主题名 |
|---|---|
| light / light-daltonized | `GitHub` |
| dark / dark-daltonized / default | `Monokai Extended` |
| light-ansi / dark-ansi | `ansi` |

`GitHub` 与 `ansi` 取自 two-face 主题集（`two_face::theme::extra()`），与语法集一并加载，不额外引入资源文件；
`Monokai Extended` 仍是内嵌 tmTheme（bat 同款），保证 dark 配色与既有视觉一致。

主题切换后 markdown 正文必须重建，否则只有边框/状态栏变色：`ContentBlockView::Text.rendered`
是构建 ViewModel 时的渲染快照，而 `content_hash` 明确忽略 `rendered`。修复链：

- `cc-tui/src/app/panel_theme.rs` 切换主题后调用 `rebuild_vms_from_all()` 全量重建 VM（流式进行中跳过）
- `cc-tui/src/ui/render_thread.rs` 用 `theme::fingerprint()` 检测主题变化，变化时清空 `message_hashes` 强制整屏重渲染

## Feature 附录

### issue_2026-10-10_php-highlight-memory

**现象：** 会话消息和 Markdown LRU 估算仅数 MiB，但 PHP 代码块高亮后仍保留大量分配。

**根因：** 全局 `SyntaxSet` 持有延迟编译的语法正则；原 `default-fancy` 引擎编译部分 PHP Unicode 正则时产生大量常驻分配。清空 Markdown LRU 或调用 `mi_collect(true)` 都不会释放这些存活的正则。

**修复：** 改为 `default-onig`，保留现有语法集、主题、高亮接口和未知语言回退行为。`/gc` 补充高亮引擎估算遗漏，Windows resident 明确为全进程 WorkingSet，避免将其解释为 mimalloc 独占内存；allocated 不保证覆盖原生库分配。

**回放证据（Windows/MSVC，debug 构建）：** 对同一会话文本分别运行旧、新引擎，解析结束后释放输入、渲染输出并清空 Markdown LRU，再调用当前线程 `mi_collect(true)`。通过 `GetProcessMemoryInfo` 采样全进程 RSS，包含 Oniguruma 的 C 分配。

| 独立回放 | default-fancy RSS | default-onig RSS | 解析与回收耗时 |
|------|------|------|------|
| 单段 27,613 字节 PHP Markdown | 89.6 MiB | 15.6 MiB | 9.07 s → 0.16 s |
| 会话全部 419 段文本（含推理） | 118.4 MiB | 17.5 MiB | 11.60 s → 0.56 s |

以上是独立回放程序的结果，不是完整 TUI 内存，也不能证明会话内全部未识别分配均来自高亮引擎。旧引擎的历史决策保留在下方归档条目中。

### feature_20260429_F001_syntect-codeblock-highlight

**摘要:** 使用 syntect 为 Markdown 多行代码块添加语法高亮
**关键决策:**

- 通过 feature flag markdown-highlight 控制启用，不影响默认构建
- 使用 syntect default-fancy（纯 Rust）避免 C 库 oniguruma 编译问题
- SyntaxSet/ThemeSet 通过 once_cell::sync::Lazy 延迟初始化
- 未识别语言或无语言标签时回退到统一颜色行为
- 单行代码块不做语法高亮，保持 theme.code() 统一颜色
- 默认使用 base16-ocean.dark 主题，与 TUI 暗色背景协调
**归档:** [链接](../../archive/feature_20260429_F001_syntect-codeblock-highlight/)
**归档日期:** 2026-04-30

---

## 相关 Feature

- → [tui.md](./tui.md) — Markdown 渲染集成点
- → [tui-widgets.md](./tui-widgets.md) — cc-widgets MarkdownRenderer 组件

最后更新：2026-10-22
