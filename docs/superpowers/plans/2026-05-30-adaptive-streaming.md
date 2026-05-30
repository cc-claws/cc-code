# 自适应流式帧率 (Adaptive Streaming Frame Rate) 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `MessagePipeline::check_throttle()` 从固定 100ms 节流策略替换为自适应分块策略（`AdaptiveChunkingPolicy`），在 Smooth 模式（逐行提交）和 CatchUp 模式（批量排空）之间动态切换，在高速流式输出时降低延迟，低速时减少无效重绘。

**Architecture:** 当前 `check_throttle()` 使用 `throttle_armed: bool` + `throttle_last_fire: Option<Instant>` 做固定 100ms 窗口节流。新策略引入 `AdaptiveChunkingPolicy` 结构体，跟踪队列深度（累积未消费的 chunk 行数）和最老行年龄（首次 chunk 到达至今的时间），根据阈值在 Smooth/CatchUp 两种模式间切换，返回 `DrainPlan`（Single 或 Batch）控制每次消费量。

**Tech Stack:** Rust, std::time::Instant/Duration

---

## 当前问题

### 固定 100ms 节流（message_pipeline/mod.rs:701-716）

```rust
pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
    if !self.throttle_armed { return None; }
    let now = Instant::now();
    let should_fire = match self.throttle_last_fire {
        None => true,
        Some(last) => now.duration_since(last) >= Duration::from_millis(100),
    };
    if should_fire {
        self.throttle_last_fire = Some(now);
        self.throttle_armed = false;
        return Some(self.build_rebuild_all(prefix_len));
    }
    None
}
```

**问题**：
1. 高速输出（>50 token/s）：队列积压，显示落后实际输出 1-2 秒
2. 低速输出（<10 token/s）：每 100ms 重绘一次，多数帧无新内容
3. 队列深度突增：仍以 100ms 间隔逐个消费，无法快速收敛

## File Structure

| 文件 | 操作 | 职责变更 |
|------|------|----------|
| `peri-tui/src/app/message_pipeline/mod.rs` | 修改 | 引入 `AdaptiveChunkingPolicy`，替换 `throttle_armed`/`throttle_last_fire`，重写 `check_throttle()` |
| `peri-tui/src/app/message_pipeline/message_pipeline_test.rs` | 修改 | 新增自适应策略单元测试 |

---

## Task 1: 定义 AdaptiveChunkingPolicy 和 DrainPlan 类型

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

- [ ] **Step 1: 在 `mod.rs` 顶部（`use` 块后、`PendingTool` 定义前）添加新类型**

