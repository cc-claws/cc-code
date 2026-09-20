# [BUG/DESIGN] CMD 控制台状态栏特殊字符重影排查与跨平台高保真视觉自适应方案

- **状态**：已立项 / 方案已确定
- **创建日期**：2026-09-20
- **优先级**：高
- **模块**：TUI / 状态栏渲染 (Status Bar) / Windows 控制台兼容
- **GitHub Issue**：#168 (https://github.com/cc-claws/cc-code/issues/168)
- **关联分支**：`fix/cmd-render-ghosting`
- **问题截图凭证**：`C:/Users/adim/AppData/Local/Temp/04f6b874fc9844b68f2a02e7b3f1ac5d.png`

---

## 一、问题现象与证据解剖

用户在 Windows 传统控制台（CMD / conhost.exe，默认新宋体 8×16，代码页 CP936）下运行 CC Code 时，底部状态栏及提示区域发生明显的**字符重影、光标偏移、行尾字符无法擦除（如孤立的 `)`、`e`、数字）以及方块破裂**。

现场捕获的状态栏包含以下三行内容：

```text
[gemini-3.8-flash-high] █░░░░░░░░░ 10% | peri git:(fix/cmd-render-ghosting*) | tok: 1.4M (in: 96.3k, out: 588) | ⏱  6m53s
✓ Read ×10 | ✓ Grep ×9 | ✓ WebFetch ×4 | ✓ WebSearch ×3 | +2 more
Bypass (Shift+Tab to cycle)  CPU 39% · MEM 74MB
```

### 1.1 涉事字符与异常特征分析

| 状态栏行 | 关键字符 | Unicode 码点 | 程序预计列宽 | CMD 实际物理推进 | 故障表现 |
| :--- | :--- | :--- | :---: | :---: | :--- |
| **第 1 行** | `█` / `░` (进度条) | U+2588 / U+2591 | 1 | 1~2 | 点阵字体下像素撕裂、高低不齐、两端存在字符缝隙，视觉粗糙 |
| **第 1 行** | `⏱` (耗时秒表) | U+23F1 | 1~2 | 异常回退 | 传统 CMD 缺失 Segoe UI Emoji 字体支撑，呈现为白框 `□` 或跳格 |
| **第 2 行** | `✓` (工具成功对勾) | U+2713 | 1 | 2 (双宽) | 触发东亚歧义宽度，物理多走 1 列，导致工具名整体右移 |
| **第 2 行** | `×` (调用频次乘号) | U+00D7 | 1 | 1~2 | 在 CP936 全角映射冲突时造成数字紧贴或跳列 |
| **第 3 行** | `·` (资源分隔中圆点)| U+00B7 | 1 | 2 (双宽) | **残影重灾区**。光标多走 1 列，`MEM 74MB` 右偏，刷新缩短时行尾留残影 |

---

## 二、底层根因分析

1. **实际列宽与逻辑单元格（Cell Width）分裂**：
   - 现代 TUI 引擎（Ratatui、`unicode-width` 或前端字符分词器）默认将大多数符号判定为 1 列宽（Halfwidth）。
   - Windows CMD (conhost) 在中文字体（如新宋体）及 CP936 编码下，通过 GBK 字库映射，将 `·` (0xA1A4)、`✓` 等强制按全角 2 列进行硬件绘制。
2. **差分刷新机制的盲区**：
   - TUI 采用增量刷新（Diff-based Redraw）。当终端物理光标超出逻辑计算的坐标时，上一帧尾部溢出到物理屏幕外的字符无法被当前帧覆盖。
   - 当某一行总长度因数据刷新而缩短（如数字减少或工具列表变短），右侧溢出区就会长期残留无法清除的“幽灵残影”。

---

## 三、跨平台自适应与高质感视觉设计方案

为了在 Windows 传统 CMD 下**彻底杜绝重影错位**，同时在**Linux Shell、macOS 终端及 Windows Terminal 上 100% 保留原生现代化精致视觉**，设计如下三层自适应架构。

