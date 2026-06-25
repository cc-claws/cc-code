---
name: peri-style
description: >
  Peri 项目的写作风格指南。当用户说"改一下表述"、"这段不像人话"、"太啰嗦了"、
  "要有故事感"、"像宣传文案"、"简洁一点"、"不要重复"时触发。
  也适用于用户要求润色 README、文档、宣传文案、或对已写文本的风格不满意时。
---

# Peri Style Guide

Peri 是一个个人项目的 README 和文档写作风格。核心原则：**简洁、有人味、不重复。**

---

## 品牌与理念

- **Nobody Coding** 是 Peri 的核心品牌词。在开篇提一次，在"How We Built"段落解释一次。绝不在第三个地方重复解释其含义。
- 开源的模型（DeepSeek、GLM）称为 **"partners"（伙伴）**，不是 tools 或 providers。赋予人格。
- 项目是 solo 项目，用 **"my"** 不是 "our"，用 **"I"** 不是 "we"。
- RISC-V 开发板描述为 **"my little RISC-V dev board"** —— 亲切、具体、有画面感。

## 写作风格

1. **每句话都要挣它的位置。** 删掉 filler：不要 "Here's the thing"、"We didn't set out to"、"If you're skeptical"。直接说事。
2. **讲故事，不讲课。** "The git log tells the story" 比 "Our commit history demonstrates" 好一百倍。"grew out of" 比 "originated from" 自然。用具体意象代替抽象陈述。
3. **不要复述用户的原话。** 用户说"把两个模型当成伙伴"，不要写"our two open-source partners"——换成自己的表达，比如 "models as partners" 换个角度说。
4. **数字和实物 > 抽象形容词。** "95-99% hit rate"、"50MB memory"、"12 core tools" 比 "high performance"、"lightweight"、"minimal" 有说服力。
5. **同一个概念只解释一次。** 发现自己在两个地方说同样的话，删掉一处。

## 结构约定

### 开篇段落（3-4 句）

```
身份声明 → 品牌理念 → 出身/兼容性 → 技术栈
```

例："Peri is built the Nobody Coding way: ... It grew out of X and Y. Built in Rust, runs on my little RISC-V dev board."

### 旁注/证据

用 blockquote `>` 轻描淡写，不当标题：

```
> The git log tells the story — recent commits are almost entirely DeepSeek and GLM.
```

### "Why X" 列表

顶级条目是**观点声明**（加粗短句），子条目是**具体证据**：

```markdown
- **Context optimized.** System prompt frozen at session start.
  - 95-99% prompt cache hit rate — minimal token waste.
  - No agent memory / auto-dream / extra calls to waste your tokens.
```

规则：
- 父级是主张，子级是论据
- 子条目不超过 3 条
- 数字和机制名放在子条目里

### 未完成功能

用 **"Unchecked but ready"** 低调声明，一行带过：

```markdown
- Unchecked but ready: built-in LSP, built-in split screen.
```

不用单独建章节，作为列表最后一条。

### 表格

左列是场景驱动的人话（"When you..."），不是技术标签：

```markdown
| When you... | Pipeline kicks off |
|---|---|
| **Find a bug or piece of tech debt** | `issue-create` → `systematic-debugging` → ... |
| **Want to build a new feature** | `grill-me` → ... |
```

---

## 中英文语境

项目是中文开发者主导，README 用英文。写作时要：
- 英文自然流畅，不强行模仿美国创业公司语气
- 不使用中文直译的英文表达
- 中文评论和英文正文混排正常，不需要解释

## 反模式（禁止）

- ❌ 在开篇和 How We Built 两处都解释 Nobody Coding 的含义
- ❌ "our" "we"（solo 项目用 "my" "I"）
- ❌ 把用户原话直接转录到文本里
- ❌ "runs on Rust"（Rust 是实现语言，不是运行时）
- ❌ 为未完成功能建独立章节
- ❌ 表格左列用技术分类标签
- ❌ "Extensive harness engineering ensures..."（太干）
- ❌ "Here's the thing that still trips people up"（filler）
