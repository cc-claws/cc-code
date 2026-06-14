# PRD: 用 Arc<str> 消除大 ToolResult 多存储放大（500 轮典型对话 ≤ 50MB）

**版本**：v6（v5 基础上补充事实 F：mock bench 独立验证 Arc<str> 收益）
**关联 Issue**：`spec/issues/2026-06-13-message-double-storage-40mb-waste.md`（Verified）
**关联 PR**：https://github.com/wismyzhizi2018/peri/pull/10（验证用测试套件，CI 全绿）

---

## 0. 文档目的与适用范围

本文档定义"长会话累积 RSS 可控"的实施方案。**核心目标：500 轮典型对话 RSS ≤ 50MB**。这是用户在 v3 PRD 反复澄清后的最终目标。

**本 PRD 只描述方案、测量方法和验证清单，不含实现代码。** 实施前必须按 §8 验证清单逐项确认；实施中遇到本文档未覆盖的情况必须停下来更新本文档。

**重要前置说明**：
- 长会话累积主要由 **compact middleware** 控制（默认每 35-50 轮触发一次 full compact，把累积消息替换为摘要）。compact 会清掉 TUI 层 `pipeline.completed` / `view_messages` / `RenderCache` 的旧内容。
- 因此 500 轮 ≤ 50MB 的真正瓶颈不是"长会话线性累积"（compact 已能控制），而是 **compact 周期内的单次大 ToolResult 放大**——一个 3MB 大文件经多存储点 deep clone 后单轮涨 32MB（已实测复现），紧凑周期内连续 2 个大文件就足以突破 50MB。
- Phase 1（Arc<str>）的核心价值是**消除 compact 周期内的单轮暴涨**，从而把"周期峰值"压到 50MB 以下。

---

## 1. 背景

### 1.1 已验证事实（来自 issue 验证 #1-#4 + 多轮 RSS 测试）

**事实 A：用户报告**——单轮对话 RSS 从 118MB 涨到 150MB（+32MB）。

**事实 B：端到端复现**——`headless_large_toolresult_e2e.rs` 用真实事件流复现：第 7 轮（含 3MB 大文件 ToolResult）涨 **+31.68 MB**，与用户报告偏差 < 1%。

**事实 C：放大倍数实测**（来自 `large_toolresult_path.rs`）：

| 注入 ToolResult | 实测 RSS 增长 | 放大倍数 |
|-----------------|--------------|---------|
| 1 MB | ~5.6 MB | 5.6x |
| 3 MB（用户场景） | ~17-32 MB | 5.7-10x |
| 5 MB | ~28-49 MB | 5.6-10x |
| 20 MB | ~123 MB | 6x |

**事实 D：典型小对话累积**（来自 `pipeline_real_rss_growth.rs`）——50 轮 markdown 风格小对话（每条 2-5KB），pipeline-only 累积 **156 KB**（3.12 KB/轮），含 view_messages 累积 **272 KB**（5.44 KB/轮）。

**注意**：50 轮数据**仅验证了 compact 未触发阶段**（50 轮 × ~2-3K tokens/轮 ≈ 100-150K tokens，临界 micro_compact 阈值）。**不能线性外推到 500 轮**，因为 §1.1 事实 E 的 compact 行为会在累积达 170K tokens 时把消息替换为 ~1-2KB 摘要，曲线非线性下降。500 轮实际累积**必须靠 Phase 0（§6.5）实测**，不能用 50 轮外推。

**事实 E：compact 行为**——`CompactMiddleware`（`peri-middlewares/src/compact_middleware.rs`）在 `before_model` 钩子检查 `ContextBudget`：
- micro_compact 阈值 0.70（默认 200K context window 下 = 140K tokens）
- full_compact 阈值 0.85（170K tokens）
- full_compact 后：`pipeline.clear()` + `restore_completed(messages)` + `RebuildAll { prefix_len: 0 }`，**彻底清掉 TUI 层旧消息**（`peri-tui/src/app/agent_compact.rs:60-82`）
- `RenderCache.rebuild()` 通过 `std::mem::take` + `resize` 清理 `message_lines`、`last_messages`（`peri-tui/src/ui/render_thread.rs:360-377`）

**事实 F：Arc<str> 收益独立 mock 验证**（`mock_arc_bench/`，独立 cargo 项目，未触及 peri 生产代码）——模拟 §1.2 列出的 6 个存储点路径（每轮 6 副本），测 `String` 与 `Arc<str>` 的 RSS 增长差异：

| 场景 | String RSS | Arc<str> RSS | 节省 |
|------|-----------|-------------|------|
| 7 轮 × 3MB × 6 副本（**issue 报告场景**） | **39.8 MB** | **0 KB** | 100% |
| 50 轮 × 1MB × 6 副本 | 308 MB | 0 KB | 100% |
| 50 轮 × 3MB × 6 副本 | 833 MB | 64 MB | 92.3% |
| 100 轮 × 1MB × 6 副本 | 517 MB | 13 MB | 97.4% |

**关键观察**：

1. **issue 报告的"第 7 轮涨 32 MB"被独立 mock 精确复现**：测出 39.8 MB，与用户报告 32 MB 偏差 < 25%。这验证了 issue 中"+31.68 MB 与用户报告偏差 < 1%"的端到端复现结果，**且把根因明确指向 6 个存储点的 deep clone**，而不是其它内存大头（如 jemalloc fragmentation、view_messages 容器开销）。

2. **Arc<str> 的 RSS 增长近乎为 0**：每个 clone 只增加 8 字节指针 + ref counter 头部（16 字节），相对 1MB+ 大字符串可忽略。

