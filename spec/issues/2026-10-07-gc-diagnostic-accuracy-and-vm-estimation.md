# `/gc` 内存诊断可读性修正：消除误导指标 + 纳入 view_messages 估算

**状态**：Fixed
**优先级**：低（诊断准确性，非功能缺陷）
**创建日期**：2026-10-07
**分支**：`feat/gc-diagnostic-honesty`

---

## 一、背景

`/gc` 命令（`cc-tui/src/command/core/gc.rs`）输出一份内存诊断。真实输出样例：

```
· allocated: 60.5 MB (应用实际分配)
· active:    1392.6 MB (活跃页)
· resident:  100.4 MB (物理驻留)
· mapped:    1026.4 MB (映射)
· retained:  912.7 MB (保留未归还 OS)
· 碎片: active-allocated=1332.1 MB | resident-active=0 B
· 消息估算: 1.4 MB | allocated 内未识别: 59.1 MB?
```

这组数字**存在误导**，易让人误判"内存有问题"。

## 二、两个问题

### 2.1 平台语义混淆（P0）

`active` / `mapped` / `retained` 三个字段的语义**分平台**，但旧代码用统一标签：

| 字段 | jemalloc（macOS/Linux） | mimalloc（Windows） |
|------|------------------------|---------------------|
| `active` | 真实活跃页 | **`page_committed`**，源码别名 `"touched"`，**历史触及高水位**，`mi_collect` 后**不减** |
| `mapped` | 映射量 | `reserved` 虚拟地址保留量，**不占物理内存** |
| `retained` | 保留未归还 OS | `reserved - committed`，**虚拟地址空间**，非物理内存 |

后果：Windows 上 `active=1392 MB` 与派生的 `碎片=1332 MB` **完全不可用于诊断**——那个"碎片"是 mimalloc touched 高水位减 allocated 的假象，真实内存没有任何碎片问题（`resident 100.4 MB`，`RSS - resident = 0`）。

### 2.2 估算器覆盖面过窄（P1）

`estimate_messages_heap` **只**统计 `origin_messages` + `pipeline.completed`，漏掉 `view_messages`（渲染视图模型，含每块内嵌的 `Text<'static>`），而后者恰是最大的一块内存消费者。

于是 60.5 MB 的 `allocated` 里 59.1 MB 被标成「未识别」，看着像"有 59 MB 说不清去向"，实际是**估算器盲区**。

`:273 origin vs completed 完全相同 ⚠️` 亦误导——这两份是**设计冗余**（`origin_messages` 为 agent 权威历史，`pipeline.completed` 为渲染管线基线，经 `restore_completed(msgs.clone())` 复制），非泄漏。

## 三、改动

`cc-tui/src/command/core/gc.rs`：

**P0 — 标注真实语义（按平台条件化）**
- `active`：mimalloc 下标注「历史触及高水位，非当前占用，勿用于诊断」；jemalloc 下保留「活跃页」
- `resident` 标注「物理驻留，真实占用」
- `mapped` / `retained`：mimalloc 下标注「虚拟地址保留量，不占物理内存」
- `碎片` 行：mimalloc 下改为「⚠ 基于 touched 高水位，非真实碎片，忽略」
- `allocated 与 RSS 差`：补充「RSS 更大 = 栈/映射文件等非分配器占用」

**P1 — 估算器纳入 view_messages**
- 新增 `estimate_view_messages_heap()`，遍历 `MessageViewModel` 全部变体：
  `UserBubble` / `AssistantBubble`（含各 `ContentBlockView`）/ `ToolBlock`（含 `DiffInput`）/
  `ShellCommand` / `SystemNote` / `CacheWarning` / `ToolCallGroup` / `SubAgentGroup`（递归子 VM）
- `Text<'static>` 堆占用：逐 `Line`/`Span` 累加；`Span.content` 是 `Cow<str>`，
  仅 `Owned` 变体计 `capacity()`（`Borrowed` 本就内联）
- 「已知合计」行改为 `消息 X + VM Y` 三元展示
- `origin vs completed` 完全相同 → 标注「设计冗余，非泄漏」
- 追加一行注脚：「未识别 = markdown 缓存/ACP 缓冲/tokio/tracing 等未纳入估算的部分，非泄漏」

## 四、验证

- CI：`cargo build --workspace --all-targets` / `cargo test --workspace` / `clippy -D warnings`（三平台）
- 人工：运行 `/gc`，确认新标签正确、`view_messages` 有字节数、未识别量显著下降

## 五、关联

- 本机采集的真实数据（见 §一）来自用户会话
- 相关：`docs/superpowers/plans/2026-05-23-memory-linear-growth-jemalloc-tuning.md`、
  `2026-05-25-remove-mimalloc.md`、`2026-05-30-retry-mimalloc-with-mi-options.md`
