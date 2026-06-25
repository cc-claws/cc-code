> 归档于 2026-05-31，原路径 spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md

# 重新引入 mimalloc 作为全局分配器（带 MI_OPTION 调参）

**状态**：Fixed
**优先级**：中
**创建日期**：2026-05-30
**实施日期**：2026-05-30

## 问题描述

当前 peri-tui 使用系统默认分配器（无第三方全局分配器），长时间对话后 RSS 仍持续增长。此前两次尝试更换分配器（jemalloc → mimalloc → 系统默认）均未解决问题，且上次 mimalloc 测试时**未配置任何 MI_OPTION 参数**，可能在未调优的情况下就下了「mimalloc 更差」的结论。现在 AgentPool 已实现 LLM 实例复用（减少每轮瞬态分配），场景已发生变化，值得重新评估 mimalloc。

## 背景：历史尝试

### jemalloc 阶段
- 症状：每轮 ~40 MB RSS 增长，arena 碎片化 17.5 MB/轮
- 原因：macOS `background_thread` 不工作（依赖 pthread），`dirty_decay_ms` 调优效果有限
- 参考：`spec/archive-issues/2026-05-24-build-agent-per-turn-arc-transient-fragmentation.md`

### mimalloc 阶段（第一次）
- 症状：普通对话即 100MB+ RSS，比 jemalloc 同期更差
- 可能原因：**未配置 MI_OPTION**（PAGE_RESET/DECOMMIT/BACKGROUND_THREAD 均未启用），同时每轮 build_agent 产生大量瞬态分配
- 参考：`spec/archive-issues/2026-05-25-mimalloc-worse-than-jemalloc.md`

### 当前状态（系统默认分配器）
- 症状：长时间对话后 RSS 增长
- 无诊断工具（`/heapdump` 已随 mimalloc 一起移除）
- AgentPool 已实现 LLM 实例复用，分配 churn 已降低

## 实施方案（最小化引入）

1. **添加 mimalloc 依赖**：workspace `Cargo.toml` + `peri-tui/Cargo.toml`
2. **声明全局分配器**：`peri-tui/src/main.rs` 中 `#[global_allocator] static GLOBAL: mimalloc::MiMalloc`
3. **配置 MI_OPTION 环境变量**：在 `main()` 最开头设置，在首次分配前生效
   - `MIMALLOC_PAGE_RESET=1` — 页面释放时立即重置
   - `MIMALLOC_DECOMMIT=1` — 归还虚拟地址空间给 OS
   - `MIMALLOC_BACKGROUND_THREAD=1` — 后台线程回收内存
4. **恢复 `alloc_collect`**：在 `/clear`、`/compact`、切换会话后调用 `mi_collect(true)` 触发主动回收

### 不做的事项
- 不恢复 `/heapdump` 命令（最小化引入）
- 不引入 jemalloc-ctl 依赖

## 涉及文件

- `Cargo.toml`（workspace 依赖声明）—— 添加 mimalloc
- `peri-tui/Cargo.toml`（crate 依赖）—— 添加 mimalloc
- `peri-tui/src/main.rs`（全局分配器声明 + MI_OPTION 设置）—— `#[global_allocator]` + `init_mimalloc_conf()`
- `peri-tui/src/app/thread_ops.rs`（内存回收）—— 恢复 `alloc_collect()` 调用 `mi_collect(true)`

## 关联 Issue

- `spec/archive-issues/2026-05-25-replace-jemalloc-with-mimalloc.md`（第一次 mimalloc 尝试）
- `spec/archive-issues/2026-05-25-mimalloc-worse-than-jemalloc.md`（mimalloc 表现更差的记录）
- `spec/archive-issues/2026-05-24-build-agent-per-turn-arc-transient-fragmentation.md`（arena 碎片化根因分析）
- `docs/superpowers/plans/2026-05-23-memory-rss-growth-fix.md`（jemalloc 调优方案）

## 实施记录（2026-05-30）

已实施内容：

| 项目 | 文件 | 状态 |
|------|------|------|
| mimalloc 依赖 | `Cargo.toml`, `peri-tui/Cargo.toml` | ✅ |
| 全局分配器声明 | `peri-tui/src/main.rs:28-29` | ✅ |
| MI_OPTION 配置 | `peri-tui/src/mimalloc_config.rs` → `init_mimalloc_conf()` | ✅ |
| 内存回收 | `peri-tui/src/app/thread_ops.rs` → `alloc_collect()` (`mi_collect(true)`) | ✅ |

**结果**：内存线性增长问题**仍然存在**（现象未改善），详见 `spec/issues/2026-05-22-memory-linear-growth-no-compact.md`。
