> 归档于 2026-05-16，原路径 spec/issues/2026-05-14-cache-breakpoint-structural-inefficiency.md

# Prompt Cache 断点结构性效率缺陷：82% system 未缓存 + message 断点浪费

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-14
**Reopen 日期**：2026-05-14

## 问题描述

分析 `msg_202605141504387ad56b6fb18b450f` 的 `cache_read_input_tokens: 0` 缓存丢失时发现两个结构性效率缺陷：(1) system prompt 中 82.6% 的内容（29,505 chars，CLAUDE.md + middleware 注入）没有 cache_control，永远无法被缓存；(2) `apply_cache_to_messages` 的第二断点策略在 tool_result-only 消息上静默失效，实际有效 message 断点仅 2 个而非设计的 3 个。

0080 请求本身的缓存丢失经逐字节对比确认是服务端瞬时驱逐（payload 前缀完全一致），但结构性问题限制了整体缓存命中率天花板。

## 症状详情

### 缓存命中率时序

| 请求 | cache_read | input | 命中率 | 间隔 |
|------|-----------|-------|--------|------|
| 0078 | 30,016 | 137 | 99.5% | — |
| 0079 | 25,600 | 4,589 | 84.8% | +15s |
| **0080** | **0** | **30,341** | **0%** | **+16s** |
| 0081 | 30,144 | 749 | 97.6% | +9s |
| 0082 | 30,336 | 4,127 | 88.0% | +5s |

### System block 结构分析

```
block[0]  6,226 chars (17.4%)  cache_control ✓  ← 仅此部分可缓存
block[1] 29,505 chars (82.6%)  cache_control ✗  ← CLAUDE.md + middleware 永不缓存
```

block[1] 内容包含 Deferred Tools 说明、SubAgent 文档、Skills 列表、CLAUDE.md 全文等跨请求稳定内容，但因通过 `messages_to_anthropic()` 归入 BOUNDARY 之后的动态段，序列化时无 cache_control。

### Message 断点失效分析

`apply_cache_to_messages` 设计 3 个 message 断点（first / second-to-last / last），但 second-to-last user message 在多轮工具调用中几乎一定是 tool_result-only 消息。`rfind(type=="text")` 返回 None，断点静默跳过：

```
0079: 2nd-to-last = msg[36] [tool_result] → cc 未生效（无 text block）
0080: 2nd-to-last = msg[37] [text]        → cc 生效，但 last=39 也是 tool_result → last 未生效
```

### 前缀对比验证

对 0079 和 0080 的请求 payload 做了逐字节对比：

| 对比项 | 结果 |
|--------|------|
| system block[0] | IDENTICAL |
| system block[1] | IDENTICAL |
| tools (14 个) | IDENTICAL |
| msg[0..37] | IDENTICAL |
| cache_control 位置 | 相同 |

0080 仅新增 msg[38] (assistant) 和 msg[39] (tool_result)，在最后一个断点 msg[37] 之后。缓存丢失确认是 Anthropic 服务端瞬时驱逐。

## 复现条件

- **复现频率**：服务端缓存丢失偶发，结构性效率问题必现
- **触发条件**：
  1. 使用 Anthropic 兼容 API，`enable_cache = true`
  2. 多轮工具调用对话（second-to-last 为 tool_result-only）
  3. system prompt 含大量 middleware 注入内容（CLAUDE.md、Skills 等）

## 修复记录

### Fix 1: `apply_cache_to_messages` 回退搜索

当目标 user 消息（second-to-last / last）无 text block 时，沿 `user_indices` 向前搜索最近的含 text block 的 user message，对其添加 cache_control。保留去重逻辑避免重复标记。

**文件**：`peri-agent/src/llm/anthropic.rs` `apply_cache_to_messages()`

### Fix 2: 断点重组

移除 `tools[last]` cache_control（已被 msg[first] 的缓存前缀覆盖，属于冗余断点），新增 `system[last]` cache_control（序列化时对最后一个 system block 标记）。

**变更前 4 断点**（实际有效 2-3 个）：

1. `system[0]` cc — ~2K tokens
2. `tools[last]` cc — 与断点 3 重叠
3. `msg[first]` cc — ~30K tokens
4. `msg[last]` cc — ~30K+ tokens

**变更后 4 断点**（实际有效 3-4 个）：

1. `system[0]` cc — 小粒度回退
2. `system[last]` cc — **新增**，缓存整个 system（~17K tokens）
3. `msg[first]` cc — system + tools + first user
4. `msg[second-to-last]` cc — **Fix 1 使其生效**，缓存上一轮前缀

**文件**：`peri-agent/src/llm/anthropic.rs`

- 删除 tools cache_control（L472-478）
- 新增 system[last] cache_control（序列化逻辑 L511-525）

## 涉及文件

- `peri-agent/src/llm/anthropic.rs` — `apply_cache_to_messages()`、system 序列化、tools 序列化

## 现象 2（Reopen 2026-05-14）：cache_control 断点迁移导致消息区域缓存批量失效

Fix 1/2 修复后，system block 缓存已正常（两个 block 均有 ephemeral），但 message 区域仍存在结构性问题。

### 缓存命中率时序（新 session）

Session `019e2582-aca8-78a0-8b41-070124722a08`，41 轮请求，整体命中率 94.7%，但出现 4 次断崖：

| 轮次 | input | cache_read | 命中率 | 前一轮命中率 | 类型 |
|------|-------|-----------|--------|-------------|------|
| 4 | 30,849 | 19,072 | 61.8% | 98.8% | 失效 |
| **24** | **42,251** | **16,896** | **40.0%** | **99.5%** | **失效** |
| **27** | **68,607** | **42,944** | **62.6%** | **99.6%** | **稀释** |
| 31 | 78,938 | 69,312 | 87.8% | 99.3% | 稀释 |

