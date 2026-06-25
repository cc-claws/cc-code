# MiMo Code UI vs Peri TUI 对比分析

> 分析对象：MiMo Code（GitHub: XiaomiMiMo/MiMo-Code）的 UI 布局与实现
> 分析目的：评估 Peri TUI 能否借鉴其优点
> 结论日期：2026-06-22

## 核心结论

**MiMo Code 是 Web UI（SolidJS + 浏览器渲染），Peri 是终端 TUI（ratatui）——两者根本不在一个赛道。**

MiMo 的很多布局优势来自像素级 GUI 能力（拖拽排序、响应式断点、富文本编辑器、动画），这些在终端里**物理上做不到**。逐项筛选后，真正值得借鉴的只有**主题系统**。

---

## 逐项对比

### 已经打平或 Peri 更强的

| MiMo 特性 | Peri 现状 | 判定 |
|-----------|----------|------|
| 消息虚拟滚动（turnInit/turnBatch 窗口） | viewport 二分裁剪 + committed scrollback | **Peri 更好**，O(log n) |
| PromptInput（附件/图片/斜杠/历史） | multiline + slash + @mention + shell + 图片粘贴 + 历史 | **打平** |
| 插件系统 / 技能系统 / MCP | 全有 | **打平** |
| 键盘快捷键体系 | 比 MiMo 更完整（HITL/permission mode/streaming） | **Peri 更多** |
| 代码 diff | 内联 diff（Ctrl+O），MiMo 是独立 review tab | 终端里内联更省空间 |

### 终端做不了 / 没意义的

- **侧边栏拖拽排序** — 终端没有鼠标拖拽
- **响应式断点（768px）** — 终端永远是"桌面"
- **富文本编辑器** — 终端 textarea 是纯文本
- **动画/抽屉式侧边栏** — 终端帧率受限
- **面板宽度拖拽调整** — 终端只能字符级

### 真正值得借鉴的（1 项）

**主题系统** —— 这是最大的差距，也是最容易落地的：

- MiMo：多主题 + light/dark/system 切换 + JSON 自定义主题 + 实时预览
- Peri：**单个硬编码暗色主题**（`peri-tui/src/ui/theme.rs` 里 ~25 个颜色常量写死）

终端里完全可做——ratatui 支持运行时换色。实现思路：

1. 把 `theme.rs` 的颜色常量改成运行时可配置的结构体
2. 支持从 `~/.peri/themes/*.json` 加载自定义主题
3. `/theme` 命令或 Config 面板里切换
4. 至少加一个 light theme

### 可以考虑但不急的

- **文件树面板** — MiMo 有侧边文件树，Peri 没有。可以加个 `FileTree` panel variant，但对 agent coding 场景价值不大（agent 自己管文件）。
- **轻量 session 快切** — Peri 有 ThreadBrowser 面板，但 MiMo 的 `Alt+↑/↓` 快速切会话更轻量。可以加个快捷键调出精简版 session 列表。

---

## 附：Peri TUI 现有能力概览（供参考）

| 维度 | 现状 |
|------|------|
| 布局 | 单列垂直栈（消息区 + 输入 + 状态栏 + 底部面板），无侧边栏 |
| 面板 | 12 种 PanelKind（Model/Login/Config/Agent/Hooks/ThreadBrowser/Mcp/Plugin/Cron/Tasks/Status/Memory） |
| 会话管理 | 单活跃会话 + ThreadBrowser 面板浏览历史（SQLite 持久化） |
| 消息渲染 | 语义 hash diff + viewport 二分裁剪 + 流式 markdown + 文本选择 |
| 输入 | multiline textarea + slash 命令 + @mention + shell 模式 + 图片粘贴 + 历史导航 |
| Diff | 内联 diff（Ctrl+O 切换），word-level 粒度 |
| 分屏 | 已移除（2026-06-01 计划） |
| 终端面板 | 无集成 PTY，仅 shell 模式（`!` 前缀）和 Bash 工具 |
| 主题 | 单个硬编码暗色主题 |
| 性能 | 后台渲染线程 + 语义 hash 增量 + 鼠标事件合并 + resize 防抖 |

---

## 行动建议

| 优先级 | 事项 | 理由 |
|--------|------|------|
| P1 | 主题系统可配置化 | 唯一实质性差距，终端可落地，用户感知强 |
| P3 | 文件树面板 | 锦上添花，agent 场景价值有限 |
| P3 | 轻量 session 快切 | ThreadBrowser 已够用，优化体验而已 |