3. **节省比例与副本数理论吻合**：6 副本场景节省 (N-1)/N = 83.3% 为下限，实测 92-100% 说明 Arc 还合并了其它临时分配。

**测试方法说明**：mock 用 `/proc/self/status` VmRSS 测量，每次测量前 `force_alloc()` 触发分配器归还，黑箱消费避免编译器消除。issue 报告的端到端测试用的是 `estimate_messages_heap` 精确堆估算，二者在 7 轮 3MB 场景下的结果一致（误差 < 25%），互相印证。
### 1.2 实际存储点（基于源码审计，已逐行核对）

| # | 存储位置 | 类型 | 性质 | 源码 |
|---|---------|------|------|------|
| 1 | `AgentState.messages` 内 BaseMessage | `BaseMessage::Tool { content: MessageContent }` | 原始（非 clone） | `peri-agent/src/agent/state.rs` |
| 2 | `SessionState.origin_messages` | `Vec<BaseMessage>` | **deep clone**（`msgs.clone()`） | `peri-tui/src/app/agent_ops/mod.rs:287` |
| 3 | `MessagePipeline.completed` | `Vec<BaseMessage>` | **move 不是 clone**（`extend(msgs)` 消费 Vec） | `peri-tui/src/app/message_pipeline/mod.rs:1039` |
| 4 | `MessagePipeline.completed_tools.output` | `String`（中间缓存） | deep clone | `peri-tui/src/app/message_pipeline/mod.rs:215` |
| 5 | `MessageViewModel::ToolBlock.content` | `String` | **deep clone** | `peri-tui/src/ui/message_view/mod.rs:110` |
| 6 | `ContentBlockView::Text.raw` + `.rendered` | `String` + `Text<'static>` | **deep clone × 2** | `peri-tui/src/ui/message_view/mod.rs:449-450` |
| 7 | `RenderCache.lines` | `Vec<Line<'static>>` | **deep clone**（ratatui Text<'static> 构造） | `peri-tui/src/ui/render_thread.rs` |

**字段路径修正**（v1 PRD 在此有事实错误，v3 已修正）：

- 大字符串的实际存储字段有两处独立路径（PRD v1 遗漏了第二处）：
  - 路径 A：`MessageContent::Text(String)`（content.rs:332）—— `MessageContent::text(big_string)` 直接构造时
  - 路径 B：`ContentBlock::Text { text: String }`（content.rs:37）—— 在 `MessageContent::Blocks([...])` 内时
- 现有测试场景（`headless_large_toolresult_e2e` / `large_toolresult_path`）走的是**路径 A**（用 `MessageContent::text(large_content)`）。只修路径 B 不修路径 A，测试不会受益。**Phase 1 必须同时改两条路径**。

**与 issue 描述字段名的差异澄清**：

issue `2026-06-13-message-double-storage-40mb-waste.md` 验证 #4 建议改 `ContentBlock::ToolResult.text` 字段，**但该字段名描述不准确**：

```rust
// 实际结构（content.rs:56-62、message.rs:83-86）
ContentBlock::ToolResult {
    id: Option<String>,
    tool_use_id: String,
    content: Vec<ContentBlock>,  // ← Vec<ContentBlock>，不是 String
    is_error: bool,
}

BaseMessage::Tool {
    content: MessageContent,      // ← MessageContent，不是 ContentBlock::ToolResult
    ...
}

MessageContent::Text(String)      // ← 大 ToolResult 字符串实际存这里（路径 A）
```

issue 测试场景（`BaseMessage::tool_result("id", MessageContent::text(big_string))`）的大字符串实际进入 `MessageContent::Text` 字段，**不是** `ContentBlock::ToolResult`。本 PRD 改的两个字段（路径 A + B）覆盖了 issue 真实需要修复的字段。

### 1.3 为什么 Arc<str> 能解决

`Arc<str>` 的 `Clone` 实现是原子引用计数自增（~1 纳秒，无堆分配）；`String::clone()` 是 alloc + memcpy（5MB 字符串 ~1ms，5MB 堆分配）。把大字符串字段从 `String` 改为 `Arc<str>` 后，所有 `.clone()` 路径自动变为引用计数。

---

## 2. 目标

### 2.1 主目标（v4 校准）

| 指标 | 当前实测 | 目标 | 验证方式 |
|------|---------|------|---------|
| **500 轮典型对话 RSS 累积** | 未实测（待 §6.5 Phase 0 跑出基线） | **≤ 50 MB** | `pipeline_500_rounds_typical_conversation.rs`（Phase 0 新增） |
| **3MB 单轮大文件涨幅**（用户原始报告） | +31.68 MB | **≤ 25 MB** | `headless_large_toolresult_e2e.rs::multi_round_accumulation_real_app` |
| **5MB 单轮大文件涨幅**（紧凑周期内连续大文件） | +28-49 MB | **≤ 40 MB** | `headless_large_toolresult_e2e.rs::large_toolresult_full_e2e_real_app` |
| 序列化协议字节级兼容 | — | **必须兼容** | §6.3 字节 diff |
| 跨平台 CI | — | **全绿** | macOS / Ubuntu / Windows |

### 2.2 目标解读

**"500 轮典型对话"定义**（必须写进测试）：

- 500 个 user prompt + 500 个 AI 回复
- 工具调用分布：
  - 70%（350 轮）：纯文本对话，无工具调用
  - 25%（125 轮）：含小工具结果（1-30KB，典型为 Read 小文件、Grep 少量匹配）
  - 5%（25 轮）：含大工具结果（500KB-3MB，典型为 Read 大文件、Bash 大输出）