```rust
// ─── 自适应分块策略 ──────────────────────────────────────────────────────

/// 排空计划：控制每次 check_throttle 的消费量
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrainPlan {
    /// 正常模式：提交一行（单次 RebuildAll）
    Single,
    /// 积压模式：一次性排空所有积压行（单次 RebuildAll 含全部内容）
    Batch,
}

/// 自适应分块策略：根据队列压力在 Smooth/CatchUp 模式间动态切换。
///
/// Smooth 模式（默认）：每次 tick 提交一行，保证流畅感。
/// CatchUp 模式：队列积压时一次性排空，快速收敛显示。
///
/// 进入 CatchUp 条件（满足任一）：
/// - 队列深度 ≥ `queue_depth_threshold`（默认 8 行）
/// - 最老行年龄 ≥ `oldest_age_threshold`（默认 120ms）
///
/// 退出 CatchUp 条件（同时满足）：
/// - 队列深度 ≤ `exit_depth`（默认 2 行）
/// - 最老行年龄 ≤ `exit_age`（默认 40ms）
pub(crate) struct AdaptiveChunkingPolicy {
    /// 当前是否处于 CatchUp 模式
    mode: ChunkingMode,
    /// 累积的未消费行数（按换行符计）
    pending_lines: usize,
    /// 首个未消费 chunk 的到达时间（用于计算最老行年龄）
    oldest_chunk_at: Option<Instant>,
    /// 进入 CatchUp 的队列深度阈值
    queue_depth_threshold: usize,
    /// 进入 CatchUp 的最老行年龄阈值
    oldest_age_threshold: Duration,
    /// 退出 CatchUp 的队列深度阈值
    exit_depth: usize,
    /// 退出 CatchUp 的最老行年龄阈值
    exit_age: Duration,
}

/// 分块模式（内部状态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkingMode {
    /// 平滑模式：逐行提交
    Smooth,
    /// 追赶模式：批量排空
    CatchUp,
}

impl AdaptiveChunkingPolicy {
    /// 使用默认参数创建策略
    fn new() -> Self {
        Self {
            mode: ChunkingMode::Smooth,
            pending_lines: 0,
            oldest_chunk_at: None,
            queue_depth_threshold: 8,
            oldest_age_threshold: Duration::from_millis(120),
            exit_depth: 2,
            exit_age: Duration::from_millis(40),
        }
    }

    /// 通知策略有新的 chunk 到达。
    /// 按换行符统计行数，并记录首个 chunk 的时间戳。
    fn on_chunk(&mut self, chunk: &str) {
        let new_lines = chunk.lines().count().max(1);
        self.pending_lines += new_lines;
        if self.oldest_chunk_at.is_none() {
            self.oldest_chunk_at = Some(Instant::now());
        }
    }

    /// 通知策略有新的推理 chunk 到达（同样累积压力）
    fn on_reasoning_chunk(&mut self) {
        self.pending_lines += 1;
        if self.oldest_chunk_at.is_none() {
            self.oldest_chunk_at = Some(Instant::now());
        }
    }

    /// 检查当前是否应该触发重绘，若触发则返回 DrainPlan。
    ///
    /// 策略逻辑：
    /// - Smooth 模式：检查基础节流间隔（最小 16ms，约 60fps），满足则返回 Single
    /// - CatchUp 模式：立即返回 Batch，无节流间隔限制
    /// - 每次调用检查是否需要模式切换
    fn check(&mut self) -> Option<DrainPlan> {
        if self.pending_lines == 0 {
            return None;
        }

        self.update_mode();

        match self.mode {
            ChunkingMode::Smooth => {
                // Smooth 模式下仍需最小间隔防止 CPU 空转
                // 但不再固定 100ms——最小 16ms 保证 60fps 级别刷新率
                Some(DrainPlan::Single)
            }
            ChunkingMode::CatchUp => {
                // CatchUp 模式立即排空
                Some(DrainPlan::Batch)
            }
        }
    }

    /// 消费后排空积压计数
    fn drain(&mut self) {
        self.pending_lines = 0;
        self.oldest_chunk_at = None;
    }

    /// 重置策略状态（用于 done/interrupt/begin_round）
    fn reset(&mut self) {
        self.mode = ChunkingMode::Smooth;
        self.pending_lines = 0;
        self.oldest_chunk_at = None;
    }

    /// 根据队列深度和最老行年龄更新模式
    fn update_mode(&mut self) {
        let now = Instant::now();
        let oldest_age = self
            .oldest_chunk_at
            .map(|t| now.duration_since(t))
            .unwrap_or(Duration::ZERO);

        match self.mode {
            ChunkingMode::Smooth => {
                // 进入 CatchUp：满足任一条件
                if self.pending_lines >= self.queue_depth_threshold
                    || oldest_age >= self.oldest_age_threshold
                {
                    self.mode = ChunkingMode::CatchUp;
                }
            }
            ChunkingMode::CatchUp => {
                // 退出 CatchUp：同时满足两个条件
                if self.pending_lines <= self.exit_depth && oldest_age <= self.exit_age {
                    self.mode = ChunkingMode::Smooth;
                }
            }
        }
    }

    /// 当前是否处于 CatchUp 模式（诊断用）
    #[allow(dead_code)]
    fn is_catch_up(&self) -> bool {
        self.mode == ChunkingMode::CatchUp
    }
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -30`
Expected: 无错误（新类型尚未使用，不应产生编译错误）

- [ ] **Step 3: 提交**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat(tui): add AdaptiveChunkingPolicy and DrainPlan types for streaming throttle"
```

---

## Task 2: 编写 AdaptiveChunkingPolicy 单元测试（TDD）

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/message_pipeline_test.rs`

- [ ] **Step 1: 在测试文件末尾添加 AdaptiveChunkingPolicy 测试**

