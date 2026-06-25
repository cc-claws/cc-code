# Compact 摘要文本包裹 `<system-reminder>` 标签，TUI 折叠展示

**状态**：Open
**优先级**：中
**创建日期**：2026-06-02

## 问题描述

当前 compact 后生成的对话摘要作为普通 Human 消息推入消息列表，TUI 以完整 UserBubble 渲染，占据大片显示空间。实际上用户不需要反复查看压缩摘要详情——只需知道"压缩发生了"及关键统计（文件数、技能数）。

项目已有 `<system-reminder>` 机制——`recall_items` 通过该标签向 LLM 注入跨轮次状态（如工具可用性变更），提示词 `14_system_reminder.md` 指导 LLM 静默处理。将 compact 摘要也纳入此机制，可以让 TUI 识别并折叠展示。

## 症状详情

- compact 后的摘要文本目前被视为普通用户输入，完整渲染在聊天区
- 摘要内容通常较长（数十行 Markdown），视觉干扰大
- 用户无法区分"真实用户输入"和"系统注入的上下文摘要"

## 期望行为

### LLM 侧

compact 的 Human 消息文本包裹一层 `<system-reminder>`：

```
<system-reminder>
此会话从之前的对话延续。以下是之前对话的摘要。

{summary}

[上下文已压缩，请根据摘要继续工作]
</system-reminder>
```

**注意**：仅 Human 摘要消息包标签。re_inject 产生的 `[最近读取的文件]` / `[激活的 Skill 指令]` 等 System 消息保持原样，不纳入标签内。

LLM 行为不变——已有 `14_system_reminder.md` 指导其静默读取。

### TUI 侧

检测 Human 消息内容是否以 `<system-reminder>` 开头/结尾：
- **展开态**：正常渲染内部内容
- **折叠态（默认）**：显示单行简略提示 `📋 上下文已压缩（N 个文件，M 个技能）`，可展开查看详情

Micro compact 不涉及摘要文本，不受影响。

## 涉及文件

- `peri-middlewares/src/compact_middleware.rs:227-231` —— summary 构造点，此处添加 `<system-reminder>` 包裹
- `peri-tui/src/ui/message_view/mod.rs:466` —— `from_base_message_with_cwd` 中 Human→UserBubble 转换，需检测 `<system-reminder>` 并设置折叠态
- `peri-tui/src/ui/message_render.rs` —— UserBubble 渲染，需实现折叠/展开切换
- `peri-tui/src/app/agent_ops/mod.rs:274` —— `handle_compact_completed`，CompactCompleted 事件携带 files/skills 统计，TUI 折叠提示需消费此数据