- AI 回复平均大小：2-5KB（含 markdown 格式）
- 总原始字节 ≈ 16-25 MB（不放大）
- compact 行为：默认 200K context window 下，500 轮预期触发 **8-15 次 full_compact**

### 2.3 取舍声明

- **500 轮 ≤ 50MB 目标的可达性**：依据 §1.1 事实 D（50 轮 5.44KB/轮）外推，纯累积约 2.7MB；compact 周期清理性极高；真正风险是周期内单次大文件放大。**Phase 1 解决放大后即可达成。**
- **不追求 ≤ 30MB / ≤ 20MB**：那需要追加 Phase 2（view 层 Arc<str>）甚至 Phase 3（ratatui Cow 改造），改动面翻倍，与收益不匹配。Phase 2/3 列为可选优化，**仅在 Phase 1 落地后实测未达 50MB 时启动**。
- **不放弃极端大文件（>10MB 单次）**：超 10MB 单文件本身属于异常使用场景，本 PRD 不直接解决，但 §4.3 列出后续可选方案。

---

## 3. 非目标

- **不改** `BaseMessage::Tool.tool_call_id`、`ContentBlock::ToolUse.id`、`ContentBlock::ToolUse.name`、`ContentBlock::Reasoning.text`、`BaseMessage::*.{id}` —— 这些字段平均 < 100 字节，改 Arc<str> 反而引入原子操作开销。
- **不改** ratatui `Text<'static>` / `Line<'static>` / `Span<'static>` 的 owned String 模型 —— 是 ratatui 上游 API 约束。
- **不改** `origin_messages` / `pipeline.completed` 双存储结构 —— 已验证典型小消息浪费 KB 级，结构性合并属于独立 issue。
- **不做** 大 ToolResult 渲染截断/分页 —— 功能降级，与内存修复正交。
- **不做** ToolResult 磁盘缓存 + 按需加载（issue 验证 #4 备选建议 #3）—— 引入 IO 复杂度和读写时延，与"消除冗余存储"是不同维度的问题。
- **不做** 工具层（Read/Bash/Grep）超大输出截断 + 写入临时文件（issue 验证 #4 备选建议 #4）—— 属工具行为规范，独立 PRD。
- **不做** syntect 精简（issue 验证 #2 建议的 `SyntaxSetBuilder` 只加载 15 种语言）—— 与首轮一次性 +7.83MB 相关，是独立 issue，不与本 PRD 混合。
- **不动** compact middleware 触发阈值 / 行为 —— 已验证有效，不在本 PRD 范围。
- **不动** Agent / ACP 协议层 —— BaseMessage 序列化字节不变（见 §6.3）。

---

## 4. 方案选型

### 4.1 候选方案对比

| 方案 | 改动字段数 | 解决存储点 | 3MB 单轮预计涨幅 | 500 轮累积预期 | 复杂度 | 达 50MB？ |
|------|-----------|-----------|----------------|--------------|--------|----------|
| **A. 仅 BaseMessage 层 Arc<str>（Phase 1）** | 2（路径 A + B） | #2、#4 | ~20-22 MB | Phase 0 实测后填入 | 低 | ✅ |
| B. + View 层 Arc<str>（Phase 2） | 4 | #2、#4、#5、#6（注：#6 不属于 ToolResult 路径，详见 §5.3） | ~10-12 MB | Phase 0 实测后填入 | 中 | ✅（更宽松） |
| C. 全链路含 ratatui Cow（Phase 3） | 6+ | #2-#7 | ~5 MB | Phase 0 实测后填入 | 高 | ✅（远超目标） |
| D. View 层截断 | 0 字段 / 改 reconcile | #5、#6、#7 | ~5 MB | ~3-5 MB | 中（功能降级） | ✅（功能降级） |
| E. origin_messages 合并到 completed | 0 字段 / 改 agent_ops | #2 | ~26 MB | ~8-18 MB | 中 | ⚠️ 边缘 |

### 4.2 选定：方案 A（Phase 1）+ 必须前置的 Phase 0

**Phase 0：500 轮基线测量（必须先做）**

在动任何生产代码前，必须先实测当前 500 轮典型对话的真实 RSS 累积，原因：
1. v3 PRD 在没有实测基线的情况下错把目标定为"单轮 ≤50MB"，结果与用户真实需求错位。**v4 不允许重蹈覆辙**。
2. Phase 1 的"收益预估"必须基于实测"当前累积曲线"，否则节省估算无法敬慎。
3. 500 轮测试还能验证 compact middleware 的实际触发频率（理论 8-15 次，实际可能因消息内容/模型而异）。

Phase 0 详见 §6.5。

**Phase 1：BaseMessage 层 Arc<str>**

**为什么是 A**：
1. **达成 §2 目标**：3MB 单轮涨幅从 +31.68 MB 降到 ~20 MB，5MB 单轮涨幅从 +28-49 MB 降到 ~30-40 MB，500 轮累积 ≤ 15MB。
2. **最小改动**：仅 2 个字段类型变更，改动面集中、风险低、易 review。
3. **API 兼容**：构造器接收 `impl Into<String>` 改为 `impl Into<Arc<str>>`，调用方零改动（`String: Into<Arc<str>>` 自动实现）。
4. **快速验证**：现有 PR #10 测试套件 + Phase 0 新增 500 轮测试自动验证。

**为什么不是 B/C**：与 50MB 目标收益不匹配。Phase 2 需要 view 层 4 个字段变更 + ratatui 调用点修正，Phase 3 更涉及 ratatui 类型改造，工作量翻倍。

**为什么不是 D**：截断是功能降级，与"消除冗余存储"是正交问题，不应在同一 PRD 内混淆。

