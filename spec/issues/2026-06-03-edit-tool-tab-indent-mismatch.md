# Edit 工具无法编辑 tab 缩进的文件（Python 等）

**状态**：Open
**优先级**：中
**创建日期**：2026-06-03

## 问题描述

在编辑使用 tab 字符缩进的文件（如 Python tab 风格代码）时，Edit 工具的精确字符串匹配失败。LLM 通过 Read 工具读取文件后，生成 `old_string` 时无法正确复现 tab 字符（通常会转为空格），导致 `content.contains(old_string)` 匹配不上，返回 `old_string not found` 错误。

## 症状详情

| 场景 | 表现 |
|------|------|
| Python tab 缩进文件 | LLM 调用 Edit 时 `old_string` 中的缩进被替换为空格，匹配失败 |
| 其他 tab 缩进文件（Makefile 等） | 同样问题 |

**具体表现**：
1. LLM 通过 Read 工具读取含 tab 缩进的文件
2. Read 工具原样输出内容（tab 字符被保留）
3. LLM 在生成 Edit 调用时，`old_string` 参数中的 tab 被替换为空格
4. Edit 工具执行精确匹配 `content.contains(old_string)` → 失败
5. 返回 `Error: old_string not found`

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 准备一个 Python 文件，使用 tab 缩进（而非空格）
  2. 让 LLM 通过 Read 读取该文件
  3. 让 LLM 尝试 Edit 修改该文件中的代码
  4. Edit 工具返回 `old_string not found` 错误
- **环境**：所有模型均可能受影响（LLM 对 tab 字符的保真度普遍较低）

## 涉及文件

- `peri-middlewares/src/tools/filesystem/edit.rs` —— Edit 工具实现，使用精确字符串匹配
- `peri-middlewares/src/tools/filesystem/read.rs` —— Read 工具实现，原样输出 tab 字符

## 可能的改进方向

- 在 Edit 工具中增加 tab/空格模糊匹配（当精确匹配失败时，尝试将 tab/空格等价匹配）
- 在 Read 工具输出中对 tab 字符做可视化标记（如 `→`），帮助 LLM 识别 tab
- 在 Edit 工具的描述中增加对 tab 文件的特殊提示

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-03 | — | Open | agent | 创建 |

## 修复记录

（由 fix-issue 或 issue-verify skill 追加，创建时留空）
