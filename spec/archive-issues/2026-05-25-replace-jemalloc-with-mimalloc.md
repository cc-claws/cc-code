> 归档于 2026-05-27，原路径 spec/issues/2026-05-25-replace-jemalloc-with-mimalloc.md

# 使用 mimalloc 替换 jemalloc 全局分配器

**状态**：Closed（废弃了，现在无分配器）
**优先级**：中
**创建日期**：2026-05-25

## 问题描述

当前 peri-tui 使用 tikv-jemallocator 作为全局分配器，但在高分配 churn 场景下（每轮 ~68 万次瞬态 malloc/free），jemalloc arena 碎片化严重：`/clear` 后 active - allocated 仍有 17.5 MB 碎片化空闲页，mapped 虚拟地址空间膨胀至 116+ MB。macOS 不支持 `background_thread`（依赖 pthread），dirty_decay_ms 调优效果有限。需完全移除 jemalloc，改用 mimalloc 作为全局分配器。

## 现状

- **全局分配器**：`peri-tui/src/main.rs` 中 `#[global_allocator] static GLOBAL: tikv_jemallocator::Jemalloc`
- **jemalloc 配置模块**：`peri-tui/src/jemalloc_config.rs`（编译时 malloc_conf + 运行时 mallctl 调优）
- **Workspace 依赖**：`Cargo.toml` 中 `tikv-jemallocator = "0.6"` 和 `tikv-jemalloc-ctl = { version = "0.6", features = ["stats", "use_std"] }`
- **诊断命令**：`/heapdump`（`command/core/heapdump.rs`）依赖 `tikv-jemalloc-ctl` 输出 allocated/active/resident/mapped/huge 等详细统计 + 配置诊断
- **主动 purge**：`thread_ops.rs` 中 `jemalloc_decay()` 使用 epoch advance + arena decay/purge
- **compact 清理**：`agent_compact.rs` 中调用 `jemalloc_decay()` 尝试回收内存

jemalloc 调优措施（dirty_decay_ms:200, lg_tcache_max:16）已实施但效果有限，arena 碎片化仍是 RSS 线性增长的主要贡献者之一（~17 MB/轮）。

## 期望改进方向

1. **替换全局分配器**：`mimalloc` 替换 `tikv-jemallocator`，利用 mimalloc 更积极的内存归还策略缓解碎片化
2. **完全移除 jemalloc 依赖**：移除 `tikv-jemallocator` 和 `tikv-jemalloc-ctl` 两个 workspace 依赖
3. **删除 jemalloc 相关代码**：`jemalloc_config.rs` 整个模块、`main.rs` 中的 `#[global_allocator]` 和 `mod jemalloc_config`
4. **迁移 /heapdump**：用 mimalloc 的统计 API 替换 jemalloc stats 输出
5. **迁移内存回收**：`thread_ops::jemalloc_decay()` 改用 mimalloc 对应的内存回收机制

## 涉及文件

- `Cargo.toml`（workspace 依赖声明）—— 移除 tikv-jemallocator/tikv-jemalloc-ctl，添加 mimalloc
- `peri-tui/Cargo.toml`（crate 依赖）—— 同上
- `peri-tui/src/main.rs`（全局分配器声明 + jemalloc_config 模块引用）—— 替换全局分配器
- `peri-tui/src/lib.rs`（jemalloc_config 公开模块声明）—— 替换或移除
- `peri-tui/src/jemalloc_config.rs`（jemalloc 配置模块）—— 替换为 mimalloc 配置模块或删除
- `peri-tui/src/command/core/heapdump.rs`（/heapdump 命令）—— 替换 jemalloc stats 为 mimalloc stats
- `peri-tui/src/app/thread_ops.rs`（jemalloc_decay() 函数）—— 替换为 mimalloc 内存回收
- `peri-tui/src/app/agent_compact.rs`（compact 后调用 jemalloc_decay）—— 适配新的回收函数

## 关联 Issue

- `spec/issues/2026-05-22-memory-linear-growth-no-compact.md`（内存线性增长，P1 第 13 项提及 mimalloc）
- `spec/archive-issues/2026-05-24-build-agent-per-turn-arc-transient-fragmentation.md`（arena 碎片化定量数据）