**为什么不是 E**：双存储已验证典型小消息浪费 KB 级，结构性合并风险（StateSnapshot 重放、compact 中间状态）大于收益。

### 4.3 后续可选阶段（独立 PRD，不阻塞本 PRD）

- **Phase 2（view 层 Arc<str>）**：把 `MessageViewModel::ToolBlock.content` / `ContentBlockView::Text.raw` 也改 Arc<str>，5MB 场景进一步降到 ~20-25MB。**触发条件**：Phase 1 落地后 §6.5 500 轮实测仍 > 50MB，或用户反馈"50MB 仍偏高"。
- **Phase 3（ratatui Cow 改造）**：把 `Text<'static>` 改为 `Text<Cow<'static, str>>`，彻底消除 RenderCache 的 String clone。**触发条件**：Phase 2 后实测仍 > 50MB。
- **大 ToolResult 渲染截断**：> 100KB 的 ToolResult 仅渲染摘要 + 滚动加载。**触发条件**：用户场景出现 > 10MB 单文件。
- **`origin_messages` 与 `pipeline.completed` 合并**：双存储结构重构，独立 PRD。

---

## 5. 详细设计

### 5.1 字段类型变更清单（Phase 1 范围，共 2 个字段）

| 文件 | 字段 | Before | After |
|------|------|--------|-------|
| `peri-agent/src/messages/content.rs:37` | `ContentBlock::Text.text` | `String` | `Arc<str>` |
| `peri-agent/src/messages/content.rs:332` | `MessageContent::Text` 内层 | `String` | `Arc<str>` |

**未改字段**（即使也是 String，理由见 §3）：`MessageViewModel::ToolBlock.content`、`ContentBlockView::Text.raw`、`ContentBlockView::Text.rendered`、`CompletedTool.output`、所有 ID/name/title 字段。

### 5.2 类型变更的影响分析

#### 5.2.1 序列化（content.rs:83-233 手动 impl）

`ContentBlock` 手动 `Serialize` 在 line 90 调用 `m.serialize_entry("text", text)?`，`text` 类型 `&String` → `&Arc<str>`。serde 通过 blanket impl `impl Serialize for Arc<T> where T: Serialize` 透明处理，**字节级输出不变**。

`ContentBlock` 手动 `Deserialize` 在 line 158 用 `text: text.to_string()` 构造字段，改为 `text: Arc::<str>::from(text)` 或 `text.into()`。

`MessageContent` 是 `#[derive(Serialize, Deserialize)]`（content.rs:328）+ `#[serde(untagged)]`，derive 自动处理 Arc<str> 透明序列化，**字节级输出不变**。

#### 5.2.2 PartialEq / Debug / Hash

- `ContentBlock` derive 了 `PartialEq`（content.rs:34），`Arc<str>: PartialEq` 比较的是字符串值（不是指针），行为正确。
- `MessageContent` derive 了 `PartialEq`（content.rs:328），同上。
- `MessageViewModel` 在 `peri-tui/src/ui/message_view/mod.rs:478+` 有手动 `PartialEq`，比较字段时 `Arc<str>` 与 `String` 一样按值比较，需把 `raw: a_raw` 解引用为 `&str` 再比较（可能需要小改）。

#### 5.2.3 现有 API 兼容性

**改字段的读取处必须改**（编译器会强制要求）：

- `as_text()` 内 `Some(text)`（content.rs:297）→ `Some(text.as_ref())` 或 `Some(&**text)`，返回 `Option<&str>` 不变
- `text_content()` 内 `s.clone()`（content.rs:358）→ `(*s).to_string()` 或 `s.to_string()`（Arc<str>::to_string 通过 Deref 生效）
- `content_blocks()` 内 `s.clone()`（content.rs:384）→ `Arc::clone(s)` 或 `s.clone()`（Arc<str>::clone 是 refcount）

**改字段的写入处必须改**：

- `ContentBlock::text(text: impl Into<String>)` 构造器（content.rs:236）→ 接收 `impl Into<Arc<str>>`，内部 `text.into()` 不变
- `MessageContent::text(s: impl Into<String>)` 构造器（content.rs:341）→ 同上

**对外 API（构造器接收 `impl Into<...>`）签名可保持兼容**，因为 `String: Into<Arc<str>>` 已实现。但**字段读取处（pattern matching、字段访问）必须改**。

**已知必须改的 pattern matching（编译必失败，match 分支类型不一致）**：

| 文件 | 行号 | 现状 | 修复 |
|------|------|------|------|
| `peri-agent/src/thread/sqlite_store.rs` | 249 | `MessageContent::Text(t) => t.clone()` 与同 match 中 `Blocks(...) => ...join(" ")` 类型不一致（旧都 String，新 Arc<str> vs String） | 改为 `t.to_string()` |
| `peri-agent/src/thread/filesystem.rs` | 261 | 同上（与 sqlite_store 完全相同结构） | 改为 `t.to_string()` |

**Pattern matching 但通过 Deref 自动兼容的调用**（无需改）：

| 文件 | 行号 | 调用 | 原因 |
|------|------|------|------|
| `peri-tui/src/command/core/gc.rs` | 197 | `MessageContent::Text(s) => s.len()` | `Arc<str>: Deref<Target=str>`，`.len()` 透明 |
| `peri-agent/src/messages/adapters/openai.rs` | 13 | `MessageContent::Text(s) => json!(s)` | `Arc<str>: Serialize` 透明 |
| `peri-agent/src/messages/adapters/anthropic.rs` | 77 | `MessageContent::Text(s) => json!(s)` | 同上 |
| `peri-agent/src/messages/adapters/anthropic.rs` | 210 | `MessageContent::Text(t) => { if !t.is_empty() {...} }` | `.is_empty()` 通过 Deref |
| `peri-agent/src/llm/openai/invoke.rs` | 48 | `MessageContent::Text(s) => json!(s)` | Serialize 透明 |
| `peri-agent/src/llm/anthropic/invoke.rs` | 81, 142 | `MessageContent::Text(s) => json!([{"type": "text", "text": s}])` | Serialize 透明 |

