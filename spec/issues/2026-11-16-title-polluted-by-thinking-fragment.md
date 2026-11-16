# 首轮会话标题被思考残片污染（`max_tokens` 截断的思考正文被当成标题落库）

**状态**：Fixed（#1065）
**优先级**：中
**创建日期**：2026-11-16

## 问题描述

新开会话发出首轮消息后，标题栏显示的**不是会话主题，而是模型的思考过程**，形如：

> 我们需要回答用户？但注意系统说你是会话主题生成工具，请根据用户首轮输入，概括提炼出最精准的,...

该污染标题还会写入会话库 `threads.title` 持久化，`-c` / `-r` 恢复会话后依然可见。

## 根因

标题提炼请求在 `cc-tui/src/terminal_title.rs` 里硬编码了 `max_tokens: 30` 并调用 `provider.into_model()`——**继承了会话的 thinking 配置**。

推理模型的**思考与正文共享同一份输出预算**：30 个 token 会被思考全部吃光，API 以 `stop_reason = max_tokens` 返回截断结果。此时响应里的 `content` 是思考残片而非标题，被原样渲染成标题并落库。

实测复现（同一首轮输入，本机网关）：

| 请求配置 | `stop_reason` | `content` 取值 |
| --- | --- | --- |
| adaptive + `effort=max`，`max_tokens=30` | `max_tokens` | `我们需要回答用户。用户任务输入：…`（残片） |
| 同配置，`max_tokens=1024` | `end_turn` | `会话主题生成` |

## 修复（#1065）

- 移除标题请求上硬编码的 `max_tokens: 30`，改为继承 provider 上限（`max_tokens` 是上限而非预留，放宽不额外计费）；
- 拆出 `build_title_request` / `extract_title_from_response` 两个纯函数；
- 新增截断守卫：`stop_reason == MaxTokens` 一律丢弃，回退轨 1 本地确定性标题。

## 涉及文件

- `cc-tui/src/terminal_title.rs` —— 标题提炼请求构造与响应提取
- `cc-tui/src/terminal_title_test.rs`（新增）—— 8 条回归：锁死「标题请求不得自带 `max_tokens`」与「截断响应必须丢弃」

## 教训

1. **给推理模型的请求硬编码 `max_tokens` 时，思考预算会与正文争抢同一上限**——小上限 + thinking 配置 = 正文被挤成残片。同一教训在规则提炼路径已有记录（`cc-middlewares/src/hitl/jev/rules.rs`：「刻意不设 `max_tokens`，它继承 provider 配置」），本次对齐该结论。
2. **截断的结果（`stop_reason == MaxTokens`）永远不该被当成正常结果消费**——消费方应显式守卫并走降级路径，而不是照单全收。

## 验证

- `cargo test -p cc-tui --lib -- terminal_title` 8/8 通过；
- `cargo clippy -p cc-tui --all-targets -- -D warnings` 干净；
- 真实网关端到端对照：修复前 5/5 用例均被截断为思考残片；修复后 5/5 输出正常标题，延迟均在 8s 硬超时内。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-11-16 | — | Open | agent | 创建 |
| 2026-11-16 | Open | Fixed | agent | #1065 合并（v0.6.282），补齐症状文档 |
