> 归档于 2026-05-16，原路径 spec/issues/2026-05-14-test-separation-convention-debt.md

# 测试分离规范债务：89.8% 源文件仍内联测试 → 已完成

**状态**：Resolved
**优先级**：中
**创建日期**：2026-05-14
**解决日期**：2026-05-15

## 问题描述

项目规范要求测试与源码分离为同目录 `_test.rs` 文件（≥30 行必须分离），但实际执行率仅 10.2%。137 个含测试的源文件中，113 个仍使用 `#[cfg(test)]` 内联测试，其中 73 个内联测试超过 100 行。

## 最终状态

### 按严重程度分布

| 严重级别 | 阈值 | 初始 | 最终 |
|---------|------|------|------|
| CRITICAL | >100 行，必须分离 | 73 | **0** |
| MAJOR | 30-100 行，应分离 | 40 | **0** |
| INFO | <30 行，可保持 | 6 | **6** |
| 已分离 | `*_test.rs` 文件 | 24 | **152** |

### 按 Crate 分布（最终）

| Crate | 已分离 | INFO 残留 | 违规率 |
|-------|--------|-----------|--------|
| peri-agent | 31+ | 0 | 0% |
| peri-middlewares | 45+ | 1 | ~2% |
| peri-tui | 43+ | 3 | ~7% |
| peri-widgets | 20+ | 1 | ~5% |
| langfuse-client | 5 | 0 | 0% |
| peri-lsp | 8 | 0 | 0% |

## 分离模式

两轮分离使用了两种不同的外部引用模式：

### 模式 1：`#[path = "..._test.rs"] mod tests;`（第一轮，76 个文件）

```rust
// source.rs
#[cfg(test)]
#[path = "source_test.rs"]
mod tests;

// source_test.rs
use super::*;
// ... test code
```

### 模式 2：`include!("..._test.rs")` （第二轮，76 个文件）

```rust
// source.rs
#[cfg(test)]
mod tests {
    use super::*;
    include!("source_test.rs");
}

// source_test.rs
// ... test code（保留原始缩进）
```

**为什么引入模式 2**：`#[path]` 将测试模块定义到外部文件后，`use super::*;` 只能看到父模块的**公有**项（Rust 2018+ 同文件模块特殊规则失效）。导致 `async_trait`、`Arc` 等私有 import 不可见。`include!` 是文本级包含，完全保留原始作用域语义，无需修改任何 import。

## INFO 残留文件（<30 行，可选分离）

| 文件 | 行数 |
|------|------|
| `peri-middlewares/src/mcp/middleware.rs` | 20 |
| `peri-tui/src/ui/main_ui/panels/cron.rs` | 28 |
| `peri-tui/src/main.rs` | 23 |
| `peri-tui/src/command/plugin_command.rs` | 26 |
| `peri-tui/src/command/doctor.rs` | 23 |
| `peri-widgets/src/spinner/verb.rs` | 24 |

## 涉及文件

- 152 个 `*_test.rs` 外部测试文件
- 152 个源文件已修改为外部引用模式（76 `#[path]` + 76 `include!`）
- 6 个 INFO 级别文件保留内联测试（<30 行，符合规范）
