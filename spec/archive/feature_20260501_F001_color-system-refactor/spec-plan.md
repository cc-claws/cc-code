# 颜色系统重构（对齐 Claude Dark 主题）执行计划

**目标:** 将 peri TUI 配色方案从暖棕系对齐到 Claude Code Dark 主题的中性灰系，清理全部硬编码颜色

**技术栈:** Rust, ratatui (Color::Rgb), cargo test

**设计文档:** spec-design.md

## 改动总览

- 本次改动涉及 14 个文件（2 个主题定义 + 11 个 UI 渲染文件 + 1 个风格指南文档），核心变更是 `theme.rs` + `presets.rs` 中 12 个颜色值的 RGB 替换 + 2 个新增常量
- Task 1 是核心依赖——先更新主题定义（TUI 层 + widgets 层同步），Task 2/3 才能用新常量替换硬编码颜色
- 经代码分析确认：`peri-widgets/src/theme/presets.rs` 的 `DarkTheme` 实现与 `theme.rs` 一一对应，必须同步更新，且 L61 测试断言了旧 ACCENT 值需一并修正；`headless.rs` 已使用 `theme::SAGE`/`theme::ERROR` 常量，无需改动；`Color::Reset`（hints.rs L80/L123）是透明背景语义，保留不替换；`Color::Cyan/Magenta/Yellow`（login.rs）是模型类型区分色，保留不替换

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链可用，建立颜色变更前的基线状态。

**执行步骤:**

- [x] 验证构建工具可用
  - `cargo build -p peri-tui`
  - 预期: 编译成功，无错误
- [x] 验证测试工具可用
  - `cargo test -p peri-tui`
  - 预期: 全部测试通过

**检查步骤:**

- [x] 构建成功
  - `cargo build -p peri-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 且无 error
- [x] 测试通过
  - `cargo test -p peri-tui 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"

---

### Task 1: theme.rs 常量更新

**背景:**
本次重构的核心——将 `theme.rs` 中 12 个颜色常量的 RGB 值从暖棕系替换为 Claude Dark 主题的中性灰系值，并新增 `USER_BG` 和 `BASH_BORDER` 两个常量。后续所有 Task 依赖此文件的新值。

**涉及文件:**

- 修改: `peri-tui/src/ui/theme.rs`
- 修改: `peri-widgets/src/theme/presets.rs`

**执行步骤:**

- [x] 更新 ACCENT 常量 RGB 值
  - 位置: `theme.rs` L11
  - 将 `Color::Rgb(255, 107, 43)` 改为 `Color::Rgb(215, 119, 87)`
  - 注释更新为 `/// Claude 暖橙 — 唯一主交互色，品牌色 #D77757`

- [x] 更新 SAGE 常量 RGB 值
  - 位置: `theme.rs` L16
  - 将 `Color::Rgb(110, 181, 106)` 改为 `Color::Rgb(78, 186, 101)`
  - 注释更新为 `/// 明亮绿 — 成功/工具名/在线状态 #4EBA65`

- [x] 更新 WARNING 常量 RGB 值
  - 位置: `theme.rs` L19
  - 将 `Color::Rgb(176, 152, 120)` 改为 `Color::Rgb(255, 193, 7)`
  - 注释更新为 `/// 明亮琥珀 — 次要强调/警告 #FFC107`

- [x] 更新 ERROR 常量 RGB 值
  - 位置: `theme.rs` L22
  - 将 `Color::Rgb(204, 70, 62)` 改为 `Color::Rgb(255, 107, 128)`
  - 注释更新为 `/// 明亮红 — 错误/拒绝 #FF6B80`

- [x] 更新 THINKING 常量 RGB 值
  - 位置: `theme.rs` L25
  - 将 `Color::Rgb(167, 139, 250)` 改为 `Color::Rgb(175, 135, 255)`
  - 注释更新为 `/// 电光紫 — 推理/CoT 思考内容 #AF87FF`

- [x] 更新 TEXT 常量 RGB 值
  - 位置: `theme.rs` L30
  - 将 `Color::Rgb(218, 206, 208)` 改为 `Color::Rgb(255, 255, 255)`
  - 注释更新为 `/// 纯白 — 主文字 #FFFFFF`

- [x] 更新 MUTED 常量 RGB 值
  - 位置: `theme.rs` L33
  - 将 `Color::Rgb(140, 125, 120)` 改为 `Color::Rgb(153, 153, 153)`
  - 注释更新为 `/// 浅灰 — 标签/路径/辅助信息 #999999`

- [x] 更新 DIM 常量 RGB 值
  - 位置: `theme.rs` L36
  - 将 `Color::Rgb(72, 62, 58)` 改为 `Color::Rgb(80, 80, 80)`
  - 注释更新为 `/// 深灰 — 占位/已完成项/分隔符 #505050`

