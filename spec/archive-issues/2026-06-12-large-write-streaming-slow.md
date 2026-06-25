> 归档于 2026-06-24，原路径 spec/issues/2026-06-12-large-write-streaming-slow.md

# Write 工具超长内容流式输出时 LLM Provider 响应极慢

**状态**：Fixed
**优先级**：中
**创建日期**：2026-06-12

## 问题描述

当 Agent 使用 Write 工具写入较大文件时（几百行级别），LLM provider 流式输出超大 JSON（tool_use 的 `input` 对象包含完整 `content` 字段）时 token 生成速度急剧下降，表现为纯粹的响应变慢、无报错。用户期望通过分段输出机制来保证成功率和流畅度。

## 症状详情

| 现象 | 细节 |
|------|------|
| 触发规模 | 几百行内容即开始明显变慢 |
| 表现 | 纯性能下降，无 timeout/connection reset 等报错 |
| 现有机制 | Write 工具已支持 `append` 模式，工具描述提示"超过 200 行建议分段写入" |
| 模型遵从度 | 模型不一定遵循分段建议，仍可能一次性生成超长 JSON |

### 历史关联

此问题与已归档 issue `2026-05-15-write-tool-missing-filepath-max-tokens` 相关但维度不同：
- 历史 issue：超长内容触发 `max_tokens=4096` 截断，JSON 不完整导致 `file_path` 缺失（已通过 `max_tokens` 调整 + `append` 模式修复）
- 本次 issue：即使输出 token 预算充足，超大 JSON 的流式生成本身导致 provider 侧性能劣化

## 复现条件

- **复现频率**：文件内容在几百行以上时必现
- **触发步骤**：
  1. LLM 决定用 Write 工具写入一个几百行的文件
  2. LLM 生成包含完整 `content` 字段的 tool_use JSON
  3. Provider 流式输出过程中 token 生成速度逐渐下降
- **环境**：不确定是否特定于某个 Provider/模型

## 涉及文件

- `peri-middlewares/src/tools/filesystem/write.rs`（142 行）—— Write 工具实现，包含 `append` 模式及分段写入描述
- 工具提示词中的分段建议（第 19 行）：`"For files longer than 200 lines, consider writing in chunks"`——当前仅为建议性提示，无法保证模型执行

## 状态变更记录

| 日期 | 从 | 到 | 操作人 | 说明 |
|------|-----|-----|--------|------|
| 2026-06-12 | — | Open | agent | 创建 |
| 2026-06-12 | Open | Fixed | agent | 添加 Write 工具 2 分钟超时检测，超时后引导分段写入 |

## 修复记录

### 修复 #1（2026-06-12）

- **操作人**：agent
- **用户原意**：Write 大文件时 LLM 流式输出变慢，希望通过超时机制强制引导模型使用 append 分段写入
- **修复内容**：在 Write 工具 invoke 中包裹 `tokio::time::timeout(Duration::from_secs(120), ...)`，超时时返回英文错误提示引导 Agent 使用 `append=true` 分段写入
- **涉及 commit**：`f1b20650`
- **验证状态**：待验证
