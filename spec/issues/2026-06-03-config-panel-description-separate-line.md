# Config 面板 description 与值挤在同一行，布局拥挤

**状态**：Open
**优先级**：低
**创建日期**：2026-06-03

## 问题描述

`/config` 面板中，每个字段的说明文字（description）紧跟在值后面挤在同一行，导致行过长、信息密度过高、难以快速扫视。用户希望 description 放到字段标签下方独立一行（全宽），让布局更清晰。

## 症状详情

当前布局（description 和值同行）：
```
  Autocompact     [ON]  OFF  (ON/OFF — auto compact on context budget)
  Threshold       85█  (50-99, auto compact threshold)
  Language        [English]  简体中文  (auto/en/zh-CN)
```

期望布局（description 独立一行，标签下方全宽）：
```
  Autocompact     [ON]  OFF
  (ON/OFF — auto compact on context budget)
  Threshold       85█
  (50-99, auto compact threshold)
  Language        [English]  简体中文
  (auto/en/zh-CN)
```

## 涉及文件

- `peri-tui/src/ui/main_ui/panels/config.rs`（307 行）—— config 面板渲染，所有字段 description 作为 `Span` 追加在值后面

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-03 | — | Open | agent | 创建 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
