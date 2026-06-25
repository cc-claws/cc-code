> 归档于 2026-05-27，原路径 spec/issues/2026-05-23-micro-compact-repeated-triggering.md

# Micro Compact 重复触发，每轮工具调用后都显示"自动清理"通知

**状态**：verify
**优先级**：中
**创建日期**：2026-05-23

## 问题描述

当对话上下文超过一定阈值后，micro compact（自动清理工具调用结果）被重复触发。每次工具调用执行后都会在聊天流中显示"自动清理：释放了 1 个工具调用结果"通知，与工具调用结果交替出现，导致界面噪音很大。用户期望 micro compact 不应如此频繁地触发或显示通知。

## 根因

`CompactMiddleware::before_model()` 在每轮 LLM 调用前都检查 `should_warn()`（默认 70% 阈值）。micro compact 将旧工具结果替换为 `[compacted: N chars]`，但压缩量（几百 token）远小于总上下文（140k+），仍在 >70% 区间。下一轮 `before_model` 再次判定超过阈值，再次触发 micro compact。

每轮恰好只有 1 个新工具结果"过期"进入可压缩范围（`stale_steps=5`），所以每次清 1 个。形成振荡：micro compact 压缩量 < 新增量，永远降不到 70% 以下。

**核心缺陷**：`CompactMiddleware` 缺少 once-per-prompt 守卫。micro compact 在同一轮 `execute_prompt` 中应只触发一次，清理完后若仍 >85% 应由 full compact 接管。

## 修复

给 `CompactMiddleware` 添加 `micro_compact_done: AtomicBool` 标志：

- micro compact 触发一次后设置标志，后续轮次不再重复触发
- full compact（85% 阈值）不受影响，仍可正常触发
- 每次 `execute_prompt` 创建新的 `CompactMiddleware` 实例，标志天然 per-prompt 作用域

**修改文件**：

- `peri-middlewares/src/compact_middleware.rs`：添加 `micro_compact_done: AtomicBool` 字段 + `before_model` 守卫
- `peri-middlewares/src/compact_middleware_test.rs`：添加 `test_micro_compact_once_per_prompt` 测试 + 更新 `make_middleware()`

## 症状详情

上下文超过一定阈值后，聊天流中的表现如下：

```
· 自动清理：释放了 1 个工具调用结果

● Shell(bun run src/index.ts --list 2>&1 | tail -5)

· 自动清理：释放了 1 个工具调用结果

● Shell(bun run src/index.ts --list 2>&1 | tail -5)

· 自动清理：释放了 1 个工具调用结果
```

- "自动清理：释放了 1 个工具调用结果"通知与工具调用结果在聊天流中交替出现
- 每次触发的 `count` 都是 1
- 触发后，后续每轮工具调用都会重复出现此通知

## 复现条件

- **复现频率**：必现（上下文超过阈值后）
- **触发步骤**：
  1. 进行多轮对话，使上下文长度超过 micro compact 阈值
  2. 之后每次工具调用都会触发 micro compact
  3. 聊天流中每轮工具调用都显示"自动清理"通知
- **环境**：正常使用场景

## 涉及文件

- `peri-middlewares/src/compact_middleware.rs` —— `CompactMiddleware` 实现（修复位置）
- `peri-middlewares/src/compact_middleware_test.rs` —— 测试
- `peri-agent/src/agent/compact/micro.rs` —— `micro_compact_enhanced` 压缩逻辑
- `peri-agent/src/agent/token.rs` —— `ContextBudget::should_warn()` 阈值判断
