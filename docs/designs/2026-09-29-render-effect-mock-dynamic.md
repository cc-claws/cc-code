# 渲染效果 Mock（动态时间线）：Spinner 演变全过程

> 状态：**效果预演（未实现）**
> 日期：2026-09-29
> 关联：`docs/designs/2026-09-29-thinking-status-line-and-tool-summary.md`（设计）、`docs/designs/2026-09-29-render-effect-mock.md`（静态对照）
> 数据：真实 peri 会话（thread「订单解析报错」`01a0ead8-…`）+ 真实帧序列（`animation.rs` 的 `BRAILLE_FRAMES`）
>
> **本文件目的**：让评审者**体验到 spinner 随时间的动态变化**（静态四态看不出来）。

---

## 1. 真实帧序列（取自代码，非编造）

`peri-widgets/src/spinner/animation.rs`：

```rust
const BRAILLE_FRAMES: &[char] = &[
    '✵', '✶', '✷', '✸', '✹', '✺', '✻', '✼', '❃', '❊', '✼', '✻', '✺', '✸', '✹', '✷',
];
// tick_to_frame(tick) = BRAILLE_FRAMES[tick % 16]
```

**16 帧循环**。且 `SpinnerState::advance_tick()` 每 **2 个 raw tick** 才推进 1 帧（`spinner/mod.rs:110`），故实际约 **32 raw-tick 一轮回**。

---

## 2. 逐帧时间线：一次「思考 → 工具 → 再思考」全过程

> 场景**取自真实会话轮 1→2**：`pwd && ls -la`（Bash，60 行输出）→ 失败 Read。
> **秒数【模拟】**（真实 thread 无耗时）；**帧字符【真实】**（按 tick 推进）。

### 2.1 阶段 A：进入思考（0s → 5s）

```
t=0.0s   ✵ 思考中… (0s · thinking)
t=0.2s   ✶ 思考中… (0s · thinking)
t=0.4s   ✷ 思考中… (0s · thinking)
t=0.6s   ✸ 思考中… (0s · thinking)
t=0.8s   ✹ 思考中… (0s · thinking)
t=1.0s   ✺ 思考中… (1s · thinking)
t=1.2s   ✻ 思考中… (1s · thinking)
t=1.4s   ✼ 思考中… (1s · thinking)
t=1.6s   ❃ 思考中… (1s · thinking)
t=1.8s   ❊ 思考中… (1s · thinking)
t=2.0s   ✼ 思考中… (2s · thinking)
t=2.2s   ✻ 思考中… (2s · thinking)
t=2.4s   ✺ 思考中… (2s · thinking)
t=2.6s   ✸ 思考中… (2s · thinking)
t=2.8s   ✹ 思考中… (2s · thinking)
t=3.0s   ✷ 思考中… (3s · thinking)     ← 一轮 16 帧走完，循环
...
t=5.0s   ✵ 思考中… (5s · thinking)
```

**观察点**：`· thinking` 期间，**字符旋转、时长增长**，状态保持 `thinking`。

### 2.2 阶段 B：思考结束（5s），切 `thought for Ns`

```
t=5.0s   ✻ 思考中… (5s · thinking)
         ↓ 收到首条 TextChunk / ToolStart（思考段结束）
t=5.0s   ✻ 思考中… (5s · thought for 5s)        ← 尾字段切换，耗时定格
```

### 2.3 阶段 C：工具执行（5s → 6.2s）

工具执行期间 spinner 转 `ToolUse` 态，**第三字段保持上一次思考结果**（或清空，见 §4-待定）：

```
t=5.2s   ✼ 执行工具… (5s · thought for 5s)
t=5.4s   ❃ 执行工具… (5s · thought for 5s)
t=5.6s   ❊ 执行工具… (5s · thought for 5s)
t=5.8s   ✼ 执行工具… (5s · thought for 5s)
t=6.0s   ✻ 执行工具… (6s · thought for 5s)
t=6.2s   ✺ 执行工具… (6s · thought for 5s)
```

