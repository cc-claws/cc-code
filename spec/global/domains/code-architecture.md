# 代码架构 领域

## 领域综述

代码架构领域记录影响整体项目结构的重大变更，包括 crate 增删、依赖关系重构等。

核心职责：

- Workspace crate 结构管理
- 废弃功能完整清理
- 依赖关系调整

## 核心流程

### Relay Server 移除流程

```
1. 删除 rust-relay-server crate 目录
2. Workspace Cargo.toml members 移除引用
3. TUI 中清理 20+ 文件的 Relay 集成:
   - 面板（RelayPanel）
   - 命令（/relay）
   - 事件转发（RelayMessage）
   - CLI 参数（--remote-control）
   - 配置类型（RemoteControlConfig）
4. App 结构体从 4 子结构体缩减为 3（去掉 RelayState）
5. 评估 MessageAdded 事件若仅被 Relay 使用则从核心框架移除
6. 旧配置文件中 remote_control 字段无需主动清理，serde 自然忽略
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| Workspace 结构 | 3 crate → 移除 relay 后维持 3+peri-widgets |
| 配置兼容 | serde 忽略旧字段，无需主动清理 |
| 遗留文件 | Dockerfile.relay 保留作历史记录 |

## Feature 附录

### feature_20260427_F001_relay-removal

**摘要:** 完整删除废弃的 Relay Server 远程控制功能及相关代码
**关键决策:**

- 整体删除 rust-relay-server crate（含 server/client feature 及 web 前端）
- 清理 TUI 中 20+ 文件的 Relay 集成
- App 结构体从 4 子结构体缩减为 3（去掉 RelayState）
- 评估 MessageAdded 事件若仅被 Relay 使用则从核心框架一并移除
- 旧配置文件中 remote_control 字段无需主动清理，serde 自然忽略
- workspace 从 4 crate 减为 3 crate
**归档:** [链接](../../archive/feature_20260427_F001_relay-removal/)
**归档日期:** 2026-04-30

---

## Issue 经验附录

### issue_2026-05-14-dead-code-unfinished-features-cleanup

**摘要:** 24 处 #[allow(dead_code/unused)] 抑制了真正的死代码和未完成功能
**状态:** Fixed
**归档日期:** 2026-05-15
**关键词:** 死代码, allow注解, 代码清理, 编译器警告
**问题本质:** 编译器零警告不代表代码健康。24 处 allow 注解中，1 处为真正的死代码（CaptureLLM 完整实现但未接入测试），多处为未完成功能的预留字段/方法。
**通用模式:** 定期审计 #[allow(dead_code/unused)] 注解可以发现未完成功能和技术债。allow 注解是技术债的信号标记——它压制了编译器检测但保留了问题。
**涉及文件:** peri-middlewares/src/subagent/tool_test.rs, peri-tui/src/app/message_pipeline.rs, peri-middlewares/src/tool_search/tool_index.rs, peri-tui/src/app/agent_comm.rs, peri-tui/src/prompt.rs
**CLAUDE.md 链接:** false

### issue_2026-05-14-test-separation-convention-debt

**摘要:** 89.8% 源文件内联测试违反规范，两轮分离后 152 个文件外部化
**状态:** Resolved
**归档日期:** 2026-05-15
**关键词:** 测试分离, include!, #[path], 模块可见性
**问题本质:** `#[path = "..._test.rs"]` 模式将测试模块定义到外部文件后，`use super::*;` 只能看到父模块的公有项（Rust 2018+ 同文件模块特殊规则失效），导致 async_trait、Arc 等私有 import 不可见。
**通用模式:** 外部测试文件有两种引用模式：(1) `#[path]` — 适用于纯公有 API 测试；(2) `include!` — 文本级包含，完全保留原始作用域语义，适用于需要访问私有导入的测试。选择标准：如果测试依赖父模块的私有 use 语句（如 async_trait、Arc、内部模块路径），用 `include!`；否则用 `#[path]`。
**涉及文件:** 152 个 *_test.rs 文件，152 个源文件
**CLAUDE.md 链接:** false

### issue_2026-05-17-middleware-heavy-files

**摘要:** peri-middlewares 下 3 个大文件拆分：subagent/tool.rs（1091 行→define/invoke/schema）、plugin/installer.rs（756 行→download/extract）、plugin/marketplace.rs（728 行→api/types）
**状态:** Fixed
**归档日期:** 2026-05-18
**涉及文件:** peri-middlewares/src/subagent/tool.rs, peri-middlewares/src/plugin/installer.rs, peri-middlewares/src/plugin/marketplace.rs
**说明:** 纯代码组织优化，无领域认知提炼。

### issue_2026-05-17-mod-rs-cohesion

**摘要:** 4 个 mod.rs 子模块数量超标，command/mod.rs 最严重（25 子模块）
**状态:** Fixed
**归档日期:** 2026-05-18
**关键词:** mod.rs, 内聚度, 分组目录, command
**问题本质:** command/mod.rs 25 子模块平铺，sync/mod.rs(13)、panels/mod.rs(11)、mcp/mod.rs(12) 接近阈值。子模块过多表明文件职责边界不清晰。
**通用模式:** 子模块超过 15 个时考虑按功能维度分组到子目录（core/panel/session），降低单文件认知负担。mod.rs 应仅作为路由入口 + 公共导出，不应承载领域逻辑。
**架构影响:** command/ 重组为 command/core/（基础命令）+ command/panel/（面板命令）+ command/session/（会话命令）三组。
**技术决策:** 优先处理 command/mod.rs；其余 3 个暂未突破严重阈值，等需求驱动时再调整。
**涉及文件:** peri-tui/src/command/mod.rs, peri-tui/src/sync/mod.rs, peri-tui/src/ui/main_ui/panels/mod.rs, peri-middlewares/src/mcp/mod.rs
**CLAUDE.md 链接:** false