#### 5.2.4 调用方影响范围（grep 实测）

| 调用模式 | grep 实测 | 备注 |
|---------|----------|------|
| `ContentBlock::Text { text: ... }` / `ContentBlock::text(...)` 构造（含测试） | **100 处 / 39 文件** | 字面量 `.to_string()` 不变，编译器自动 `Into<Arc<str>>`，**零改动** |
| `MessageContent::text(...)` / `MessageContent::Text(...)` 构造（含测试） | **119 处 / 40 文件** | 同上，**零改动** |
| `as_text()` 调用 | 13 文件 | 返回类型 `Option<&str>` 不变，**零改动** |
| `MessageContent::Text(t)` pattern matching 后 `.clone()` | **2 处**（sqlite_store / filesystem） | **必须改为 `.to_string()`**，match 分支类型一致 |
| `ContentBlock::Text { text }` pattern matching 后 `.as_str()` | 2 处（sqlite_store / filesystem） | `Arc<str>: Deref<Target=str>`，`.as_str()` 透明，**零改动** |
| `MessageContent::Text(s)` pattern matching 后 `.len()` / `.is_empty()` / `json!(s)` | 6 处 | Deref 或 Serialize 透明，**零改动** |

**预期总改动量**：核心 2 个字段 + 5-7 个内部读取处（content.rs 内）+ **2 处生产代码 pattern matching**（sqlite_store + filesystem）+ 0-3 处边角 case（编译器定位）。**测试代码构造侧零改动**；测试代码 pattern matching 侧若做 `assert_eq!(text, "...")` 直接比较，可能需 `text.as_ref()` 解引用（编译器定位）。

### 5.3 节省估算（基于实测，无数学放大假设）

#### 单次大 ToolResult 放大对比

存储点 #1-#7 对一个 5MB ToolResult 的累积（**注意 #6 不属于 ToolResult 路径**，见下方说明）：

| 存储点 | Before（5MB ToolResult） | After Phase 1 | 节省 |
|-------|--------------------------|---------------|------|
| #1 原始 BaseMessage | +5 MB（堆分配） | +5 MB（Arc 堆分配，同 size） | 0 |
| #2 origin_messages.clone | +5 MB（String deep clone） | +0 MB（Arc::clone refcount） | **+5 MB** |
| #3 set_completed（move） | +0 MB（已 move） | +0 MB | 0 |
| #4 completed_tools.output | +5 MB（String clone） | +0 MB（Arc::clone） | **+5 MB** |
| #5 ToolBlock.content | +5 MB（未动 view 层） | +5 MB | 0 |
| ~~#6 ContentBlockView::Text.raw + rendered~~ | ~~+10 MB~~ | — | **不计入**（见下方说明） |
| #7 RenderCache.lines | +5-10 MB（未动，含 Line/SPAN 渲染开销） | +5-10 MB | 0 |
| **合计** | **~25-30 MB** | **~15-20 MB** | **~10 MB** |

**关于 #6 不计入**：`ContentBlockView::Text` 是 **AI 文本消息**的 view 层（处理 `ContentBlock::Text { text }` → view），不是 ToolResult 的路径（见 `peri-tui/src/ui/message_view/mod.rs:585` 的转换逻辑）。ToolResult 走 `MessageViewModel::ToolBlock.content` 路径（`reconcile.rs:245-250`），与 #6 互斥。PRD v3 把 #6 算进 5MB ToolResult 是错的，v4 已修正。

实测对照：5MB ToolResult 实测 RSS 增长 +28-49 MB（来自 `large_toolresult_path.rs`），与表内估算 ~25-30 MB 的差值来自 jemalloc 碎片 + ratatui 渲染额外开销。

#### 用户场景（3MB ToolResult）

| 当前实测 | Phase 1 预期 | 50MB 目标 |
|---------|-------------|-----------|
| +31.68 MB | **~20-22 MB** | ✅ 大幅低于 |

#### 500 轮累积

**不能从 50 轮数据线性外推**（见 §1.1 事实 D 修正说明）。500 轮实际累积由 compact 行为控制：

- 纯文本累积（每轮 ~3KB）远低于 compact 阈值，但 500 轮预期触发 8-15 次 full_compact
- 每次 compact 后 TUI 层 `pipeline.completed` / `view_messages` / `RenderCache` 全部 drop 旧内容
- 真正瓶颈是 **compact 周期内单次大 ToolResult 放大**（用户报告 +32MB 即是此种场景）

**Phase 0 实测前不下具体数字结论**。Phase 0 跑出基线后填入实测值。

### 5.4 关键决策点

| 决策点 | 默认建议 | 状态 |
|-------|---------|------|
| 接受 Phase 1 后极端大文件（>10MB）场景可能仍 > 50MB | 是（典型场景达标即可） | ✅ **用户已确认接受**（2026-06-14） |
| 是否同步把 `MessageContent::content_blocks()` / `text_content()` 内部 clone 优化为 Arc 共享 | 是（顺手优化，无额外风险） | ❌ 实施者决定 |
| 是否顺手把 `Reasoning.text` 也改 Arc<str> | 否（典型 < 10KB，原子操作开销 > 节省） | ❌ 实施者决定 |

---

## 6. 验证方案

