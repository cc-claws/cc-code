# Agent 指引文件 领域

## 领域综述

本领域负责 **Agent 指引文件**（`AGENTS.md` / `CLAUDE.md` 及其变体）的发现、加载、合并、限额与注入——即「agent 每次会话开始时读到的项目/用户规则」。

核心目标：**对齐 DeepSeek 官方 `dsh`（deepseek-harness）的加载模型**，即：

- **同目录**：候选文件**都加载并合并**（不是"先命中者独占"），内容（trim 后）相同的**去重**；
- **跨目录**：从**项目根**（含 `.git` 的目录）**逐级向下到 cwd** 拼接，越靠后（越具体）优先级越高，每段带 provenance 头；
- **用户全局层**：单一文件，置于链首（最宽）。

关键约束（与 [system-prompt.md](./system-prompt.md) 一致）：**注入内容必须在 `session/new` 一次性冻结**，会话内不变，保 Prompt Cache 前缀稳定。

> 决策背景：Codex / Hermes 是「同层取一个」，只有 dsh 是「同层都加载+去重」。本仓库**最终采用 dsh 方案**（issue #352）。
>
> 状态：**一期已实现**（`cc-middlewares/src/agents_md/`）。
>
> **对齐范围（逐条对着 dsh `packages/context/agent-instructions/src/` 源码核过）**
>
> | 类别 | 内容 |
> |---|---|
> | **与 dsh 一致** | 候选与顺序（`AGENTS.md`/`CLAUDE.md` 同级并列、都存在就都加载；`.local` 在其后）、项目根标记 `.git` 向上搜索与「未命中回退 cwd」、目录链 root→cwd（宽→具体）、用户全局层置于链首（`{APP_HOME}/AGENTS.md`）、**同目录** trim 内容去重保留最早、不同目录不去重、绝对路径去重、单文件上限 1 MiB |
> | **本项目自研扩展**（dsh 无） | `@import`（含范围限制）、`excludes` glob、UTF-8 BOM 剥离、候选里多一个 `.claude/AGENTS.md`（历史兼容） |
> | **有意偏离**（已知不同，非疏漏） | ① 超 `max_source_bytes` 的文件：dsh **整份忽略**，我们**截头保留** + 标记；② 空文件：dsh 保留「存在但为空」的信号，我们跳过；③ 渲染超预算：dsh **丢宽保具体**，我们**保宽丢具体**（待定，见 §七）；④ 段头：dsh `Instructions from: <path>`，我们 `## <path>`；⑤ dsh 用 `<system-reminder>` 框 + 转义框闭合标签 + 开场白权威声明（dsh 自称这是注入缓解的三件套，见其笔记 `2026-06-24-workspace-context.md`），我们三者皆无；⑥ 注入载体/生命周期：dsh 是**普通 user 消息 + 每步重算/reconcile**，我们是**单条 System 消息 + `session/new` 冻结**（为 Prompt Cache 前缀稳定）；⑦ dsh 有**子目录渐进发现**，我们未做（二期）；⑧ `max_bytes`：dsh 必填无默认（`dsh-base` 用 64 KiB），我们默认 256 KiB |

---

## 一、现状（改造前，`cc-middlewares/src/agents_md/mod.rs`）

- 只在 **`{cwd}` 一层**做三选一（`AGENTS.md` > `CLAUDE.md` > `.claude/AGENTS.md`），先命中者独占；
- **不遍历目录**（不向上到 git root，也不向下到子目录）；
- 全局候选仅 `~/.claude/AGENTS.md`，且只在 cwd 无项目文件（非冻结兜底路径）时生效；
- `@import` 仅对 `CLAUDE*` 主文件解析；`CLAUDE.local.md` 只追加、不解析；
- **Bug**：`find(|p| p.is_file()).and_then(判空→None)` —— **空文件遮蔽后续候选**
  （**新加载器已修**；但 `read_frozen_content`——只喂 Jev 规则提炼的那份——仍是旧写法，bug 尚在，
  只是不再影响注入内容）；
- 注入方式：`before_agent` 里 `prepend_message(System)`。

---

## 二、目标模型（dsh）—— 权威规则

### 2.1 候选与顺序

