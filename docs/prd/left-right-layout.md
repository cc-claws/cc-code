# PRD: 左右分栏布局

## 1. 背景

当前 peri-tui 采用单列垂直布局，所有内容（消息、面板、输入框）都在同一列中。随着功能增加（面板系统、后台 Agent、文件树等），单列布局在宽屏下空间利用率低，且面板与消息区互相挤压。

mimo-code 采用左右分栏布局：
- 左侧：固定宽度 sidebar（项目列表/文件树）
- 右侧：主内容区（消息 + 输入）

用户希望在 peri 中实现类似的左右分栏布局，提升宽屏体验。

## 2. 目标

- 支持左右分栏布局，左侧为侧边栏，右侧为主内容区
- 保持现有单列布局作为默认模式（兼容窄屏）
- 快捷键切换布局模式
- 面板系统可选择在侧边栏或主内容区显示

## 3. 布局方案

### 3.1 布局模式

| 模式 | 描述 | 适用场景 |
|------|------|----------|
| `Single` | 当前单列布局（默认） | 窄屏 < 120 列 |
| `Sidebar` | 左侧固定侧边栏 + 右侧主内容区 | 宽屏 ≥ 120 列 |

### 3.2 区域划分

```
┌─────────────────────────────────────────────────────────────┐
│                     Sticky Header                           │
├──────────┬──────────────────────────────────────────────────┤
│          │                                                  │
│  Sidebar │              Message Area                        │
│  (24col) │                                                  │
│          │                                                  │
│  - 项目  │                                                  │
│  - 文件树│                                                  │
│  - 面板  │                                                  │
│          ├──────────────────────────────────────────────────┤
│          │              Attachment Bar                       │
│          ├──────────────────────────────────────────────────┤
│          │              Panel Area (可选)                    │
│          ├──────────────────────────────────────────────────┤
│          │              Input Area                          │
│          ├──────────────────────────────────────────────────┤
│          │              Status Bar                          │
└──────────┴──────────────────────────────────────────────────┘
```

### 3.3 尺寸约束

| 区域 | 宽度 | 高度 |
|------|------|------|
| Sidebar | 固定 24 列（可折叠） | 全高 |
| 主内容区 | 剩余宽度 | 全高 |
| Sticky Header | 主内容区宽度 | 动态（1-3 行） |
| Message Area | 主内容区宽度 | 动态（优先） |
| Attachment Bar | 主内容区宽度 | 3 行（有附件时） |
| Panel Area | 主内容区宽度 | 动态（60-75% 屏幕） |
| Input Area | 主内容区宽度 | 3-40% 屏幕高度 |
| Status Bar | 主内容区宽度 | 3 行 |
| BG Agent Bar | 主内容区宽度 | 动态（有后台 Agent 时） |

### 3.4 与 peri 现有面板系统兼容

peri 现有 12 种面板（`PanelKind`），分为 Session 和 Global 两种作用域：

**Session 面板**：
- `Agent`、`Hooks`、`Model`、`Login`、`Config`、`ThreadBrowser`

**Global 面板**：
- `Mcp`、`Plugin`、`Cron`、`Status`、`Memory`、`Tasks`

Sidebar 可选择承载部分面板，保持 `PanelComponent` trait 兼容：

| Sidebar Tab | 承载的面板 | 说明 |
|-------------|-----------|------|
| 项目 | - | 项目列表（新功能） |
| 文件 | - | 文件树（新功能） |
| 面板 | Model/Config/Plugin 等 | 复用现有 `PanelComponent` trait |

## 4. 功能设计

### 4.1 布局切换

- **快捷键**: `Ctrl+B` 切换 Sidebar 显示/隐藏
- **自动切换**: 终端宽度 < 120 列时自动切换为 Single 模式
- **状态持久化**: 布局模式保存到 session state

### 4.2 Sidebar 内容

Sidebar 支持多种内容模式（通过 Tab 切换）：

| Tab | 内容 | 说明 |
|-----|------|------|
| 项目 | 项目列表 | 多项目切换 |
| 文件 | 文件树 | 快速打开文件 |
| 面板 | 当前面板 | Model/Config/Plugin 等 |

### 4.3 面板布局策略

面板可选择在不同区域显示：

| 策略 | 描述 |
|------|------|
| `Bottom` | 在主内容区底部显示（当前行为） |
| `Sidebar` | 在 Sidebar 中显示 |
| `Right` | 在右侧新开一列（三栏布局，未来扩展） |

### 4.4 快捷键设计

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+B` | 切换 Sidebar 显示/隐藏 |
| `Ctrl+1` | Sidebar 切换到项目 Tab |
| `Ctrl+2` | Sidebar 切换到文件 Tab |
| `Ctrl+3` | Sidebar 切换到面板 Tab |
| `Ctrl+\` | 切换面板布局策略（Bottom/Sidebar） |

## 5. 技术方案

### 5.1 数据结构

```rust
/// 布局模式
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutMode {
    /// 单列布局（窄屏）
    Single,
    /// 左右分栏布局
    Sidebar,
}

/// Sidebar 内容模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarTab {
    /// 项目列表
    Projects,
    /// 文件树
    Files,
    /// 面板
    Panels,
}

/// 面板布局策略
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelPlacement {
    /// 主内容区底部（当前行为）
    Bottom,
    /// Sidebar 中
    Sidebar,
}

