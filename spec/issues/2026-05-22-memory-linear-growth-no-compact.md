# 长对话内存持续增长，无自动释放机制

**状态**：Open
**优先级**：高
**类型**：性能
**创建日期**：2026-05-22

## 问题描述

Agent 对话过程中，内存（RSS）随对话轮数线性增长，每轮约增长 40 MB，且不会自动下降。持续跑 50-100 轮对话后可达数 GB，最终导致 OOM。**debug 和 release 模式下均表现相同**：`/clear` 后 RSS 不会下降。**根因为双重问题**（详见现象 6 修正）：jemalloc arena 碎片化（~17 MB/轮）+ 非 jemalloc 运行时基础设施持有（~20 MB/轮，tokio/hyper/reqwest/rustls），而非单纯的数据��构泄漏。**注**：`background_thread` 在 macOS 上不可用（依赖 pthread），已从配置中移除（2026-05-24）。

## 症状详情

| 维度 | 观察 |
|------|------|
| 增长模式 | 对话轮数相关，非时间相关 |
| 增长速度 | ~几十 MB/轮 |
| 是否自动下降 | 否，只增不减 |
| 触发场景 | 各类操作均有（SubAgent/大文件读取/纯文本） |
| 手动缓解 | `/clear` (new_thread) **无法释放**（debug/release 均如此） |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI，正常对话
  2. 每发一轮消息，观察 RSS 增长
  3. 持续对话数轮后，RSS 持续上升
  4. `/clear` 后 RSS 不下降
- **环境**：macOS，Rust 2021，任何模型下均出现
- **诊断工具**：`/heapdump` 命令（已集成，输出 `.tmp/heapdump-*.txt`）

### 现象 2（2026-05-23）：debug 模式下 `/clear` 后 RSS 不下降

| 维度 | 观察 |
|------|------|
| 编译模式 | debug（`./dev.sh` 启动） |
| `/clear` 前 RSS | 几百 MB |
| `/clear` 后 RSS | 无明显变化，仍在几百 MB |
| 与 release 对比 | 未对比，待确认 release 下 `/clear` 是否能正常释放 |

**推测**：debug 模式下无优化，Rust 全局分配器（jemalloc/system allocator）倾向于保留已释放的内存页不归还 OS，导致 RSS 数值不降。~~需对比 release 模式确认是否为 debug 专属现象~~。**已确认 release 也有同样问题**（见现象 3），推测已推翻。

### 现象 3（2026-05-23）：release 模式下 `/clear` 后 RSS 也不下降

| 维度 | 观察 |
|------|------|
| 编译模式 | release（`--release` 构建） |
| 增长速度 | 比 debug 慢，但仍然持续线性增长 |
| `/clear` 后 RSS | 无效果，不下降 |
| 测量方式 | 内部内存记录工具 |

**意义**：此前推测"debug 模式分配器不归还内存"已被推翻——release 下 `/clear` 同样无法释放。~~初步推测为数据结构引用未释放~~ → 现象 5 确认为 **jemalloc 分配器碎片化**：高 churn 场景下 dirty pages 积累速度 > decay purge 速度，与数据生命周期无关。优先级从「中」提升至「高」。

### 现象 4（2026-05-23）：jemalloc profiling 定量分析

使用 `/heapdump` 对一轮典型对话前后进行对比（debug 模式，macOS）：

| 指标 | 对话前 | 对话后 | 增长 |
|------|--------|--------|------|
| **RSS** | 54.4 MB | 93.1 MB | **+38.7 MB** |
| jemalloc allocated | 11.1 MB | 23.4 MB | +12.3 MB |
| jemalloc active | 17.5 MB | 37.2 MB | +19.7 MB |
| jemalloc resident | 24.8 MB | 51.8 MB | +27.0 MB |
| jemalloc mapped | 68.8 MB | 95.5 MB | +26.7 MB |
| huge allocations | 0 | 0 | 0 |
| non_arena (mapped-active) | 51.3 MB | 58.4 MB | +7.1 MB |
| RSS - resident（非 jemalloc） | 29.6 MB | 41.4 MB | **+11.8 MB** |

