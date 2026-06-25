# @ Mention 搜索与注入设计

## 概述

在 TUI 输入框中实现 `@` 触发的文件搜索与引用功能，对齐 Claude Code 的 @ mention 交互模式。分阶段交付：阶段一文件引用，后续迭代 MCP 资源和 Agent 引用。

## 架构决策

- **独立模块**：新建 `at_mention/` 模块，不复用 `/` 命令提示（hint_ops）逻辑，关注点分离
- **仿 Read 工具消息注入**：文件内容以 `Ai[ToolUse{Read}] + Tool[ToolResult]` 消息序列注入，与 `SkillPreloadMiddleware` 模式对齐
- **实时 glob 搜索 + fuzzy-matcher 评分**：不做预索引，简单直接
- **输入框上方弹窗**：复用 `BorderedPanel` 渲染，与 hints 互斥

## 数据流

```
TUI 输入框: "请看 @src/main.rs 和 @lib/utils.rs#L10-20"
    ↓ submit_message() 原样发送
    ↓ executor → before_agent 阶段
    ↓ AtMentionMiddleware::before_agent()
    ↓ 正则提取 @path 和行范围
    ↓ 读取文件内容
    ↓ 注入 Ai[ToolUse{Read}] + Tool[ToolResult] 消息序列
    ↓ LLM 看到消息历史中已有 Read 调用结果
```

## Part 1：状态管理与触发检测

### 核心状态（`AtMentionState`）

新增文件：`peri-tui/src/app/at_mention/mod.rs`

```rust
pub struct AtMentionState {
    pub active: bool,
    pub query: String,           // @ 后面的搜索词
    pub query_start: usize,      // textarea 中 @ 符号的位置（字符索引）
    pub candidates: Vec<AtCandidate>,
    pub selected: usize,
    pub scroll_offset: usize,
}

pub struct AtCandidate {
    pub path: String,            // 相对路径
    pub display: String,         // 显示文本（中间截断）
    pub is_dir: bool,
    pub score: i64,              // fuzzy 评分
}
```

### 触发检测

在 `keyboard.rs` 的按键处理中，textarea 内容变化后检测：

1. 取光标前的文本
2. 正则匹配 `(^|\s)@([\p{L}\p{N}_\-./\\]*|"[^"]*"?)$`
3. 匹配成功且 query 非空 → 激活搜索
4. 匹配失败或 query 为空 → 关闭

### 交互键

| 按键 | 行为 |
|------|------|
| `↑/↓` / `Ctrl+P/Ctrl+N` | 导航候选列表 |
| `Tab` | 补全公共前缀（保持弹窗打开） |
| `Enter` | 选中当前候选，注入路径 |
| `Esc` | 关闭弹窗，保留 `@query` 文本 |
| 其他字符 | 正常输入，更新 query |

## Part 2：文件搜索与模糊匹配

新增文件：`peri-tui/src/app/at_mention/file_search.rs`

### 搜索流程

```
用户输入 @mai
    ↓ 提取 query = "mai"
    ↓ 确定搜索根路径
    ↓   - query 含 "/" → base=目录部分, pattern=文件名部分
    ↓   - query 无 "/" → base=".", pattern="mai"
    ↓ glob 搜索: base/**/*pattern*
    ↓ fuzzy-matcher 评分排序
    ↓ 取 top 15，构造 AtCandidate 列表
```

### glob 策略

- `glob::glob()` 在 `tokio::task::spawn_blocking` 中执行
- 自动排除 `target/`、`node_modules/`、`.git/`、`dist/`、`build/`
- 最大结果 200（glob 阶段截断），fuzzy 后取 top 15
- query 以 `/` 或 `./` 或 `~/` 开头时视为路径前缀，做前缀匹配

### fuzzy 评分

- 使用 `fuzzy_matcher::skim::SkimMatcherV2`
- 匹配目标：文件名（权重高）+ 完整路径（权重低）
- 排序：score 降序，同分按路径长度升序

### 去抖

- 输入后 50ms 去抖触发搜索
- 上一次搜索未返回时取消（`CancellationToken`）

## Part 3：弹窗渲染与路径注入

### 弹窗布局