/// 布局状态
pub struct LayoutState {
    /// 当前布局模式
    pub mode: LayoutMode,
    /// Sidebar 是否展开
    pub sidebar_expanded: bool,
    /// Sidebar 当前 Tab
    pub sidebar_tab: SidebarTab,
    /// 面板布局策略
    pub panel_placement: PanelPlacement,
    /// Sidebar 宽度（列数）
    pub sidebar_width: u16,
}
```

### 5.2 渲染逻辑

```rust
pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    
    // 自动切换布局模式
    if area.width < 120 && app.layout.mode == LayoutMode::Sidebar {
        app.layout.mode = LayoutMode::Single;
    }
    
    match app.layout.mode {
        LayoutMode::Single => render_single_layout(f, app, area),
        LayoutMode::Sidebar => render_sidebar_layout(f, app, area),
    }
}

fn render_sidebar_layout(f: &mut Frame, app: &mut App, area: Rect) {
    // 水平分割：Sidebar | 主内容区
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(if app.layout.sidebar_expanded { 24 } else { 0 }),
            Constraint::Min(1),
        ])
        .split(area);
    
    let sidebar_area = chunks[0];
    let main_area = chunks[1];
    
    // 渲染 Sidebar
    if app.layout.sidebar_expanded {
        render_sidebar(f, app, sidebar_area);
    }
    
    // 渲染主内容区（复用现有逻辑）
    render_session_column(f, app, main_area);
}
```

### 5.3 Sidebar 渲染（兼容 PanelComponent trait）

```rust
fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    // 垂直分割：Tab 栏 | 内容区
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tab 栏
            Constraint::Min(1),    // 内容区
        ])
        .split(area);
    
    // 渲染 Tab 栏
    render_sidebar_tabs(f, app, chunks[0]);
    
    // 渲染内容区
    match app.layout.sidebar_tab {
        SidebarTab::Projects => render_project_list(f, app, chunks[1]),
        SidebarTab::Files => render_file_tree(f, app, chunks[1]),
        SidebarTab::Panels => {
            // 复用现有 PanelComponent trait
            if let Some(panel) = app.get_active_panel() {
                panel.render(f, app, chunks[1]);
            }
        }
    }
}
```

### 5.4 面板布局策略（兼容现有 PanelManager）

```rust
// 在 render_session_column 中根据 panel_placement 决定面板渲染位置
fn render_session_column(f: &mut Frame, app: &mut App, area: Rect) {
    // ... 现有逻辑 ...
    
    // 面板区域渲染
    if panel_height > 0 {
        let panel_area = chunks[3];
        
        // 检查面板是否应该在 Sidebar 中显示
        if app.layout.panel_placement == PanelPlacement::Sidebar 
            && app.layout.sidebar_expanded 
            && app.layout.sidebar_tab == SidebarTab::Panels
        {
            // 面板已在 Sidebar 中渲染，跳过
        } else {
            // 在主内容区底部渲染面板（现有逻辑）
            if let Some(panel) = app.get_active_panel() {
                panel.render(f, app, panel_area);
            }
        }
    }
}
```

## 6. 实现计划

### Phase 1: 基础布局框架（1-2 天）

- [ ] 添加 `LayoutState` 数据结构
- [ ] 实现 `LayoutMode::Sidebar` 渲染逻辑
- [ ] 实现 `Ctrl+B` 快捷键切换
- [ ] 终端宽度自动检测

### Phase 2: Sidebar 基础功能（2-3 天）

- [ ] 实现 Sidebar Tab 栏
- [ ] 实现项目列表 Tab
- [ ] 实现文件树 Tab（基础版）
- [ ] 面板可选择在 Sidebar 显示

### Phase 3: 优化与完善（1-2 天）

- [ ] Sidebar 宽度可调整
- [ ] 动画过渡效果
- [ ] 窄屏自动折叠
- [ ] 状态持久化

## 7. 兼容性

### 7.1 布局兼容

- 保持现有 `Single` 布局为默认模式
- 所有现有功能在两种布局模式下都可用
- 快捷键不冲突（`Ctrl+B` 当前未使用）

### 7.2 面板系统兼容

- 复用现有 `PanelComponent` trait，面板无需修改即可在 Sidebar 中显示
- `PanelManager` 和 `PanelScope`（Session/Global）机制保持不变
- 面板互斥组（`MutexGroup`）逻辑不变

### 7.3 消息渲染兼容

- `MessagePipeline` 和 `RenderCache` 机制不变
- `wrap_map` 根据主内容区宽度重新计算
- `render_view_model` 已支持响应式宽度（thinking 渲染已修复）

### 7.4 其他组件兼容

| 组件 | 兼容性说明 |
|------|-----------|
| Sticky Header | 宽度跟随主内容区 |
| Attachment Bar | 宽度跟随主内容区 |
| Status Bar | 宽度跟随主内容区 |
| BG Agent Bar | 宽度跟随主内容区 |
| Input Area | 宽度跟随主内容区 |
| 弹窗系统 | 居中显示，不受 Sidebar 影响 |

## 8. 参考

- mimo-code: `packages/app/src/pages/layout/sidebar-shell.tsx`
- VS Code: 左侧 Activity Bar + Sidebar + Editor
- Cursor: 类似 VS Code 的左右分栏布局