```text
候选（同目录，按序，全部存在的都加载）:
  基础层:  ["AGENTS.md", "CLAUDE.md", ".claude/AGENTS.md"]  // 末项为 cc-code 历史位置，保留兼容（dsh 无）
  本地覆盖层(在基础层之后): ["AGENTS.local.md", "CLAUDE.local.md"]   // 通常 gitignore
用户全局层(链首，单一): {APP_HOME}/AGENTS.md                          // APP_HOME = ~/.cc-code
项目根标记: [".git"]
```

### 2.2 完整顺序（宽 → 具体）

```text
1) ~/.cc-code/AGENTS.md                       ← 用户全局（最宽）
2) {root}/AGENTS.md , {root}/CLAUDE.md
   {root}/AGENTS.local.md , {root}/CLAUDE.local.md
3) {root}/{sub}/AGENTS.md , …                 ← 逐级
   …
N) {cwd}/AGENTS.md , {cwd}/CLAUDE.md
   {cwd}/AGENTS.local.md , {cwd}/CLAUDE.local.md   ← 最具体（最后）
```

- 越靠后越具体，**后者可覆盖前者**（模型按顺序阅读）。
- 每段前加 **provenance 头**：`## <相对项目根路径>`（如 `## AGENTS.md`、`## sub/AGENTS.md`）
  ——注：dsh 的段头是 `Instructions from: <path>`，格式不同、无功能影响（**有意偏离**）；
  用户全局文件用 `## ~/.cc-code/AGENTS.md`；不在 root 下（理论上不出现）才退化为绝对路径。
  **不产生** `../..` 形式的相对路径。

### 2.3 去重规则（关键）

1. **同目录内容去重**：同一目录内，若两个候选文件 **trim 后内容完全相同**，**只保留最早的一个**（`AGENTS.md` 先于 `CLAUDE.md`；基础层先于 `.local` 层）。
2. **绝对路径去重**：同一绝对路径只加载一次（防符号链接/重复访问）。
3. **excludes 在去重之前生效**（**本项目扩展**；dsh 无 excludes 功能）：被排除的候选不占去重槽位，
   也不会「连坐」挤掉同内容但未排除的候选（反例：`AGENTS.md` 与 `CLAUDE.md` 内容相同、只排除前者
   ——若先按内容去重后过滤，最终会一条都不注入）。

### 2.4 项目根与目录链

- `findProjectRoot(cwd)`：**标记搜索**是从 cwd **向上**走到文件系统根，第一个含 `projectRootMarkers`（默认 `[".git"]`）的目录即为 root。
- 若**始终没有命中标记**，则 root = cwd 本身——**目录链退化为 `[cwd]`**，不会去扫祖先目录的指引文件（避免 `/tmp`、`$HOME` 的 `AGENTS.md` 泄漏到无关会话）。
- 目录链 = `root → … → cwd`（含两端），按「宽 → 具体」顺序；cwd 不在 root 下时同样退化为 `[cwd]`。

### 2.5 限额与截断

| 项 | 值 | 说明 |
|---|---|---|
| `max_source_bytes` | **1 MiB**（默认，与 dsh 同值）| **单文件** UTF-8 字节上限；超限见 2.5.1 |
| `max_bytes` | 可配（本项目默认 **256 KiB**）| **渲染结果**上限（含段头、段间空行与超限标记）。注：dsh 的 `maxBytes` **必填无默认**，`dsh-base` 用 **64 KiB**——默认值不同属**有意偏离**（数值可配） |

**硬保证**：`render_instruction_set` 的返回值字节数 `<= max_bytes` 恒成立——省略标记先按最长预留；
上限小到连标记都放不下时整条省略或整体硬切（宁可标记残缺，也不越界）。

#### 2.5.1 单文件超限截断（**本项目策略**；dsh 是整份忽略）

⚠️ **与 dsh 不同**：dsh 对超过 `maxSourceBytes` 的文件**整份忽略**（`files.ts` 的 `readBounded` 返回 `undefined`），
不留任何内容；我们选择**截断保留头部**并加标记——理由：静默丢掉一份 1 MiB+ 的 `AGENTS.md`
比留下头部 + 明确标记更糟。dsh 的截断只发生在**渲染超预算**时，且**只留头部**（无尾保留）。

按字符（CJK 安全，字符级操作）**头 70% + 尾 20%** 保留，中间插入标记（10% 额度）：

