# Bash 前台跑阻塞 `sleep N` 无提示，也缺少「等条件成立」与「纯等待」的替代手段

**状态**：Fixed（#1070）
**优先级**：中
**创建日期**：2026-11-17

## 问题描述

让 Agent 等一会儿再继续时，模型会直接在前台执行 `sleep 45` 这类命令，把整个工具调用挂住直到睡满。此时命令行上只有一行阻塞的 Bash 调用，用户看不出这是「在等」还是「卡死了」，也不知道有更合适的做法。

对应的提示文案形如：

```
Blocked: sleep 45 followed by: ...
To wait for a condition, use Monitor with an until-loop (e.g. `until <check>; do sleep 2; done`).
```

同类工具生态里，这类前台阻塞 sleep 是被前置拦截的，并引导模型改用两个专用工具：等条件成立用 Monitor、单纯等待用 Sleep。peri 此前没有这两个工具，也没有任何拦截。

## 现状（修复前）

| 能力 | peri 修复前 | 期望 |
| --- | --- | --- |
| 前台阻塞 `sleep N` | 直接执行，挂住工具调用 | 前置拦截并给出替代方案提示 |
| 等条件成立（轮询直到成功） | 只能自己写 shell 循环 | 提供 Monitor 工具 |
| 单纯等待一段时间 | 只能跑 `sleep N` 占一个 shell 进程 | 提供 Sleep 工具，不占 shell 进程 |
| 短 sleep（<2s 限流用途） | — | 放行，不拦截 |

## 涉及文件（修复后）

- `cc-tools/src/bash/mod.rs` —— Bash 工具新增 `validateInput` 入口，拦截前台阻塞 sleep
- `cc-tools/src/tools/monitor/`（新增）—— Monitor 工具：以 until-loop 轮询条件，条件成立即返回
- `cc-tools/src/tools/sleep/`（新增）—— Sleep 工具：`duration_seconds`，不占 shell 进程，可被中断
- `cc-tools/src/bash/prompt.ts` 对应提示词段落 —— 补充 sleep 相关的子项引导
- 既有测试适配 —— 原先用裸 `sleep N` 启动真实进程的用例改为 `while true; do sleep 1; done`

## 修复（#1070）

- Bash 工具新增输入校验钩子：命中 `^sleep\s+(\d+)\s*$` 且秒数 ≥ 2 时拒绝执行，并在提示里给出 Monitor / Sleep 的用法；
- 秒数 < 2 的 sleep 视为限流用途，放行；
- `sleep N` 后面还跟了其他命令时，提示里带上「followed by」的剩余部分，便于模型理解被拦的是哪一段；
- 新增 Monitor 与 Sleep 两个工具，并纳入工具清单；
- PowerShell 工具同步做等价拦截（`Start-Sleep -Milliseconds` 放行）；
- 适配所有依赖「裸 sleep 启动长进程」的既有测试，改用不会命中拦截的写法。

## 验证方式

1. 让 Agent 执行 `sleep 45`，应被拒绝并收到指向 Monitor / Sleep 的提示；
2. 执行 `sleep 1`（短 sleep），应正常放行；
3. 用 Monitor 轮询一个延迟满足的条件，条件成立后应及时返回；
4. 用 Sleep 等待指定时长，等待期间不应占用 shell 进程，且可被取消。

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-11-17 | — | Open | agent | 事后再补建（功能已由 PR #1070 实现并合并，merge commit `3dbb8494`） |
| 2026-11-17 | Open | Fixed | agent | 由 PR #1070 实现并合并 |

## 修复记录

### 修复 #1（2026-11-17）

- **操作人**：agent
- **用户原意**：希望 peri 在 Bash 前台跑阻塞 sleep 时给出明确提示，并提供「等条件」与「纯等待」的专用工具，文案对齐现有 peri 拦截风格。
- **修复内容**：新增 Bash 阻塞 sleep 前置拦截（<2s 放行）、新增 Monitor 与 Sleep 工具、PowerShell 侧等价拦截、适配依赖裸 sleep 的既有测试。
- **涉及 commit**：merge commit `3dbb8494`（PR #1070）
- **验证状态**：待验证
