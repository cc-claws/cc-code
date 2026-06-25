> 归档于 2026-05-16，原路径 spec/issues/2026-05-14-grep-tool-capability-gap.md

# Grep 工具能力缺口：声明不实现 + 标准能力缺失

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-14

## 问题描述

Grep 工具（`peri-middlewares/src/tools/filesystem/grep.rs`）基于 Rust `grep` + `ignore` crate 实现，但存在两类问题：多个已声明参数未实际实现（`multiline`、`-n`、`whole_word`），且缺少标准 grep 的高频能力（`-v`、`-F`、`-A`/`-B` 等）。这会导致 LLM 在调用时产生错误预期——例如依赖 `multiline` 写跨行正则却得不到正确结果，或因缺少 `invert_match` 而构造不必要的复杂正则。

## 症状详情

### 声明但未实现的参数

| 参数 | 位置 | 实际行为 | 影响 |
|------|------|----------|------|
| `multiline` | `GrepInput._multiline` | 从未传递给 regex engine，声明无效 | LLM 写跨行正则得到错误结果 |
| `-n` | `GrepInput._line_number` | 硬编码 `line_number(true)`，无法关闭 | 参数存在但无效果 |
| `whole_word` (`-w`) | `ParsedArgs.whole_word` | 始终 `false`，未暴露给 JSON 参数 | 内部有实现但未对接 |

### 缺失的高频标准能力

| 优先级 | 选项 | 功能 | 影响 |
|--------|------|------|------|
| P1 | `-v` / `invert_match` | 选择不匹配的行 | 无法排除式搜索 |
| P1 | `output_mode` 默认值 | 当前为必填参数 | 绝大多数场景用 `"content"`，应设默认值 |
| P2 | `-F` / `fixed_strings` | 字面量匹配 | 搜索含特殊字符的字符串需手动转义 |
| P2 | `-A`/`-B` | 非对称上下文 | 仅支持对称 `-C` |
| P3 | `-L` | 列出无匹配文件 | 只能通过 `count` + LLM 手动比较 |
| P3 | `--max-depth` | 限制递归深度 | 无变通方案 |

### 行为差异 vs 标准 grep

- 隐藏文件：默认搜索（`hidden(true)`），rg 默认跳过
- 符号链接：默认跟随，`grep -r` 不跟随
- 权限错误：静默跳过，系统 grep 会报错
- `offset` 语义：后处理切片（非搜索时跳过），与 `head_limit` 组合可能导致结果空洞

### 设计问题

- `output_mode` 为必填参数（`required: ["pattern", "output_mode"]`），标准 grep 无此概念
- `head_limit` + `offset` 后处理语义：先截断再跳过，`head_limit=250, offset=200` 最多只返回 50 行
- Rust regex 不支持反向引用、前瞻/后顾断言

## 修复优先级

| 优先级 | 项目 | 理由 |
|--------|------|------|
| **P0** | 实现 `multiline` 或从 JSON schema 移除 | 声明但不工作导致 LLM 写出错误正则 |
| **P0** | 实现 `-n` 开关或从 JSON schema 移除 | 同上 |
| **P1** | 暴露 `-w`（`whole_word`） | 已有内部实现，对接成本极低 |
| **P1** | 添加 `-v`（`invert_match`） | grep 最核心能力之一 |
| **P1** | `output_mode` 默认 `"content"` | 减少 LLM 调用负担 |
| **P2** | 添加 `-F`（`fixed_strings`） | 搜索含特殊字符的字符串很常见 |
| **P2** | 添加 `-A`/`-B`（分离上下文） | 可用 `-C` 变通 |
| **P3** | 添加 `-L`（`files_without_matches`） | 中频需求 |
| **P3** | 添加 `--max-depth` | 低频但无变通方案 |
| **P3** | `offset` 改为搜索时跳过 | 当前语义可用但不够直观 |

## 涉及文件

- `peri-middlewares/src/tools/filesystem/grep.rs`（Grep 工具实现）

## 参考

- 完整分析文档：`docs/grep-tool-capability-gap.md`
