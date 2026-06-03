# Session 恢复后 compact 前后的消息同时存在，导致对话重复

**状态**：Open
**优先级**：高
**创建日期**：2026-06-02

## 问题描述

会话触发 compact（自动或手动）后关闭 TUI，再次恢复该会话时，对话中同时出现了 compact 之前和 compact 之后的内容——完整旧对话加上 compact 后的回复和紧凑摘要，两者拼接在一起。

## 症状详情

| 阶段 | 现象 |
|------|------|
| compact 触发前 | 对话有大量消息（如 20+ 轮工具调用） |
| compact 执行后 | 对话被压缩为摘要 + 后续新消息 |
| 关闭 TUI 再恢复会话 | 完整旧对话（20+ 轮原样）+ 紧凑后的内容同时存在 |

用户看到的恢复结果：完整的 compact 前的旧对话，后面拼接着 compact 后的新消息和 summary，像是两段互不相关的对话粘在了一起。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 进行一次会产生大量消息的对话，触发自动 compact（或手动执行 /compact）
  2. 确认 compact 后的摘要和后续新消息正常显示
  3. 退出 TUI
  4. 重新启动 TUI，用 `-r` 恢复该会话
  5. 查看消息列表——compact 前的旧对话原样存在，compact 后的消息完整显示在后面

## 涉及文件

- `peri-tui/src/acp_server/prompt.rs:146-151` —— 执行结束后向 ThreadStore 持久化新消息的逻辑
- `peri-middlewares/src/compact_middleware.rs:227-247` —— compact 执行处，修改内存消息状态