**TUI 组件数据**（/clear 后采样）：agent_state_messages=0, pipeline_completed=0, view_messages=0 — TUI 前端已完全释放。

**jemalloc 分配统计**：

| 指标 | 增长 |
|------|------|
| small malloc 次数 | **+786,935**（80 万次小对象分配/轮） |
| large malloc 次数 | +294 |
| 768KB large class 存活数 | 0 → 6（**4.5 MB**，推测为 LLM streaming response body buffer） |
| arena dirty pages | 1.2 MB → 9.0 MB（+7.8 MB，已 free 未 purge） |

**初步泄漏源定位**（现象 4，后被现象 5 修正）：

1. **arena dirty pages（+7.8 MB）**：jemalloc 已释放但未 purge 的 page。`dirty_decay_ms=1000` 配置已确认写入成功，但 decay 在 macOS 上效果有限
2. ~~**arena live objects（+12.3 MB allocated）**：Rust 堆上的活跃对象。`/clear` 后 TUI 前端数据归零，但这些对象在 ACP Server / Agent Executor 侧仍被持有~~ → 现象 5 推翻：allocated 不增长（9.5→9.0 MB），executor 不是泄漏源
3. ~~**非 jemalloc 内存（+11.8 MB RSS-resident）**：tokio runtime stack / reqwest TLS buffer / HTTP body buffer，不受 jemalloc 管理~~ → 现象 5 推翻：非 jemalloc 反而减少 2.8 MB

### 现象 5（2026-05-23）：第二轮 heapdump 对比——根因修正

使用 `/heapdump` 对一轮对话前后进行对比（release 模式，macOS）：

| 指标 | 空会话（21:22:24） | 5 tool calls 后（21:27:21） | 增长 |
|------|--------|--------|------|
| **RSS** | 52.9 MB | 94.8 MB | **+41.9 MB** |
| jemalloc allocated | 9.5 MB | 9.0 MB | **-0.5 MB** |
| jemalloc active | 15.8 MB | 29.4 MB | +13.6 MB |
| jemalloc resident | 23.3 MB | 68.0 MB | +44.7 MB |
| jemalloc mapped | 67.3 MB | 204.5 MB | **+137.2 MB** |
| huge allocations | 0 | 0 | 0 |
| dirty extents | ~1.5 MB | ~27.1 MB | **+25.6 MB** |
| non_arena (mapped-active) | 51.5 MB | 175.2 MB | +123.7 MB |
| RSS-resident（非 jemalloc） | 29.6 MB | 26.8 MB | **-2.8 MB** |
| tcache_bytes | 7.2 MB | 5.7 MB | -1.5 MB |

**Session 数据**：agent_state_messages=8 / 0.0MB, tool_calls=5, tokens_in=90698。

**分配统计**：

| 指标 | 值 |
|------|-----|
| total mallocs | 700,782 |
| total frees | 681,795 |
| **free/malloc 比** | **97.3%** |
| net live allocs | +853（几乎不变） |
| decay madvises | 5,025 |
| decay purged pages | 16,825（≈65 MB 已 purge，但不够） |

**关键发现——与现象 4 的根因假设矛盾**：

1. **allocated 不增长**（9.5 → 9.0 MB）：Rust 堆活跃对象未增长，现象 4 中 "arena live objects +12.3 MB" 在本轮未复现
2. **Session 数据极小**（0.0 MB）：ACP executor / State.messages 不是泄漏源
3. **非 jemalloc 内存反而减少**（29.6 → 26.8 MB）：tokio/reqwest 没有持续积累
4. **增长集中在 jemalloc 分配器碎片化**：active +13.6 MB, resident +44.7 MB, mapped +137.2 MB
5. **97.3% 的分配是瞬态的**：68 万次 malloc 中绝大部分已被 free，但导致 arena 页面碎片化
6. **jemalloc 在 purge 但跟不上**：16,825 页已 purged（≈65 MB），dirty extents 仍积压 27 MB