```rust
// ─── AdaptiveChunkingPolicy 测试 ───────────────────────────────────────

use super::{AdaptiveChunkingPolicy, ChunkingMode, DrainPlan};

/// 辅助：创建策略并用指定数量的行填充
fn make_policy_with_lines(line_count: usize) -> AdaptiveChunkingPolicy {
    let mut policy = AdaptiveChunkingPolicy::new();
    // 每行一个 chunk，模拟逐行到达
    for _ in 0..line_count {
        policy.on_chunk("hello\n");
    }
    policy
}

/// 测试：新策略初始状态为 Smooth，无积压，check 返回 None
#[test]
fn test_policy_initial_state() {
    let policy = AdaptiveChunkingPolicy::new();
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    assert_eq!(policy.pending_lines, 0);
    assert!(policy.oldest_chunk_at.is_none());
}

/// 测试：单个 chunk 正确累积行数
#[test]
fn test_policy_on_chunk_single_line() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("hello");
    assert_eq!(policy.pending_lines, 1);
    assert!(policy.oldest_chunk_at.is_some());
}

/// 测试：多行 chunk 正确累积行数
#[test]
fn test_policy_on_chunk_multi_line() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("line1\nline2\nline3");
    assert_eq!(policy.pending_lines, 3);
}

/// 测试：空 chunk 仍记为 1 行（min 1）
#[test]
fn test_policy_on_chunk_empty() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("");
    assert_eq!(policy.pending_lines, 1);
}

/// 测试：积压为 0 时 check 返回 None
#[test]
fn test_policy_check_no_pending() {
    let mut policy = AdaptiveChunkingPolicy::new();
    assert!(policy.check().is_none());
}

/// 测试：Smooth 模式下少量积压返回 Single
#[test]
fn test_policy_smooth_returns_single() {
    let mut policy = AdaptiveChunkingPolicy::new();
    // 低于 queue_depth_threshold(8)，仍在 Smooth
    policy.on_chunk("hello\n");
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Single));
}

/// 测试：队列深度达到阈值（8 行）触发 CatchUp
#[test]
fn test_policy_catchup_by_depth() {
    let mut policy = make_policy_with_lines(8);
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Batch));
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：队列深度超过阈值（10 行）也触发 CatchUp
#[test]
fn test_policy_catchup_by_depth_overflow() {
    let mut policy = make_policy_with_lines(10);
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Batch));
}

/// 测试：最老行年龄达到阈值（120ms）触发 CatchUp
#[test]
fn test_policy_catchup_by_age() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_chunk("hello\n");
    // 手动设置 oldest_chunk_at 为 150ms 前
    policy.oldest_chunk_at = Some(Instant::now() - Duration::from_millis(150));
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Batch));
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：队列深度 7 + 年龄 119ms → 不触发 CatchUp（两个条件都未达阈值）
#[test]
fn test_policy_no_catchup_below_threshold() {
    let mut policy = AdaptiveChunkingPolicy::new();
    // 7 行，低于阈值 8
    for _ in 0..7 {
        policy.on_chunk("hello\n");
    }
    // 年龄 119ms，低于阈值 120ms
    policy.oldest_chunk_at = Some(Instant::now() - Duration::from_millis(119));
    let plan = policy.check();
    assert_eq!(plan, Some(DrainPlan::Single));
    assert_eq!(policy.mode, ChunkingMode::Smooth);
}

/// 测试：CatchUp 模式下 drain 后队列清空，但模式保持 CatchUp
/// （需要同时满足 exit_depth 和 exit_age 才退出）
#[test]
fn test_policy_catchup_drain_stays_catchup() {
    let mut policy = make_policy_with_lines(10);
    // 触发 CatchUp
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后队列为空
    policy.drain();
    assert_eq!(policy.pending_lines, 0);
    // 下一个 check：无积压，返回 None
    assert!(policy.check().is_none());
}

/// 测试：CatchUp → Smooth 退出条件：队列深度 ≤ 2 且年龄 ≤ 40ms
#[test]
fn test_policy_exit_catchup() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后重新填充少量数据（2 行，年龄 < 40ms）
    policy.drain();
    policy.on_chunk("a\n");
    policy.on_chunk("b\n");
    let plan = policy.check();
    // 退出条件满足：depth=2 ≤ 2 且 age ≈ 0ms ≤ 40ms
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    assert_eq!(plan, Some(DrainPlan::Single));
}

/// 测试：CatchUp 不退出：深度 ≤ 2 但年龄 > 40ms
#[test]
fn test_policy_no_exit_age_too_old() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后重新填充 2 行，但设置年龄为 50ms（> 40ms）
    policy.drain();
    policy.on_chunk("a\n");
    policy.on_chunk("b\n");
    policy.oldest_chunk_at = Some(Instant::now() - Duration::from_millis(50));
    let _ = policy.check();
    // 不满足退出：年龄 50ms > 40ms
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：CatchUp 不退出：年龄 ≤ 40ms 但深度 > 2
#[test]
fn test_policy_no_exit_depth_too_high() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    // drain 后重新填充 3 行（> exit_depth=2），年龄约 0ms
    policy.drain();
    for _ in 0..3 {
        policy.on_chunk("a\n");
    }
    let _ = policy.check();
    // 不满足退出：depth=3 > 2
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
}

/// 测试：reset 恢复到初始状态
#[test]
fn test_policy_reset() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    policy.reset();
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    assert_eq!(policy.pending_lines, 0);
    assert!(policy.oldest_chunk_at.is_none());
}

/// 测试：drain 只清空积压，不改变模式
#[test]
fn test_policy_drain_preserves_mode() {
    let mut policy = make_policy_with_lines(10);
    let _ = policy.check();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    policy.drain();
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    assert_eq!(policy.pending_lines, 0);
}

/// 测试：on_reasoning_chunk 正确累积
#[test]
fn test_policy_on_reasoning_chunk() {
    let mut policy = AdaptiveChunkingPolicy::new();
    policy.on_reasoning_chunk();
    assert_eq!(policy.pending_lines, 1);
    assert!(policy.oldest_chunk_at.is_some());
}

/// 测试：连续多轮 chunk → check → drain → chunk 的完整生命周期
#[test]
fn test_policy_lifecycle_smooth_to_catchup_and_back() {
    let mut policy = AdaptiveChunkingPolicy::new();

    // 第一轮：Smooth，3 行
    for _ in 0..3 {
        policy.on_chunk("hello\n");
    }
    assert_eq!(policy.check(), Some(DrainPlan::Single));
    assert_eq!(policy.mode, ChunkingMode::Smooth);
    policy.drain();

    // 第二轮：积压爆发，12 行 → CatchUp
    for _ in 0..12 {
        policy.on_chunk("burst\n");
    }
    assert_eq!(policy.check(), Some(DrainPlan::Batch));
    assert_eq!(policy.mode, ChunkingMode::CatchUp);
    policy.drain();

    // 第三轮：恢复正常，1 行 → 应回到 Smooth
    policy.on_chunk("normal\n");
    // drain 后 oldest_chunk_at 已清空，重新填充后年龄约 0ms
    // depth=1 ≤ 2 且 age ≈ 0 ≤ 40ms → 退出 CatchUp
    assert_eq!(policy.check(), Some(DrainPlan::Single));
    assert_eq!(policy.mode, ChunkingMode::Smooth);
}
```

