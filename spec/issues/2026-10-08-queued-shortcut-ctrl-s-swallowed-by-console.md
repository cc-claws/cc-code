# Windows 下排队消息 Ctrl+S（Send now）快捷键完全无响应

**状态**：Fixed
**优先级**：中（不影响鼠标操作，有 workaround；但键盘入口在 Windows 上完全不可用）
**创建日期**：2026-10-08
**分支**：`fix/queue-shortcut-alt-keys`
**GitHub Issue**：[#344](https://github.com/cc-claws/cc-code/issues/344)

---

## 一、问题描述

Agent 运行中把消息排队后，队列行尾提示 `Ctrl+S send now · Ctrl+X delete`。在 **Windows 终端（Windows Terminal / ConPTY）** 下：

- **Ctrl+X 正常**：能删除队首排队消息；
- **Ctrl+S 完全无响应**：不能触发「立即补充（Send now）」，按键像被吞掉。

期望：两个键都能在 Windows 下生效。

---

## 二、症状详情

| 操作 | 期望 | 实际（Windows） |
|------|------|-----------------|
| `Ctrl+X` | 删除队首排队消息 | ✅ 正常删除 |
| `Ctrl+S` | 立即补充队首排队消息 | ❌ 无任何反应 |

对照关系很关键：两个分支在应用层代码里**完全对称**、共用同一套守卫与处理函数，但只有 Ctrl+S 失效——说明 Ctrl+S 的按键事件**根本没到达应用层**。

---

## 三、复现条件

- **复现频率**：Windows 终端下必现
- **触发步骤**：
  1. 在 Windows（Windows Terminal / ConPTY）启动 TUI；
  2. Agent 运行中，输入一条消息回车排队；
  3. 按 `Ctrl+S`（无反应）、再按 `Ctrl+X`（正常删除）。
- **环境**：Windows + Windows Terminal（conhost/ConPTY 链路）；macOS / Linux 终端无此问题

---

## 四、根因

**Ctrl+S 是终端层的保留键**：conhost 把 `Ctrl+S` 当作「暂停输出」键（XOFF，传统上是 `VK_PAUSE` 的别名），在宿主层截走并丢弃，事件不写入输入缓冲区，应用无从读取。

crossterm 在 Windows 走 `ReadConsoleInputW`（原生控制台 API），一切输入必经 conhost 的 `InputBuffer`，因此应用层无法绕过。

conhost 源码 `src/host/inputBuffer.cpp` 的拦截（`_WriteBuffer`）：

```cpp
// Ctrl-S is traditionally considered an alias for the pause key.
static bool IsPauseKey(const KEY_EVENT_RECORD& event) {
    if (event.wVirtualKeyCode == VK_PAUSE) return true;
    const auto ctrlButNotAlt = WI_IsAnyFlagSet(event.dwControlKeyState, CTRL_PRESSED)
                            && WI_AreAllFlagsClear(event.dwControlKeyState, ALT_PRESSED);
    return ctrlButNotAlt && event.wVirtualKeyCode == L'S';
}

// in _WriteBuffer():
if (WI_IsFlagSet(InputMode, ENABLE_LINE_INPUT) && IsPauseKey(inEvent.Event.KeyEvent)) {
    WI_SetFlag(gci.Flags, CONSOLE_SUSPENDED);   // 视为暂停，丢弃该键
    continue;
}
```

`Ctrl+X` 在 conhost 中**没有任何特判**，故一路畅通。这正是「S 被吞、X 正常」的原因。

> 同类先例：本仓已为终端保留键做过规避——Ctrl+V 被现代终端宿主拦截，遂增加 `Alt+V`（`normal_keys.rs` 注释）；Ctrl+C 被 ConPTY 转成信号，`main.rs` 用 `ctrl_handler` 注入 KeyEvent 绕行。Ctrl+S 属同一类「宿主保留键」问题。

---

## 五、改动

核心思路：**避开终端保留键，Ctrl 系 → Alt 系**（与 `Alt+V` 规避 Ctrl+V 同源）。

| 文件 | 改动 |
|------|------|
| `cc-tui/src/event/keyboard/normal_keys.rs` | 排队快捷键由 `Ctrl+S`/`Ctrl+X` 改为 `Alt+S`/`Alt+X`；并匹配 macOS Option 组合字符（⌥S=`ß`、⌥X=`≈`，⌥V=`√`）；Ctrl 绑定移除 |
| `cc-tui/src/app/queued_messages.rs` | 更新 doc 注释，写明弃用 Ctrl+S 的原因 |
| `cc-tui/locales/{en,zh-CN}/main.ftl` | `queue-keys-tip` 文案改为 Alt 系 |
| `TUI-STYLE.md` | 快捷键表两行 + Queued 区例外说明同步 |

**未采用**：继续硬用 Ctrl+S —— 应用层无法让 conhost 放行该键，除非改走 VT 输入模式等更深层改造，代价与风险远高于换键。

---

## 六、验证

- 新增/改写单测：`test_alt_s_is_consumed_by_queued_message_shortcut`、`test_alt_x_deletes_head_queued_message`、`test_ctrl_x_is_no_longer_bound_after_alt_migration`（负向：Ctrl+X 迁移后不再生效）、`test_macos_option_compose_chars_trigger_queue_shortcuts`。
- `cargo test -p cc-tui --lib`：相关用例 46 passed / 0 failed。
- 鼠标 `[Send now]` / `[×]` 行为不变；队列为空时仍不拦截按键、放行给 textarea。

---

## 七、人工验证要点

1. Windows 终端启动 TUI，Agent 运行中排一条消息；
2. 按 `Alt+S` → 应立即补充队首消息；按 `Alt+X` → 应删除队首消息；
3. 队列行尾提示应显示 `Alt+S send now · Alt+X delete`；
4. 确认 `Ctrl+S` / `Ctrl+X` 不再触发队列操作（迁移后已解绑）。

---

## 八、关联

- 引入方：**#282**（排队消息支持 Ctrl+S / Ctrl+X 键盘操作）
- 同源规避先例：**Alt+V**（规避 Ctrl+V 宿主拦截）、**ctrl_handler**（规避 Ctrl+C 被 ConPTY 转信号）

---

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-08 | — | Open | agent | 创建 |
| 2026-10-08 | Open | Fixed | agent | Ctrl 系迁移为 Alt 系，附单测 |

## 修复记录

### 修复 #1（2026-10-08）

- **操作人**：agent
- **用户原意**：Ctrl+S 在 Windows 终端下按了没反应，要能用键盘对排队消息执行「立即补充 / 删除」
- **修复内容**：排队快捷键由 `Ctrl+S`/`Ctrl+X` 迁移为 `Alt+S`/`Alt+X`，并匹配 macOS Option 组合字符；同步文案与文档
- **涉及 commit**：`fix/queue-shortcut-alt-keys` 分支
- **验证状态**：待验证