### 现象 6（2026-05-23）：P0 修复后 `/clear` 仍不降——jemalloc 调优效果有限，非 jemalloc 内存是主要 RSS 来源

`configure_jemalloc()` 已合入并生效（`dirty_decay_ms: 200`），但 `/clear` 后 heapdump 仍然显示高 RSS。

**heapdump 采样**（`/clear` 后，release 模式）：

```
RSS:                    81.8 MB
jemalloc allocated:      9.3 MB   ← 实际在用极少
jemalloc active:        26.8 MB
jemalloc resident:      38.4 MB
jemalloc mapped:       116.2 MB
background_thread:    false      ← 未生效！
dirty_decay_ms:         200      ← 已生效
non_arena:             89.4 MB   ← mapped - active
```

**TUI 组件数据**：agent_state_messages=1/0.0MB, pipeline_completed=1/0.0MB, view_messages=2 — 数据已清空。

**RSS 构成分解（81.8 MB）**：

| 来源 | 大小 | 说明 |
|------|------|------|
| arena allocated（实际在用） | 9.3 MB | Rust 堆活跃对象 |
| arena active - allocated（碎片化空闲） | 17.5 MB | slabs ��留的未使用页面 |
| arena metadata（base + internal） | ~7.6 MB | jemalloc 元数据 |
| tcache | 4.4 MB | 线程本地缓存 |
| **非 jemalloc（RSS - jemalloc resident）** | **43.4 MB** | tokio/hyper/reqwest/rustls |

**关键发现——P0 修复效果有限的原因**：

1. **`background_thread: false`**：~~`configure_jemalloc()` 中 `raw::write("background_thread", true)` **写入失败**~~ **macOS 不支持 `background_thread`**：该功能依赖 pthread API，macOS 线程模型不兼容，jemalloc 启动时打印 `option background_thread currently supports pthread only` 并忽略。无论是 `_rjem_malloc_conf` 全局符号还是运行时 `raw::write`，在 macOS 上均无效。Linux 上可能有效但未验证
2. **非 jemalloc 内存（43.4 MB）才是 RSS 的主要组成**：tokio runtime 线程栈（默认 8MB×N threads）、hyper/reqwest HTTP 连接池、rustls TLS session 缓冲区、tokio 任务缓冲区。这些不受 jemalloc 管理，`/clear` 不会释放，`jemalloc_decay()` 也无法触及
3. **arena 碎片化（17.5 MB）是次要来源**：`dirty_decay_ms: 200` 已生效，但 macOS 上无 `background_thread`（不支持 pthread），前台 decay 在空闲时仍不够积极
4. **mapped 虚拟地址空间（116.2 MB）膨胀**：jemalloc 地址空间保留，macOS 上 munmap 策略保守，decommit 后虚拟地址空间仍保留。不影响 RSS 但说明 jemalloc 管理开销大

**与之前根因分析的修正**：

现象 5 将根因归为 "jemalloc 分配器碎片化"。现象 6 的数据表明这是**双重问题**：

- **arena 碎片化（17.5 MB）**：可通过 jemalloc 调优缓解（`dirty_decay_ms: 200` + 主动 `jemalloc_decay()`）。`background_thread` 在 macOS 上不可用（依赖 pthread）
- **非 jemalloc 运行时持有（43.4 MB）**：这是 tokio/hyper/reqwest 的**正常基础设施开销**，不是泄漏，但 `/clear` 无法回收。系统分配器（非 jemalloc）测试也表现出同样行为，进一步确认这不是分配器层面的问题

**每轮增长 ~40 MB 的实际组成**（结合现象 5 + 6）：