```text
[...truncated AGENTS.md: kept <head>+<tail> of <total> chars. Use file tools to read the full file.]
```

> 实现注意：**必须** 用字符级切分（`chars().take()` / `char_indices()`），禁止 `&s[..n]`（CJK panic）。
> 字符数与字节数不同量纲（CJK 1 字符 = 3 字节），若字符级结果仍超字节上限，**按字节再切一次**（`is_char_boundary` 回退），
> 此时标记文案换成 `kept head+tail of <N> bytes`。
> 上限小到放不下标记时，退化为「只保留头部」——**任何情况下返回值字节数都不超过上限**。
> 首个文件自己就顶破 `max_bytes` 时：**正文**硬截断，但尽量保住 `## <display>` provenance 头。

### 2.6 `@import`（cc-code 保留能力）

- 语法：`<!-- @import <path> -->`（相对当前文件所在目录解析）。
- 作用范围：**所有主候选文件**（`AGENTS.md` / `CLAUDE.md` / 全局 / `.local`）；dsh 无此能力，属本仓库扩展。
- 深度上限 **3**，带**环检测**（visited canonical paths，含自身）。
- **在去重之前**展开（先展开、后按内容去重；因此「同一内容被 import 进两个文件」也会被去重）。
- **范围限制按来源分层（默认）**：项目树内发现的指引文件只能 import **所属项目根内**的文件
  （不在任何项目根下时 = 该文件所在目录）；`../` 穿越与绝对路径越界一律**保留占位符** + `tracing::warn!`。
  这是安全边界：处理恶意仓库时，其 `AGENTS.md` 不能借 `@import` 把项目外文件（`~/.ssh/id_rsa` 等）
  读进 System 消息——指引注入发生在 `session/new`，**不经过 HITL**。
  - **用户自有文件不受此限**：`~/.cc-code/AGENTS.md`（用户全局层）、`~/.claude/CLAUDE.md`（Jev 来源）
    按 `Unrestricted` 处理。它们不是攻击者可控输入，而 `<!-- @import ~/rules/x.md -->` 这类跨项目
    共享是常见做法；若一并收窄，规则会**静默消失**——对门而言是变松，不是变紧。
  - 受限范围下 `canonicalize` **必须成功**：失败（权限 / 环 / 长路径）时无法确认是否越界 →
    fail-closed 保留占位符（不受限路径才容忍 `canonicalize` 失败）。
- 展开失败（文件不存在/不可读/成环/超深/越界）**保留原占位符**并静默跳过，不 panic；
  「存在但不可读」（权限 / 瞬时 IO 错）另发 `tracing::warn!` 留痕。
- 被 import 进来的内容同样归一 CRLF。

### 2.7 注入方式

- **本项目做法（有意偏离 dsh）**：`before_agent` 里把**合并后的整段**作为**一条 `System` 消息** `prepend_message`。
  dsh 是把指引作为**普通 user 消息**注入、并在每次 pre-step 重算 + 按 scope reconcile（改文件下一轮即生效）；
  本项目为保 Prompt Cache 前缀稳定，选择**会话内一次性冻结**。
- 内容冻结来自 `session/new`（写入 `FrozenSessionData.instructions`，见 [system-prompt.md](./system-prompt.md)）。
- 说明：不要把多段拆成多条消息（会改变 Prompt Cache 前缀结构）；也不在本期改成写进 system prompt 字符串。

---

## 三、接口与数据结构（实现契约）

### 3.1 配置结构

```rust
// cc-middlewares/src/agents_md/config.rs
pub struct AgentsMdConfig {
    /// 项目根标记（默认 [".git"]）
    pub project_root_markers: Vec<String>,
    /// 同目录基础候选（有序，默认 ["AGENTS.md", "CLAUDE.md", ".claude/AGENTS.md"]）
    pub instruction_file_candidates: Vec<String>,
    /// 同目录本地覆盖候选（有序，默认 ["AGENTS.local.md", "CLAUDE.local.md"]）
    pub local_instruction_file_candidates: Vec<String>,
    /// 单文件字节上限
    pub max_source_bytes: usize,   // 1 MiB
    /// 渲染后总量上限
    pub max_bytes: usize,          // 256 KiB（可配）
    /// 排除 glob（按**发现路径**匹配，去重之前生效）
    pub excludes: Vec<String>,
    /// `@import` 允许范围：`ProjectRoot`（默认，限项目根内）/ `Unrestricted`（旧行为）
    pub import_scope: ImportScope,
    /// 用户全局文件（默认 {APP_HOME}/AGENTS.md）
    pub user_global_file: PathBuf,
}
```

