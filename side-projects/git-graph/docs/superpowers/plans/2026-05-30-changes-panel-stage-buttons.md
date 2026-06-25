# Changes 面板文件暂存/取消暂存按钮 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Changes 面板的每个文件/目录行右侧添加 `[+]`/`[-]` 按钮，点击执行 `git add`/`git restore --staged`，实现文件在 Unstaged/Untracked 和 Staged 之间转移。

**Architecture:** 分三层实现：底层 `ops.rs` 新增 `stage_file`/`unstage_file` 方法；中间层 `status_panel.rs` 在每行右侧渲染按钮并记录按钮位置到 `StatusLayout`；顶层 `event.rs` 在鼠标点击时检测按钮区域并调用对应 git 操作后刷新。

**Tech Stack:** Rust, ratatui (TUI), git2 + CLI git, FNV hash

---

## File Structure

| 文件 | 变更类型 | 职责 |
|------|----------|------|
| `src/git/ops.rs` | Modify | 新增 `stage_file()` 和 `unstage_file()` |
| `src/ui/sidebar/status_panel.rs` | Modify | 每行渲染 `[+]`/`[-]`，`StatusLayout` 记录按钮位置 |
| `src/event.rs` | Modify | 鼠标点击检测按钮区域，执行操作并刷新 |
| `src/app.rs` | No change | 无需新增字段 |

---

### Task 1: 新增 git stage/unstage 操作

**Files:**
- Modify: `src/git/ops.rs`

- [ ] **Step 1: 在 `GitRepo impl` 中添加 `stage_file` 方法**

在 `src/git/ops.rs` 的 `impl GitRepo` 块中（`delete_branch` 之后），添加：

```rust
/// 将文件添加到暂存区（git add <path>）
pub fn stage_file(&self, path: &str) -> Result<()> {
    self.run_git(&["add", path])
}

/// 将文件从暂存区移回工作区（git restore --staged <path>）
pub fn unstage_file(&self, path: &str) -> Result<()> {
    self.run_git(&["restore", "--staged", path])
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build`
Expected: 编译成功，无错误

- [ ] **Step 3: Commit**

```bash
git add src/git/ops.rs
git commit -m "feat: add stage_file/unstage_file git operations"
```

---

### Task 2: StatusLayout 记录按钮位置

**Files:**
- Modify: `src/ui/sidebar/status_panel.rs`

- [ ] **Step 1: 扩展 `StatusLayout` 结构体**

在 `src/ui/sidebar/status_panel.rs` 中，`StatusLayout` 的 `dir_rows` 字段后添加按钮行记录：

```rust
/// 记录 status 面板中每个 section header 和目录行的行号（用于点击检测）
#[derive(Default)]
pub struct StatusLayout {
    pub staged_header_row: Option<u16>,
    pub unstaged_header_row: Option<u16>,
    pub untracked_header_row: Option<u16>,
    /// 目录行：(行号, 展开标识路径)  如 "staged:src/components/"
    pub dir_rows: Vec<(u16, String)>,
    /// 按钮行：(相对行号, 按钮所在列起始 x, 按钮类型, 操作路径)
    pub button_rows: Vec<(u16, u16, StatusButton, String)>,
}

/// status 面板按钮类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusButton {
    /// [+] 暂存文件/目录
    Stage,
    /// [-] 取消暂存文件/目录
    Unstage,
}
```

- [ ] **Step 2: 修改 `render_tree` 函数签名和逻辑，接收 section 类型并渲染按钮**

将 `render_tree` 的签名改为：

```rust
/// 将压缩后的目录树渲染为带缩进的行列表
/// `section` 标识当前属于哪个 section（决定按钮类型）
/// `area_width` 为渲染区域宽度（用于计算按钮右侧对齐位置）
fn render_tree(
    nodes: &[FlatNode],
    depth: usize,
    prefix: &str,
    collapsed: &HashSet<String>,
    dir_rows: &mut Vec<(u16, String)>,
    button_rows: &mut Vec<(u16, u16, StatusButton, String)>,
    row: &mut u16,
    lines: &mut Vec<Line<'static>>,
    theme: &crate::theme::GigTheme,
    section: StatusButton,
    area_width: u16,
) {
```

