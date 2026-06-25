> 归档于 2026-05-16，原路径 spec/issues/2026-05-11-context-usage-miscalculation-openai-compatible.md

# OpenAI 兼容第三方 Provider 上下文用量计算不准确

**状态**：Fixed + Verify
**优先级**：高
**创建日期**：2026-05-11
**上次修复 commit**：`1497d5b` fix(context): sync context_window from model to TUI layer
**Reopen 日期**：2026-05-12

## 问题描述

使用 OpenAI 兼容第三方 Provider 时，缓存命中率计算不准确。

## 症状详情

### 现象 1（已修复）：context_window 硬编码

TUI 显示的上下文用量（203k/200K，101%）与 API 实际报告的 input token 数（~100k）存在约 2 倍差距。已通过 `1497d5b` 修复。

| 指标 | TUI 显示 | API 实际值 |
|------|---------|-----------|
| 上下文用量 | 203k（101%） | input ~100k |
| context_window | 200k（硬编码默认值） | 模型实际 256k |
| 缓存 token | 100755 | — |

### 现象 2（待修复）：缓存命中率偏低

| 指标 | API 原始值 |
|------|-----------|
| input_tokens (prompt_tokens) | 76041 |
| cache_read_input_tokens (cached_tokens) | 75821 |
| 实际缓存命中率（当次） | 75821/76041 ≈ 99.7% |
| TUI 显示的缓存命中率 | 77% |

**根因已确认**：状态栏 `status_bar.rs:104` 调用了 `tracker.cache_hit_rate()`（**会话累计**命中率），而非 `tracker.last_cache_hit_rate()`（当次命中率）。

累计公式 `total_cache_read / total_input_tokens` 会将所有历史轮次的 input/cache_read 求和。早期轮次（缓存尚未建立时 prompt_tokens 高而 cached_tokens 低或为 0）持续稀释累计比率，导致多轮对话后累计命中率远低于当次命中率。

**修复方案**：状态栏改用 `last_cache_hit_rate()`，显示当次 LLM 调用的缓存命中率，与用户对"当前请求缓存效率"的直觉一致。

- **环境**：OpenAI 兼容第三方 Provider
- **触发场景**：多轮对话后观察状态栏缓存百分比

### 附带发现：token.rs 测试断言错误

`estimated_context_tokens()` 在某次重构中去掉了 `+ output_tokens`（避免双重计算），但 8 个测试的注释和断言仍期望 `input + output`。代码逻辑正确，测试需要修复：

| 失败测试 | 期望值 | 实际值 |
|----------|--------|--------|
| `test_estimated_context_tokens_some` | 2000 | 1500 |
| `test_estimated_context_tokens_no_cache` | 1500 | 1000 |
| `test_estimated_context_tokens_openai_with_cached_tokens` | 160K | 150K |
| `test_context_usage_percent` | 50% | 37.5% |
| `test_context_budget_should_auto_compact` | true | false |
| `test_context_budget_should_warn` | true | false |
| `test_context_budget_emits_warning_event` | 1 event | 0 events |

## 相关代码

- `peri-agent/src/agent/token.rs:57-65` —— `cache_hit_rate()` 实现（会话累计）
- `peri-agent/src/agent/token.rs:70-78` —— `last_cache_hit_rate()` 实现（当次，应改用此方法）
- `peri-agent/src/llm/openai.rs:557-576` —— OpenAI adapter 的 usage 解析：`input_tokens = prompt_tokens`，`cache_read = cached_tokens`
- `peri-agent/src/agent/token.rs:21-36` —— `accumulate()` 方法：`total_input_tokens` 累加逻辑
- `peri-tui/src/ui/main_ui/status_bar.rs:104` —— **修复点**：`cache_hit_rate()` → `last_cache_hit_rate()`
