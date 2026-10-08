# 详细模式超长命令运行时，状态刷新覆盖 header 续行

**状态**：Fixed
**优先级**：中（仅影响显示、不影响命令执行；但详细模式正是为「看清完整命令」而设，此时命令被吞掉与设计目标直接冲突）
**创建日期**：2026-10-08
**分支**：`fix/bash-running-line-not-fixed-index`
**PR**：[#341](https://github.com/cc-claws/cc-code/pull/341)

---

## 一、问题描述

详细模式（Ctrl+O）下运行**超长 Bash 命令**（header 因超长折成多行）且命令**运行超过 2 秒**时，工具块渲染错乱：

```text
● Bash((cd /d/code/peri/9router && grep -rn "registry/index|import p0|p0" from scripts/*.mjs
       Running… (49s)    (timeout 10m))          ← 命令续行被覆盖，残留一个 ")"
  ⎿ Running… (15s)    (timeout 10m)              ← 真正的状态行，秒数被冻结
    (ctrl+b to run in background)
```

两个可见异常：

1. **命令续行被就地改写**成 `Running… (Xs)`，原命令文本丢失，并残留一个未配对的 `)`；
2. 真正的状态行**秒数被冻结**（停在首次渲染的时刻），于是屏幕上出现**两处 `Running…` 且时间不一致**（如 49s / 15s）。

期望：命令续行保持完整，仅状态行的秒数随时间前进。

> 复现要点：详细模式 + 命令长到 header 折行 + 运行时长 > 2 秒（tap `TOOL_INDICATOR_TICK_INTERVAL`，200ms 一次）。

## 二、根因

详细模式下超长命令会让 `ToolBlock` header 折成多行（由 **#264** 引入，见 `message_render.rs` 的 `wrap_full` 分支），但**渲染线程的 tick 增量刷新仍沿用「header 恒 1 行」的旧假设**，在 `cc-tui/src/ui/render_thread.rs` 中有两处体现：

| 位置 | 旧代码 | 多行 header 下的后果 |
|------|--------|---------------------|
| 场景 B（行数不变的增量更新） | `lines.get_mut(1)` 更新 `Running…` 秒数 | 下标 1 在多行 header 下是**命令续行**，被就地改写为 `Running…`；真正的状态行（下标 ≥2）因不在下标 1 而**永不刷新** |
| 场景 A（跨 2 秒阈值触发重建） | `cached_line_count < 3` 推断「状态行尚未渲染」 | 多行 header 下缓存行数早已 ≥ 3，判定**恒为假**，首次跨阈值时状态行**永远不出现** |

```
场景 B（改写错行 + 冻结真行）
    if let Some(running_line) = lines.get_mut(1) {          // ← 写死下标 1
        if running_line.spans.len() >= 2
            && running_line.spans[1].content.as_ref() != new_running_text
        {
            running_line.spans[1].content = new_running_text.into();
            msg_changed = true;
        }
    }
```

### 为什么是 #264 而不是 #287

本问题常被误判为 #287 引入（两者都涉及「详细模式超长命令折行」），实测否证：

- 检出 **#264 之前**的代码运行同一复现：header **恒为 1 行**（命令被单行截断 `…`），状态行落在 `lines[1]`，增量刷新**行为正确**。
- #264 让详细模式改为**完整折行展示**，header 由此可能变成多行 → 触发本缺陷。
- **#287 无关**：它只调整了折行时的词边界回退逻辑（长 token 场景），反而让更多长命令折成多行，**更易触发**本缺陷。

一句话：**#264 改变了 header 的行数契约，却没有同步更新依赖该契约的增量刷新路径。**

## 三、改动

核心思路：**状态行按内容定位，不再依赖固定下标 / 固定行数**（与子 Agent 路径已有的内容定位做法一致）。

| 文件 | 改动 |
|------|------|
| `cc-tui/src/ui/message_render.rs` | 新增 `SHELL_RUNNING_TEXT_PREFIX` 常量与 `is_shell_running_status_line(&Line) -> bool`（判定 `spans[0] == "  ⎿ "` 且 `spans[1]` 以 `Running…` 开头） |
| `cc-tui/src/ui/render_thread.rs` | 场景 B：`lines.iter_mut().find(is_shell_running_status_line)` 取代 `lines.get_mut(1)`；场景 A：以 `lines.iter().any(is_shell_running_status_line)` 的「状态行是否存在」取代 `cached_line_count < 3` |
| `cc-tui/src/ui/render_thread_test.rs` | 新增 3 条回归测试 + 构造辅助 `detail_mode_long_cmd_running_task()` |

**未采用的做法**：在 header 折行后回填「状态行下标」之类的脆弱约定——任何再次改动 header 折行的补丁都可能重新引入不一致；按内容定位对 header 行数变化天然免疫。

## 四、验证

新增 3 条回归测试，且**全部做了反向验证**（临时回退修复后三条均失败）：

| 测试 | 覆盖 | 反向验证表现 |
|------|------|-------------|
| `test_refresh_running_bash_multiline_header_keeps_command_and_status` | 命令续行不被覆盖、状态行恰好一条 | 失败信息精确复现：命令续行变成 `"       Running… (5s))"` |
| `test_refresh_running_bash_multiline_header_updates_elapsed` | 状态行秒数随 tick 前进 | 失败于「秒数应变化」断言（被冻结） |
| `test_refresh_running_bash_multiline_header_triggers_rebuild_at_threshold` | 多行 header 跨阈值时补上状态行 | 失败于「跨阈值后应存在状态行」（旧固定行数判定漏渲染） |

其他：

- `cargo test -p cc-tui --lib`：**1062 passed**。
- `cargo clippy --workspace --all-targets -- -D warnings`：无告警。
- CI（PR #341）：macOS / Ubuntu / Windows 三平台 Build 全部 **pass**（CI 的 `Run tests` 步骤即 `cargo test --workspace`）。

## 五、人工验证要点

1. 详细模式（Ctrl+O）下运行一条**超长无空格/长路径**命令（如 `grep -rn <超长模式> ... | head; ...`），使 header 折成 ≥2 行。
2. 让命令运行超过 2 秒：
   - header 续行应**保持完整**，不被 `Running…` 覆盖、无残留 `)`；
   - 状态行 `⎿ Running… (Xs)` 的秒数应**持续前进**；
   - 全屏应**只有一处** `Running…`。
3. 非详细模式 / 短命令（header 单行）路径行为不变。

## 六、关联

- 引入方：**#264**（Bash 命令超长改为限制宽度截断，详细模式完整折行对齐）
- 易触发方（非引入方）：**#287**（详细模式超长无空格命令的头行折行不再孤立）
- 相关历史 issue：`spec/issues/2026-09-30-*`（#266 折行末段回退、#258 详细模式折行对齐）
