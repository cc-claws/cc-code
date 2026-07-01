# peri-tui

基于 [Ratatui](https://ratatui.rs) 的终端用户界面，纯 ACP client 前端。

## 概述

`peri-tui` 是 cc-code 的终端界面，特点：

- **纯 ACP 客户端**：通过 MpscTransport 与 ACP Server 通信
- **全屏 TUI**：alternate screen + 鼠标支持
- **流式渲染**：16ms 自适应帧率
- **i18n 支持**：中英文切换
- **丰富交互**：面板系统、弹窗、快捷键

## 启动

```bash
# 标准启动
cargo run -p peri-tui

# HITL 审批模式
cargo run -p peri-tui -- -a

# 继续上次会话
cargo run -p peri-tui -- -c

# 恢复指定会话
cargo run -p peri-tui -- -r <session-id>
```

## 界面布局

```
┌─────────────────────────────────────────────┐
│ Sticky Header (滚动时固定显示)               │
├─────────────────────────────────────────────┤
│                                             │
│ Message Area (消息列表 + 滚动条)            │
│                                             │
├─────────────────────────────────────────────┤
│ Attachment Bar (图片附件)                    │
├─────────────────────────────────────────────┤
│ Panel Area (面板区域，可选)                  │
├─────────────────────────────────────────────┤
│ Input Area (输入框，3-40% 高度)             │
├─────────────────────────────────────────────┤
│ Status Bar (状态栏，3行)                     │
├─────────────────────────────────────────────┤
│ BG Agent Bar (后台 Agent 列表)               │
└─────────────────────────────────────────────┘
```

## 快捷键

### 全局

| 快捷键 | 说明 |
|--------|------|
| `Enter` | 发送消息（idle）/ 缓冲消息（loading） |
| `Shift+Enter` / `Alt+Enter` | 插入换行 |
| `Esc` | 取消/关闭（双击打开 Rewind 选择器） |
| `Ctrl+C` | 中断 Agent（loading）/ 双击退出（idle） |
| `Ctrl+T` | 切换模型 alias |
| `Ctrl+P` | 命令面板（Provider/Model/Effort） |
| `Ctrl+B` | 后台运行 Shell 命令 |
| `Ctrl+O` | 切换详细模式 |
| `Ctrl+V` | 粘贴剪贴板（优先图片，回退文字） |
| `Shift+Tab` | 循环切换权限模式 |
| `Up/Down` | 消息区滚动 |
| `PageUp/PageDown` | 半页滚动（textarea 空时） |
| `Home/End` | 滚动到顶/底（textarea 空时） |
| `Del` | 删除最后一个待发送附件 |

### 输入框编辑

| 快捷键 | 说明 |
|--------|------|
| `Ctrl+A` | 光标移到行首 |
| `Ctrl+E` | 光标移到行尾 |
| `Ctrl+K` | 删除光标到末尾 |
| `Ctrl+U` | 删除开头到光标 |
| `Ctrl+Up/Down` | 光标上下移动 / 命令历史 |
| `Tab` | @ 提及补全 / 命令提示补全 |

### 面板

| 快捷键 | 说明 |
|--------|------|
| `Tab` | 切换焦点 |
| `↑/↓` | 列表导航 |
| `←/→` | 横向切换（枚举字段） |
| `Space` | 选择/切换 |
| `Enter` | 确认 |
| `Esc` | 关闭面板 |
| `Ctrl+D` | 删除条目（Provider/Cron/Plugin 等） |

### 文本选择

| 操作 | 说明 |
|------|------|
| 单击 | 定位光标 |
| 双击 | 选中单词/整行 |
| 拖动 | 选择文本 |
| 松开鼠标 | 自动复制到剪贴板 |

## Slash 命令

### 核心命令

| 命令 | 说明 |
|------|------|
| `/help` | 列出所有命令 |
| `/clear` | 清空对话 |
| `/config` | 查看/编辑运行时配置 |
| `/history` | 历史对话浏览 |
| `/doctor` | 诊断配置完整性 |
| `/gc` | 手动内存回收 + RSS/jemalloc 诊断 |
| `/export` | 对话导出为 Markdown |
| `/exit` | 退出程序 |

### 面板命令

| 命令 | 说明 |
|------|------|
| `/model [alias]` | 打开模型选择面板或直接切换 |
| `/login` | Provider 配置管理 |
| `/plugin` | 插件市场/管理面板 |
| `/mcp` | MCP 服务器管理面板 |
| `/hooks` | Hooks 配置查看（只读） |
| `/cron` | 定时任务管理面板 |
| `/agents` | SubAgent 定义管理 |
| `/memory` | Memory 文件管理面板 |
| `/tasks` | 后台任务面板 |

### Session 命令

| 命令 | 说明 |
|------|------|
| `/rename [name]` | 查看或修改当前会话标题 |
| `/channel` | Channel 配置 |
| `/context` | 上下文窗口使用情况 |
| `/cost` | Token 用量和成本 |
| `/lang <lang>` | 切换语言（en/zh-CN） |
| `/effort <level>` | 设置推理力度（low/medium/high/max） |
| `/loop` | 循环执行 |
| `/setup` | 重新运行 Setup Wizard |
| `/init` | 初始化项目配置 |
| `/commit` | Git 提交（透传到 Agent） |
| `/review` | PR 代码审查（透传到 Agent） |

## 面板系统

12 种面板，分 Session/Global 作用域：

### Session 面板

| 面板 | 说明 |
|------|------|
| ModelPanel | 模型选择 |
| LoginPanel | 登录配置 |
| ConfigPanel | 配置管理 |
| AgentPanel | Agent 列表 |
| HooksPanel | Hooks 配置 |
| ThreadBrowser | 会话历史 |

### Global 面板

| 面板 | 说明 |
|------|------|
| McpPanel | MCP 服务器 |
| PluginPanel | 插件管理 |
| CronPanel | 定时任务 |
| TasksPanel | 任务列表 |
| StatusPanel | 系统状态 |
| MemoryPanel | 记忆管理 |

## 弹窗系统

4 种弹窗，通过 `InteractionPrompt` 互斥管理：

| 弹窗 | 说明 |
|------|------|
| HITL 审批 | 敏感操作拦截 |
| AskUser 问答 | Agent 向用户提问 |
| OAuth 授权 | MCP OAuth 认证 |
| Setup Wizard | 首次配置向导 |

## i18n

支持中英文切换：

```bash
/lang en    # 切换到英文
/lang zh-CN # 切换到中文
```

翻译资源：`locales/{lang}/main.ftl`

## 配置

```bash
# 权限模式
--permission-mode bypass|default|dont-ask|accept-edit|auto-mode

# 模型
--model <name>

# 推理强度
--effort low|medium|high|max
```

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI API Key |
| `OPENAI_MODEL` | 默认模型 |
| `YOLO_MODE` | 跳过 HITL |
| `RUST_LOG` | 日志级别 |

## 依赖关系

```
peri-tui
  ├── peri-acp          # ACP 客户端（运行时通信）
  ├── peri-agent        # 类型依赖（BaseMessage 等）
  ├── peri-middlewares   # 类型依赖
  ├── peri-widgets      # UI 组件
  ├── ratatui           # TUI 框架
  └── crossterm         # 终端控制
```

## 核心文件

| 文件 | 职责 |
|------|------|
| `src/main.rs` | 入口，终端初始化 |
| `src/app/mod.rs` | 应用状态管理 |
| `src/app/agent.rs` | 事件映射 |
| `src/app/message_pipeline.rs` | 消息管线 |
| `src/ui/main_ui/mod.rs` | 主布局 |
| `src/acp_client/client.rs` | ACP 客户端 |
| `src/i18n/` | 国际化 |

## 测试

```bash
cargo test -p peri-tui
```

## 详细文档

- [CLAUDE.md](./CLAUDE.md) — 开发指南和陷阱记录
- [ACP 协议](../peri-acp/README.md)
- [Widget 组件](../peri-widgets/README.md)