在 `FlatNode::Dir` 分支的 `spans` 末尾（`lines.push` 之前）添加按钮渲染：

```rust
// 目录行右侧添加按钮
let btn_text = match section {
    StatusButton::Stage => " +",
    StatusButton::Unstage => " -",
};
let btn_x = area_width.saturating_sub(4); // 右侧留 4 字符空间
let padding = btn_x.saturating_sub(current_line_width(&spans));
for _ in 0..padding {
    spans.push(Span::raw(" "));
}
spans.push(Span::styled(
    btn_text.to_string(),
    Style::default().fg(theme.accent()),
));
// 路径需要收集该目录下所有文件的路径前缀
// 对于目录行，使用完整压缩路径作为操作路径
let dir_path = format!("{}{}", prefix.trim_end_matches(':'), display_path);
button_rows.push((*row, btn_x, section, dir_path));
```

在 `FlatNode::File` 分支的 `spans` 末尾（`lines.push` 之前）添加按钮渲染：

```rust
// 文件行右侧添加按钮
let btn_text = match section {
    StatusButton::Stage => " +",
    StatusButton::Unstage => " -",
};
let btn_x = area_width.saturating_sub(4);
let padding = btn_x.saturating_sub(current_line_width(&spans));
for _ in 0..padding {
    spans.push(Span::raw(" "));
}
spans.push(Span::styled(
    btn_text.to_string(),
    Style::default().fg(theme.accent()),
));
// 文件路径需要拼接完整路径
let full_path = format!("{}{}", prefix.trim_end_matches(':'), name);
button_rows.push((*row, btn_x, section, full_path));
```

添加辅助函数：

```rust
/// 计算 spans 当前的显示宽度
fn current_line_width(spans: &[Span<'_>]) -> u16 {
    let w: usize = spans.iter().map(|s| s.content.len()).sum();
    w as u16
}
```

- [ ] **Step 3: 更新 `draw` 函数中的 `render_tree` 调用**

在 `draw` 函数中，三处 `render_tree` 调用分别改为：

```rust
// Staged section — 按钮为 Unstage (-)
if app.status_staged_expanded {
    let tree = build_tree(&status.staged);
    render_tree(&tree, 0, "staged:", &app.status_dir_collapsed,
        &mut layout.dir_rows, &mut layout.button_rows, &mut row, &mut lines, theme,
        StatusButton::Unstage, inner.width);
}

// Unstaged section — 按钮为 Stage (+)
if app.status_unstaged_expanded {
    let tree = build_tree(&status.unstaged);
    render_tree(&tree, 0, "unstaged:", &app.status_dir_collapsed,
        &mut layout.dir_rows, &mut layout.button_rows, &mut row, &mut lines, theme,
        StatusButton::Stage, inner.width);
}

// Untracked section — 按钮为 Stage (+)
if app.status_untracked_expanded {
    let tree = build_tree(&status.untracked);
    render_tree(&tree, 0, "untracked:", &app.status_dir_collapsed,
        &mut layout.dir_rows, &mut layout.button_rows, &mut row, &mut lines, theme,
        StatusButton::Stage, inner.width);
}
```

- [ ] **Step 4: 在 `draw` 函数滚动偏移修正中增加 button_rows 修正**

在已有的 scroll 偏移修正代码块后（`for (r, _) in &mut layout.dir_rows` 之后）添加：

```rust
for (r, _, _, _) in &mut layout.button_rows {
    *r = r.saturating_sub(scroll_u16);
}
```

- [ ] **Step 5: 验证编译**

Run: `cargo build`
Expected: 编译成功

- [ ] **Step 6: Commit**

```bash
git add src/ui/sidebar/status_panel.rs
git commit -m "feat: render +/- buttons on each status panel row"
```

---

### Task 3: 鼠标点击检测按钮并执行操作

**Files:**
- Modify: `src/event.rs`

- [ ] **Step 1: 在 status 面板点击处理中添加按钮检测逻辑**

在 `src/event.rs` 的 `handle_mouse` 函数中，找到 `// 目录行点击切换展开/折叠` 的 `for` 循环之后，添加按钮点击检测：