```
RSS 增长/轮 (~40 MB)
├── jemalloc arena 碎片化 (~17 MB)       ← 可缓解：dirty_decay_ms + jemalloc_decay()
│   └── active slabs 碎片 + dirty pages 积压
├── 非 jemalloc 运行时增长 (~20 MB)       ← 难优化：tokio/hyper/reqwest 基础设施
│   ├── reqwest HTTP 连接池 + TLS session 缓存
│   ├── tokio runtime 任务缓冲区增长
│   └── hyper 响应体缓冲区
└── jemalloc metadata/base 增长 (~3 MB)   ← 不可控
```

## 根因分析

### 泄漏层级（现象 5 原版，现象 6 有修正）

```
RSS 增长 (+41.9 MB)
├── jemalloc resident (+44.7 MB)           ← 现象 5 定位的主要泄漏源
│   ├── dirty extents (+25.6 MB)           ← 已 free 未 purge 的 arena 页面
│   │   └── 高分配 churn（68万次/轮）导致 dirty pages 积累速度 > decay purge 速度
│   ├── active pages (+13.6 MB)            ← arena slabs 保留的页面（碎片化）
│   └── metadata (+5.2 MB)                 ← base + internal 增长
├── 非 jemalloc (-2.8 MB)                  ← 现象 5 中实际下降
│   └── tokio/reqwest 在 prompt 间隙自然释放
└── mapped 虚拟内存膨胀 (+137.2 MB)        ← jemalloc 地址空间保留，非 RSS 贡献
    └── macOS 上 munmap 策略保守，已 decommit 的 extent 仍占虚拟地址空间
```

**现象 6 修正**：上述层级是执行过程中的瞬时快照。`/clear` 后 jemalloc resident 降到 38.4 MB（arena 碎片化仍占 17.5 MB），但**非 jemalloc 内存 43.4 MB 成为 RSS 主要组成**（此时 RSS 81.8 MB = jemalloc 38.4 + 非 jemalloc 43.4）。两个来源的贡献在不同阶段此消彼长，但总量持续增长。

### `/clear` 后不释放的原因（现象 6 最终修正）

现象 4 原假设：ACP executor 数据被引用钉住 → **推翻**（allocated 不增长）
现象 5 修正：jemalloc 分配器碎片化 → **部分正确但不完整**

**最终根因是双重问题**：

1. **jemalloc arena 碎片化（17.5 MB）**：每轮 68 万次瞬态分配造成 arena slabs 碎片化。macOS 不支持 `background_thread`（依赖 pthread），只能依赖 `dirty_decay_ms: 200` + 主动 `jemalloc_decay()` 调用进行 purge
2. **非 jemalloc 运行时持有（43.4 MB）**：tokio runtime 线程栈、hyper/reqwest HTTP 连接池、rustls TLS session 缓冲区。这些是基础设施的正常持有，`/clear` 不释放。**系统分配器对照实验确认这不是 jemalloc 独有问题**

### 现象 4 与现象 5 的差异说明

现象 4 中 allocated +12.3 MB 可能因以下原因未在现象 5 复现：
- 现象 4 为 debug 模式（对象布局更大），现象 5 为 release 模式
- 对话内容复杂度不同（现象 4 未记录 tool_calls 数量）
- 现象 4 的 `/clear` 后采样时机可能在 executor drop 前执行

无论哪种场景，**RSS 增长 ~40 MB/轮**的结论一致。

## 修复方向

### P0：分配器调优（部分已实施，background_thread 不可用）

1. ~~**`dirty_decay_ms` 降至 100-200ms**~~：✅ 已实施，设为 200ms（heapdump 确认生效）
2. ~~**修复 `background_thread` 设置失败**~~：❌ **不可修复**。`background_thread` 依赖 pthread API，macOS 不支持。jemalloc 启动时打印 `<jemalloc>: option background_thread currently supports pthread only` 并忽略该选项。已从 `_rjem_malloc_conf` 全局符号和 `configure_jemalloc()` 运行时写入中移除 `background_thread:true`，消除启动警告。macOS 上 arena purge 只能依赖 `dirty_decay_ms` + 主动 `jemalloc_decay()` 调用
3. ~~**限制 tcache 大小**~~：✅ 已实施，`lg_tcache_max=16`（tcache_bytes 从 7.2 MB 降至 4.4 MB）