### 2.4 阶段 D：第二轮思考开始 → `thinking more`（6.2s → ）

**本轮已产出过（工具执行完）后再次思考** → 判定为第 2+ 段：

```
t=6.4s   ✸ 思考中… (6s · thinking more)
t=6.6s   ✹ 思考中… (6s · thinking more)
...
t=10s    ✵ 思考中… (10s · thinking more)
```

### 2.5 阶段 E：长思考超 60s → `still thinking`

第二轮思考**未中断**持续到 60s：

```
t=59.8s  ✼ 思考中… (59s · thinking more)
t=60.0s  ❃ 思考中… (1m 0s · still thinking)      ← 跨过 60s 阈值，切 still
t=60.2s  ❊ 思考中… (1m 0s · still thinking)
...
t=72.0s  ✵ 思考中… (1m 12s · still thinking)
```

### 2.6 完整一行流（同一行原地刷新，非多行）

**关键**：以上所有帧都渲染在**同一行**（`message_area.rs` 的 `spinner_line`），原地覆盖刷新。终端里你看到的是**一行在转**：

```text
✵ 思考中… (0s · thinking)
✶ 思考中… (0s · thinking)
...
✻ 思考中… (5s · thinking)
✻ 思考中… (5s · thought for 5s)          ← 瞬切
✼ 执行工具… (5s · thought for 5s)
...
✺ 执行工具… (6s · thought for 5s)
✸ 思考中… (6s · thinking more)           ← 瞬切
...
❃ 思考中… (1m 0s · still thinking)       ← 跨 60s 瞬切
```

---

## 3. 俯视视角：一屏内的三行共存

spinner 行在**消息区底部**。真实场景里的层级：

```text
┌─ 消息区 ─────────────────────────────────────────────────────────────┐
│ ❯ SF20260924983025 这单解析是解析sku绑定还是listing呀  为啥报错了啊   │
│                                                                       │
│ Thought for 4s (ctrl+o to expand)                                     │
│ ● Bash(pwd && ls -la)                                                 │
│   ⎿ /d/code/nt_order                                                  │
│     total 1762                                                        │
│     drwxr-xr-x 1 adim 197121      0 Sep 28 12:40 .                     │
│     ... (57 more lines)                                               │
│                                                                       │
│ Thought for 1s, read 1 file (ctrl+o to expand)                        │
│   ⎿ Error: File not found at /d/code/nt_order/CLAUDE.md               │
└───────────────────────────────────────────────────────────────────────┘
✸ 思考中… (6s · thinking more)                    ← 唯一活动的 spinner 行
┌─ 输入框 ─────────────────────────────────────────────────────────────┐
│ ❯                                                                     │
└───────────────────────────────────────────────────────────────────────┘
```

> spinner 只有**一行**，在消息区与输入框之间；它随状态切换而**内容变化**，不是堆叠。

---

## 4. 待实测 / 待定（本 mock 中标注为不确定）

1. **工具执行期第三字段**：`执行工具… (6s · thought for 5s)` 中 `thought for 5s` 是否应保留？还是工具期清空为 `(6s)`？【Claude 截图未见此态】
2. **`thinking more` 的精确判定点**：是「收到 ToolStart 后再次 AiReasoning」还是「收到 TextChunk 后」？本 mock 按"曾产出即算"处理。
3. **帧推进速率**：真实 tick 频率取决于主循环帧率，本 mock 按 ~5 fps 近似。
4. **turn 结束行**：`✻ Cogitated for 15s · done 10:30`（`message_area.rs:100`）在 spinner 停止后显示，本 mock 未展开。

---

## 5. 与静态 mock 的关系

| 文件 | 视角 | 用途 |
|------|------|------|
| `...render-effect-mock.md` | **静态**：改造前 vs 改造后整屏 | 看**信息密度**变化 |
| **本文件** | **动态**：spinner 逐帧时间线 | 看**状态演变**过程 |

两份配合看：前者回答"值不值得改"，后者回答"改完长什么样、怎么动"。