```rust
// 按钮点击检测（优先于目录行点击）
for &(btn_row, btn_x, btn_type, ref path) in &status_layout.button_rows {
    if rel_row == btn_row {
        // 计算按钮在屏幕上的绝对 x 坐标
        let abs_btn_x = sl.status_inner_y; // 不对，用 sidebar area 的 x
        let abs_btn_x = sa.x + 1 + btn_x; // +1 是左边框
        // 鼠标 column 在按钮范围内（btn_x ~ btn_x+2）
        if mouse.column >= abs_btn_x && mouse.column < abs_btn_x + 3 {
            match btn_type {
                status_panel::StatusButton::Stage => {
                    if let Err(e) = app.repo.stage_file(path) {
                        app.remote_status = Some(format!("暂存失败: {}", e));
                    } else {
                        let _ = app.reload();
                    }
                }
                status_panel::StatusButton::Unstage => {
                    if let Err(e) = app.repo.unstage_file(path) {
                        app.remote_status = Some(format!("取消暂存失败: {}", e));
                    } else {
                        let _ = app.reload();
                    }
                }
            }
            app.dirty = true;
            return; // 按钮点击不继续处理目录行点击
        }
    }
}
```

**重要**：按钮检测必须放在目录行点击检测 **之前**（在代码顺序上），这样按钮点击会先匹配到，不会被目录行折叠逻辑吃掉。因此需要将上面这段代码插入到 `for &(dir_row, ref key) in &status_layout.dir_rows` 循环 **之前**。

- [ ] **Step 2: 添加必要的 import**

在 `src/event.rs` 文件顶部，确保 `use crate::ui::sidebar::status_panel;` 可用。由于 `status_panel::StatusButton` 和 `status_panel::StatusLayout` 需要在 `event.rs` 中使用，需要确保模块导出正确。

在 `src/event.rs` 顶部添加：

```rust
use crate::ui::sidebar::status_panel;
```

- [ ] **Step 3: 重新组织 status 面板点击处理代码**

将 `handle_mouse` 中 status 面板点击部分（`else { // status 面板点击` 分支内）改为如下顺序：

