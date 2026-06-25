> 归档于 2026-05-26，原路径 spec/issues/2026-05-26-login-panel-hardcoded-chinese-no-i18n.md

# Login 面板硬编码中文未走 i18n

**状态**：Fixed
**优先级**：低
**创建日期**：2026-05-26

## 问题描述

Login 面板（`/login` 命令）的两处代码存在硬编码中文字符串，未使用 `LcRegistry::tr()` 进行国际化。切换到英文语言后，login 面板的所有提示文字和快捷键描述仍显示中文。

## 症状详情

### 1. `status_bar_hints` 快捷键描述（`component.rs:336-367`）

`status_bar_hints` 方法接收 `_lc: &LcRegistry` 参数但完全未使用，所有描述直接硬编码为中文字面量。对应 FTL key 已存在但未被引用：

| 硬编码值 | 应使用的 i18n key |
|----------|------------------|
| `"导航"` | `hint-login-browse` |
| `"激活"` | `hint-login-activate` |
| `"编辑"` | `hint-login-edit` |
| `"新建"` | `hint-login-new` |
| `"删除"` | `hint-login-delete` |
| `"关闭"` | `hint-login-close` |
| `"字段"` | `hint-login-field` |
| `"保存"` | `hint-login-save` |
| `"粘贴"` | `hint-login-paste` |
| `"切换"` | `hint-login-toggle` |
| `"返回"` | `hint-login-back` |
| `"确认删除"` | `login-confirm-delete` |
| `"取消"` | `key-cancel` |

### 2. 渲染函数硬编码中文（`panels/login.rs`）

面板标题和内容文字全部硬编码：

| 位置 | 硬编码内容 | FTL key 状态 |
|------|-----------|-------------|
| 行 25 | `" /login — Provider 管理 "` | `login-panel-title-browse` 已存在 |
| 行 26 | `" /login — 编辑 Provider "` | `login-panel-title-edit` 已存在 |
| 行 27 | `" /login — 新建 Provider "` | `login-panel-title-new` 已存在 |
| 行 28 | `" /login — 确认删除 "` | **缺失**，需新增 |
| 行 82 | `"（未设置）"` | **缺失**，需新增 |
| 行 114 | `"  （无 provider，按 Ctrl+N 新建）"` | **缺失**，需新增 |
| 行 249 | `"  确认删除 "` | 同 `login-panel-title-confirm-delete` |
| 行 256 | `" ？"` | 同上可合并 |

## 涉及文件

- `peri-tui/src/app/login_panel/component.rs` — `status_bar_hints()` 方法，需改用 `_lc.tr()`
- `peri-tui/src/ui/main_ui/panels/login.rs` — 渲染函数，面板标题和提示文字需 i18n 化
- `peri-tui/locales/en/main.ftl` — 需补充缺失 key：`login-panel-title-confirm-delete`、`login-no-model`、`login-empty-hint`
- `peri-tui/locales/zh-CN/main.ftl` — 同步补充对应中文翻译
