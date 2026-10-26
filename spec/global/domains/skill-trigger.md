# Skills 触发 领域

## 领域综述

Skills 触发领域负责 Skills 的触发机制设计，将触发键从 # 统一到 / 前缀，与命令系统共用命名空间。

核心职责：
- Skills 触发键从 # 改为 /，与命令共用命名空间
- 提示浮层为统一扁平列表（命令与 Skills 混排，总上限 10 条）
- 命令匹配优先，Skill 名与命令名冲突时命令优先执行
- 消息解析中 /xxx token 直接触发 Skill 预加载

## 核心流程

### Skills 触发流程

```
用户输入 / 前缀
  → 提示浮层显示统一扁平候选列表（上限 10 条，前缀匹配 > 类别 rank > 字母序）
  → Tab 补全: 在合并候选列表中定位
  → Enter 触发:
      先尝试命令 dispatch → 命中则执行命令
      未命中 → 尝试 Skill 匹配 → 匹配则走 Submit 流程
```

## 技术方案总结

自然语言任务匹配技能时，模型必须先调用一级核心工具 `Skill({"skill":"name","args":"可选参数"})`，工具结果携带完整 Markdown 指令。系统摘要只列名称和描述，单项描述最多 1536 字符，总列表默认按 8000 字符预算缩短描述，不删技能名称。仅在回复中提及技能不会加载。

`SkillPreloadMiddleware` 从最新用户消息识别 `/skill-name args`，或读取 SubAgent 定义中的 skills 列表，与工具共用查找和参数展开逻辑。显式 Slash 直接追加 `<skill-invocation>` Human 元消息，不再让模型重复调用，也不伪造 Ai/Tool 消息。TUI 只在展示层把 `display:true` 的加载结果投影成 Skill 卡片。新轮次可以重新加载，同轮重复注入保持幂等；失败不留下半份注入历史。

目录优先级保持用户级 → 全局配置 → 项目级 → 插件，同名取先出现者；支持完整命名空间和唯一简称，歧义或路径输入返回错误。Frontmatter 使用 YAML 解析，支持 `allowed-tools`、`context`、`agent`、`model`、`arguments`、`disable-model-invocation` 和 `user-invocable`。参数支持 `$ARGUMENTS`、`$ARGUMENTS[n]`、`$n`、命名参数、引号分词与 `${CLAUDE_SKILL_DIR}` / `${CLAUDE_SESSION_ID}`；无占位符时追加 `ARGUMENTS:`。加载正文保持原语言。

`context: fork` 与 参考实现 一致：在独立子代理中执行并等待最终结果，返回 `status: "forked"`、`agentId`、`result`，主上下文不接收技能正文。复用子代理定义、模型选择、工具过滤与权限链，取消随父调用级联，事件始终带子来源。普通 `Agent` 工具仍使用原有后台语义。`allowedTools` 是权限元数据，不能突破现有审批或禁止规则；fork 的 `model` / `agent` 用于选择执行器，inline 的 model 仅返回元数据。

成功 inline 激活记录保存在当前会话独立 registry，同名最新；手动与自动 compact 共用记录，最近优先、单项及总预算限制，截断不覆盖原始正文。压缩恢复只追加 Human 元消息，不改变冻结的系统提示词；clear / rewind 重建记录，子代理使用独立 registry。恢复进程时可从已有历史或恢复 marker 建档。

卡片头只显示技能名，参数不进入头部；默认折叠为头部一行和结果一行：`Initializing…`、`Successfully loaded skill`（可附工具数量与模型）或 fork 的 `Done`。错误保持可见，详情模式可展开 Markdown。

以上以参考实现 `SkillTool.ts`、`UI.tsx`、`prompt.ts`、`processSlashCommand.tsx`、`argumentSubstitution.ts` 和 `compact.ts` 为行为参考，优先于 PRD 中的后台 fork、单行整卡和仅最近一轮恢复描述。Peri 通过 ToolContent 的 JSON `instruction` 承载全文，保持现有 Function Calling 引擎接口及 thinking 签名原样配对。

| 维度 | 选型 |
|------|------|
| 触发键 | / 前缀（统一命令和 Skills 命名空间） |
| 提示浮层 | render_unified_hint() 统一扁平列表，单一上限 10 条，排序：前缀匹配 > 类别 rank > 字母序 |
| 优先级 | 命令 > Skills（同名冲突时命令优先） |
| 消息解析 | /xxx token 直接触发 Skill 预加载，无需排除命令名 |

## Feature 附录

### feature_20260429_F001_skill-slash-trigger
**摘要:** Skills 触发键从 # 统一到 / 前缀
**关键决策:**
- Skills 触发键从 # 改为 /，与命令共用命名空间
- 提示浮层为统一扁平列表（命令与 Skills 混排，上限 10 条）
- Enter 触发时命令匹配优先，未命中再尝试 Skill 匹配
- Skill 名与命令名冲突时命令优先执行
- 消息解析中 /xxx token 直接触发 Skill 预加载，无需排除命令名
**归档:** [链接](../../archive/feature_20260429_F001_skill-slash-trigger/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [tui.md](./tui.md) — TUI 提示浮层渲染
- → [agent.md](./agent.md) — SkillsMiddleware 和 SkillPreloadMiddleware

最后更新：2026-10-26
