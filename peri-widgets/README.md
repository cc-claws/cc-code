# peri-widgets

基于 [Ratatui](https://ratatui.rs) 的可复用 TUI Widget 组件库。

## 概述

`peri-widgets` 提供 cc-code TUI 界面所需的所有基础组件，特点：

- **零业务依赖**：仅依赖 ratatui + pulldown-cmark
- **主题系统**：统一的 DarkTheme 配色方案
- **可测试**：每个组件配套 `_test.rs` 文件
- **Feature 控制**：Markdown 渲染通过 feature 按需启用

## 组件列表

| 组件 | 模块 | 说明 |
|------|------|------|
| **BorderedPanel** | `bordered_panel` | 带标题边框的面板容器 |
| **DiffViewer** | `diff` | Diff 渲染（行级 + 词级高亮） |
| **FileTree** | `file_tree` | 文件树视图，支持展开/折叠 |
| **Form** | `form` | 表单组件（字段 + 焦点管理） |
| **InputField** | `input_field` | 单行输入框 |
| **List** | `list` | 可滚动列表 |
| **ListOverlay** | `list_overlay` | 浮层列表（弹窗选择） |
| **MessageBlock** | `message_block` | 消息块渲染 |
| **RadioGroup** | `radio_group` | 单选按钮组 |
| **CheckboxGroup** | `checkbox_group` | 复选框组 |
| **ScrollableArea** | `scrollable` | 可滚动区域 + 滚动条 |
| **Spinner** | `spinner` | 加载动画（多种模式） |
| **TabBar** | `tab_bar` | 标签栏 |
| **ToolCallWidget** | `tool_call` | 工具调用状态展示 |
| **Markdown** | `markdown` | Markdown 渲染（需 `markdown` feature） |

## 使用示例

### Diff 渲染

```rust
use peri_widgets::diff::{DiffInput, compute_diff};

let input = DiffInput {
    file_path: "src/main.rs".to_string(),
    old_content: "fn main() {}".to_string(),
    new_content: "fn main() { println!(\"hello\"); }".to_string(),
    is_new_file: false,
    is_deleted_file: false,
    is_binary: false,
};

let result = compute_diff(&input);
// result.hunks 包含结构化的 diff 数据
```

### 文件树

```rust
use peri_widgets::file_tree::{FileTreeState, FileNode};

let mut state = FileTreeState::new();
state.set_root(vec![
    FileNode {
        name: "src".to_string(),
        is_dir: true,
        children: vec![
            FileNode {
                name: "main.rs".to_string(),
                is_dir: false,
                children: vec![],
                expanded: false,
                loaded: true,
                path: Some("src/main.rs".to_string()),
            },
        ],
        expanded: true,
        loaded: true,
        path: Some("src".to_string()),
    },
]);
```

### Spinner 动画

```rust
use peri_widgets::spinner::{SpinnerState, SpinnerMode, SpinnerWidget};

let mut spinner = SpinnerState::new();
spinner.set_mode(SpinnerMode::Thinking);

// 在渲染循环中
spinner.tick(); // 更新动画帧
frame.render_widget(SpinnerWidget::new(&spinner), area);
```

### 主题系统

```rust
use peri_widgets::theme::{Theme, DarkTheme};

let theme = DarkTheme;
// 使用 theme 获取颜色配置
```

## 主题系统

```rust
use peri_widgets::theme::{Theme, DarkTheme};

let theme = DarkTheme;

// 使用主题颜色
let style = Style::default()
    .fg(theme.text_color())
    .bg(theme.background_color());
```

## Feature Flags

| Feature | 默认 | 说明 |
|---------|------|------|
| `markdown` | 否 | 启用 Markdown 渲染（pulldown-cmark） |
| `markdown-highlight` | 否 | 启用代码高亮（syntect） |

## 设计原则

1. **纯渲染**：组件只负责渲染，不持有业务状态
2. **状态分离**：`*State` 结构体管理交互状态，`*Widget` 负责渲染
3. **可组合**：组件可嵌套使用
4. **跨平台**：正确处理 CJK 字符宽度（unicode-width）

## 测试

```bash
cargo test -p peri-widgets
```

每个组件都有对应的 `_test.rs` 文件，确保渲染逻辑正确。