### issue_2026-05-14-mega-functions-split

**摘要:** 超长函数拆分：event.rs（1120 行）和 agent_ops.rs（890 行）等 5 个超长单函数拆分
**状态:** Closed
**归档日期:** 2026-05-20
**关键词:** 大文件拆分, 模块化, 单函数过长, 认知复杂度
**问题本质:** 5 个巨型函数（总计 ~6791 行）各自承担 5-20 种职责，认知复杂度极高，修改时难以理解和测试。所有逻辑平铺在一个函数体内，无法独立测试任何事件分支。
**通用模式:** 按职责类型（键盘/鼠标/粘贴）拆分为独立 handler 函数；面板按类型垂直拆分为独立文件；LLM invoke 按阶段（构建请求/解析响应）提取子函数。拆分后函数自然定位到对应子模块，无需大型分发器。AOP 关注点（如 `request_rebuild()`）在拆分后会被各 handler 单独调用，需确保一致性。
**架构影响:** `run_universal_agent` 被 ACP 架构整体替代，event.rs 从 1447 行减为 event/ 子目录。这表明架构升级可以自然消灭超长函数——不需要手动拆分。
**涉及文件:** peri-tui/src/event.rs, peri-tui/src/app/agent_ops.rs, peri-tui/src/app/panel_ops.rs, peri-agent/src/llm/anthropic.rs, peri-tui/src/app/agent.rs
**CLAUDE.md 链接:** false

### issue_2026-05-14-tui-app-mod-decomposition

**摘要:** TUI app/ 目录模块化拆分——48 个子模块、多个 1000+ 行文件
**状态:** Resolved (基本完成)
**归档日期:** 2026-05-24
**关键词:** 模块化拆分, include!宏分组, plugin_panel拆分, agent_ops拆分
**问题本质:** app/mod.rs 单文件声明 48 个子模块，多个文件超 1000 行混合多职责
**通用模式:** 大型模块按 include! 宏分组（panels/state/agent/system）；子模块按职责拆分（handlers/component/ops/types）；panel API 面用 pub(crate) 收窄
**技术决策:** include! 宏分组优于手动声明（可按类别维护）；子模块拆分为 handlers/ 子目录优于单文件内部模块
**涉及文件:** peri-tui/src/app/mod.rs, peri-tui/src/app/modules_*.inc, peri-tui/src/app/plugin_panel/, peri-tui/src/app/agent_ops/, peri-tui/src/app/mcp_panel/, peri-tui/src/app/message_pipeline/
**CLAUDE.md 链接:** false

### issue_2026-05-25-mimalloc-worse-than-jemalloc
**摘要:** mimalloc 替换 jemalloc 后内存峰值反而更高，回退到系统默认分配器
**状态:** 已关闭
**归档日期:** 2026-05-26
**关键词:** mimalloc, jemalloc, RSS, global allocator
**问题本质:** mimalloc 在本项目工作负载（大量 Arc 克隆 + 临时 arena 分配）下内存膨胀速度比 jemalloc 更快，不符合"更积极归还"预期
**通用模式:** 全局分配器替换需基于实际工作负载基准测试，不能仅凭理论特性选择。Rust async + Arc 密集型应用的分配模式可能与通用基准差异显著。
**技术决策:** 最终采用系统默认分配器（macOS malloc / Linux glibc malloc），移除所有第三方全局分配器。/heapdump 命令随 mimalloc 一起移除。
**涉及文件:** Cargo.toml, peri-tui/Cargo.toml, peri-tui/src/main.rs, peri-tui/src/app/thread_ops.rs, peri-tui/src/command/core/heapdump.rs
**CLAUDE.md 链接:** false

### issue_2026-05-30-retry-mimalloc-with-mi-options

**摘要:** 重新引入 mimalloc 作为全局分配器（带 MI_OPTION 调参）
**状态:** Fixed
**归档日期:** 2026-05-31
**关键词:** mimalloc, 分配器, MI_OPTION, RSS 增长, 内存管理
**问题本质:** 未调参的 mimalloc 测试结论不可靠，重新评估需配合 MI_OPTION（PAGE_RESET/DECOMMIT/BACKGROUND_THREAD）
**通用模式:** 评估第三方库/工具时必须考虑配置调优，未调优的测试结论不可迁移；全局分配器切换需配合环境调优
**技术决策:** 分配器选择需结合实际负载特征（AgentPool 已减少瞬态分配），单独调参可能不足以解决根本问题
**涉及文件:** Cargo.toml, peri-tui/Cargo.toml, peri-tui/src/main.rs, peri-tui/src/app/thread_ops.rs
**CLAUDE.md 链接:** false

---

## 相关 Feature

- → [tui.md](./tui.md) — TUI App 结构体变更
