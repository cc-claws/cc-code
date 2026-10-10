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
| 主题 | base16-ocean.dark（与 TUI 暗色背景协调） |
| 回退 | 未识别语言或无标签 → theme.code() 统一颜色 |
| 单行代码 | 不做语法高亮，保持 theme.code() |

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

最后更新：2026-10-10
