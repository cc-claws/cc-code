# cc-code

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 Claude Code 全家桶**

[![npm](https://img.shields.io/npm/v/@cc-claw/code)](https://www.npmjs.com/package/@cc-claw/code)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE)

## 最近更新

v0.6.137：修复后台 shell 完成通知的**多轮级联唤醒**。通过 `Bash(run_in_background=true)` 启动的任务完成后会唤醒 agent，但旧逻辑在一轮内积压多条通知时每次 `Done` 只消费 1 条，导致 n 条通知触发 n 轮 turn（每轮再触发下一条），表现为连续无意义的「收到」。现按批次**合并注入**：空闲态把本批 + 队列中同会话待注入通知一次性 drain 拼接为**单条消息**（n 条压成 1 轮 turn），推理中统一入队、`Done` 后合并；合并仅在消息层拼接、保留每条通知 XML，信息不丢失（#491）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.136：一批 TUI 修复在同一版本落地。① 切入刚跑完的后台子 Agent 独立视口时，底部现常驻显示 `✻ Brewed for 32s · done 16:30` 完成总结行（此前完全空白），被停止/失败的子 Agent 不显示成功文案（#485）。② Windows 下默认静音全局分配器 mimalloc 的底层 `stderr` 裸打，避免长时间或高负载会话被 `mimalloc: cannot commit OS memory` 打花撕裂界面（#483）。③ 行首状态圆点统一为公共常量 `●`，消除子 Agent 行比其他工具「小一号」的字形割裂（#487）。④ 修复 auto-compact 被误判为手动 compact、中途提前关闭 loading 而引发的 Tip/spinner 消失、无法中断、回车反转等连锁问题（#481）。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.130：子 Agent 统一为**只有后台模式**，底栏抽屉是它们的唯一呈现。空输入框按 `↓` 浏览后台任务，`↑/↓` 移动游标、`Enter` 进入独立视口（独立滚动位置、输入草稿与工具统计）、`x` 停止选中任务、`Ctrl+X Ctrl+K` 停止全部；正在阅读的子 Agent 结束时留在屏幕上（该行转绿显示 `✓ completed`），离开视口才回收。启动摘要改为对齐的 `├── / └──` 树形块，抽屉只用亮白/灰两档文字、游标行不设底色，右侧 token 指标为跟随流向的单箭头。`fork` 与普通子 Agent 合并为同一条构建路径，`run_in_background` 参数退场。完整记录见 [CHANGELOG](https://github.com/cc-claws/cc-code/blob/main/CHANGELOG.md)。

v0.6.129：输入 `@` 即显示文件候选。默认仅列当前目录，最多 15 项；面板最多显示 10 行并适配终端高度，继续输入关键词可搜索更多文件。搜索中、空目录和无匹配结果均有提示。

## 安装

[![install](https://img.shields.io/badge/npm_install-g%20@cc--claw%2Fcode-47d147?style=for-the-badge&logo=npm&logoColor=white)](https://www.npmjs.com/package/@cc-claw/code)

```bash
npm install -g @cc-claw/code
```

## 升级

```bash
npm update -g @cc-claw/code
```

## 快速开始

```bash
# 启动交互式 TUI
cc-code

# 直接给任务
cc-code "解释这个项目的目录结构"

# 指定模型
cc-code --model deepseek/deepseek-chat "重构这个函数"
```

## 支持的平台

| 平台 | 架构 |
|------|------|
| Linux | x86_64, aarch64 |
| macOS | x86_64 (Intel), aarch64 (Apple Silicon) |
| Windows | x86_64 |

## 常用快捷键

| 快捷键 | 功能 |
|--------|------|
| `Shift+Tab` | 循环切换权限模式（auto / bypass，共两档） |
| `Ctrl+P` / `Alt+P` | 打开命令面板（Provider / Model / Effort 配置） |
| `Ctrl+B` | 将正在运行的前台 Shell 命令转入后台执行 |
| `Alt+V` / `Ctrl+V` | 从剪贴板粘贴图片附件（`Alt+V` 专用于穿透 Windows Terminal 等现代终端对 `Ctrl+V` 的拦截） |
| `Ctrl+O` | 切换详细模式（Verbose） |
| `Ctrl+U` / `Ctrl+D` | 消息列表快速半页滚动（输入框为空时） |
| `双击 Esc` | 打开 Rewind 会话历史回滚选择器 |
| `Ctrl+C` | 中断执行（执行中）/ 双击退出（空闲时） |

## 常用 Slash 命令

| 命令 | 说明 |
|------|------|
| `/model` | 浏览并选择 Provider/模型，或直接切换 |
| `/commit` | 基于当前代码改动一键生成 commit 并提交 |
| `/review` | 针对当前 PR 或分支代码发起代码审查 |
| `/export` | 将当前会话导出为干净的 Markdown 文档 |
| `/gc` | 手动释放内存并输出内存占用诊断信息 |
| `/lang <lang>` | 切换系统提示与界面语言（en / zh-CN） |
| `/recap` | 生成一句话会话回顾（目标 + 当前任务 + 下一步），别名 `/away`、`/catchup` |
| `/clear` | 清空屏幕开启新会话 |

## 为什么选 cc-code？

| 对比项 | 其他终端 Agent | cc-code |
|--------|---------------|------|
| 运行时 | Node.js / Bun，动辄吃 1GB 内存 | Rust 原生，启动快，~50MB 内存 |
| 模型绑定 | 锁死一家 LLM | 随便换：Anthropic、OpenAI 兼容、DeepSeek、GLM |
| Prompt 缓存 | 每轮重算，token 白烧 | 冻结 system prompt，95-99% 缓存命中率 |
| Claude Code 生态 | 不兼容 | 直接用 `.claude/` 配置、agents、skills、hooks、MCP |

## 链接

- [GitHub](https://github.com/cc-claws/cc-code)
- [Issues](https://github.com/cc-claws/cc-code/issues)
- [English README](https://github.com/cc-claws/cc-code#readme)