### 3.1 终端环境指纹感知器 (Terminal Capability Detector)

在渲染流水线最前端注入轻量感知逻辑：

```typescript
export type TerminalType = 'modern' | 'legacy-cmd';

export function getTerminalProfile(): {
  isCmd: boolean;
  canRenderEmoji: boolean;
  useSmoothBar: boolean;
} {
  const isWin = process.platform === 'win32';
  const isWindowsTerminal = !!process.env.WT_SESSION;
  const isVSCode = process.env.TERM_PROGRAM === 'vscode';
  const isCmd = isWin && !isWindowsTerminal && !isVSCode;

  return {
    isCmd,
    canRenderEmoji: !isCmd && process.env.TERM !== 'linux',
    useSmoothBar: isCmd, // CMD 下启用纯色反转平滑进度条
  };
}
```

### 3.2 方案一：GUI 级纯色反转平滑进度条（彻底解决方块撕裂）

* **原理**：不使用任何 Unicode 方块字符（`█`、`░`），直接利用 ANSI 背景色翻转（Space Inversion）：
  - **已使用部分**：输出连续纯空格 `' '`，背景色置为亮青（`Bg(Color::Cyan)`）。
  - **剩余部分**：输出连续纯空格 `' '`，背景色置为深灰/暗底（`Bg(Color::DarkGray)`）。
* **收益**：
  1. 纯空格 `' '` 在全球任何操作系统、控制台和字体下**物理推进绝对严格等于 1 列**，零误差。
  2. 在 CMD 中呈现为一条**完全饱满、无缝隙、无锯齿的实心色块槽**，视觉高级感拉满。

### 3.3 方案二：安全高质感字符映射矩阵

针对必须维持 1 列宽的图标元素，在传统 CMD 下使用原生 GBK 安全字形平替，拒绝丑陋的 `?` 或 `*`：

| 元素 | 现代 WT / Linux Shell | 传统 CMD 降级（原态） | **方案采纳的高质感 CMD 平替** | 视觉设计说明 |
| :--- | :---: | :---: | :---: | :--- |
| **中圆点** | `·` | 留残影 | `\|` (深灰细竖线) 或 空格 | 纤细通透，逻辑与物理严格 1 列 |
| **对勾符** | `✓` | 留残影 | `√` (数学根号) 或 `v` | 原生 GBK `0xA1DC` 安全字符，优雅精致 |
| **乘号** | `×` | 错位 | `x` (精细浅灰调色) | 视觉上与数学乘号高度一致 |
| **秒表** | `⏱️` | 变方块 `□` | `~ 6m53s` 或 高亮色彩直接展示 | 规避缺失字体回退崩溃，聚焦耗时数据 |

### 3.4 方案三：行尾物理擦除防护（Erase-in-Line Guard）

在 TUI 或状态栏每一行输出末尾，显式追加 ANSI 行尾擦除指令：
```text
\x1b[K  (EL - Erase to end of line)
```
确保无论当前行文本如何缩短、光标因何种原因产生轻微抖动，行尾绝不会残留上一帧的历史字符。

### 3.5 Linux / macOS / Windows Terminal 原生无损保障

- **Linux Shell（GNOME, Kitty, Alacritty 等）**：命中 `modern` 通道，继续原生输出彩色 Emoji（`⏱️`）、Unicode 进度条（`█░`）及原生歧义字符，**享受 100% 顶级视觉，零降级**。
- **Windows Terminal**：环境变量检测到 `WT_SESSION`，直通现代渲染通道。

---

## 四、落地实施与验证计划

1. **状态栏渲染器改造**：
   - 提取状态栏符号到配置映射表（Glyph Table），依据 `isCmd` 动态分发。
   - 重构进度条渲染器，支持背景色翻转模式。
2. **自动化回归测试**：
   - 在独立隐藏控制台（CP936 + 新宋体）下运行自动化字符宽度探针。
   - 验证多轮不同长度内容刷新后，终端缓冲区无任何残留字符。