```
┌─ @ 文件搜索 ─────────────────────┐
│  src/main.rs                      │  ← 选中项（高亮）
│  src/middleware/main_handler.rs    │
│  tests/main_test.rs               │
│  ...                              │
└───────────────────────────────────┘
┌─ 输入框 ──────────────────────────┐
│ 请 @mai|                          │
└───────────────────────────────────┘
```

- 最大显示 10 项，超出滚动
- 每项单行：`{图标} {中间截断路径}`，图标 `+` 文件 / `/` 目录
- 选中项主题色高亮，未选中 dim
- 弹窗宽度与输入框对齐

### 渲染集成

在 `render_session_column()` 中与 hints 弹窗互斥渲染。

### 路径注入

```
Before: "请 @mai|"
After:  "请 @src/main.rs |"
```

- 删除 textarea 中 `@query`，替换为 `@{selected_path}`
- 路径含空格用引号：`@"my file.rs"`
- 目录：路径后加 `/`，保持弹窗打开
- 文件：路径后加空格，关闭弹窗

## Part 4：消息发送时的附件解析（Middleware）

### 注入模式

仿 `SkillPreloadMiddleware`，在 `before_agent` 阶段注入 Read 工具消息：

```text
[Human "请看 @src/main.rs 和 @lib/utils.rs#L10-20"]
[Ai]    [ToolUse{Read, call_xxx, {path: "src/main.rs"}},
         ToolUse{Read, call_yyy, {path: "lib/utils.rs", offset: 10, limit: 11}}]
[Tool]  ToolResult{call_xxx, "src/main.rs 完整内容..."}
[Tool]  ToolResult{call_yyy, "lib/utils.rs 第 10-20 行内容..."}
```

### 新增文件

| 文件 | 职责 |
|------|------|
| `peri-middlewares/src/at_mention/mod.rs` | `AtMentionMiddleware` |
| `peri-middlewares/src/at_mention/parser.rs` | 正则提取 @path + 行范围 |
| `peri-middlewares/src/at_mention/file_reader.rs` | 文件读取 + 截断 |

### 解析正则

- 带引号：`@"([^"]+)"`
- 普通：`@([^\s]+)`
- 行范围后缀：`#L(\d+)(?:-(\d+))?`

### 边界处理

- 文件不存在：不注入，保留原文
- 文件超大（>2000 行）：截断，加截断提示
- 目录：列出子项（max 100）
- 原文 `@path` 保留不删

### middleware 从 state 获取上下文

- 从 `state.messages()` 取最后一条 Human 消息做正则提取
- `cwd` 从 middleware 构造时传入

## Part 5：模块结构与集成点

### 新增文件

```
peri-tui/src/app/at_mention/
├── mod.rs              # AtMentionState + 交互逻辑
└── file_search.rs      # glob 搜索 + fuzzy 评分（异步）

peri-middlewares/src/at_mention/
├── mod.rs              # AtMentionMiddleware
├── parser.rs           # 正则提取 @path + 行范围
└── file_reader.rs      # 文件读取 + 截断
```

### 修改文件

| 文件 | 改动 |
|------|------|
| `peri-tui/src/app/ui_state.rs` | 新增 `at_mention: AtMentionState` |
| `peri-tui/src/event/keyboard.rs` | `@` 触发检测 + 导航键拦截 |
| `peri-tui/src/ui/main_ui/mod.rs` | 弹窗渲染（与 hints 互斥） |
| `peri-acp/src/agent/builder.rs` | 加 `AtMentionMiddleware`（在 SkillPreload 之后） |
| `peri-middlewares/src/lib.rs` | 导出 `at_mention` |
| `peri-tui/src/app/mod.rs` | 导出 `at_mention` |

### 依赖新增

- `peri-tui/Cargo.toml`：加 `glob` + `fuzzy-matcher`

### 测试策略

- `parser.rs`：正则提取单元测试（各种 @path 格式、行范围、带引号）
- `file_reader.rs`：文件读取 + 截断测试
- `AtMentionMiddleware`：集成测试（state + Human 消息 → 验证注入的 ToolUse/ToolResult）
- `file_search.rs`：glob + fuzzy 评分测试
- `AtMentionState`：触发检测 + 路径注入测试

## 分阶段路线

| 阶段 | 内容 |
|------|------|
| 一 | 文件 @ mention（本文档） |
| 二 | MCP 资源引用（`@server:resource/path`） |
| 三 | Agent 引用（`@agent-name`） |