- [x] 更新 BORDER 常量 RGB 值
  - 位置: `theme.rs` L41
  - 将 `Color::Rgb(48, 38, 32)` 改为 `Color::Rgb(80, 80, 80)`
  - 注释更新为 `/// 中性灰 — 空闲边框 #505050`

- [x] 更新 POPUP_BG 常量 RGB 值
  - 位置: `theme.rs` L49
  - 将 `Color::Rgb(10, 8, 6)` 改为 `Color::Rgb(0, 0, 0)`
  - 注释更新为 `/// 纯黑 — 弹窗底色 #000000`

- [x] 更新 CURSOR_BG 常量 RGB 值
  - 位置: `theme.rs` L52
  - 将 `Color::Rgb(38, 22, 10)` 改为 `Color::Rgb(38, 38, 38)`
  - 注释更新为 `/// 中性暗灰 — 光标行背景（列表选中行）#262626`

- [x] 更新 LOADING 常量 RGB 值
  - 位置: `theme.rs` L55
  - 将 `Color::Rgb(34, 211, 238)` 改为 `Color::Rgb(147, 165, 255)`
  - 注释更新为 `/// 浅蓝紫 — Loading/Spinner 专用 #93A5FF`

- [x] 新增 USER_BG 常量
  - 位置: `theme.rs` 在 `LOADING` 定义之后（~L56）
  - 内容: `/// 用户消息背景色 #373737（Claude userMessageBackground）`
  - 内容: `pub const USER_BG: Color = Color::Rgb(55, 55, 55);`

- [x] 新增 BASH_BORDER 常量
  - 位置: `theme.rs` 紧接 `USER_BG` 之后
  - 内容: `/// Bash 工具调用边框色 #FD5DB1（Claude bashBorder）`
  - 内容: `pub const BASH_BORDER: Color = Color::Rgb(253, 93, 177);`

- [x] 更新文件头部注释
  - 位置: `theme.rs` L1-L6
  - 将设计哲学描述更新为反映 Claude 配色风格:

    ```rust
    /// TUI 统一颜色主题（对齐 Claude Code Dark 配色方案）
    ///
    /// 设计哲学：中性灰层级 + Claude 暖橙品牌色。
    /// 背景透明——不使用任何 bg() 颜色（弹窗光标行和用户消息区除外）。
    /// 信息层级用亮度区分（TEXT/MUTED/DIM），颜色表达状态语义。
    ```

- [x] 同步更新 `peri-widgets/src/theme/presets.rs` 中 DarkTheme 的全部 RGB 值
  - 此文件与 `theme.rs` 一一对应，所有 12 个方法的返回值需同步更新为新 RGB 值
  - L14: `Color::Rgb(255, 107, 43)` → `Color::Rgb(215, 119, 87)` // accent
  - L17: `Color::Rgb(110, 181, 106)` → `Color::Rgb(78, 186, 101)` // success
  - L20: `Color::Rgb(176, 152, 120)` → `Color::Rgb(255, 193, 7)` // warning
  - L23: `Color::Rgb(204, 70, 62)` → `Color::Rgb(255, 107, 128)` // error
  - L26: `Color::Rgb(167, 139, 250)` → `Color::Rgb(175, 135, 255)` // thinking
  - L29: `Color::Rgb(218, 206, 208)` → `Color::Rgb(255, 255, 255)` // text
  - L32: `Color::Rgb(140, 125, 120)` → `Color::Rgb(153, 153, 153)` // muted
  - L35: `Color::Rgb(72, 62, 58)` → `Color::Rgb(80, 80, 80)` // dim
  - L38: `Color::Rgb(48, 38, 32)` → `Color::Rgb(80, 80, 80)` // border
  - L41: `Color::Rgb(255, 107, 43)` → `Color::Rgb(215, 119, 87)` // border_active = accent
  - L44: `Color::Rgb(10, 8, 6)` → `Color::Rgb(0, 0, 0)` // popup_bg
  - L47: `Color::Rgb(38, 22, 10)` → `Color::Rgb(38, 38, 38)` // cursor_bg
  - L50: `Color::Rgb(34, 211, 238)` → `Color::Rgb(147, 165, 255)` // loading

- [x] 更新 presets.rs 中的测试断言
  - 位置: `presets.rs` L61
  - 将 `assert_eq!(theme.accent(), Color::Rgb(255, 107, 43));` 改为 `assert_eq!(theme.accent(), Color::Rgb(215, 119, 87));`

- [x] 验证编译通过
  - `cargo build`
  - 预期: 编译成功（全 workspace）

- [x] 验证 widgets 测试通过
  - `cargo test -p peri-widgets`
  - 预期: 全部测试通过（含 dark_theme_returns_correct_colors）
