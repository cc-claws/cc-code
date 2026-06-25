# Feature: 20260501_F001 - color-system-refactor

## 需求背景

peri TUI 当前颜色系统（`theme.rs`）采用暖棕色调（ACCENT #FF6B2B、TEXT #DACED0 冷白、BORDER #302620 暖棕），与 Claude Code CLI 的视觉风格差异明显。Claude Code 使用中性暖橙 #D77757 为品牌色、纯白文字、中性灰层级，整体观感更现代、对比度更高。

此外，当前约有 28 处硬编码颜色（`Color::White` 18 处、`Color::DarkGray` 3 处等）散落在 17 个文件中，未通过主题常量统一管理。

## 目标

- 将 peri TUI 配色方案对齐到 Claude Code Dark 主题，提升视觉一致性和品牌辨识度
- 清理全部硬编码颜色，统一通过 `theme::*` 常量引用
- 保持现有命名体系和代码结构不变，仅替换 RGB 值和新增必要常量

## 方案设计

### 1. 现有常量 RGB 值更新

`peri-tui/src/ui/theme.rs` 中 12 个常量的 RGB 值从 Claude Code Dark 主题语义映射：

| 常量名 | 旧 RGB | 新 RGB | Claude 来源 |
|--------|--------|--------|------------|
| `ACCENT` | (255,107,43) #FF6B2B | (215,119,87) #D77757 | `claude` 品牌色 |
| `TEXT` | (218,206,208) #DACED0 | (255,255,255) #FFFFFF | `text` |
| `MUTED` | (140,125,120) #8C7D78 | (153,153,153) #999999 | `inactive` |
| `DIM` | (72,62,58) #483E3A | (80,80,80) #505050 | `subtle` |
| `SAGE` | (110,181,106) #6EB56A | (78,186,101) #4EBA65 | `success` |
| `ERROR` | (204,70,62) #CC463E | (255,107,128) #FF6B80 | `error` |
| `WARNING` | (176,152,120) #B09878 | (255,193,7) #FFC107 | `warning` |
| `THINKING` | (167,139,250) #A78BFA | (175,135,255) #AF87FF | `autoAccept` |
| `LOADING` | (34,211,238) #22D3EE | (147,165,255) #93A5FF | `claudeBlue_FOR_SYSTEM_SPINNER` |
| `BORDER` | (48,38,32) #302620 | (80,80,80) #505050 | `subtle` |
| `POPUP_BG` | (10,8,6) #0A0806 | (0,0,0) #000000 | `clawd_background` |
| `CURSOR_BG` | (38,22,10) #261608 | (38,38,38) #262626 | 调整为中性 |
| `MODEL_INFO` | (160,130,95) #A0825F | 保留不变 | 无对应 |

**映射原则：** 保留 peri 语义命名（ACCENT、SAGE、MUTED 等），仅将 RGB 值替换为 Claude Dark 主题中对应语义的值。整体色调从暖棕系转为中性灰系，品牌色从橙红转为 Claude 暖橙。

### 2. 新增常量

```rust
/// 用户消息背景色，对应 #373737（Claude userMessageBackground）
pub const USER_BG: Color = Color::Rgb(55, 55, 55);

/// Bash 工具调用边框色，对应 #FD5DB1（Claude bashBorder）
pub const BASH_BORDER: Color = Color::Rgb(253, 93, 177);
```

`USER_BG` 替换 `main_ui.rs` 中 2 处硬编码的 `Color::Rgb(74, 70, 66)` 用户消息背景和头部背景。`BASH_BORDER` 为 bash 工具调用提供 Claude 风格的粉色边框。

### 3. 硬编码颜色清理

涉及 17 个文件，共约 28 处替换：