```rust
} else {
    // status 面板点击
    app.focus = Focus::Status;
    let sl = &app.sidebar_layout;
    if let Some(ref status_layout) = sl.status_layout {
        // 计算点击的相对行号
        let rel_row = mouse.row.saturating_sub(sl.status_inner_y);
        // 加上滚动偏移得到实际行号
        let scroll = app.status_scroll;

        // 1. 按钮点击检测（最高优先级）
        for &(btn_row, btn_x, btn_type, ref path) in &status_layout.button_rows {
            if rel_row == btn_row {
                let abs_btn_x = sa.x + 1 + btn_x;
                if mouse.column >= abs_btn_x && mouse.column < abs_btn_x + 3 {
                    match btn_type {
                        status_panel::StatusButton::Stage => {
                            if let Err(e) = app.repo.stage_file(path) {
                                app.remote_status = Some(format!("暂存失败: {}", e));
                            } else {
                                let _ = app.reload();
                            }
                        }
                        status_panel::StatusButton::Unstage => {
                            if let Err(e) = app.repo.unstage_file(path) {
                                app.remote_status = Some(format!("取消暂存失败: {}", e));
                            } else {
                                let _ = app.reload();
                            }
                        }
                    }
                    app.dirty = true;
                    return;
                }
            }
        }

        // 2. Section header 点击
        if let Some(header_row) = status_layout.staged_header_row {
            if rel_row == header_row {
                app.status_staged_expanded = !app.status_staged_expanded;
            }
        }
        if let Some(header_row) = status_layout.unstaged_header_row {
            if rel_row == header_row {
                app.status_unstaged_expanded = !app.status_unstaged_expanded;
            }
        }
        if let Some(header_row) = status_layout.untracked_header_row {
            if rel_row == header_row {
                app.status_untracked_expanded = !app.status_untracked_expanded;
            }
        }

        // 3. 目录行点击切换展开/折叠
        for &(dir_row, ref key) in &status_layout.dir_rows {
            if rel_row == dir_row {
                if app.status_dir_collapsed.contains(key) {
                    app.status_dir_collapsed.remove(key);
                } else {
                    app.status_dir_collapsed.insert(key.clone());
                }
            }
        }
    }
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo build`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add src/event.rs
git commit -m "feat: handle +/- button clicks in status panel"
```

---

### Task 4: 目录级按钮路径修正

**Files:**
- Modify: `src/ui/sidebar/status_panel.rs`

目录行的按钮操作需要对整个目录执行 `git add` 或 `git restore --staged`。git 命令本身支持 `git add src/dir/` 这样的路径参数。但需要确保路径格式正确。

- [ ] **Step 1: 检查目录按钮路径生成逻辑**

在 Task 2 的 `render_tree` 中，`FlatNode::Dir` 分支的按钮路径生成需要确认：

- 目录路径格式为 `staged:src/components/`，需要去掉前缀 section 标识
- `prefix` 形如 `"staged:"`、`"unstaged:"`、`"untracked:"`
- 目录的 `display_path` 形如 `"src/components/"`

修正路径拼接逻辑为：

```rust
// 对于目录行，收集该目录下所有文件的实际路径
// prefix 形如 "staged:src/components/"，提取纯路径部分
let dir_path = if prefix.contains(':') {
    // 有 section 前缀的情况，取冒号后的部分 + display_path
    format!("{}{}", &prefix[prefix.find(':').unwrap() + 1..], display_path)
} else {
    display_path.clone()
};
```

注意：实际上更简单的做法是，在 `draw` 函数中传递给 `render_tree` 的 `prefix` 改为只包含 section 名称，路径由 `render_tree` 内部拼接。但为了最小改动，在按钮操作时使用 `prefix`（含 section 名）去掉冒号前缀即可。

**更正**：回看代码，`prefix` 在首次调用时为 `"staged:"`、`"unstaged:"`、`"untracked:"`，递归子目录时会变为 `"staged:src/components/"`。所以路径部分是 `prefix` 去掉开头的 section 名。

更简洁的方案：直接在 `render_tree` 中把完整的 `key`（即 `prefix + display_path`）作为按钮操作路径，在 event.rs 中去掉 section 前缀后传给 git 命令。

```rust
// 在 render_tree 的 Dir 分支中
let key = format!("{}{}", prefix, display_path);
// ... 渲染按钮 ...
button_rows.push((*row, btn_x, section, key));
```

```rust
// 在 render_tree 的 File 分支中
let full_path = format!("{}{}", prefix, name);
// ... 渲染按钮 ...
button_rows.push((*row, btn_x, section, full_path));
```

这样 `button_rows` 中的路径格式为 `"staged:src/main.rs"` 或 `"unstaged:src/components/"`。

在 `event.rs` 中执行操作时，去掉 section 前缀：

```rust
// 从 "staged:src/main.rs" 提取 "src/main.rs"
let git_path = if let Some(colon_pos) = path.find(':') {
    &path[colon_pos + 1..]
} else {
    path.as_str()
};
```

- [ ] **Step 2: 更新 event.rs 中的路径处理**

在 Task 3 的按钮点击处理中，将 `app.repo.stage_file(path)` 改为：

```rust
let git_path = if let Some(colon_pos) = path.find(':') {
    &path[colon_pos + 1..]
} else {
    path.as_str()
};
match btn_type {
    status_panel::StatusButton::Stage => {
        if let Err(e) = app.repo.stage_file(git_path) {
            app.remote_status = Some(format!("暂存失败: {}", e));
        } else {
            let _ = app.reload();
        }
    }
    status_panel::StatusButton::Unstage => {
        if let Err(e) = app.repo.unstage_file(git_path) {
            app.remote_status = Some(format!("取消暂存失败: {}", e));
        } else {
            let _ = app.reload();
        }
    }
}
```

- [ ] **Step 3: 验证编译和功能**

Run: `cargo build`
Expected: 编译成功

Run: `cargo run`
Expected: 在 Changes 面板中能看到每行右侧的 `+`/`-` 按钮，点击后文件在 Staged/Unstaged 之间移动

- [ ] **Step 4: Commit**

```bash
git add src/ui/sidebar/status_panel.rs src/event.rs
git commit -m "fix: correct directory button paths and section prefix stripping"
```

---

### Task 5: 处理 dir_rows 中路径格式一致性

**Files:**
- Modify: `src/ui/sidebar/status_panel.rs`

`dir_rows` 中的 key 格式为 `"staged:src/components/"`，而 `button_rows` 中也用相同的 key。需要确保 `render_tree` 中 `dir_rows` 和 `button_rows` 使用同样的 key 生成逻辑，并且在行号对齐上——目录行同时出现在两个列表中时，按钮检测优先。

- [ ] **Step 1: 确认 render_tree 中 Dir 分支的 key 生成一致**

在 `render_tree` 的 `FlatNode::Dir` 分支中，`key` 和按钮路径应该使用同一个变量：

```rust
FlatNode::Dir { display_path, children } => {
    let key = format!("{}{}", prefix, display_path);
    let is_expanded = !collapsed.contains(&key);
    let marker = if is_expanded { "▾" } else { "▸" };
    let mut spans = indent_spans(depth, theme);
    spans.push(Span::styled(
        format!("{} {}", marker, display_path),
        Style::default().fg(theme.text()),
    ));
    dir_rows.push((*row, key.clone()));

    // 目录行按钮
    let btn_text = match section {
        StatusButton::Stage => " +",
        StatusButton::Unstage => " -",
    };
    let btn_x = area_width.saturating_sub(4);
    let padding = btn_x.saturating_sub(current_line_width(&spans));
    for _ in 0..padding {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(btn_text.to_string(), Style::default().fg(theme.accent())));
    button_rows.push((*row, btn_x, section, key));

    lines.push(Line::from(spans));
    *row += 1;
    if is_expanded {
        render_tree(children, depth + 1, &key, collapsed, dir_rows, button_rows, row, lines, theme, section, area_width);
    }
}
```

注意：这里 `key` 已经包含 section 前缀（如 `"staged:src/"`），与 `dir_rows` 和 `button_rows` 一致。

- [ ] **Step 2: 确认 File 分支路径拼接**

```rust
FlatNode::File { name, status } => {
    let (ch, color) = status_style(*status, theme);
    let mut spans = indent_spans(depth, theme);
    spans.push(Span::styled(name.clone(), Style::default().fg(theme.muted())));
    spans.push(Span::styled(format!(" {}", ch), Style::default().fg(color)));

    // 文件行按钮
    let file_key = format!("{}{}", prefix, name);
    let btn_text = match section {
        StatusButton::Stage => " +",
        StatusButton::Unstage => " -",
    };
    let btn_x = area_width.saturating_sub(4);
    let padding = btn_x.saturating_sub(current_line_width(&spans));
    for _ in 0..padding {
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(btn_text.to_string(), Style::default().fg(theme.accent())));
    button_rows.push((*row, btn_x, section, file_key));

    lines.push(Line::from(spans));
    *row += 1;
}
```

- [ ] **Step 3: 验证编译和完整功能**

Run: `cargo build && cargo run`
Expected: 编译成功，按钮功能完整

- [ ] **Step 4: Commit**

```bash
git add src/ui/sidebar/status_panel.rs
git commit -m "refactor: unify path generation for dir_rows and button_rows"
```

---

## Self-Review Checklist

### 1. Spec Coverage

| 需求 | 对应 Task |
|------|-----------|
| Unstaged/Untracked 行右侧 [+] → git add | Task 1 (stage_file) + Task 2 (渲染) + Task 3 (点击) |
| Staged 行右侧 [-] → git restore --staged | Task 1 (unstage_file) + Task 2 (渲染) + Task 3 (点击) |
| 目录行按钮操作整个目录 | Task 4 (路径修正，git add dir/ 原生支持) |
| 立即执行 + 自动刷新 | Task 3 (reload() 刷新) |

### 2. Placeholder Scan

- 无 TBD/TODO
- 每个步骤都有完整代码
- 无 "similar to Task N" 引用

### 3. Type Consistency

- `StatusButton` 枚举在 `status_panel.rs` 中定义，在 `event.rs` 中通过 `status_panel::StatusButton` 引用
- `button_rows` 类型为 `Vec<(u16, u16, StatusButton, String)>`，所有使用处一致
- `stage_file(&self, path: &str)` 和 `unstage_file(&self, path: &str)` 签名与调用处匹配
- `render_tree` 新增参数 `section: StatusButton` 和 `area_width: u16`，所有调用处已更新