### 3.2 核心函数（`cc-middlewares/src/agents_md/mod.rs`）

```rust
/// 从 cwd 向上找含 markers 的目录；始终找不到则 root = cwd
/// （标记搜索向上，但「未命中时」不会再扫祖先目录的指引文件）
pub fn find_project_root(cwd: &Path, markers: &[String]) -> PathBuf;

/// 发现链上所有存在的指引文件（自带 provenance），宽→具体有序。
/// excludes 在此生效（去重之前）。
pub fn discover_instruction_files(cwd: &Path, cfg: &AgentsMdConfig) -> Vec<InstructionFile>;

pub struct InstructionFile {
    pub source_path: PathBuf, // 发现路径（未规范化）→ excludes glob 按它匹配
    pub abs_path: PathBuf,    // canonical → 「同一绝对路径只加载一次」去重
    pub display: String,      // provenance 头用（相对 root；全局文件为 ~/…）
    pub content: String,      // 原始（未截断），已归一 CRLF、已展开 @import
}

/// 合并成单段（去重已在发现阶段完成；这里做限额 + provenance），供注入。
/// 返回值字节数恒 <= cfg.max_bytes。
pub fn render_instruction_set(files: &[InstructionFile], cfg: &AgentsMdConfig) -> String;

/// 发现 + 渲染，得到可直接注入的整段内容（None = 没有任何指引）。
pub fn load_instructions(cwd: &Path, cfg: &AgentsMdConfig) -> Option<String>;

// （`load_frozen_instructions` 已删除：会话 new 直接 `load_instructions(cwd, &cfg)`，
//   其中 cfg 从 `AppConfig` 派生（excludes 等），避免多一个"默认配置"入口。）
```

中间件侧：

```rust
AgentsMdMiddleware::new()
    .with_config(cfg)                    // 可选：覆盖配置（含 cfg.excludes）
    .with_excludes(patterns)             // 可选：排除 glob（内部写进 cfg，发现阶段生效）
    .with_frozen_instructions(rendered); // 冻结整段 → before_agent 跳过全部磁盘 I/O
```

**excludes 两条路径都生效**（#359 已修）：

- 冻结路径：`session/new` → `build_frozen_session_data(cwd, app_config, …)` 从
  `AppConfig.claude_md_excludes` 取 excludes，**渲染前**过滤；配置派生项（language / excludes）
  集中在函数内取，8 个调用点不再各自重复提取；
- 非冻结路径：`with_excludes` 写进 `cfg.excludes`，`before_agent` 走同一套发现逻辑；
- 中间件本身无法过滤**冻结成品字符串**，所以 excludes 必须在渲染前生效——这也是把
  `AppConfig` 直接传进 `build_frozen_session_data` 的原因。

### 3.3 伪代码

```text
discover_instruction_files(cwd, cfg):
    root = find_project_root(cwd, cfg.project_root_markers)   # 向上找 .git；无则 cwd
    chain = ancestor_chain(root, cwd)                         # root..=cwd，宽→具体
    out = []; seen_abs = set()
    # ① 用户全局
    if exists(cfg.user_global_file) and not excluded(p): emit(load(...), provenance = "~/.cc-code/AGENTS.md")
    # ② 逐目录（同目录内容去重）
    for dir in chain:
        seen_digest = set()
        for name in cfg.instruction_file_candidates + cfg.local_instruction_file_candidates:
            p = dir / name
            if not exists(p) or excluded(p): continue          # excludes 先于去重
            if canonical(p) in seen_abs: continue
            content = expand_imports(read_utf8(p), depth=3)     # 空内容/IO 错误 → 跳过
            if digest(trim(content)) in seen_digest: continue
            emit(content, provenance = rel(root, p))
            seen_abs.insert(abs(p)); seen_digest.insert(digest)
    return out

render_instruction_set(files, cfg):
    # 超限时：先按最长省略标记预留额度，保证「总量 <= max_bytes」是硬保证
    bodies[i]  = truncate_per_file(files[i].content, cfg.max_source_bytes)   # 头70%/尾20% + 标记
    headers[i] = "## " + files[i].display + "\n\n"
    if 全部放得下: return join(segments)                                     # 常见路径，不浪费额度
    body_limit = cfg.max_bytes - len(最长省略标记)
    take = 能完整放下的最多段数
    if take == 0: 首段 = header + truncate_bytes_head_tail(body, body_limit - len(header))  # 保 provenance 头
    return join(segments[..take]) + (省略标记 if take < len(files))
```

