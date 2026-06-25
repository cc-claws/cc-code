> 归档于 2026-05-17，原路径 spec/issues/2026-05-16-i18n-language-not-in-setup.md
# Setup 向导缺少语言配置步骤

**状态**Fixed
**优先级**：中
**创建日期**：2026-05-16

## 问题描述

Setup 向导（启动时当无可用 Provider 时触发）目前只有三个步骤：Choose（配置来源）→ Form（Provider 表单）→ Done（确认）。i18n 模块已支持 `en` 和 `zh-CN` 两种语言，`AppConfig.language` 字段也已存在，但 setup 向导中没有语言选择环节。用户首次完成 setup 后，`language` 字段为空，应用始终以 `en` 运行，无法在初始配置阶段切换到 `zh-CN`。

## 症状详情

| 现象 | 详情 |
|------|------|
| 缺少语言步骤 | Setup 向导只有 Choose / Form / Done 三步，没有语言选择页 |
| 无法切换中文 | 初次运行时无法选择 `zh-CN`，只能默认 `en` |
| 保存时遗漏 | `save_setup_to()` 不写入 `language` 字段，config 持久化后该字段为 `None` |

## 期望

在 Choose 和 Form 步骤之间增加独立的 **Language** 步骤，用户可选择 `English`（en）或 `中文`（zh-CN），选择结果写入 `AppConfig.language`。

## 涉及文件

- `peri-tui/src/app/setup_wizard.rs` —— Setup 步骤状态机定义（`SetupStep` 枚举、按键处理）
- `peri-tui/src/ui/main_ui/popups/setup_wizard.rs` —— Setup 界面渲染
- `peri-tui/src/config/types.rs:132` —— `AppConfig.language` 字段定义