### P0（新）：降低非 jemalloc 运行时开销

4. **限制 reqwest 连接池**：检查 `ClientBuilder` 的 `pool_max_idle_per_host` 和 `pool_idle_timeout`，默认无限制的连接池会持有 TLS session。建议 `pool_max_idle_per_host(2)` + `pool_idle_timeout(30s)`
5. **减小 tokio 线程栈**：默认 8MB/线程，如果有 8 个 worker threads 就是 64 MB 纯栈开销。检查是否可用 `thread_stack_size(4*1024*1024)` 减半
6. **审计 hyper 响应体缓冲区**：LLM streaming response 的 `Bytes` 是否在 response 完成后及时释放

### P1：减少每轮分配 churn（治本）

7. **消除 serde JSON 双重解析**：`run_pump` 中 `serde_json::from_value(event_value.clone())` 先 clone 再反序列化，改为零拷贝解析
8. **减少 String clone**：68 万次 malloc 中大量是字符串克隆（event 序列化/反序列化路径），审计 `AcpNotification::AgentEvent` 构造路径中的 clone
9. **LLM response body buffer 复用**：考虑用 `Bytes` pool 或复用已有 buffer，减少 large class 分配

### P2：ACP executor 生命周期管理（降级）

10. **验证 executor spawn 闭包释放**：现象 5/6 数据表明 executor 数据被正确释放（allocated 不增长），但仍需验证长时间运行场景
11. **bounded notification channel**：`AcpTuiClient` 的 `unbounded_channel` 改为 `channel(256)`，防止极端场景下的无限积压

### P3：备选方案

12. **系统分配器对照实验**：✅ 已测试，系统分配器同样出现 RSS 不降问题。排除分配器独有因素
13. **考虑 `mimalloc` 替代 jemalloc**：mimalloc 在碎片化场景下表现可能更好，且与系统分配器行为一致（直接 mmap/munmap），但需 benchmark 验证
14. **考虑定期重启策略**：对于长时间运行的 TUI 会话，在 N 轮对话后提示用户重启或自动重置 runtime

### 诊断工具

- **`/heapdump`** 已集成（`peri-tui/src/command/core/heapdump.rs`），输出 jemalloc 完整统计 + TUI 组件大小到 `.tmp/heapdump-*.txt`
- **`tikv-jemalloc-ctl`** 已启用 `stats` + `use_std` features

## 涉及文件

- `peri-tui/src/acp_server/mod.rs` —— ACP 服务器端 SessionState.history
- `peri-tui/src/app/agent_comm.rs` —— TUI 端 agent_state_messages
- `peri-tui/src/app/agent_submit.rs` —— submit_message 流程
- `peri-tui/src/app/thread_ops.rs` —— new_thread（/clear）释放逻辑 + `jemalloc_decay()` arena purge
- `peri-tui/src/acp_server/prompt.rs` —— 每轮执行后 state.history 更新
- `peri-tui/src/acp_client/client.rs` —— notification pump（`event_value.clone()` → `mem::take` 优化）
- `peri-acp/src/session/executor.rs` —— execute_prompt 内 event channel + spawn 闭包生命周期
- `peri-acp/src/session/event_sink.rs` —— event 序列化（`to_string()` → `into()` 优化）
- `peri-tui/src/command/core/heapdump.rs` —— `/heapdump` 诊断命令
- `peri-tui/src/main.rs` —— `configure_jemalloc()` 分配器调优入口
- `peri-tui/src/jemalloc_config.rs` —— jemalloc ���置模块（`background_thread` 已移除：macOS 不支持）