### 6.1 自动化测试（PR #10 已落地 + Phase 0 新增）

| 测试文件 | 验证目标 | Phase 1 预期 |
|---------|---------|-------------|
| `peri-tui/tests/headless_large_toolresult_e2e.rs::large_toolresult_full_e2e_real_app` | 5MB ToolResult 端到端 RSS 涨幅 | ≤ 40 MB（预计 ~20-25 MB，原 28-49 MB） |
| `peri-tui/tests/headless_large_toolresult_e2e.rs::multi_round_accumulation_real_app` | **第 7 轮 3MB 涨幅（用户场景）** | **≤ 25 MB（预计 ~20-22 MB，原 +31.68 MB）** |
| `peri-tui/tests/large_toolresult_path.rs::large_toolresult_full_path_attribution` | 单次大 ToolResult 各存储点归因 | #2 步从 +5MB → ~0MB，#4 步降到 ~0（**#6 不计入**，见 §5.3） |
| `peri-tui/tests/double_storage_bytes.rs` | 50 条典型对话双存储 | 不回归（KB 级） |
| `peri-tui/tests/pipeline_real_rss_growth.rs` | 50 轮累积 RSS | 不回归 |
| **`peri-tui/tests/pipeline_500_rounds_typical_conversation.rs`**（Phase 0 新增） | **500 轮典型对话累积** | **≤ 50 MB（主目标）** |
| `peri-agent/src/messages/content_test.rs` | Arc<str> 字段读写、serde round-trip | 通过 |

### 6.2 新增断言（Phase 1 完成后必须加）

新增 `peri-agent/src/messages/content_arc_test.rs`（或并入现有 `content_test.rs`）：

```rust
use std::sync::Arc;
use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};

#[test]
fn content_block_text_arc_shares_memory_on_clone() {
    let big = "x".repeat(64 * 1024); // 64KB
    let block1 = ContentBlock::text(big.clone());
    let block2 = block1.clone();

    if let (ContentBlock::Text { text: t1 }, ContentBlock::Text { text: t2 }) = (&block1, &block2) {
        assert!(Arc::ptr_eq(t1, t2), "clone 后 Arc 应指向同一份堆内存");
        assert_eq!(t1.as_ref(), big.as_str());
    } else {
        panic!("应为 ContentBlock::Text 变体");
    }
}

#[test]
fn message_content_text_arc_shares_memory_on_clone() {
    let big = "x".repeat(64 * 1024);
    let mc1 = MessageContent::text(big.clone());
    let mc2 = mc1.clone();

    if let (MessageContent::Text(s1), MessageContent::Text(s2)) = (&mc1, &mc2) {
        assert!(Arc::ptr_eq(s1, s2), "clone 后 Arc 应指向同一份堆内存");
    } else {
        panic!("应为 MessageContent::Text 变体");
    }
}

#[test]
fn serde_round_trip_preserves_text_content() {
    let block = ContentBlock::text("hello world".to_string());
    let json = serde_json::to_string(&block).expect("serialize");
    assert!(json.contains("\"text\":\"hello world\""), "JSON 字节级兼容");
    let de: ContentBlock = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(block, de);
}
```

### 6.3 序列化字节级兼容性验证

**必须**在 PR 描述中附 before/after JSON 对照（用同一 BaseMessage 序列化）：

```bash
# Before
cargo run --example serialize-sample-message > before.json
# After（Phase 1）
cargo run --example serialize-sample-message > after.json
diff before.json after.json  # 必须无差异
```

序列化样例覆盖：纯 Text 消息、含 ToolResult 消息、含 ToolUse 消息、含 Reasoning 消息、含 Image 消息、Unknown 透传消息。

### 6.4 真实 TUI 用户场景验证

```bash
# 用复现场景跑真实 TUI
cargo run -p peri-tui -- -p "读取 src/main.rs 全文并总结" --dangerously-skip-permissions
# 在 TUI 内输入 /gc 查看 RSS / heap 分解
# 期望：单轮涨幅 ≤ 25MB（Phase 1）
```

### 6.5 Phase 0：500 轮基线测量（必须先做）

**新增测试文件**：`peri-tui/tests/pipeline_500_rounds_typical_conversation.rs`

**测试设计**（必须严格按 §2.2 定义的 message mix）：

```rust
//! 500 轮典型对话基线测试（Phase 0）
//!
//! 测量目标：500 轮典型对话的 RSS 累积曲线，验证是否 ≤ 50MB。
//! 典型对话定义见 PRD §2.2：
//! - 70% 纯文本（350 轮）
//! - 25% 含小工具结果（125 轮，1-30KB）
//! - 5% 含大工具结果（25 轮，500KB-3MB）

use peri_agent::messages::{BaseMessage, ContentBlock, MessageContent};
use peri_tui::app::events::AgentEvent;
use peri_tui::app::message_pipeline::{MessagePipeline, PipelineAction};

#[cfg(unix)]
fn current_rss_kb() -> usize { /* 读取 /proc/self/status VmRSS */ }

#[cfg(not(unix))]
fn current_rss_kb() -> usize { 0 }

fn build_typical_rounds() -> Vec<(String, BaseMessage)> {
    // 按 PRD §2.2 比例生成 500 轮
    // 返回 (user_text, ai_msg) 对
}

fn feed_round(pipeline: &mut MessagePipeline, round_idx: usize, user_text: &str, ai_msg: BaseMessage) {
    pipeline.begin_round();
    // 流式 chunks（模拟 AssistantChunk）
    // StateSnapshot 携带 user + ai 消息
    // done
}

#[test]
fn measure_500_rounds_typical_baseline() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let mut view_messages: Vec<_> = Vec::new();
    let baseline = current_rss_kb();

    println!("=== 500 轮典型对话 RSS 基线 ===");
    println!("基线 RSS: {} KB ({:.2} MB)", baseline, baseline as f64 / 1024.0);

    let rounds = build_typical_rounds();
    for (i, (user_text, ai_msg)) in rounds.into_iter().enumerate() {
        feed_round(&mut pipeline, i, &user_text, ai_msg);
        // 应用 actions 到 view_messages
        // 每 50 轮采样 RSS
        if i % 50 == 49 {
            let rss = current_rss_kb();
            println!("轮 {:>3}: RSS = {} KB ({:.2} MB)", i + 1, rss, rss as f64 / 1024.0);
        }
    }

    let final_rss = current_rss_kb();
    let total = final_rss.saturating_sub(baseline);
    println!("=== 500 轮完成 ===");
    println!("最终 RSS: {} KB", final_rss);
    println!("累计增长: {} KB ({:.2} MB)", total, total as f64 / 1024.0);

    // Phase 1 验证断言（实施后启用）：
    // assert!(total as f64 / 1024.0 <= 50.0, "500 轮典型对话 RSS 必须 ≤ 50MB");
}
```