注意：测试中 `ChunkingMode` 是私有枚举。为了让测试访问，需要在 `mod.rs` 中将 `ChunkingMode` 的字段可见性调整为 `pub(crate)` 或使用 `pub(crate)` 标注。测试直接在子模块中，可以访问 `pub(crate)` 成员。

- [ ] **Step 2: 确认 ChunkingMode 可被测试访问**

在 `mod.rs` 中 `ChunkingMode` 枚举定义的每个变体字段前确认是 `pub(crate)` 级别（测试文件通过 `#[path]` 在同一模块内，可访问 `pub(crate)`）。

实际上，`ChunkingMode` 的 `mode` 字段在 `AdaptiveChunkingPolicy` 上需要暴露给测试。最简方案是在测试中通过 `policy.mode` 直接比较。由于测试文件通过 `#[path = "..."]` 在 `mod tests` 中，它属于子模块，可以访问父模块的 `pub(crate)` 项。

需将 `AdaptiveChunkingPolicy` 的 `mode` 和 `pending_lines` 字段改为 `pub(crate)`：

```rust
pub(crate) struct AdaptiveChunkingPolicy {
    pub(crate) mode: ChunkingMode,
    pub(crate) pending_lines: usize,
    pub(crate) oldest_chunk_at: Option<Instant>,
    // ... 其他字段保持私有
}
```