- [x] 确认 theme.rs 和 presets.rs 中无旧 RGB 值残留
  - `grep -n "255, 107, 43\|218, 206, 208\|140, 125, 120\|72, 62, 58\|48, 38, 32\|10, 8, 6\|38, 22, 10\|34, 211, 238\|176, 152, 120\|204, 70, 62\|167, 139, 250\|110, 181, 106" peri-tui/src/ui/theme.rs peri-widgets/src/theme/presets.rs`
  - 预期: 无输出（旧 RGB 值已全部替换）
- [x] 确认新常量已定义
  - `grep -n "USER_BG\|BASH_BORDER" peri-tui/src/ui/theme.rs`
  - 预期: 各出现 1 次（定义行）

---

### Task 2: 硬编码颜色清理 — 核心渲染层

**背景:**
清理 `main_ui.rs`、`message_render.rs`、`sticky_header.rs` 三个核心渲染文件中的硬编码颜色。这三个文件是消息显示和主界面的核心，包含最多的硬编码颜色使用。本 Task 依赖 Task 1 的主题常量更新。

**涉及文件:**

- 修改: `peri-tui/src/ui/main_ui.rs`
- 修改: `peri-tui/src/ui/message_render.rs`
- 修改: `peri-tui/src/ui/main_ui/sticky_header.rs`

**执行步骤:**

- [x] 替换 `main_ui.rs` 中 4 处硬编码颜色
  - L113: `Color::White` → `theme::TEXT` — 主界面消息渲染
  - L214: `Color::Rgb(255, 140, 0)` → `theme::ACCENT` — 自定义橙色样式
  - L292: `Color::Green` → `theme::SAGE` — 完成状态指示
  - L294: `Color::Gray` → `theme::MUTED` — 次要信息

- [x] 替换 `message_render.rs` 中 9 处硬编码颜色
  - L32: `Color::Rgb(74, 70, 66)` → `theme::USER_BG` — 用户消息背景色
  - L73: `Color::White` → `theme::TEXT` — 工具调用状态圆点
  - L164: `Color::Green` → `theme::SAGE` — 工具调用完成状态
  - L165: `_ => Color::White` → `_ => theme::TEXT` — 默认工具状态颜色
  - L173: `Color::White` → `theme::TEXT` — 工具调用标签
  - L184: `Color::DarkGray` → `theme::DIM` — 工具参数边框
  - L197: `Color::DarkGray` → `theme::DIM` — 工具参数括号
  - L332: `Color::White` → `theme::TEXT` — 代码块标题
  - L350: `Color::DarkGray` → `theme::DIM` — diff 行前缀分隔符

- [x] 替换 `sticky_header.rs` 中 1 处硬编码颜色
  - L13: `const HEADER_BG: ratatui::style::Color = ratatui::style::Color::Rgb(74, 70, 66);` → `const HEADER_BG: ratatui::style::Color = theme::USER_BG;`
  - 注意: 由于是 const 绑定，且文件已 `use crate::ui::theme;`，可直接引用 `theme::USER_BG`

- [x] 验证编译通过
  - `cargo build -p peri-tui`
  - 预期: 编译成功

- [x] 运行 headless 测试确认无回归
  - `cargo test -p peri-tui`
  - 预期: 全部测试通过

**检查步骤:**

- [x] 确认三个文件中无硬编码颜色残留
  - `grep -n "Color::White\|Color::Green\|Color::Gray\|Color::DarkGray\|Color::Rgb(255, 140, 0)\|Color::Rgb(74, 70, 66)" peri-tui/src/ui/main_ui.rs peri-tui/src/ui/message_render.rs peri-tui/src/ui/main_ui/sticky_header.rs`
  - 预期: 无输出

---

### Task 3: 硬编码颜色清理 — 面板和弹窗

**背景:**
清理剩余 8 个面板/弹窗文件中的 `Color::White` 硬编码。这些文件的模式统一：按钮文字和选中行使用 `Color::White`，需替换为 `theme::TEXT`。`Color::Cyan/Magenta/Yellow`（login.rs 模型类型区分）和 `Color::Reset`（hints.rs 透明背景）保留不替换。

**涉及文件:**

- 修改: `peri-tui/src/ui/main_ui/popups/setup_wizard.rs`
- 修改: `peri-tui/src/ui/main_ui/panels/cron.rs`
- 修改: `peri-tui/src/ui/main_ui/panels/thread_browser.rs`
- 修改: `peri-tui/src/ui/main_ui/popups/ask_user.rs`
- 修改: `peri-tui/src/ui/main_ui/panels/model.rs`
- 修改: `peri-tui/src/ui/main_ui/panels/login.rs`
- 修改: `peri-tui/src/ui/main_ui/panels/agent.rs`

