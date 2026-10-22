# 输入 `/` 触发命令提示后删除，视口 allocated_rows 不收缩导致底部大面积空白

**状态**：Fixed
**优先级**：中
**创建日期**：2026-10-21

---

## 一、问题背景与现场

首次进入应用（growing 模式），用户输入 `/` 触发命令提示浮层，然后删除 `/` 取消命令。此时消息区域底部出现大面积空白留白，视口高度不会回缩。

用户截图确认：欢迎卡上方正常，但欢迎卡与输入框之间出现了大量空白行。

## 二、复现路径

1. 启动 peri（空会话，growing 模式，非全屏）
2. 输入 `/` → 命令提示浮层出现，`required_rows` 增大
3. `ViewportState::prepare()` 中 `allocated_rows` 增大到匹配 `required_rows`
4. 删除 `/` → 命令提示浮层消失，`required_rows` 回落
5. **但 `allocated_rows` 保持在大值** → 消息区域底部留白

## 三、根因

`cc-tui/src/terminal_backend/viewport.rs:67`：

```rust
let next_rows = self.allocated_rows.max(required_rows).max(1);
```

`allocated_rows` 是**单调递增**的 — 只增不减。这是防止删除输入时视口抖动的设计意图，但对**临时浮层**（hint 弹窗）参与 `required_rows` 计算的场景不适用：

- `required_rows()` 在 `viewport_layout.rs:91` 中将 `hints::desired_height(app)` 纳入 `popup` 变量
- hint 弹窗消失后 `popup = 0`，`required_rows` 回落，但 `allocated_rows` 已经记住了含弹窗高度的值
- 多出的空间分配给消息区（`Constraint::Min(1)`），底部出现空白

极端情况：如果 hint 弹窗高度足够大（命令多、候选 10 条），可能把 `next_rows` 推过 `size.height` 触发 `fullscreen_latched = true`，**提前锁定全屏**。

## 四、涉及文件

- `cc-tui/src/terminal_backend/viewport.rs` — `ViewportState::prepare()` 中 `allocated_rows` 单调递增
- `cc-tui/src/ui/main_ui/viewport_layout.rs` — `required_rows()` 将 hint 弹窗高度计入 `required_rows`
- `cc-tui/src/ui/main_ui/popups/hints.rs` — `desired_height()` 返回命令候选高度

## 五、修复方向

hint 弹窗高度不应永久撑大 `allocated_rows`。可选方案：

1. **分离 popup 高度**：`required_rows` 中 popup 部分不纳入 `allocated_rows` 的记忆，仅影响当帧渲染（popup 覆盖在消息区上方，不实际占用行数）
2. **回收 popup 贡献的高度**：跟踪 `allocated_rows` 中由 popup 贡献的部分，popup 消失时回收
3. **popup 不参与 `required_rows`**：改为 hint 弹窗始终覆盖在已有视口上绘制（浮层语义），不改变布局

方案 1 存在严重物理缺陷（0c619369 实装后依然复现）：
- 浮层 12 行使 `frame_rows` 在标准 24 行终端下达到 27 行，`frame_rows >= size.height` 假性击穿并永久锁死全屏，删除 `/` 后留白 9 行；
- 原生滚屏不可逆，即使逻辑收缩，终端滚屏上移也会导致底部悬空留白；
- Ratatui Fixed Viewport `resize` 缩小时不清理下方释放的区域，造成幽灵残影。

**最终采纳方案 3**：
浮层（Hints 和 @-mention）作为 Overlay 覆盖在输入框上方已有空间自适应渲染，本身代码已具备 `available` 边界自适应。浮层完全不参与 `required_rows`，视口高度不随浮层展开/关闭抖动，彻底杜绝留白与误切全屏。

## 六、状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-10-21 | — | Open | agent | 用户截图报告，代码分析确认 |
| 2026-10-21 | Open | Fixed | agent | `required_rows` 拆分 `total`/`persistent`；（方案 1，后证实依然复现） |
| 2026-10-22 | Fixed | Reopen | user | 用户反馈最新版本依然复现留白（0c619369 方案 1 缺陷） |
| 2026-10-22 | Reopen | Fixed | agent | 彻底移除 `required_rows` 中浮层高度，浮层作为 Overlay 视口内自适应（方案 3） |