- [ ] **Step 3: 运行测试**

Run: `cargo test -p peri-tui -- adaptive_chunking 2>&1 | tail -30`
Expected: 所有新增测试通过

- [ ] **Step 4: 提交**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs peri-tui/src/app/message_pipeline/message_pipeline_test.rs
git commit -m "test(tui): add AdaptiveChunkingPolicy unit tests (TDD)"
```

---

## Task 3: 替换 MessagePipeline 中的固定节流逻辑

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/mod.rs`

- [ ] **Step 1: 在 `MessagePipeline` 结构体中替换节流字段**

将当前的：
```rust
// ── 节流状态 ──
/// 是否有待发射的节流 RebuildAll（有流式 chunk 积累但尚未发射）
throttle_armed: bool,
/// 上次节流发射的时间
throttle_last_fire: Option<Instant>,
```

替换为：
```rust
// ── 节流状态 ──
/// 自适应分块策略（替代固定 100ms 节流）
adaptive_policy: AdaptiveChunkingPolicy,
/// 上次节流发射的时间（Smooth 模式下的最小间隔守卫）
throttle_last_fire: Option<Instant>,
```

- [ ] **Step 2: 更新 `MessagePipeline::new()` 构造函数**

将：
```rust
throttle_armed: false,
throttle_last_fire: None,
```

替换为：
```rust
adaptive_policy: AdaptiveChunkingPolicy::new(),
throttle_last_fire: None,
```

- [ ] **Step 3: 更新 `push_chunk()` 方法，通知策略有新 chunk**

当前：
```rust
pub fn push_chunk(&mut self, chunk: &str) {
    self.current_ai_text.push_str(chunk);
}
```

替换为：
```rust
pub fn push_chunk(&mut self, chunk: &str) {
    self.current_ai_text.push_str(chunk);
    self.adaptive_policy.on_chunk(chunk);
}
```

- [ ] **Step 4: 更新 `push_reasoning()` 方法，通知策略有新推理 chunk**

当前：
```rust
pub fn push_reasoning(&mut self, text: &str) {
    self.current_ai_reasoning.push_str(text);
}
```

替换为：
```rust
pub fn push_reasoning(&mut self, text: &str) {
    self.current_ai_reasoning.push_str(text);
    self.adaptive_policy.on_reasoning_chunk();
}
```

- [ ] **Step 5: 重写 `check_throttle()` 方法**

当前：
```rust
/// 检查节流计时器，若 100ms 已过则发射 RebuildAll。
/// 由 poll_agent() 每帧调用。
pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
    if !self.throttle_armed {
        return None;
    }
    let now = Instant::now();
    let should_fire = match self.throttle_last_fire {
        None => true,
        Some(last) => now.duration_since(last) >= Duration::from_millis(100),
    };
    if should_fire {
        self.throttle_last_fire = Some(now);
        self.throttle_armed = false;
        return Some(self.build_rebuild_all(prefix_len));
    }
    None
}
```

替换为：
```rust
/// 检查自适应节流策略，根据队列压力决定是否发射 RebuildAll。
///
/// 策略：
/// - Smooth 模式：最小 16ms 间隔（~60fps），返回 Single（单次 RebuildAll）
/// - CatchUp 模式：无间隔限制，立即排空，返回 Batch（单次 RebuildAll 含全部内容）
///
/// 由 poll_agent() 每帧调用。
pub fn check_throttle(&mut self, prefix_len: usize) -> Option<PipelineAction> {
    let plan = self.adaptive_policy.check()?;

    match plan {
        DrainPlan::Single => {
            // Smooth 模式：应用最小间隔守卫，防止 CPU 空转
            let now = Instant::now();
            let min_interval = Duration::from_millis(16);
            let should_fire = match self.throttle_last_fire {
                None => true,
                Some(last) => now.duration_since(last) >= min_interval,
            };
            if !should_fire {
                return None;
            }
            self.throttle_last_fire = Some(now);
            self.adaptive_policy.drain();
            Some(self.build_rebuild_all(prefix_len))
        }
        DrainPlan::Batch => {
            // CatchUp 模式：立即排空，不受间隔限制
            self.throttle_last_fire = Some(Instant::now());
            self.adaptive_policy.drain();
            Some(self.build_rebuild_all(prefix_len))
        }
    }
}
```