---

## 四、边缘情况

| 场景 | 期望 |
|---|---|
| 候选文件为**空** | **跳过**，继续其它候选（修掉现有「空文件遮蔽」bug）|
| 候选命中 excludes | **在去重之前**丢弃（不占去重槽位、不挤掉未排除的同内容候选）|
| 同一文件既是基础又是 `.local`（同名）| 按绝对路径去重 |
| 两候选 trim 后内容相同 | 只保留最早（`AGENTS.md`），`CLAUDE.md` 丢弃 |
| 非 git 仓库 | 标记搜索未命中 → root = cwd，链退化为 `[cwd]`（不扫祖先目录）|
| cwd 在 git root 之外或 root==cwd | 链退化为单目录 |
| 单个文件超 `max_source_bytes` | 头 70%/尾 20% 截断 + 标记 |
| 总量超 `max_bytes` | 停止追加 + 末尾标记（提示用 file 工具读被截断的文件）|
| `@import` 环 / 超深 / 文件缺失 | 保留原占位符，静默跳过该 import（不 panic、不死循环）|
| 读文件超时/IO 错 | 跳过该文件 + `tracing::warn!` |
| 文件含 CRLF | 归一为 LF 后处理（去重按归一内容）|
| 单文件超限且上限小到放不下标记 | 退化为头部截断（`is_char_boundary` 安全），**绝不超上限、绝不 panic** |
| 文件带 UTF-8 BOM | 读入即剥离（BOM 不属 Unicode White_Space，`trim()` 去不掉；留着既污染 prompt 又破坏同目录去重）|
| `@import` 越出项目根 | 保留占位符 + `warn!`（默认 `ImportScope::ProjectRoot`，`Unrestricted` 才放行）|

---

## 五、测试计划

单测（`cc-middlewares/src/agents_md/agents_md_test.rs`，`tempfile` 建临时目录树；**均已实现**，当前 48 启用 + 1 `#[ignore]`（数量随迭代变化，以 `cargo test` 实跑为准））：

1. `test_same_dir_agents_and_claude_both_loaded` —— 同目录两文件内容不同 → **都出现**；
2. `test_same_dir_identical_content_deduped` —— 两文件 trim 后相同 → **只出现一次**；
3. `test_local_overlay_after_base` —— `.local` 排在基础之后；
4. `test_directory_chain_root_to_cwd_order` —— 三层 `AGENTS.md` 按 宽→具体 排列 + provenance 头正确；
5. `test_empty_file_does_not_shadow` —— 空 `AGENTS.md` 不遮蔽 `CLAUDE.md`（回归现有 bug）；
6. `test_non_git_only_cwd` / `test_find_project_root_walks_up` —— 无 `.git` 时只查 cwd；
7. `test_global_file_prepended` —— 用户全局文件在链首；
8. `test_import_simple` / `test_import_in_agents_md_too` / `test_import_nested` / `test_import_cycle_and_depth_in_load` —— `@import` 展开 + 环保护 + 深度上限；
9. `test_truncation_head_tail` —— 超 `max_source_bytes` 头/尾保留 + 标记；
10. `test_total_max_bytes_stops` / `test_first_file_over_total_limit_still_included` —— 总量超限停止 + 标记（首个文件不空手）；
11. `test_utf8_boundaries_multibyte` / `test_truncation_tiny_budget_degrades` —— 多字节 / emoji / 极小上限不 panic、不超限。

