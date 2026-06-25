> 归档于 2026-05-16，原路径 spec/issues/2026-05-11-separate-test-code-from-impl.md

# 分离测试代码与业务代码，缩减文件体积

**状态**：Fixed + Verify
**优先级**：中
**创建日期**：2026-05-11
**修复 commit**：`2fd5826` refactor: extract inline tests to separate files with #[path] attribute

> issue 列出的全部 13 个文件均已完成 `#[path = "..._test.rs"]` 提取。

## 问题描述

多个 Rust 源文件中 `#[cfg(test)] mod tests` 内联了大量测试代码，导致文件总行数虚高，实际业务代码占比低。影响代码可读性和导航效率。

## 现状数据

| 文件 | 总行数 | 业务代码 | 测试代码 | 测试占比 |
|------|--------|---------|---------|---------|
| `peri-tui/src/ui/headless.rs` | 3416 | 57 | 3359 | 98% |
| `peri-agent/src/agent/executor/mod.rs` | 1283 | 299 | 984 | 77% |
| `peri-middlewares/src/plugin/loader.rs` | 1418 | 572 | 846 | 60% |
| `peri-middlewares/src/plugin/installer.rs` | 1622 | 753 | 869 | 54% |
| `peri-middlewares/src/subagent/tool.rs` | 1847 | 868 | 979 | 53% |
| `peri-tui/src/app/message_pipeline.rs` | 1506 | 757 | 749 | 50% |
| `peri-agent/src/llm/openai.rs` | 1256 | 663 | 593 | 47% |
| `peri-middlewares/src/hooks/middleware.rs` | 1250 | 682 | 568 | 45% |
| `peri-middlewares/src/mcp/config.rs` | 1164 | 638 | 526 | 45% |
| `acpx-g/src/schema.rs` | 1167 | 526 | 641 | 55% |
| `langfuse-client/src/types.rs` | 1464 | 1005 | 459 | 31% |
| `peri-tui/src/app/setup_wizard.rs` | 1178 | 801 | 377 | 32% |
| `peri-middlewares/src/plugin/marketplace.rs` | 1164 | 725 | 439 | 38% |

## 修复方案

使用 Rust 的 `#[path]` 属性将测试模块外置到独立文件：

```rust
// module.rs 底部，替换原有的 #[cfg(test)] mod tests { ... }
#[cfg(test)]
#[path = "module_test.rs"]
mod tests;
```

将测试代码移至同目录下的 `module_test.rs`。

**优势**：

- 测试仍在 `mod tests` 内，可访问私有函数和结构体（与 `tests/` 目录不同）
- `#[cfg(test)]` 保证不编入生产构建
- 业务文件只增加一行声明，零噪音
- bin crate 兼容（不受 `tests/` 目录限制）

## 影响范围

需处理测试占比超过 30% 的文件（上表全部）。优先处理测试占比超过 50% 的 6 个文件。