| 硬编码 | 出现次数 | 替换为 |
|--------|---------|--------|
| `Color::White` | 18 | `theme::TEXT` |
| `Color::Green` | 2 | `theme::SAGE` |
| `Color::Gray` | 1 | `theme::MUTED` |
| `Color::DarkGray` | 3 | `theme::DIM` |
| `Color::Rgb(255,140,0)` | 1 | `theme::ACCENT` |
| `Color::Rgb(74,70,66)` | 2 | `theme::USER_BG` |
| `Color::Cyan/Magenta/Yellow` | 各 2 | 保留终端标准色（login 面板模型类型区分，无需主题化） |

**清理范围：** 以下文件中的硬编码颜色需逐一替换：

- `main_ui.rs` — 用户消息背景、头部背景
- `message_view.rs` — 工具名、光标行白色文字
- `sticky_header.rs` — 标题文字
- `status_bar.rs` — 状态信息文字
- `welcome.rs` — 欢迎文字
- `hints.rs` — 提示文字
- `setup_wizard.rs` — 向导文字
- `ask_user.rs` — 问答面板文字
- `hitl.rs` — 审批面板文字
- `thread_browser.rs` — 浏览器列表
- `cron.rs` — 定时任务面板
- `login.rs` — 登录面板（Cyan/Magenta/Yellow 保留）
- `model.rs` — 模型面板
- `agent.rs` — Agent 面板
- `headless.rs` — 测试用例

### 4. 注释和文档更新

`theme.rs` 文件头部注释需同步更新：

- 设计哲学描述从"极简锋利，单色制胜"调整为反映 Claude 配色风格
- 每个常量的注释更新 RGB 值和 Claude 来源
- `TUI-STYLE.md` 风格指南对应更新

### 5. peri-widgets 主题适配

`peri-widgets` crate 中的 `Theme` trait 定义颜色查询接口，本次变更只需确认 widgets crate 的 Theme 实现能正确传递新 RGB 值即可，无需修改 trait 接口。

## 实现要点

- **视觉回归风险：** 整体色调从暖棕系大幅转向中性灰系，所有 UI 元素颜色同时变化。建议实现后做一轮视觉走查，特别关注：弹窗可读性、工具调用边框对比度、状态栏信息辨识度
- **CURSOR_BG 调整：** 从橙调暗棕 #261608 改为中性暗灰 #262626，选中行高亮风格变化明显
- **WARNING 色变化较大：** 从低调暖棕 #B09878 改为明亮琥珀 #FFC107，所有使用 WARNING 的地方视觉冲击增强
- **headless 测试：** 颜色变更可能影响 `headless.rs` 中的 `contains()` 断言（如果测试字符串中嵌入了颜色值），需逐一检查

## 约束一致性

- 符合 `spec/global/constraints.md` 中"字符串显示宽度"约束（颜色变更不影响宽度计算）
- 符合 `spec/global/architecture.md` 中 Workspace 依赖方向（theme.rs 在 peri-tui 内部，widgets crate 通过 Theme trait 解耦）
- 无新增外部依赖
- 无架构偏离

## 验收标准

- [ ] `theme.rs` 中 12 个常量 RGB 值更新为 Claude Dark 主题对应值
- [ ] 新增 `USER_BG` 和 `BASH_BORDER` 两个常量
- [ ] 全部 `Color::White` 替换为 `theme::TEXT`（login 面板除外）
- [ ] 全部 `Color::Green` 替换为 `theme::SAGE`
- [ ] 全部 `Color::Gray` 替换为 `theme::MUTED`
- [ ] 全部 `Color::DarkGray` 替换为 `theme::DIM`
- [ ] 硬编码 `Rgb(255,140,0)` 替换为 `theme::ACCENT`
- [ ] 硬编码 `Rgb(74,70,66)` 替换为 `theme::USER_BG`
- [ ] `cargo build` 编译通过
- [ ] `cargo test` 全量测试通过
- [ ] headless 测试中无因颜色值变更导致的断言失败
- [ ] TUI 视觉走查确认配色效果符合预期