另含 `test_crlf_normalized_and_deduped`（CRLF 归一）、`test_bom_stripped_and_dedup`（BOM 剥离 + 不破坏去重）、
`test_import_relative_escape_blocked` / `test_import_absolute_path_outside_blocked` / `test_import_unrestricted_scope_allows_outside` /
`test_import_inside_root_still_works` / `test_user_global_import_not_scope_limited`（`@import` 范围边界：项目内受限、用户全局不受限）、`test_frozen_instructions_single_message`（冻结路径产出单条 System 消息）、
`test_import_in_local_and_global_too`（`@import` 对 `.local` / 全局文件同样生效）、
`test_excludes_*` + `test_excludes_applied_before_dedup`（excludes 生效且先于去重）、
`test_first_file_truncation_keeps_provenance_header`、`test_ancestor_chain_*`（链退化分支）。

> 测试hermeticity：tempdir 用例统一用 `repo_dir()`（内嵌 `.git`）把 `find_project_root` **钉死**在 tempdir，
> 否则 `TMPDIR` 恰落在某个 git 仓库内时，祖先目录的指引文件会被一并加载，结果就依赖环境了。

另有**真实仓库冒烟**（`#[ignore]`，不进 CI，供人工核查真实大文件上的表现）：

```bash
cargo test -p cc-middlewares --lib agents_md::tests::test_repo_self_smoke -- --ignored --nocapture
# 本仓库实测：root = workspace 根；发现本仓库的 CLAUDE.md / CLAUDE.local.md；
# 渲染结果 ≤ max_bytes(262144) ✅（具体字节数随仓库文件变化，跑一次即见）
```

集成：`cc-acp/src/session/frozen.rs` 的 `FrozenSessionData.instructions` 由 `load_instructions(cwd, cfg)` 产出（cfg 派生自
`AppConfig`：`language` / `claude_md_excludes`），`builder.rs` 经 `with_frozen_instructions` 交给中间件，并**同一份字符串**
透传给 `SubAgentMiddleware::with_inherited_instructions`（子 Agent 与父同 cwd，不再每轮重读磁盘）；
`before_agent` 注入的 System 消息为**单条**。子 Agent 侧另有
`test_subagent_chain_inherits_frozen_instructions` / `test_subagent_chain_without_inheritance_reads_disk` 两条对照用例。

---

## 六、分期落地

| 期 | 内容 | 状态 |
|---|---|---|
| **一期（本 issue）** | 多候选都加载 + 同目录去重 + 跨目录 root→cwd 拼接 + provenance + 全局层 + 限额/截断 + 空文件跳过（修 bug）+ 保留 `@import` | ✅ 已完成 |
| **二期（可选）** | **渐进子目录发现**（dsh **已有**：`descendantDirsBetween` + `read/write/edit` 成功后 reconcile；会话中触达子目录时按需注入该目录指引）；注入位置评估（System 消息 vs system prompt）| 未做 |
| **三期（可选）** | 指引内容的 **prompt-injection 防护**。⚠️ 更正（原先写成「dsh 有注入扫描」是**错的**）：经核 dsh 源码 + 全仓检索（`gh search code`：`prompt injection` / `suspicious` / `sanitize`），**dsh 没有对指引内容做任何扫描/拦截**；它靠三件事降低风险——`<system-reminder>` **框**、**转义**正文里的框闭合标签、以及**开场白的权威声明**。dsh 自己的威胁模型笔记写得很明确（`.agents/notes/archived/feature/2026-06-24-workspace-context.md`）：<br>「Repository text remains untrusted input. Lower-authority user-role framing, explicit precedence language, and delimiter escaping reduce risk but do not eliminate prompt injection.」<br>本项目对应缺口：① 我们注入为**纯 System 文本**、无框、无转义；② 无「更具体优先 / 不覆盖系统与用户指令」的开场白（dsh 有）。**`@import` 读取范围限制**已在第一期做掉 | 未做 |

---

## 七、影响面

- 改动集中在 `cc-middlewares/src/agents_md/`（`config.rs` + `mod.rs` 加载器）+ `cc-acp/src/session/frozen.rs`（冻结调用）、`cc-acp/src/agent/builder.rs`、`cc-acp/src/session/executor.rs`。
- **行为变化（破坏性）**：项目同时有 `AGENTS.md` 与 `CLAUDE.md` 时，两者**都会**进入上下文（此前只取 `AGENTS.md`）；项目根到 cwd 之间各级 `AGENTS.md` 也都会进入。这会让 prompt 变长——用 `max_bytes` 兜底。
- **API 变化**：
  - `AcpAgentConfig.frozen_claude_md` + `frozen_claude_local_md` → 合并为 `frozen_instructions`；
  - `AgentsMdMiddleware::with_frozen_content(main, local)` → `with_frozen_instructions(rendered)`；
  - `FrozenSessionData` 新增 `instructions`，并**删除**已无消费者的 `claude_md` / `claude_local_md`
    （Jev 规则提炼用的是 `frozen.rs` 里的局部变量，从不经这两个字段）。