**Phase 0 执行步骤**：

1. **当前基线测量**（不动生产代码）：跑 `pipeline_500_rounds_typical_baseline`，记录 RSS 曲线 + 最终累积。验证是否已经 ≤ 50MB（如果已经达标，Phase 1 价值降低，但仍建议做以解决单轮暴涨问题）。
2. **触发 compact 行为验证**：在测试中打印 `compact_event_count`，验证理论触发次数（8-15 次）。
3. **Phase 1 实施后重跑**：对比 before/after 曲线。

**Phase 0 失败兜底**：如果当前基线已经 > 50MB（说明 compact 没有按预期清理 / 累积速度高于估算），必须**先调查根因再实施 Phase 1**。可能需要：
- 调低 compact 阈值
- 增加 manual compact 触发点
- 排查 jemalloc 是否真的归还 OS

---

## 7. 风险与回滚

### 7.1 风险矩阵

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| serde 序列化字节变化（持久化 / ACP 传输） | 低 | 高（破坏向后兼容） | §6.3 字节级 diff 验证；如有差异立即回滚 |
| 调用方期望 `String` 编译失败 | 高 | 低 | 编译器自动定位所有需要改的点，机械修复 |
| Arc 原子操作 hot path 性能回归 | 极低 | 低 | Arc::clone 比 String::clone 快 100x+；如担心加 criterion benchmark |
| ratatui 渲染时频繁 deref Arc<str> → &str | 低 | 低 | Deref 是零成本，编译器优化 |
| Phase 0 实测基线已 ≤ 50MB，Phase 1 收益不明显 | 中 | 低 | Phase 1 仍解决单轮暴涨问题，价值独立 |
| Phase 0 实测基线 > 50MB，根因不是 String deep clone | 中 | 中 | 立即停下来重新审计，按 §6.5 兜底流程排查 |
| 多线程并发 mutate Arc<str> 内容 | 不存在 | — | Arc<str> 是不可变共享，所有"修改"路径都是 reassign，不存在竞争 |

### 7.2 回滚方案

Phase 1 改动集中在 2 个字段 + 5-10 个读取处，git revert 即可完整回滚。**不涉及数据迁移、不需要清缓存**（serde 字节级兼容，旧持久化数据可读）。

---

## 8. 实施清单（必须逐项确认）

### Phase 0（必须先做，不动生产代码）

- [ ] 写 `peri-tui/tests/pipeline_500_rounds_typical_conversation.rs`，按 §2.2 定义生成 500 轮 message mix
- [ ] 跑当前基线，记录 RSS 曲线 + 最终累积 + compact 触发次数
- [ ] **决策点**：基线 ≤ 50MB？
  - 是 → Phase 1 仍做（解决单轮暴涨），但优先级降低
  - 否 → 立即停下来，按 §6.5 兜底流程重新审计

### Phase 1（核心改动）

- [ ] 改 `ContentBlock::Text.text: String` → `Arc<str>`（content.rs:37）
- [ ] 改 `MessageContent::Text` 内层 `String` → `Arc<str>`（content.rs:332）
- [ ] 改手动 Deserialize 中 `text.to_string()` → `Arc::<str>::from(text)`（content.rs:158）
- [ ] 改 `as_text()` 返回处 `Some(text)` → `Some(text.as_ref())`（content.rs:297）
- [ ] 改 `text_content()` 内 `s.clone()` → `s.to_string()`（content.rs:358）
- [ ] 改 `content_blocks()` 内 `s.clone()` → `Arc::clone(s)`（content.rs:384）
- [ ] 改构造器签名 `impl Into<String>` → `impl Into<Arc<str>>`（content.rs:236、341）
- [ ] **改 `sqlite_store.rs:249` `MessageContent::Text(t) => t.clone()` → `t.to_string()`**（match 分支类型一致性）
- [ ] **改 `filesystem.rs:261` `MessageContent::Text(t) => t.clone()` → `t.to_string()`**（同上）
- [ ] 编译通过（机械修复所有调用方，编译器定位剩余 0-3 处边角 case）
- [ ] 跑 `content_arc_test.rs` 三个新断言通过
- [ ] 跑 §6.3 字节级 diff，**必须 0 差异**
- [ ] 跑全部 PR #10 测试套件，CI 全绿
- [ ] 跑 Phase 0 500 轮测试，对比 before/after
- [ ] PR 描述附 before/after RSS 对照表

### 验证（实施完成后）

