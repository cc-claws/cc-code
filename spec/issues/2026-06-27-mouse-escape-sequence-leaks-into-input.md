# ConPTY 下鼠标转义序列泄漏为输入框乱码

## 状态
- [x] Fixed（2026-06-27，PR #88，commit 45d16dc0 / 113f611e）

## 分类
- **类型:** Bug
- **严重程度:** P1 — 用户可见功能损坏
- **模块:** peri-tui / event / conpty

## 描述

TUI 运行一段时间后，输入框出现大量乱码，形如：
```
66M[<32;27;66M[<32;28;66M[<32;29;66M[<32;30;...
```

这些是 SGR 鼠标转义序列的碎片，本应由 ConPTY 翻译为 `MOUSE_EVENT_RECORD`，但因缓冲区溢出导致翻译失败，原始 VT 字节以 `KEY_EVENT_RECORD` 形式进入输入队列，被 crossterm 解析为 `Event::Key(Char(...))` 事件，最终通过 `detect_simulated_paste()` 收集为 paste blob 插入 textarea。

## 根因

`conpty.rs` 中 `ENABLE_MOUSE_TRACKING_SEQUENCE` 启用了 `\x1b[?1003h`（any-event tracking），导致终端对**每一次鼠标像素移动**都发送 SGR 事件。Agent 执行期间 TUI 忙于渲染（100ms+），事件在 Console 输入缓冲区积压。缓冲区溢出时 ConPTY 无法将 SGR 序列翻译为结构化事件，原始字节泄漏。

TUI 代码**从未处理** `MouseEventKind::Move`（hover），`?1003h` 完全多余。`?1000h`（click/release/scroll）+ `?1002h`（drag）已覆盖所有使用场景。

## 复现条件（多因素交集，因此偶发）

1. `?1003h` 启用 → 鼠标移动产生连续事件流
2. Agent 流式输出期间 TUI 渲染暂停 100ms+
3. ConPTY 翻译失败（缓冲区接近满时概率性触发）
4. 泄漏字节在 `detect_simulated_paste` 的 1ms+15ms 窗口内连续到达

## 修复方案

### 主修复：移除 `?1003h`

从 `ENABLE_MOUSE_TRACKING_SEQUENCE` 和 `DISABLE_MOUSE_TRACKING_SEQUENCE` 中移除 `\x1b[?1003h` / `\x1b[?1003l`。

- 事件量从每秒数百（motion）降到个位数（仅 click/drag/scroll）
- 缓冲区溢出概率降至接近零
- TUI 功能零退化（无 hover 处理代码）

### 兜底（可选后续）：速率过滤

在 `next_event()` 中检测 `Key(Char)` 事件速率，超过阈值（如 10ms 内 50+ 个 Char 事件）时批量丢弃，防止其他未知路径的泄漏。

## 涉及文件

| 文件 | 修改 |
|------|------|
| `peri-tui/src/conpty.rs` | 移除 `?1003h`/`?1003l`，更新注释和测试 |

## 泄漏字节样本

```
66M[<32;27;66M[<32;28;66M[<32;29;66M[<32;30;66M[<32;31;
66M[<32;32;66M[<32;33;66M[<32;34;66M[<32;35;66M[<32;36;
66M[<32;37;66M[<32;38;66M[<32;40;66M[<32;41;66M[<32;42;
66M[<32;43;66M[<32;44;66M[<32;45;66M[<32;46;66M[<32;47;
66M[<32;48;66M[<32;49;66M[<32;50;66M[<32;51;
```

拆解：`<32;` = SGR motion 事件，`66` = 列号（不变），`27→51` = 行号递增（鼠标垂直移动 24 像素），`M` = 终止符。