- [ ] **Step 6: 更新 `done()` 和 `interrupt()` 方法**

在 `done()` 中，将 `self.throttle_armed = false;` 替换为 `self.adaptive_policy.reset();`：

```rust
pub fn done(&mut self) {
    self.finalize_current_ai();
    self.current_ai_finalized = false;
    self.pending_tools.clear();
    self.completed_tools.clear();
    self.adaptive_policy.reset();  // 替换 self.throttle_armed = false;
    self.throttle_last_fire = None;
    self.active_batch = None;
    self.drain_subagent_stack();
}
```

同样更新 `interrupt()`：
```rust
pub fn interrupt(&mut self) {
    self.finalize_current_ai();
    self.current_ai_finalized = false;
    self.pending_tools.clear();
    self.completed_tools.clear();
    self.adaptive_policy.reset();  // 替换 self.throttle_armed = false;
    self.throttle_last_fire = None;
    self.active_batch = None;
    self.drain_subagent_stack();
}
```

- [ ] **Step 7: 更新 `begin_round()` 方法**

将 `self.throttle_armed = false;` 替换为 `self.adaptive_policy.reset();`：

```rust
pub fn begin_round(&mut self) {
    self.completed_len_at_round_start = self.completed.len();
    self.has_snapshot_this_round = false;
    self.adaptive_policy.reset();  // 替换 self.throttle_armed = false;
    self.throttle_last_fire = None;
    self.frozen_subagent_vms.clear();
}
```

- [ ] **Step 8: 更新 `handle_event()` 中的 chunk 事件处理**

当前 `AssistantChunk` 分支中有 `self.throttle_armed = true;`。需要移除这些行，因为现在 `push_chunk()` 和 `push_reasoning()` 内部已自动调用 `adaptive_policy.on_chunk()` / `on_reasoning_chunk()`。

在 `handle_event` 的 `AssistantChunk` 分支中，移除 `self.throttle_armed = true;`：
```rust
AgentEvent::AssistantChunk { chunk, source_agent_id } => {
    if !chunk.is_empty() {
        if let Some(ref aid) = source_agent_id {
            if let Some(sub) = self.find_running_subagent_mut(aid) {
                Self::push_chunk_to_subagent(sub, &chunk);
                self.adaptive_policy.on_chunk(&chunk);
            }
        } else if self.in_subagent() {
            if let Some(sub) = self.subagent_stack.last_mut() {
                Self::push_chunk_to_subagent(sub, &chunk);
                self.adaptive_policy.on_chunk(&chunk);
            }
        } else {
            self.push_chunk(&chunk);
            // push_chunk 内部已调用 adaptive_policy.on_chunk()
        }
    }
    vec![PipelineAction::None]
}
```

注意：当 chunk 被路由到 SubAgent 时（`push_chunk_to_subagent`），需要在路由后手动调用 `self.adaptive_policy.on_chunk(&chunk)` 以正确跟踪队列压力。

同样更新 `AiReasoning` 分支，移除 `self.throttle_armed = true;`：
```rust
AgentEvent::AiReasoning(text) => {
    if self.in_subagent() {
        if let Some(_sub) = self.subagent_stack.last_mut() {
            self.adaptive_policy.on_reasoning_chunk();
        }
    } else {
        self.push_reasoning(&text);
        // push_reasoning 内部已调用 adaptive_policy.on_reasoning_chunk()
    }
    vec![PipelineAction::None]
}
```

更新 `ToolStart` 和 `ToolEnd` 分支中的 `self.throttle_armed = false;` 替换为 `self.adaptive_policy.drain();`：

```rust
// ToolStart 分支开头：
self.adaptive_policy.drain();  // 替换 self.throttle_armed = false;

// ToolEnd 分支开头：
self.adaptive_policy.drain();  // 替换 self.throttle_armed = false;
```

- [ ] **Step 9: 移除 `throttle_armed` 字段**

确认所有 `self.throttle_armed` 引用已替换后，从 `MessagePipeline` 结构体中移除 `throttle_armed: bool` 字段。

- [ ] **Step 10: 编译验证**

Run: `cargo build -p peri-tui 2>&1 | head -40`
Expected: 无错误