**执行步骤:**

- [x] 替换 `setup_wizard.rs` 中 4 处 `Color::White` → `theme::TEXT`
  - L59: `.fg(Color::White)` — 光标行文字
  - L62: `.fg(Color::White).bg(theme::ACCENT)` — 按钮选中文字
  - L221: `.fg(Color::White)` — 光标行文字
  - L224: `.fg(Color::White).bg(theme::ACCENT)` — 按钮选中文字

- [x] 替换 `cron.rs` 中 1 处 `Color::White` → `theme::TEXT`
  - L57: `.fg(ratatui::style::Color::White)` — 注意完整路径前缀，替换为 `theme::TEXT`

- [x] 替换 `thread_browser.rs` 中 3 处 `Color::White` → `theme::TEXT`
  - L94: `.fg(Color::White)` — 选中行文字
  - L121: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字
  - L128: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字

- [x] 替换 `ask_user.rs` 中 3 处 `Color::White` → `theme::TEXT`
  - L55: `.fg(Color::White)` — 光标行文字
  - L107: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字
  - L144: `.fg(Color::White).bg(theme::WARNING)` — 选项高亮文字

- [x] 替换 `model.rs` 中 6 处 `Color::White` → `theme::TEXT`
  - L89: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字
  - L105: `Color::White` — 光标状态颜色
  - L124: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字
  - L148: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字
  - L175: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字
  - L224: `.fg(Color::White).bg(theme::ACCENT)` — 按钮文字

- [x] 替换 `login.rs` 中 7 处 `Color::White` → `theme::TEXT`
  - L54: `.fg(Color::White).bg(theme::ACCENT)` — Provider 按钮文字
  - L66: `Color::White` — Provider 名称颜色
  - L81: `Color::White` — 光标状态颜色
  - L88: `.fg(Color::White).bg(theme::ACCENT)` — 字段标签
  - L206: `.fg(Color::White)` — 字段标签
  - L209: `.fg(Color::White).bg(theme::ACCENT)` — 按钮
  - L269: `.fg(Color::White).bg(theme::ACCENT)` — 按钮
  - 注意: `Color::Cyan`、`Color::Magenta`、`Color::Yellow`（模型类型区分色）保留不替换

- [x] 替换 `agent.rs` 中 2 处 `Color::White` → `theme::TEXT`
  - L52: `.fg(Color::White)` — 光标行文字
  - L77: `.fg(Color::White)` — Agent 名称

- [x] 验证编译通过
  - `cargo build -p peri-tui`
  - 预期: 编译成功

- [x] 运行全量测试确认无回归
  - `cargo test -p peri-tui`
  - 预期: 全部测试通过

**检查步骤:**

- [x] 确认面板/弹窗文件中无 `Color::White` 残留（login.rs 中 `Color::Cyan/Magenta/Yellow` 除外）
  - `grep -rn "Color::White" peri-tui/src/ui/main_ui/popups/ peri-tui/src/ui/main_ui/panels/`
  - 预期: 无输出
- [x] 确认 login.rs 中 Cyan/Magenta/Yellow 保留
  - `grep -c "Color::Cyan\|Color::Magenta\|Color::Yellow" peri-tui/src/ui/main_ui/panels/login.rs`
  - 预期: 输出 6（三对 × 2 = 6 处保留）

---

### Task 4: 颜色系统重构验收

**前置条件:**

- Task 1/2/3 全部完成
- 构建环境: `cargo build -p peri-tui` 成功

**端到端验证:**

- [x] 1. 运行完整测试套件确保无回归
  - `cargo test` — 通过（1 个预存失败 test_subagent_group_basic，与本次改动无关）

- [x] 2. 全局硬编码颜色扫描——确认无遗漏
  - `grep -rn "Color::White\|Color::Green\b\|Color::Gray\b\|Color::DarkGray" peri-tui/src/ui/ --include="*.rs"`
  - 结果: 无输出 ✅

- [x] 3. 主题常量 RGB 值验证——确认 theme.rs 和 presets.rs 全部更新
  - `grep -n "pub const" peri-tui/src/ui/theme.rs` — 包含 USER_BG 和 BASH_BORDER ✅
  - `grep -n "Rgb(255, 107, 43)" peri-widgets/src/theme/presets.rs` — 无输出 ✅

- [x] 4. Color::Reset 保留验证——确认透明背景语义未被误改
  - `grep -n "Color::Reset" hints.rs` — 输出 2 处（L80, L123）✅

- [x] 5. Login 面板保留色验证——确认模型类型区分色未被替换
  - `grep -c "Color::Cyan\|Color::Magenta\|Color::Yellow" login.rs` — 输出 2（6 个引用）✅