### 断点迁移失效机制

以 Round 23→24（0051→0052）为例：

```
Round 23: ... msg[46] [user] cache_control=ephemeral  ← 最后一条，断点在此
Round 24: ... msg[46] [user] cache_control=NONE        ← 不再是最后一条
          ... msg[48] [user] cache_control=ephemeral  ← 新的最后一条，断点迁移
```

当 `cache_control: ephemeral` 从 msg[46] 移除并添加到 msg[48] 时，msg[46] 的 JSON 内容发生了变化（缺少 cache_control 字段）。在 Anthropic 缓存模型中，断点之前的所有内容参与缓存键计算，因此 msg[46] 的变化导致覆盖消息区域的所有缓存条目失效。

只有 system blocks 的缓存条目（不包含消息）存活，约 16,896 token ≈ system block 0 (6,226 chars) + system block 1 (30,022 chars)。

### 当前断点布局

```
BP#1  system[0] end       6,226 chars   (static system prompt)
BP#2  system[1] end      30,022 chars   (含 DYNAMIC_BOUNDARY)
       tools (14 items)                ⚠️ 无独立断点（被 BP#3 隐式覆盖）
BP#3  messages[0] end                  (first user message)
BP#4  messages[N] end                  (last user message) ← 迁移发生点
```

### 根因分析

`apply_cache_to_messages()` 每轮重新计算断点位置时，会将 `cache_control` 标注从旧的最后一条 user message 移除，添加到新的最后一条。这个「移除旧标注」操作改变了该消息的序列化内容，导致缓存键不匹配。

本质上，`cache_control` 注解本身成为了缓存不稳定的来源——它既是指示缓存位置的元数据，又改变了被缓存内容的哈希值。

### 缓存稀释 vs 失效区分

| 特征 | 缓存失效（Round 24） | 缓存稀释（Round 27） |
|------|---------------------|---------------------|
| input 增量 | +196 token | +25,625 token |
| cache_read 变化 | -24,960 token | +128 token |
| 前一轮缓存是否保留 | 否 | 是 |
| 下一轮恢复 | 是（98.6%） | 是（99.6%） |
| 根因 | 断点迁移 | 大量新内容（grep 结果） |

稀释属于正常行为（新内容需一轮才能被缓存），失效是结构性问题。

### 追加观察（Systematic Debugging 2026-05-14）

对 Round 23→24 做了完整的逐层对比分析后，发现上述「断点迁移」机制描述与实际数据不符。以下为修正性观察。

**ZhipuAI Provider 的 token 报告格式与 Anthropic 原生不同**：

| 字段 | Anthropic 原生 | ZhipuAI (`open.bigmodel.cn`, glm-5.1) |
|------|---------------|----------------------------------------|
| `input_tokens` | 总输入 token | **非缓存输入**（仅新 token） |
| `cache_read_input_tokens` | ≤ input_tokens | **可超过 input_tokens** |
| `cache_creation_input_tokens` | 有值 | **始终为 0** |

验证：R1 total = 18,742 + 0 = 18,742；R2 total = 211 + 18,688 = 18,899（+157，符合追加 2 条消息的增量）。

**实际 cache_control 布局（与上述描述不一致）**：

```
R1-R23: 仅 msg[0] 有 cache_control（因中间 user 消息均为 tool_result，has_text_block 返回 false，回退搜索因 target_indices 排除约束也失败）
R24+:   msg[0] + msg[48] 有 cache_control（msg[48] 是含 text 的 user 消息，直接命中）
```

**关键事实：没有任何 cache_control 被移除过**。msg[46] 在 R23 和 R24 中均无 cache_control。「断点迁移」假说被数据否定。

**Provider 端的全前缀缓存**：R2-R23 的 cache_read 从 18,688 稳步增长到 41,856，远超三个断点（system[0]+system[1]+msg[0]）覆盖的 ~16,896 token。说明 ZhipuAI 实现了类似 vLLM automatic prefix caching 的机制，缓存整个前缀而不仅是断点覆盖范围。

**R24 失效的修正解释**：客户端前缀完全一致（system blocks identical、tools identical、前 47 条 messages 含 cache_control 注解全部 identical），无法解释 ~25K token 的缓存失效。cache_read 精确回退到断点覆盖量（16,896），说明断点内的小粒度缓存条目存活，而断点外的大粒度前缀缓存被 Provider 端驱逐。41 轮中仅此一次，符合 LRU/容量驱逐的随机性。

**结论**：R24 的缓存失效更可能是 **Provider 端（ZhipuAI）的缓存驱逐事件**，而非客户端断点迁移。客户端断点策略改进（Fix 1/2）本身是正确的，但无法防御 Provider 端的缓存驱逐。

**对客户端的启示**：由于 `cache_creation_input_tokens` 始终为 0，ZhipuAI 可能不通过 Anthropic 的 cache_control 断点机制创建缓存。断点的作用可能是提示 Provider 保留这些位置的 KV cache 条目（较小的条目更不容易被驱逐）。在 messages 区域增加更多断点（如每 10-15 条消息一个）可能有助于在 Provider 驱逐大条目时保留更多中间缓存段，但需要实测验证。

## 相关 Issue

- `2026-05-13-system-prompt-dynamic-cache-invalidation.md` — BOUNDARY 标记拆分（已修复，但 middleware 内容仍未缓存）
- `2026-05-13-prompt-cache-hit-rate-risks.md` — H3 断点落在 tool_result（已修复跳过逻辑，本次增强为回退搜索）
- `2026-05-13-askuserquestion-cache-hit-rate-drop.md` — 缓存下降子集表现（已修复）