- **用户全局层迁移**：注入层读 `~/.cc-code/AGENTS.md`（此前非冻结路径读 `~/.claude/AGENTS.md`，冻结路径根本不读全局文件）。HITL 的 Jev 规则仍读 `~/.claude/CLAUDE.md` / `~/.claude/AGENTS.md`（个人规则），两者互不影响。
- **顺带改变（影响很小）**：`read_frozen_content`（Jev 的项目级来源）现在对 `AGENTS.md` 也展开 `@import`（旧实现只对 `CLAUDE*` 展开）。
- 不改变注入机制（仍是单条 System 消息）。
- **本轮（审计后）追加修复**：
  1. `claude_md_excludes` 贯通冻结路径（#359 已修）：`build_frozen_session_data` 改收 `&AppConfig`，excludes 在渲染前生效；
  2. 子 Agent 继承父的**冻结**指引（#360 已修）：新增 `SubAgentMiddleware::with_inherited_instructions`，
     沿 `SubAgentMiddlewareConfig` / `SubAgentTool` 透传到子链，消除每轮重读造成的 prompt 前缀抖动；
     继承体是 `InheritedInstructions { cwd, rendered }`——`Agent` 工具的 `cwd` 是 **LLM 可传参**，
     只有「子 Agent cwd == 渲染该指引时的 cwd」才套用父快照，否则回退读盘（否则会给子 Agent
     注入**别的项目**的规则，且因冻结短路读不到目标项目自己的指引）；
  3. `@import` 默认限项目根内（#361 已修）：新增 `ImportScope`，越界保留占位符 + `warn!`；
  4. UTF-8 BOM 剥离（`normalize_content`）：BOM 不属 White_Space，`trim()` 去不掉，会污染 prompt 且破坏同目录去重；
  5. **另修一处相邻的既有安全语义漏洞**（不在本领域，见 [hitl-permissions.md](./hitl-permissions.md)）：
     HITL 门控此前只拿 RTK **改写后**的命令匹配用户显式规则，装了 `rtk` 的机器上
     `disallowed_commands`（如 `kubectl delete*`）会被 `rtk kubectl delete pod x` 绕过 —— 现按
     「原始 + 改写后」并集判定（#358 已修）。
- **已知偏离 ③（触发条件：渲染超 `max_bytes`）**：超预算时我们**丢最具体、保最宽**
  （`render_instruction_set` 从链首填满预算即停），而 dsh 是**丢最宽、保最具体**
  （`render.ts` 先 `files.slice(start)` 丢前缀，再二分截断最具体那段）。
  **决定：本 PR 不修**——指引的优先级/取舍后续由 **Jev（规则提炼 + 优先级）** 侧统一处理，
  这里只把行为如实在文档与代码注释里记清，不做半套实现。
  （修正时若要自洽，注意本 spec §2.2 声明的是「越靠后越具体、后者可覆盖前者」。）
- **仍然遗留（登记，不在本 PR 解决）**：
  - `read_frozen_content` 的「空文件遮蔽」旧 bug 仍在（只影响 Jev 来源，不影响注入内容）；
  - 三期「prompt-injection 扫描」未做（`@import` 范围限制只堵了其中一条路径）。
- 工程注意：`cargo fmt --all` 会重排本仓库大量历史文件（仓库未按 rustfmt 归一，CI 也不校验 fmt）——只对**动过的文件**格式化，别整仓 fmt。

---

## 相关 Feature

- → [system-prompt.md](./system-prompt.md) — 系统提示词稳定性、`frozen_*` 冻结、`@import` 边界
- → [agent.md](./agent.md) — `AgentsMdMiddleware` 在中间件链的位置（链首）
- → [acp.md](./acp.md) — `session/new` → `FrozenSessionData.instructions` 的冻结路径
- → [hitl-permissions.md](./hitl-permissions.md) — HITL 门控如何评估 Bash 命令（含 RTK 改写 × 显式规则并集）
