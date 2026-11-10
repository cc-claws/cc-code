<div align="center">

[English](README.md) | **中文**

# cc-code

**用开源模型跑 Agent Loop — Rust 写的终端编程助手，兼容 `.claude/` 生态全家桶**

DeepSeek-V4-Pro + Mimo-2.5Pro + GLM-5.1 驱动，`.claude/` 配置零迁移。

[![npm](https://img.shields.io/npm/v/@cc-claw/code)](https://www.npmjs.com/package/@cc-claw/code)
[![GitHub stars](https://img.shields.io/github/stars/cc-claws/cc-code?style=social)](https://github.com/cc-claws/cc-code/stargazers)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square)](LICENSE)
[![Website](https://img.shields.io/badge/website-cc--claw.com-orange?style=flat-square)](https://www.cc-claw.com)
[![GitHub last commit](https://img.shields.io/github/last-commit/cc-claws/cc-code?style=flat-square)](https://github.com/cc-claws/cc-code/commits/main)

<p align="center"><code>npm install -g @cc-claw/code</code></p>

### 🌐 官网：**[cc-claw.com](https://www.cc-claw.com)**

[为什么选 cc-code](#为什么选-cc-code) · [Auto 模式](#auto-模式jev-语义门) · [核心能力](#核心能力) · [安装](#安装) · [Nobody Coding](#我们怎么用-nobody-coding-造-cc-code) · [致谢](#致谢)

</div>

## ❤️ Sponsor

> [想出现在这里？](mailto:wismyzhizi2018@gmail.com)

<details open>
<summary>Click to collapse</summary>

[![Kimi K2.6](assets/partners/logos/kimi.png)](https://platform.moonshot.cn/console?aff=4D2GLR)

Kimi K2.6 是 Moonshot AI 开源的原生多模态 Agent 模型，专为长程编程、编程驱动设计和群组任务编排而生。支持前端、DevOps、性能优化、全栈工程等复杂端到端工作流。邀请码：4D2GLR。[点击注册](https://platform.moonshot.cn/console?aff=4D2GLR)

---

<table>
<tr>
<td width="180"><a href="https://platform.xiaomimimo.com?ref=JBEYTF"><img src="assets/partners/logos/mimo.png" alt="Xiaomi MiMo" width="150"></a></td>
<td>小米顶尖模型 MiMo V2.5，通过邀请码注册：双方各得 ¥10 API 体验金 + 首单 9 折。邀请码：JBEYTF。<a href="https://platform.xiaomimimo.com?ref=JBEYTF">点击注册</a>（注册后自动填入 · 体验金 40 天有效）</td>
</tr>

<tr>
<td width="180"><a href="https://www.bigmodel.cn/glm-coding?ic=MR7BVITFAY"><img src="assets/partners/logos/glm.png" alt="GLM" width="150"></a></td>
<td>智谱 GLM Coding Plan — 国内顶流编程大模型，20+ 主流工具全适配，性价比拉满。<a href="https://www.bigmodel.cn/glm-coding?ic=MR7BVITFAY">立即参与「拼好模」</a></td>
</tr>
</table>

</details>

---

## 为什么选 cc-code？

| 对比项 | 其他终端 Agent | cc-code |
|--------|---------------|------|
| 运行时 | Node.js / Bun，动辄吃 1GB 内存 | Rust 原生，启动快，~50MB 内存 |
| 模型绑定 | 锁死一家 LLM | 随便换：Anthropic、OpenAI 兼容、DeepSeek、GLM |
| Prompt 缓存 | 每轮重算，token 白烧 | 冻结 system prompt，95-99% 缓存命中率 |
| 工具加载 | 全量塞进每轮请求 | 核心工具常驻，其余 Tool Search 按需懒加载 |
| IDE 集成 | 只有终端 | ACP 协议，Zed 等 IDE 直连 |
| `.claude/` 生态 | 不兼容 | 直接用 `.claude/` 配置、agents、skills、hooks、MCP |
| 安全判定 | 每条危险命令都要你确认 | JEV 语义门 —— 独立 LLM 判定，只在判不准时才弹窗 |

---

## Auto 模式：JEV 语义门

`Auto` 是默认权限模式（fail-closed）。它不逐条问你，而是分两层决策：

- **确定性层** —— 你自己写的规则、内置硬黑名单、只读快车道。零成本，不依赖任何凭据。
- **JEV 语义门** —— 独立的 LLM 判定服务（`JEV_MODEL`，默认 `openrouter/typesafe/jev-1.13`），走自己的端点（`JEV_ENDPOINT`）和密钥（`JEV_API_KEY`）。它按条件集给这次调用打分，**只有落进不确定区间时才弹一次确认窗**。

没配判定凭据时，确定性层照常生效，再由 LLM 分类器兜底；没有确认通道时（子 agent、`-p`）一律拒绝。判定全程不占用会话模型 —— 安全校验不花你的上下文 token。`Shift+Tab` 在 `Auto` 与 `Bypass` 间切换。

> 更多底层架构、兜底分类器工作原理与故障排查详见 [Auto 模式与权限把关指南](docs/auto-mode-guide.md)。

---

## 核心能力

| 能力 | 说明 |
|------|------|
| **Rust 原生** | 快启动、低内存、零运行时开销 |
| **Context 优化** | system prompt 冻结 + 动态内容隔离，token 不浪费 |
| **多 LLM 支持** | Anthropic / OpenAI 兼容 API，DeepSeek、GLM 随便切 |
| **`.claude/` 生态兼容** | `.claude/` 配置、agents、skills、hooks、MCP、子 agent 直接复用 |
| **流式 Markdown** | 代码块、表格、diff 实时渲染 |
| **ACP 协议** | 接入 Zed 等 IDE，也支持自建 "Cloud Code" 平台 |
| **Auto Compact** | 长会话自动压缩，保持响应快且省 token |
| **Sub-Agent 并发** | 后台子 agent 并行执行，支持 fork 和 background 模式 |
| **HITL 审批** | 敏感操作由 JEV 语义门拦截 —— 独立 LLM 判定，只在判不准时才弹窗 |
| **双引擎文件搜索** | Grep/Glob 优先使用外部 ripgrep 二进制，回退到内置 Rust 引擎 |

### v0.6.x 新增功能

**审批弹窗补描述行与文件级 diff 预览（v0.6.206）：** 工具入参带 `description` 时，弹窗在标题下方单独显示说明行；`Write`/`Edit` 审批改为文件级全文 diff 预览（新建文件走带行号与语法高亮的代码预览），文件读不到、超限或片段定位不到时回退片段 diff。弹窗布局与配色对齐参考实现：顶部满宽实线、内容区上下虚线、编号选项、底部固定快捷键提示，diff 主题随 `/theme` 切换生效。详见 [CHANGELOG](./CHANGELOG.md)。

**Grep / Glob 对齐参考实现最新规范（v0.6.202）：** `Grep` 描述强调与权限审查 UI、可点击文件链接（`file_path:line_number`）集成，补 ripgrep 语法转义与跨行模式说明，参数新增 `context` 别名（等价 `-C`）并支持解析；`Glob` 明确按修改时间倒序、返回 cwd 相对路径，禁止传 `"undefined"` / `"null"` 字面量，无匹配统一返回 `No files found`、截断改用标准提示文案。ripgrep 引擎与纯 Rust 引擎两条路径都做了路径相对化，系统提示词同步强化 Grep/Glob 优先，不再在 Shell 里裸调 `grep`/`rg`/`find`。详见 [CHANGELOG](./CHANGELOG.md)。

**项目 hooks 与用户/插件 agent 定义（v0.6.199）：** 项目 `.claude/settings.json` 的 hooks 现在真正加载（此前只读全局 hooks、项目 hooks 静默失效），用 `cc-code trust-project`（或 `CC_CODE_TRUST_PROJECT_HOOKS=1`）显式信任后执行。Agent 定义除项目级外，也在用户级（`~/.claude/agents/`）和插件中查找，顺序为项目 > 用户 > 插件 > 内置。详见 [CHANGELOG](./CHANGELOG.md)。

**滚动与输入响应优化（v0.6.201）：** 后台准备布局时继续处理输入；稳定滚动复用已确认宽度，用户交互、流式输出与后台更新采用统一绘制节奏。全屏切换、窗口缩放和选区复制保持必要的帧顺序。详见 [CHANGELOG](./CHANGELOG.md)。

**只读 Bash 并入工具折叠组（v0.6.197）：** 文件查询类 Bash（`ls`、`cat`、`grep`、`rg`、`find`、`head`、`awk` 等）现在与 `Read` / `Grep` / `Glob` 并入同一个只读组，折叠摘要显示 `ran N shell commands`，与 `read`、`searched` 并列。判定改为**前端命令词法**——模型不再传 `file_inspect`（已从工具 schema 删除），`2>/dev/null` / `2>&1` 与引号内的分隔符、`>` 不再被误判成写盘。思考行收敛为一行摘要：运行中 `∴ Thinking for Ns…`、冻结后 `  Thought for Ns`，正文走 `Ctrl+O`。自动 compact 不再清空消息区，卡住检测也不再误伤连续编辑。详见 [CHANGELOG](./CHANGELOG.md)。

**文件查询类 Bash 折叠（v0.6.195）：** 只读查看文件的 Bash 调用现在可以并入只读工具组。模型通过可选参数 `file_inspect` 标记（单条命令同样适用），折叠摘要显示 `ran N shell commands`，与 `read`、`searched` 并列；失败的命令在折叠态仍保留退出码与 stderr，`Ctrl+O` 展开完整命令与输出。运行中、已转后台、模型显式设了 `timeout` 的命令不会折叠，实时输出与 `Ctrl+B` 提示不受影响。详见 [CHANGELOG](./CHANGELOG.md)。

**图片附件链接（v0.6.192）：** 每张已发送图片在用户正文下显示一行 `  ⎿  [Image #N]`。有保存路径且 OSC 8 开启时，引用保持自身柔和配色、无人工下划线，可在支持的终端中打开本地图片；`DISABLE_HYPERLINKS=1` 或缺路径时显示不可点击的普通文本，写盘失败不阻断发送。图片写入系统临时目录，恢复会话不保证链接可用。Phase 1 不向模型发送本地路径。完整范围见 [CHANGELOG](./CHANGELOG.md)。

**Skill 工作流（v0.6.191）：** 匹配任务通过核心 `Skill` 工具加载，`/skill-name args` 直接展开指令。专属卡片折叠展示加载结果，`context: fork` 等待隔离子代理完成，激活指令在压缩后恢复。详见 [技能机制](./spec/global/domains/skill-trigger.md)。

**界面语言：** TUI 和 CLI 帮助支持英文与简体中文。使用 `/lang en` 或 `/lang zh-CN` 保存偏好；未保存语言时，启动检测 `LC_ALL`、`LANGUAGE`、`LC_MESSAGES` 和 `LANG`，不支持的 locale 回退英文。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

**输入交互对齐参考实现（v0.6.166）：** Agent 运行中斜杠命令 / Skills / @ 文件候选浮层可交互——方向键/Tab 导航，Enter 仅补全已选中候选，Esc 先关浮层、再按才中断 Agent；「立即补充」遇前台命令时给 2 秒宽限期，超时自动转后台；状态栏权限模式标签新增 `⏵⏵` 前缀。详见 [CHANGELOG](./CHANGELOG.md)。

**子 Agent 取消（v0.6.139）：** 用自然语言让主 Agent 停掉某个子任务即可——模型走 `Agent(action="stop", task_id="bg-…")` 按 `task_id` 精确取消（**不做模糊匹配**，避免停错描述相似的兄弟任务），停止结果回给模型含任务摘要的确认，时间线打印 `• Agent "x" was stopped by main Agent`，抽屉行转为**红色实心 `●`** + `stopped by main Agent`（用户按键停止则显示 `stopped by user`）。等待期间主视口显示 `✻ Waiting for N background agents to finish`。

**子 Agent Prompt 卡片（v0.6.139）：** 主视口按 `Ctrl+O` 进入详细模式后，后台子任务的启动摘要由极简树形切换为经典工具卡片——`● Agent(任务预览)` + `  └ Backgrounded agent` + `  └ Prompt:` + 指令**全文**（6 列悬挂缩进折行），再次 `Ctrl+O` 收回，无需进入子视口即可核验下发给各子 Agent 的指令。未发布改动见 [CHANGELOG](./CHANGELOG.md)。

| 功能 | 版本 | 说明 |
|------|------|------|
| **工具错配自动改道支持只读 Bash** | v0.6.265 | 模型把 Bash 参数（`{command, description}`）发给只读工具时，此前只报 schema 错误加一句 `💡 Did you mean to use 'Bash'?`，需额外一轮 LLM 往返纠正，现纳入自动改道。改道同时前移到新增的阶段零，在**任何事件与审批之前**替换调用的 name/input——此前改道执行过晚，界面会显示「`Grep` 名 + Bash 参数」（取只读绿、`file_inspect` 恒 false），且 `requires_approval` 看到的是原名 `Grep` 而静默跳过审批，改道后的 Bash 无审批直接执行。Bash 是唯一例外：仅当参数严格匹配其 schema **且**命令被判定为只读查看时才构成候选，仍遵循「恰好一个候选」语义；判定属启发式而非沙箱，故改道后照常走 HITL 审批。只读判定从 `cc-tui-core` 下沉到 `cc-agent/src/shell/readonly.rs`（原路径转出）——改道逻辑在 `cc-agent`，依赖方向要求如此（#988, #989）。 |
| **Grep / Glob 的 `pattern` 误用诊断与零匹配提示** | v0.6.265 | 向 `Grep` 的 `pattern` 传路径或 glob（如 `src/*.rs`）是 schema 合法的，于是被当正则执行——`.` 通配、`*` 作量词——合法地匹配不到任何内容、静默返回 `No files found`。错配诊断、自动改道、熔断器全部挂在参数校验失败路径上，无一触发，空结果极易被误读为「该路径下没有这些文件」。现两侧 `pattern` 描述都写明各自接收何种输入（并给出就地反例 `"src/*.rs"`），且 glob 形态的 pattern 零匹配时会反向按 glob 复查，命中则追加 `💡 Did you mean to use 'Glob'?` 提示。三重收窄保证真阴性不出噪音——仅零匹配时介入、仅含 `*`/`?`/`[` 才试探、仅 glob 确有命中才提示——纯字面路径与正常搜索输出逐字节不变（#986, #987）。 |
| **状态栏提示冒号重复修复，`Alt+P` 文案修正** | v0.6.264 | 状态栏第二行快捷键提示此前渲染出两个冒号（`Ctrl+O ::详情`）：locale 的 `key-*` 文案自带前导 `:`，`format_hints` 又补了一个；键名段还带尾随空格，使键名与冒号之间多出一个空格。现将冒号收口到 `format_hints` 一处——剥掉 locale 前缀再补一个冒号，并去掉键名段尾随空格让冒号紧贴键名（空描述如 `● Verbose` 保留尾随空格以维持段间距），locale 值不动以免波及共用的面板提示。默认模式现为 `Ctrl+O:详情  Ctrl+P:内核  Alt+P:模型`，详细模式为 `● Verbose  Ctrl+O:退出详细`，OAuth / Questions / Rewind / 退出确认及各面板 `status_bar_hints()` 同步生效。顺带修正 `Alt+P` 文案：该键实际打开命令面板（Provider / Model / Effort 选择），原文案「设置」比实际范围窄，`key-status-settings` 中英文分别改为 `:模型` / `:model`（#984）。 |
| **Ctrl+P 切换内核面板，Alt+P 打开命令面板** | v0.6.263 | 此前 `Ctrl+P` 与 `Alt+P` 都绑定到命令面板，内核切换只能手动输入 `/kernel`。现拆分重叠绑定：`Ctrl+P` 弹出内核选择面板（cc / dsh / pi），复用 `/kernel` 无参入口逻辑（统一走 `App::open_kernel_panel()`）；`Alt+P` 保持打开命令面板（Provider / Model / Effort），行为不变。两面板同属 `MutexGroup::Settings` 互斥组，切换时自动关闭另一面板。状态栏提示改为 `Ctrl+O ::详情 │ Ctrl+P ::内核 │ Alt+P ::设置`，tips 与 `/help` 同步更新，`TUI-STYLE.md` 一并修订（#981, #982）。 |
| **状态栏快捷键提示首字母大写** | v0.6.261 | 状态栏第二行默认与详细模式的快捷键提示此前为小写 `ctrl+o` / `ctrl+p`，与文档及其余快捷键表述（`Ctrl+O` / `Ctrl+P`）大小写不一致，现统一为 `Ctrl+o ::详情` / `Ctrl+p ::设置` 首字母大写形式，仅文案变化、无行为改动（#975）。 |
| **思考行数字加粗与 diff 上下文行号提亮** | v0.6.260 | 推理思考摘要行（`Thought for 11s, listed 1 directory`）的时长与动作计数数字此前和普通文字同字重、扫读抓不到重点，现把文本中连续数字拆分为独立 span 单独加粗——时长 `Ns` 整体加粗，字数仅数字加粗（单位词如 `chars` 保持常规）；ToolCallGroup 自带思考行、活跃/冻结状态同款处理，无亮度跳变，底部 `Brewed` 总结行与 unit 词不加粗。Edit/Write diff 视图的上下文行号由最暗灰阶 `theme.dim()`（#505050）提亮为 `theme.muted()`（#999999，与 tip 提示同色阶），仅影响 diff 上下文行，+/- 行 decoration 色不动（#971, #970）。 |
| **diff 行号前多余空格与提前折行修复** | v0.6.259 | 把 diff 加减号 marker 移到行号右侧时行号前多加了一个空格，致行号、marker 与正文右移一列并使长行提前一列折行；本修复恢复参考截图 ` 17 +### Docs` 布局——上下文与增删行前导空格由两格恢复为一格、gutter 宽度由 n+4 恢复为 n+3，续行 marker 前占位由 n+3 恢复为 n+2 与首行对齐；修正原有布局断言，新增第 17 行列位置与折行边界回归测试（40 列宽下 35 字符正文完整显示，36 字符才折行），同步审批预览删除标记转换处的布局注释（#969）。 |
| **欢迎页显示当前内核并完善 `/kernel` 切换交互** | v0.6.258 | 欢迎页按当前会话显示 Agent Kernel，`/kernel` 提供 cc / dsh / pi 选择列表面板，切换成功复用与模型切换一致的短暂通知（同 toast 位置、2.5 秒时长）；欢迎页、session metadata 与 ACP 客户端同步当前会话内核，覆盖新建、历史恢复和切换回执；内核请求期间阻止重复操作与会话切换，保留草稿并延后队列（#967）。 |
| **diff 行加减号 marker 移到行号右侧** | v0.6.256 | diff 展示的 `+` / `-` marker 此前渲染在行号左侧（`+ 25 内容`），现按参考样式移到行号右侧（`  25 + 内容`）——两空格前缀 + 右对齐行号 + 空格 + marker，context 行的 marker 位同步右移、行号列保持对齐。影响所有 diff 渲染路径（工具结果详情、HITL 审批弹窗，删除行仍显示 U+2212 减号），新建文件预览不受影响，marker/行号仍共用 decoration 色分层，正文高亮与底色逻辑不变（#964, #965）。 |
| **command hook 支持 exec form args** | v0.6.255 | Windows 上第三方桌面工具常把 hook 写成 `command` + `args` 的 exec form（参考实现 2.1.139+ 语义），而 `HookType::Command` 此前没有 `args` 字段，serde 会静默丢弃参数并把整串 `command` 交给 `cmd /C`——可执行路径含空格（如 `C:\Program Files\nodejs\node.exe`）时被切碎，此类 hook 全部以 exit 1 失败。现 `args` 非空时直接以 `command` 为可执行文件、`args` 为 argv 启动（不经过 shell），逐项做 `${CLAUDE_PLUGIN_ROOT}` / `${CLAUDE_PLUGIN_DATA}` / `$ARGUMENTS` 变量替换；`args` 为空的旧配置行为不变，exec form 下 `shell` 字段被忽略（与参考实现语义一致）（#954, #955）。 |
> 更早版本（v0.6.0 – v0.6.254）见 [CHANGELOG](./CHANGELOG.md)。

---

## 安装

支持 macOS (x86_64 / Apple Silicon)、Linux (x86_64 / aarch64)、Windows (x86_64)。

### npm（推荐）

```bash
npm install -g @cc-claw/code
```

安装过程不执行任何脚本：二进制放在平台专属的可选依赖包（`@cc-claw/code-<平台>-<架构>`）里，npm 按操作系统与架构自动选中其中一个。因此普通安装、`--ignore-scripts`、pnpm、bun 都能用，安装期也不需要访问 GitHub。

> [!NOTE]
> 用 `--omit=optional` / `--no-optional` 安装会导致二进制缺失，此时运行 `cc-code` 会打印明确的补救说明，而不是 `ENOENT`。

Windows on ARM 暂无原生构建，可显式安装 x64 版本：`npm install -g --arch=x64 @cc-claw/code`。

### 升级

```bash
npm update -g @cc-claw/code
```

### macOS / Linux（脚本安装）

```bash
curl -fsSL https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/cc-claws/cc-code/main/scripts/install.ps1 | iex
```

---

## 我们怎么用 Nobody Coding 造 cc-code

**Nobody Coding** 字面意思：没有人类写过一行 cc-code 代码 — 架构、TUI、harness tuning 全是 AI 干的。人决定 *做什么*，AI 想 *怎么做*。你不是在结对编程，你是在管一个不睡觉的工程师。cc-code 99% 的代码都是这么来的。

> 最近的 commit 几乎全是 DeepSeek、Mimo 和 GLM 的产出。Claude 只在最初参与过。

### 典型工作流

| 你要做的事 | 流水线 |
|-----------|--------|
| 发现 bug 或技术债 | `issue-create` → `systematic-debugging` → `writing-plans` → `subagent-driven-development` → `issue-archive` → 改进 CLAUDE.md |
| 开新功能 | `brainstorming` → `writing-plans` → `subagent-driven-development` |
| 代码库变乱了 | `slop-cleaner` → `writing-plans` → `subagent-driven-development` |
| 需要理解架构 | `teacher` → 分配任务 → `teacher` |

---

## 仓库结构

```text
cc-code/
├── cc-agent/                # 核心：Agent loop、工具系统、持久化、遥测
│   └── README.md              # Agent 框架使用指南
├── cc-tools/                # 叶子工具 crate：无状态核心工具与跨平台进程包装
│   └── README.md              # 工具清单与契约
├── cc-terminal/             # 跨平台终端底层：终端后端、视口、剪贴板、ConPTY
│   └── README.md              # 终端后端与能力说明
├── cc-middlewares/           # 中间件：HITL、SubAgent、Skills、MCP、Hooks（文件/终端工具已下沉至 cc-tools / cc-terminal）
│   ├── README.md              # 中间件概览
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── cc-tui-core/             # 会话核心与展示模型：i18n、主题、markdown 解析、MessageViewModel
│   └── README.md              # 展示模型与核心状态
├── cc-tui/                  # TUI 应用 (Ratatui)
│   ├── README.md              # TUI 使用指南
│   └── CLAUDE.md              # 开发指南和陷阱记录
├── cc-acp/                  # ACP 服务层：桥接 TUI/IDE 与 Agent
│   └── README.md              # ACP 架构和数据流
├── cc-widgets/              # Widget 组件库
│   └── README.md              # 组件列表和使用示例
├── cc-lsp/                  # LSP 客户端库
│   └── README.md              # LSP 操作和配置
├── langfuse-client/           # Langfuse 遥测客户端
│   └── README.md              # 遥测配置和使用
├── npm/                       # npm 包：平台可选依赖 + bin 启动器
│   └── README.md              # npm 包说明
├── scripts/
│   ├── install.sh             # macOS / Linux 安装器
│   └── install.ps1            # Windows 安装器
├── spec/                      # 设计文档与规范
│   ├── global/                # 全局架构文档
│   ├── issues/                # Issue 分析文档
│   └── archive/               # 已归档的 feature 规范
├── docs/                      # 长文文档（prd、adr、designs、acp）
├── human/                     # 人工走查清单
├── CLAUDE.md                  # 项目开发指南
├── CHANGELOG.md               # 版本变更记录
├── CONTRIBUTING.md            # 贡献指南
├── DEVLOG.md                  # 开发日志（里程碑）
├── README.md
├── README_ZH.md
├── TUI-STYLE.md               # TUI 命令 / 快捷键 / 交互参考
└── LICENSE                    # Apache 2.0
```

---

## 致谢

| 项目 | 说明 |
|------|------|
| [Peri (KonghaYao)](https://github.com/KonghaYao/peri) | 本项目 fork 自 Peri，基于 Apache 2.0 协议分发，感谢原作者的开创性工作 |
| [Superpowers](https://github.com/obra/superpowers) & [Matt Pocock's Skills](https://github.com/mattpocock/skills) | 驱动 cc-code AI 工程工作流的 skill 套件 |
| [ACP](https://agentclientprotocol.com/) | Agent-IDE 通信开放协议 |
| [rmcp](https://github.com/anthropics/rmcp) | Rust MCP 客户端库 |
| [Ratatui](https://ratatui.rs) & [Tokio](https://tokio.rs) | TUI 框架和异步运行时 |
| [Langfuse](https://langfuse.com) | LLM 可观测性 |
| [Zed](https://zed.dev) | 第一个 ACP 兼容 IDE，验证了协议可行性 |

---

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=cc-claws/cc-code&type=Date)](https://www.star-history.com/#cc-claws/cc-code&Date)

---

## 许可证

[Apache License 2.0](LICENSE) — 可自由使用、修改、分发，包括商业用途。