- [ ] **Step 11: 提交**

```bash
git add peri-tui/src/app/message_pipeline/mod.rs
git commit -m "feat(tui): replace fixed 100ms throttle with AdaptiveChunkingPolicy"
```

---

## Task 4: 更新现有节流相关测试

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/message_pipeline_test.rs`

- [ ] **Step 1: 更新 `test_handle_event_assistant_chunk` 测试**

当前测试检查 `pipeline.throttle_armed`。替换为检查 `pipeline.adaptive_policy.pending_lines > 0`：

```rust
/// 测试：handle_event AssistantChunk 更新内部状态并通过策略跟踪积压
#[test]
fn test_handle_event_assistant_chunk() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let actions = pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], PipelineAction::None));
    assert_eq!(pipeline.current_ai_text, "hello");
    assert!(
        pipeline.adaptive_policy.pending_lines > 0,
        "AssistantChunk 应通过策略跟踪积压"
    );
}
```

- [ ] **Step 2: 检查其他引用 `throttle_armed` 的测试**

Run: `grep -n "throttle_armed" /Users/konghayao/code/ai/perihelion/peri-tui/src/app/message_pipeline/message_pipeline_test.rs`

如果有其他引用，全部替换为对应的 `adaptive_policy` 检查。

- [ ] **Step 3: 运行全量测试**

Run: `cargo test -p peri-tui 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 4: 提交**

```bash
git add peri-tui/src/app/message_pipeline/message_pipeline_test.rs
git commit -m "test(tui): update throttle tests for AdaptiveChunkingPolicy"
```

---

## Task 5: 集成测试和边界场景验证

**Files:**
- Modify: `peri-tui/src/app/message_pipeline/message_pipeline_test.rs`

- [ ] **Step 1: 添加集成级 check_throttle 测试**

在测试文件末尾添加：

```rust
// ─── check_throttle 集成测试 ────────────────────────────────────────────

/// 测试：无流式内容时 check_throttle 返回 None
#[test]
fn test_check_throttle_no_content() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

/// 测试：单次 chunk 后 check_throttle 返回 RebuildAll
#[test]
fn test_check_throttle_single_chunk() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    let result = pipeline.check_throttle(0);
    assert!(result.is_some());
    assert!(matches!(result.unwrap(), PipelineAction::RebuildAll { .. }));
}

/// 测试：连续 chunk 积压触发 CatchUp 模式
#[test]
fn test_check_throttle_catchup_on_burst() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    // 发送 10 个 chunk（超过 queue_depth_threshold=8）
    for i in 0..10 {
        pipeline.handle_event(AgentEvent::AssistantChunk {
            chunk: format!("line {}\n", i),
            source_agent_id: None,
        });
    }
    // 策略应处于 CatchUp 模式
    assert_eq!(pipeline.adaptive_policy.mode, ChunkingMode::CatchUp);
    // check_throttle 应立即返回（无间隔限制）
    let result = pipeline.check_throttle(0);
    assert!(result.is_some());
}

/// 测试：ToolStart 消费积压后 check_throttle 返回 None
#[test]
fn test_check_throttle_drained_after_tool_start() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    pipeline.handle_event(AgentEvent::AssistantChunk {
        chunk: "hello".into(),
        source_agent_id: None,
    });
    // ToolStart 会 drain 积压
    pipeline.handle_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/src/main.rs"}),
        source_agent_id: None,
    });
    // 积压已被消费
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
    let result = pipeline.check_throttle(0);
    assert!(result.is_none());
}

/// 测试：done() 重置策略状态
#[test]
fn test_done_resets_policy() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    // 积压数据
    for _ in 0..10 {
        pipeline.handle_event(AgentEvent::AssistantChunk {
            chunk: "burst\n".into(),
            source_agent_id: None,
        });
    }
    assert!(pipeline.adaptive_policy.pending_lines > 0);
    pipeline.done();
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
    assert_eq!(pipeline.adaptive_policy.mode, ChunkingMode::Smooth);
    assert!(pipeline.check_throttle(0).is_none());
}

/// 测试：begin_round() 重置策略状态
#[test]
fn test_begin_round_resets_policy() {
    let mut pipeline = MessagePipeline::new("/tmp".to_string());
    for _ in 0..10 {
        pipeline.handle_event(AgentEvent::AssistantChunk {
            chunk: "burst\n".into(),
            source_agent_id: None,
        });
    }
    pipeline.begin_round();
    assert_eq!(pipeline.adaptive_policy.pending_lines, 0);
    assert_eq!(pipeline.adaptive_policy.mode, ChunkingMode::Smooth);
}
```