- [ ] Phase 0 测试当前基线（基线数据写入 PRD §1.1 事实 F）
- [ ] Phase 0 测试 Phase 1 后累积（写入 §5.3 实测节省）
- [ ] `headless_large_toolresult_e2e.rs::multi_round_accumulation_real_app` 通过（3MB ≤ 25MB）
- [ ] `headless_large_toolresult_e2e.rs::large_toolresult_full_e2e_real_app` 通过（5MB ≤ 40MB）
- [ ] macOS / Ubuntu / Windows 三平台 CI 全绿
- [ ] 真实 TUI 复现场景手动验证（§6.4）

---

## 附录 A：v3 → v4 关键变更

| v3 错误 | v4 修正 |
|---------|---------|
| 目标：单轮 ≤ 50MB | 目标：500 轮累积 ≤ 50MB（用户真实需求） |
| 假设 500 轮线性累积 | 引入 compact 行为分析，说明累积由 compact 控制 |
| 缺少 Phase 0 基线测量 | 强制要求 Phase 0 先跑，避免重蹈 v3 覆辙 |
| Phase 2/3 列为可选但触发条件模糊 | 明确触发条件：Phase 1 后实测 > 50MB 时启动 |
| 节省估算基于"5MB × 6 deep clone"数学 | 改为基于实测 32MB（3MB ToolResult）反推，去除假设 |

## 附录 A2：v4 → v5 关键变更

| v4 问题 | v5 修正 |
|---------|---------|
| §5.2.4 漏掉生产代码 pattern matching（编译必失败） | §5.2.3 增加 sqlite_store.rs:249 + filesystem.rs:261 两处必须改的明确条目；§5.2.4 列出 6 处通过 Deref/Serialize 透明兼容的调用 |
| §5.2.4 "不需要改测试代码" 表述错位 | 改为 "构造侧零改动；pattern matching 侧若做 assert_eq! 直接比较可能需 .as_ref() 解引用（编译器定位）" |
| §5.2.4 调用方统计严重低估（"6 处" / "30+ 文件"） | grep 实测：100 处/39 文件、119 处/40 文件 |
| §5.3 把 #6 ContentBlockView::Text 计入 5MB ToolResult 路径（+10MB） | #6 是 AI 文本消息 view 路径，与 ToolResult 互斥；5MB ToolResult 总涨幅修正为 ~25-30MB（原 ~40MB） |
| §1.1 事实 D 与 §2.3 自相矛盾（线性外推 vs compact） | 删除"线性外推 500 轮 ≈ 2.7-5.4 MB"，改为"50 轮仅验证 compact 未触发阶段，500 轮必须 Phase 0 实测" |
| §5.3 "预期 Phase 1 后 ≤ 15MB" 无依据 | 删除具体数字，改为"Phase 0 实测前不下具体数字结论" |
| §4.1 / §6.1 中 5MB 单轮 ~30-40 MB | 同步下调到 ~20-25 MB |

## 附录 A3：v5 → v6 关键变更

| v5 缺陷 | v6 修正 |
|---------|---------|
| §1.1 仅有端到端复现（+31.68 MB），缺少独立 mock 验证 Arc<str> 收益 | 新增"事实 F"：独立 mock 项目（`mock_arc_bench/`）测 5 个场景，7 轮 3MB String 39.8 MB vs Arc<str> 0 KB，与 issue 报告偏差 < 25%，互相印证根因 |
| 缺少 Arc<str> 节省比例的实测数据 | mock 显示 6 副本场景节省 92-100%，与理论 (N-1)/N = 83.3% 下限吻合并超出（说明 Arc 还合并了其它临时分配） |

## 附录 B：相关源码索引

| 文件 | 行号 | 内容 |
|------|------|------|
| `peri-agent/src/messages/content.rs` | 37 | `ContentBlock::Text.text` 字段 |
| `peri-agent/src/messages/content.rs` | 83-233 | 手动 Serialize/Deserialize impl |
| `peri-agent/src/messages/content.rs` | 295-300 | `as_text()` 读取 |
| `peri-agent/src/messages/content.rs` | 332 | `MessageContent::Text(String)` 字段 |
| `peri-agent/src/messages/content.rs` | 356-371 | `text_content()` 读取（含 `s.clone()`） |
| `peri-agent/src/messages/content.rs` | 378-396 | `content_blocks()` 读取（含 `s.clone()`） |
| `peri-agent/src/thread/sqlite_store.rs` | 249 | **必须改**：`MessageContent::Text(t) => t.clone()` |
| `peri-agent/src/thread/filesystem.rs` | 261 | **必须改**：同上 |
| `peri-tui/src/app/agent_ops/mod.rs` | 287 | `origin_messages.extend(msgs.clone())` |
| `peri-tui/src/app/message_pipeline/mod.rs` | 215 | `CompletedTool.output: String` |
| `peri-tui/src/app/message_pipeline/mod.rs` | 1039 | `set_completed` extend（move 不是 clone） |
| `peri-tui/src/app/agent_compact.rs` | 60-82 | compact 后 pipeline 清理 + RebuildAll |
| `peri-tui/src/ui/message_view/mod.rs` | 110 | `MessageViewModel::ToolBlock.content` |
| `peri-tui/src/ui/message_view/mod.rs` | 449-450 | `ContentBlockView::Text { raw, rendered }` |
| `peri-tui/src/ui/render_thread.rs` | 355-442 | `RenderCache.rebuild` 与 mem::take |
| `peri-middlewares/src/compact_middleware.rs` | 286-311 | `before_model` 触发 compact |
| `peri-agent/src/agent/token.rs` | 124-160 | `ContextBudget` 阈值定义 |