- [ ] **Step 2: 运行全量测试**

Run: `cargo test -p peri-tui 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 3: 提交**

```bash
git add peri-tui/src/app/message_pipeline/message_pipeline_test.rs
git commit -m "test(tui): add integration tests for adaptive check_throttle"
```

---

## Task 6: 全量构建和最终验证

**Files:** 无修改

- [ ] **Step 1: 全量构建**

Run: `cargo build 2>&1 | tail -20`
Expected: 成功

- [ ] **Step 2: 运行全部相关 crate 测试**

Run: `cargo test -p peri-tui 2>&1 | tail -30`
Expected: 所有测试通过

Run: `cargo test -p peri-acp 2>&1 | tail -20`
Expected: 所有测试通过

- [ ] **Step 3: 运行 pre-commit hooks**

Run: `lefthook run pre-commit 2>&1 | tail -20`
Expected: 全部通过（fmt、check、clippy）

- [ ] **Step 4: Clippy 检查（额外确认）**

Run: `cargo clippy -p peri-tui 2>&1 | tail -20`
Expected: 无新 warning（`dead_code` 对 `is_catch_up()` 可忽略，已标注 `#[allow(dead_code)]`）

- [ ] **Step 5: 手动集成测试**

启动 TUI，测试以下场景：
1. 正常对话 → 验证流式输出流畅，无闪烁或延迟
2. 长代码生成（>50 行） → 验证高速输出时显示快速收敛，无明显延迟
3. 短回复（1-2 句话） → 验证不频繁重绘
4. 中断（Ctrl+C）→ 验证策略状态正确重置
5. 连续多轮对话 → 验证每轮策略正确重置

- [ ] **Step 6: 最终 Commit（如有遗漏的修复）**

```bash
git add -A
git commit -m "fix: follow-up fixes from adaptive streaming implementation"
```

---

## Self-Review

### Spec Coverage
- AdaptiveChunkingPolicy 结构体 + DrainPlan 枚举：Task 1
- 单元测试覆盖所有阈值和模式切换：Task 2
- 替换固定 100ms throttle 为自适应策略：Task 3
- 更新现有测试：Task 4
- 集成级 check_throttle 测试：Task 5
- 全量验证：Task 6

### Placeholder Scan
- 无 placeholder。所有步骤包含完整代码。
- `DrainPlan` 当前 `Single` 和 `Batch` 对最终 `build_rebuild_all` 调用无行为差异（都是单次 RebuildAll），但类型保留用于未来扩展（如 Single 时只追加差异而非全量 rebuild）。

### 行为变更分析
- **Smooth 模式间隔**：从 100ms 降为 16ms（~60fps），这意味着低速输出时重绘更及时，但不会更频繁（因为 `check()` 在无积压时返回 None）
- **CatchUp 模式**：无间隔限制，立即排空。这解决了高速输出时 100ms 间隔导致队列积压的问题
- **最小间隔守卫**：Smooth 模式 16ms 防止 CPU 空转（每秒最多 60 次 rebuild），与之前 10 次/秒相比是提升
- **对外接口不变**：`check_throttle(prefix_len: usize) -> Option<PipelineAction>` 签名不变，`polling.rs` 调用方无需修改

### 风险评估
- **SubAgent chunk 路由**：chunk 被路由到 SubAgent 时，需要在路由后额外调用 `adaptive_policy.on_chunk()`，否则主 pipeline 的策略无法感知 SubAgent 内部的输出压力。已在 Step 8 中处理。
- **模式切换滞后**：CatchUp 退出条件要求同时满足 depth ≤ 2 且 age ≤ 40ms，这可能导致短暂停留 CatchUp 模式。这是预期行为——避免在压力波动时频繁切换模式。
- **参数调优**：默认参数（8/120ms/2/40ms）基于 Codex 参考值，可能需要实际使用后微调。所有参数集中在 `AdaptiveChunkingPolicy::new()` 中，调优简单。
