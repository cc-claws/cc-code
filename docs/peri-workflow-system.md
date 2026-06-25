# Perihelion Workflow 编排系统设计文档

> 本文档定义 Perihelion 项目的 Workflow 编排系统——一个基于 YAML + Markdown + JSON 声明的原生 Rust 执行器，
> 用于替代 Claude Code 的 JavaScript Workflow 脚本，提供更适合本项目的多 Agent 编排能力。

---

## 1. 概述

### 1.1 为什么不用 Claude Code Workflow

Claude Code 的 Workflow 工具使用 JavaScript 脚本编排多 Agent 执行。但对 Perihelion 项目来说：

- **项目是纯 Rust 栈**，维护 JS 脚本与项目技术栈不一致
- **提示词混在 JS 中**，333 行的 `full-code-review.js` 中约 180 行是中文提示词文本，难以独立审查和迭代
- **Schema 定义内嵌**，约 100 行 JSON Schema 混在 JS 变量声明中，无法被 IDE 和工具链校验
- **数据变换依赖 JS 表达式**，调试困难且无法利用 Unix 工具生态

### 1.2 Perihelion Workflow 的设计目标

| 目标 | 实现方式 |
|------|----------|
| 声明式、无脚本 | YAML 定义编排逻辑，Markdown 编写提示词，JSON 定义 Schema |
| 提示词可独立审查 | 提示词放在独立的 `.md` 文件中，支持 `{{模板变量}}` |
| 原生 Rust 执行 | 通过 `peri-acp` 的 `build_agent()` 创建和执行 Agent |
| 数据变换统一 JS | 所有数据变换和条件判断通过 `run` 步骤 + JS 脚本（外部 Node/Bun 执行） |
| 可复现、可追踪 | 确定性步骤执行 + 事件流实时输出进度 |

### 1.3 对比

| 维度 | Claude Code JS Workflow | Perihelion YAML Workflow |
|------|------------------------|--------------------------|
| 声明语言 | JavaScript | YAML + Markdown + JSON |
| 执行器 | Claude Code 内置 JS Runtime | `peri-workflow` Rust 执行器 |
| 提示词位置 | 内嵌在 JS 模板字符串中 | 独立 `.md` 文件 |
| Schema 位置 | JS 对象字面量 | 独立 `.json` 文件 |
| 数据变换 | JS 表达式 | JS 脚本（`run` 步骤，需 `require` 声明） |
| Agent 创建 | Workflow Runtime 子进程 | `peri-acp::build_agent()` |
| 跨平台 | 仅 Claude Code | Perihelion 支持的所有平台 |

---

## 2. 文件组织

### 2.1 目录结构

每个 Workflow 是一个目录，包含声明文件和资源文件：

```
.claude/workflows/full-code-review/
  workflow.yaml              # 主声明文件（约 80 行）
  prompts/
    scan.md                  # Scan 阶段提示词
    review-correctness.md    # Review 各维度提示词
    review-security.md
    review-architecture.md
    review-performance.md
    review-tests.md
    review-regression.md
    verify.md                # Verify 阶段提示词
    synthesize.md            # Synthesize 阶段提示词
  schemas/
    review.json              # Review 输出 Schema
    verify.json              # Verify 输出 Schema
    synthesis.json           # Synthesis 输出 Schema
```

### 2.2 文件职责

| 文件类型 | 职责 | 编辑者 |
|----------|------|--------|
| `workflow.yaml` | 编排逻辑、参数定义、步骤序列 | 开发者 |
| `prompts/*.md` | Agent 提示词，支持 `{{变量}}` 模板 | 开发者 + 领域专家 |
| `schemas/*.json` | 结构化输出的 JSON Schema | 开发者 |

### 2.3 路径解析

所有相对路径（`prompt`、`schema` 字段）基于 `workflow.yaml` 所在目录解析。

---

## 3. YAML Schema

### 3.1 完整示例

以 `full-code-review` 为例：

```yaml
name: full-code-review
description: 多维度深度代码审查，带对抗式验证
require: [bun]

phases:
  - title: Scan
    detail: 扫描变更文件
  - title: Review
    detail: 并行审查
  - title: Verify
    detail: 对抗式验证
  - title: Synthesize
    detail: 汇总分级

params:
  range:
    type: string
    default: HEAD~10
  dimensions:
    type: array
    default: [correctness, security, architecture, performance, tests, regression]
  verify_level:
    type: string
    enum: [Critical, High, Medium, Low]
    default: Medium
  focus_files:
    type: array
    default: []

schemas:
  review: ./schemas/review.json
  verify: ./schemas/verify.json
  synthesis: ./schemas/synthesis.json

steps:
  # ── Phase 1: Scan ──────────────────────────────────
  - phase: Scan
  - id: diff_stat
    agent:
      prompt: ./prompts/scan.md
      label: scan-diff

  # ── Phase 2: Review（并行 fan-out）─────────────────
  - phase: Review
  - id: reviews
    parallel:
      over: "${params.dimensions}"
      item: dim
      agent:
        prompt: "./prompts/review-${dim}.md"
        label: "review:${dim}"
        phase: Review
        schema: review

  - log: "收到 ${reviews.length} 个审查结果"

  # ── JS 数据变换：提取 findings ─────────────────────
  - id: all_findings
    run: |
      return input.flatMap(r =>
        r.findings.map(f => ({ ...f, dimension: r.dimension }))
      );

  # ── Phase 3: Verify（pipeline 逐项验证）────────────
  - phase: Verify
  - log: "共 ${all_findings.length} 个发现需要验证"
  - id: verified
    pipeline:
      over: all_findings
      item: finding
      agent:
        prompt: ./prompts/verify.md
        label: "verify:${finding.file}"
        phase: Verify
        schema: verify
      merge: verdict

  - id: confirmed
    run: |
      return input.filter(v => v.verdict?.confirmed);
  - id: false_positives
    run: |
      return input.filter(v => v.verdict && !v.verdict.confirmed);

  - log: "验证完成：${confirmed.length} 确认 / ${false_positives.length} 误报"

  # ── Phase 4: Synthesize ───────────────────────────
  - phase: Synthesize
  - id: synthesis
    agent:
      prompt: ./prompts/synthesize.md
      label: synthesize
      phase: Synthesize
      schema: synthesis

  # ── 条件步骤：仅 Critical 时发送紧急通知 ───────────
  - id: urgent_notify
    when: "confirmed.filter(f => f.severity === 'Critical').length > 0"
    agent:
      prompt: ./prompts/urgent-notify.md
      label: urgent-notify
      schema: notify_result

return: [reviews, confirmed, false_positives, synthesis]
```

### 3.2 顶层字段

| 字段 | 必须 | 说明 |
|------|------|------|
| `name` | 是 | Workflow 唯一标识 |
| `description` | 是 | 功能描述 |
| `require` | 否 | 外部运行时依赖声明（见 [§3.4](#34-require--外部运行时依赖)） |
| `phases` | 否 | 执行阶段列表，用于进度展示 |
| `params` | 否 | 输入参数定义（类型 + 默认值） |
| `schemas` | 否 | 命名 Schema 引用（name → `.json` 路径） |
| `steps` | 是 | 执行步骤列表 |
| `return` | 否 | 返回值引用列表 |

### 3.3 参数定义（params）

```yaml
params:
  range:
    type: string
    default: HEAD~10
    description: git 范围
  dimensions:
    type: array
    default: [correctness, security]
  verify_level:
    type: string
    enum: [Critical, High, Medium, Low]
    default: Medium
```

| 字段 | 说明 |
|------|------|
| `type` | `string` / `array` / `number` / `boolean` |
| `default` | 默认值（用户未传参时使用） |
| `enum` | 可选值列表 |
| `description` | 参数说明（供 UI 展示） |

### 3.4 require — 外部运行时依赖

声明 workflow 执行所需的外部运行时。执行器在启动时检查依赖是否可用，缺失则**立即报错**而非运行到中途失败。

```yaml
require: [bun]       # 需要 bun 运行时
# 或
require: [node]      # 需要 node 运行时
```

**支持的运行时标识**：

| 标识 | 检测命令 | 启用能力 |
|------|---------|---------|
| `node` | `node --version` | JS `run` 步骤、`when` JS 条件表达式 |
| `bun` | `bun --version` | 同上（Bun 启动更快） |

**设计原则**：

- **零嵌入**：不内嵌 JS 运行时（如 rquickjs），通过子进程调用系统已安装的 node/bun
- **声明式 opt-in**：没有 `require` 的 workflow 纯 Rust 执行，零额外依赖
- **Bun 优先**：同时声明 `[node, bun]` 时优先使用 Bun（启动快 5-10x）
- **启动检查**：引擎初始化时一次性检测所有 `require` 依赖，全部可用才继续

**运行时选择逻辑**：

```rust
fn detect_js_runtime(require: &[String]) -> Option<JsRuntime> {
    if require.contains(&"bun".into()) && which("bun").is_ok() {
        return Some(JsRuntime::Bun);
    }
    if require.contains(&"node".into()) && which("node").is_ok() {
        return Some(JsRuntime::Node);
    }
    None
}
```

**不声明 `require` 时**：workflow 只能使用纯 Agent 编排步骤（agent/parallel/pipeline/loop/phase/log），不提供数据变换和条件执行能力。如需 `run` 步骤或 `when` 条件，必须声明 `require`。

---

## 4. Step 类型

所有 step（除 `phase`/`log`）均支持 `when` 条件字段。**`when` 需要 `require: [node]` 或 `require: [bun]`**（见 [4.8](#48-when--条件执行)）。

### 4.1 phase — 阶段标记

标记当前进入指定阶段，用于进度展示分组。

```yaml
- phase: Scan
```

### 4.2 agent — 单 Agent 调用

创建并执行一个 Agent，返回其输出结果。

```yaml
- id: diff_stat
  agent:
    prompt: ./prompts/scan.md     # .md 文件路径（必需）
    label: scan-diff              # 标识（必需）
    phase: Scan                   # 关联阶段（可选）
    schema: review                # 引用 schemas 中的名称（可选）
    model: sonnet                 # 模型覆盖（可选）
```

| 字段 | 必须 | 说明 |
|------|------|------|
| `prompt` | 是 | 提示词 `.md` 文件相对路径 |
| `label` | 是 | Agent 标识，用于日志追踪 |
| `phase` | 否 | 关联 `phases` 中的阶段标题 |
| `schema` | 否 | 引用 `schemas` 中定义的名称，启用结构化输出 |
| `model` | 否 | 覆盖模型（`sonnet` / `opus` / `haiku`） |

**step 级字段**（与 `agent` 同级）：

| 字段 | 必须 | 说明 |
|------|------|------|
| `when` | 否 | JS 条件表达式（需 `require` 声明），falsy 时跳过此步骤 |

**提示词文件** 中的 `{{variable}}` 在执行时替换为实际值：

```markdown
<!-- prompts/scan.md -->
运行 git diff --stat {{params.range}} 获取变更文件列表，
然后运行 git diff {{params.range}} 获取完整 diff。
输出所有变更文件的路径列表，按 crate 分组。
```

### 4.3 parallel — 并行 fan-out

对集合中的每个元素并行创建 Agent 执行，**所有 Agent 完成后才继续**（栅栏语义）。

```yaml
- id: reviews
  parallel:
    over: "${params.dimensions}"     # 迭代集合（引用变量或参数）
    item: dim                         # 迭代变量名
    agent:
      prompt: "./prompts/review-${dim}.md"  # 支持变量插值
      label: "review:${dim}"
      phase: Review
      schema: review
```

| 字段 | 必须 | 说明 |
|------|------|------|
| `over` | 是 | 要迭代的集合（引用前序 `id` 或 `params.*`） |
| `item` | 是 | 迭代变量名（在 `prompt`/`label` 中通过 `${item}` 引用） |
| `agent` | 是 | 为每个元素创建的 Agent 定义 |

**注意**：`over` 引用数组时，每个元素独立创建 Agent 并行执行。并发度由执行器控制（上限 `min(16, cpu-2)`）。

**结果**：返回数组，每个元素是对应 Agent 的输出。元素出错时该位置为 `null`。

### 4.4 pipeline — 流水线处理

对集合中的每个元素顺序执行 Agent，可选将 Agent 输出合并回原元素。

```yaml
- id: verified
  pipeline:
    over: all_findings              # 要处理的集合
    item: finding                    # 迭代变量名
    agent:
      prompt: ./prompts/verify.md
      label: "verify:${finding.file}"
      phase: Verify
      schema: verify
    merge: verdict                  # 自定义字段名
```

| 字段 | 必须 | 说明 |
|------|------|------|
| `over` | 是 | 要迭代的集合 |
| `item` | 是 | 迭代变量名 |
| `agent` | 是 | 为每个元素创建的 Agent 定义 |
| `merge` | 否 | 字段名字符串，Agent 输出以此名称合并回原元素。省略则不合并 |

**merge 行为**：

| `merge` 值 | 输出 | 示例 |
|------------|------|------|
| `verdict` | `{...原元素, verdict: Agent输出}` | `{file: "a.rs", verdict: {confirmed: true}}` |
| `result` | `{...原元素, result: Agent输出}` | `{file: "a.rs", result: {score: 0.9}}` |
| 省略 | 仅返回 Agent 输出（不保留原元素） | `{confirmed: true, reason: "..."}` |

### 4.5 run — JS 脚本执行

> 需要 `require: [node]` 或 `require: [bun]`。

所有数据变换、条件判断均通过 JS 脚本完成。通过外部 Node/Bun 运行时执行，零嵌入体积。

```yaml
# 简单过滤
- id: confirmed
  run: |
    return input.filter(v => v.verdict?.confirmed);

# 复杂聚合
- id: grouped
  run: |
    const files = input.flatMap(r =>
      r.findings.map(f => ({ ...f, dimension: r.dimension }))
    );
    const grouped = {};
    for (const f of files) {
      (grouped[f.file] ??= []).push(f);
    }
    return Object.entries(grouped).map(([file, items]) => ({
      file,
      count: items.length,
      max_severity: items.some(i => i.severity === 'Critical') ? 'Critical' : 'High'
    }));
```

**执行模型**：

1. 将 `input` 变量引用的前序结果序列化为 JSON
2. 生成临时 `.js` 文件：`const input = <JSON>; <用户脚本>; console.log(JSON.stringify(result));`
3. 通过 `bun run` 或 `node` 执行，捕获 stdout
4. 解析 stdout 为 JSON 作为输出
5. 清理临时文件

**run 步骤字段**：

| 字段 | 必须 | 说明 |
|------|------|------|
| `run` | 是 | JavaScript 代码（`input` 为输入数据，必须 `return` 结果） |
| `input` | 否 | 引用前序步骤 id，作为 `input` 变量传入 |
| `when` | 否 | JS 条件表达式（见 [4.8](#48-when--条件执行)） |

**run 中可用的全局变量**：

| 变量 | 类型 | 说明 |
|------|------|------|
| `input` | `any` | `input` 字段引用的前序步骤结果 |
| `params` | `object` | 所有参数 |
| `vars` | `object` | 所有已执行的步骤 id → 结果 |

**Bun vs Node 选择**（声明 `require: [bun, node]` 时 Bun 优先）：

| 特性 | Bun | Node |
|------|-----|------|
| 冷启动 | ~30ms | ~200ms |
| 兼容性 | Node API 子集 | 完整 |
| 推荐 | ✅ 优先 | 兼容备选 |

### 4.6 loop — 循环

支持三种循环终止条件：干涸检测、计数目标和迭代上限。

#### 循环至干涸

持续执行直到连续 K 轮无新结果：

```yaml
- id: all_bugs
  loop:
    until_dry: 2                # 连续 2 轮无新结果则停止
    max_iterations: 10          # 安全上限（防止无限循环）
    collect: bugs               # 累积变量名
    dedup_by: id                # 去重字段
    body:
      - id: new_bugs
        agent:
          prompt: ./prompts/find-bugs.md
          schema: bug_list
          label: "find-bugs:${iteration}"
```

执行器在每轮结束后将 `new_bugs` 结果追加到 `all_bugs`，并用 `dedup_by` 字段去重。当连续 `until_dry` 轮新结果为空或达到 `max_iterations` 时停止。

#### 循环至计数

循环直到累积结果达到目标数量：

```yaml
- id: findings
  loop:
    until_count: 20             # 目标数量
    max_iterations: 5           # 安全上限
    collect: items
    dedup_by: title
    body:
      - id: new_items
        agent:
          prompt: ./prompts/generate.md
          schema: item_list
          label: "generate:${iteration}"
```

#### 循环至预算

在 token 预算耗尽前持续迭代：

```yaml
- id: improvements
  loop:
    until_budget: 50000         # 剩余 token 低于此值时停止
    max_iterations: 10
    collect: deltas
    body:
      - id: delta
        agent:
          prompt: ./prompts/optimize.md
          schema: improvement
          label: "optimize:${iteration}"
```

#### loop 字段说明

| 字段 | 必须 | 说明 |
|------|------|------|
| `until_dry` | 三选一 | 连续几轮无新结果则停止 |
| `until_count` | 三选一 | 累积结果达到此数量时停止 |
| `until_budget` | 三选一 | 剩余 token 低于此值时停止 |
| `max_iterations` | 是 | 最大迭代次数（安全阀） |
| `collect` | 是 | 累积结果的变量名 |
| `dedup_by` | 否 | 去重字段名 |
| `body` | 是 | 循环体步骤列表 |

循环体内支持 `${iteration}` 变量（当前迭代序号，从 1 开始）。

### 4.7 log — 进度消息

输出进度消息到事件流。

```yaml
- log: "收到 ${reviews.length} 个审查结果"
```

支持 `${id.field}` 模板变量引用前序步骤的结果。

### 4.8 when — 条件执行

> 需要 `require: [node]` 或 `require: [bun]`。

所有 step（除 `phase`/`log`）均可附加 `when` 条件。条件为 **JS 表达式**，求值为 truthy 时执行，falsy 时跳过。

```yaml
- id: urgent_notify
  when: "confirmed.filter(f => f.severity === 'Critical').length > 0"
  agent:
    prompt: ./prompts/urgent-notify.md
    label: urgent-notify
```

**`when` 中可用的全局变量**：与 `run` 相同——`input`、`params`、`vars`。

**表达式示例**：

| 条件 | JS 表达式 |
|------|----------|
| 数组非空 | `confirmed.length > 0` |
| 字段匹配 | `vars.status === 'ok'` |
| 数值比较 | `params.threshold < 0.8` |
| 复合条件 | `confirmed.length > 0 && params.mode === 'strict'` |
| 检查步骤是否执行 | `vars.optional_result !== undefined` |

**求值方式**：将表达式包裹为 `console.log(Boolean(<表达式>))`，通过 bun/node 执行，解析 stdout 为布尔值。

**跳过行为**：当 `when` 为 falsy 时，该步骤的 `id` 变量**不会被创建**。后续步骤通过 `vars.id === undefined` 检测。

### 4.9 Step 类型选择指南

| 场景 | 选择 |
|------|------|
| 单个任务 | `agent` |
| N 个独立任务，需要全部完成 | `parallel` |
| N 个任务需逐个处理 | `pipeline` |
| 数据变换/过滤/聚合 | `run`（JS 脚本） |
| 持续搜索/生成 | `loop` |
| 条件执行 | 任意 step + `when`（JS 表达式） |
| 标记进度阶段 | `phase` |
| 输出中间状态 | `log` |

---

## 5. 变量引用

### 5.1 变量来源

| 来源 | 语法 | 示例 |
|------|------|------|
| 参数 | `${params.name}` | `${params.range}` → `HEAD~10` |
| 前序步骤 | `${id}` | `${reviews}` → `[...]` |
| 前序步骤字段 | `${id.field}` | `${reviews.length}` → `6` |
| 迭代变量 | `${item}` | `${dim}` → `security` |
| 迭代变量字段 | `${item.field}` | `${finding.file}` → `src/main.rs` |

### 5.2 变量作用域

- **params**：在整个 workflow 中可用
- **步骤 id**：在声明该 id 的步骤之后的所有步骤中可用
- **迭代变量（item）**：仅在 `parallel` 或 `pipeline` 内部的 `agent` 定义中可用

---

## 6. Schema 定义

### 6.1 外部 JSON 文件

Schema 定义为独立的 `.json` 文件，放置在 `schemas/` 目录下：

```json
// schemas/review.json
{
  "type": "object",
  "properties": {
    "dimension": { "type": "string" },
    "findings": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "file": { "type": "string" },
          "severity": { "type": "string", "enum": ["Critical", "High", "Medium", "Low"] },
          "title": { "type": "string" },
          "description": { "type": "string" },
          "suggestion": { "type": "string" }
        },
        "required": ["file", "severity", "title", "description"]
      }
    },
    "summary": { "type": "string" }
  },
  "required": ["dimension", "findings", "summary"]
}
```

### 6.2 在 YAML 中引用

通过 `schemas` 顶层字段建立名称映射：

```yaml
schemas:
  review: ./schemas/review.json
  verify: ./schemas/verify.json
  synthesis: ./schemas/synthesis.json
```

然后在 `agent` 步骤中通过名称引用：

```yaml
- agent:
    schema: review    # 引用 schemas.review 对应的 JSON Schema
```

### 6.3 结构化输出

当 `agent` 步骤指定了 `schema` 时：
- Agent 被强制输出符合 Schema 的结构化数据
- 输出通过 Schema 验证，不符合时 Agent 重试
- 后续步骤可直接引用输出中的字段（如 `${reviews[0].findings}`）

---

## 7. 提示词模板

### 7.1 模板语法

提示词 `.md` 文件支持 `{{variable}}` 模板变量：

```markdown
<!-- prompts/verify.md -->
验证这个代码审查发现是否为真问题，不要放过任何可疑点。

文件：{{finding.file}}
维度：{{finding.dimension}}
严重级别：{{finding.severity}}
标题：{{finding.title}}
描述：{{finding.description}}

请读取相关文件的实际代码，验证：
1. 该问题是否真实存在（不是误报）
2. 严重级别是否准确
3. 是否有遗漏的相关代码路径
```

### 7.2 可用变量

在 `agent` 步骤的提示词中，以下变量可用：

| 变量 | 来源 |
|------|------|
| `{{params.*}}` | 所有参数及其值 |
| `{{step_id}}` | 引用的前序步骤完整输出 |
| `{{step_id.field}}` | 前序步骤输出的特定字段 |
| `{{item}}` | parallel/pipeline 的当前迭代元素 |
| `{{item.field}}` | 迭代元素的特定字段 |

### 7.3 条件内容

使用 `{{#if variable}}...{{/if}}` 条件渲染：

```markdown
{{#if params.focus_files}}
额外重点文件（用户指定）：
{{#each params.focus_files}}
- {{this}}
{{/each}}
{{/if}}
```

---

## 8. 质量模式

Workflow 的核心价值之一是支持可重复的质量模式。以下是 8 种核心模式的 YAML 实现：

### 8.1 对抗式验证（Adversarial Verify）

N 个"怀疑者" Agent 独立审查同一组发现，多数投票确认。

```yaml
- id: votes
  parallel:
    over: [1, 2, 3]
    item: i
    agent:
      prompt: ./prompts/skeptic.md
      label: "skeptic:${i}"
      schema: verdict

- id: confirmed
  run: |
    const surviving = input.filter(v => !v.refuted);
    return surviving.length >= 2 ? surviving : [];
```

**适用场景**：安全漏洞报告、Bug 确认等需要高置信度的场景。

### 8.2 视角多样化验证（Perspective-Diverse Verify）

不同 Agent 从不同专业角度审查同一内容。

```yaml
- id: reviews
  parallel:
    over: [correctness, security, performance]
    item: lens
    agent:
      prompt: "./prompts/review-${lens}.md"
      label: "review:${lens}"
      schema: review
```

**适用场景**：复杂发现的交叉验证。

### 8.3 评审团模式（Judge Panel）

N 个 Agent 独立设计方案，评审 Agent 选出最优。

```yaml
- id: proposals
  parallel:
    over: [1, 2, 3]
    item: i
    agent:
      prompt: "./prompts/propose.md"
      label: "propose:${i}"
      schema: proposal

- id: winner
  agent:
    prompt: ./prompts/judge.md
    label: judge
    schema: judgment
```

**适用场景**：解决方案空间较大的设计任务。

### 8.4 循环至干涸（Loop-Until-Dry）

持续搜索直到连续 K 轮无新发现。

```yaml
- id: all_bugs
  loop:
    until_dry: 2
    max_iterations: 10
    collect: bugs
    dedup_by: id
    body:
      - id: new_bugs
        agent:
          prompt: ./prompts/find-bugs.md
          schema: bug_list
          label: "find-bugs:${iteration}"
```

### 8.5 多模态扫描（Multi-Modal Sweep）

从不同搜索角度并行扫描，合并去重。

```yaml
- id: scan_results
  parallel:
    over: [by_container, by_content, by_entity]
    item: strategy
    agent:
      prompt: "./prompts/scan-${strategy}.md"
      label: "scan:${strategy}"
      schema: results

- id: deduped
  run: |
    const seen = new Set();
    return input.flatMap(r => r.results).filter(r => {
      if (seen.has(r.location)) return false;
      seen.add(r.location);
      return true;
    });
```

### 8.6 完整性批评（Completeness Critic）

一个 Agent 审查"缺少了什么"。

```yaml
- id: initial
  agent:
    prompt: ./prompts/analyze.md
    schema: analysis

- id: critique
  agent:
    prompt: ./prompts/critique.md
    schema: critique

- id: supplement
  agent:
    prompt: ./prompts/supplement.md
    schema: analysis
```

### 8.7 模式组合

实际使用中，模式通常组合应用：

| 任务类型 | 推荐组合 |
|----------|----------|
| 安全审计 | 多模态扫描 → 对抗式验证（3 怀疑者）→ 完整性批评 |
| 代码审查 | 视角多样化验证 → 对抗式验证 → `run` 汇总 |
| 架构设计 | 评审团（3 方案）→ 多视角评分 → 综合 |
| Bug 搜索 | 多模态扫描 → 对抗式验证 → 循环至干涸 |
| 大规模迁移 | 多模态扫描 → pipeline 逐文件迁移 → 验证 → 综合 |

---

## 9. 执行器架构

### 9.1 新增 Crate：`peri-workflow`

```
peri-workflow/
  Cargo.toml           # 依赖：peri-agent, serde_yaml, tokio
  src/
    lib.rs              # 公共 API
    types.rs            # YAML 反序列化类型
    parser.rs           # YAML + MD + JSON 加载器
    executor.rs         # 步骤执行引擎
    template.rs         # {{var}} 模板替换
    js_runner.rs        # JS 脚本执行（通过外部 Node/Bun 子进程）
    event.rs            # WorkflowEvent 事件定义
```

依赖仅 7 个 crate：

```toml
[dependencies]
peri-agent = { path = "../peri-agent" }
serde = { workspace = true, features = ["derive"] }
serde_yaml.workspace = true
serde_json.workspace = true
tokio = { workspace = true, features = ["full"] }
anyhow.workspace = true
tracing.workspace = true
```

### 9.2 依赖关系

```
peri-workflow → peri-agent（Agent 执行）
             → serde_yaml（YAML 解析）
             → tokio（async runtime）
```

`peri-workflow` 不依赖 `peri-acp`。执行器通过 trait 接收 Agent 构建能力，具体实现由集成层（`peri-tui` 或 `peri-acp`）提供。

### 9.3 执行流程

```
WorkflowExecutor::execute(args)
  │
  ├─ 1. 初始化 params（合并默认值和传入 args）
  │
  ├─ 2. 遍历 steps
  │     ├─ phase     → emit WorkflowEvent::PhaseStarted
  │     ├─ agent     → 评估 when → load MD + render template → build_agent → execute → store result
  │     ├─ parallel  → expand over → spawn agents → join_all → store array
  │     ├─ pipeline  → iterate items → sequential agent calls → optional merge(field)
  │     ├─ run       → 生成临时 .js → 子进程执行（Node/Bun）→ 解析 stdout JSON
  │     ├─ loop      → 评估终止条件 → 执行 body → 累积 collect → 去重 dedup_by
  │     └─ log       → emit WorkflowEvent::Log
  │
  └─ 3. 收集 return 引用的变量 → WorkflowResult
```

### 9.4 事件系统

```rust
pub enum WorkflowEvent {
    PhaseStarted { title: String },
    PhaseCompleted { title: String },
    AgentStarted { label: String, phase: Option<String> },
    AgentCompleted { label: String },
    LoopIteration { iteration: usize, collected_count: usize },
    Log { message: String },
    Progress { current: usize, total: usize },
    StepSkipped { id: Option<String>, reason: String },  // when 条件为 false
    Error { step: String, message: String },
}
```

事件通过 `tokio::sync::mpsc` 通道推送，供 TUI 进度显示或 stdout 输出消费。

### 9.5 JS 执行实现

所有数据变换（`run` 步骤）和条件判断（`when` 字段）通过外部 Node/Bun 子进程执行：

```rust
struct JsRunner {
    runtime: JsRuntime,  // Bun 或 Node，启动时检测
}

impl JsRunner {
    /// 执行 run 步骤的 JS 脚本，返回 JSON 结果
    async fn execute_script(
        &self,
        script: &str,
        input: &serde_json::Value,
        vars: &HashMap<String, serde_json::Value>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // 1. 生成临时 .js 文件
        let wrapped = format!(
            "const input = {};\nconst vars = {};\nconst params = {};\n{}\n",
            input, serde_json::to_string(&vars)?, serde_json::to_string(&params)?,
            script,
        );
        let tmp = write_temp_js(&wrapped)?;
        // 2. 子进程执行
        let output = match self.runtime {
            JsRuntime::Bun => Command::new("bun").arg("run").arg(&tmp).output().await?,
            JsRuntime::Node => Command::new("node").arg(&tmp).output().await?,
        };
        // 3. 解析 stdout JSON
        let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        // 4. 清理临时文件
        std::fs::remove_file(&tmp)?;
        Ok(result)
    }

    /// 评估 when 条件表达式
    async fn evaluate_when(
        &self,
        expr: &str,
        vars: &HashMap<String, serde_json::Value>,
        params: &HashMap<String, serde_json::Value>,
    ) -> Result<bool> {
        let script = format!("console.log(Boolean({}))", expr);
        let result = self.execute_script(&script, &Value::Null, vars, params).await?;
        Ok(result.as_bool().unwrap_or(false))
    }
}
```

### 9.7 核心类型

```rust
/// Workflow YAML 声明
#[derive(Debug, Deserialize)]
pub struct WorkflowDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub require: Vec<String>,         // 外部运行时依赖（"bun" 或 "node"）
    #[serde(default)]
    pub phases: Vec<PhaseDef>,
    #[serde(default)]
    pub params: HashMap<String, ParamDef>,
    #[serde(default)]
    pub schemas: HashMap<String, String>,
    pub steps: Vec<StepDef>,
    #[serde(default)]
    pub return_values: Option<Vec<String>>,
}

/// Agent 构建能力 trait（由集成层实现）
#[async_trait]
pub trait AgentRunner: Send + Sync {
    async fn run_agent(&self, prompt: &str, schema: Option<&serde_json::Value>) -> Result<serde_json::Value>;
}

/// 执行器
pub struct WorkflowExecutor {
    workflow: WorkflowDef,
    prompts: HashMap<String, String>,
    schemas: HashMap<String, serde_json::Value>,
    variables: HashMap<String, serde_json::Value>,
    agent_runner: Arc<dyn AgentRunner>,
    js_runner: Option<JsRunner>,      // 检测到 require 中的 JS 运行时时初始化
    event_tx: mpsc::Sender<WorkflowEvent>,
}
```

---

## 10. 集成方式

### 10.1 CLI 命令

```bash
# 执行 workflow
cargo run -p peri-tui -- workflow .claude/workflows/full-code-review/ --args '{"range": "HEAD~20"}'

# -p 模式（输出最终 JSON 到 stdout）
cargo run -p peri-tui -- -p "review" -- workflow .claude/workflows/full-code-review/
```

### 10.2 ACP Slash Command（后续扩展）

```bash
# 在 TUI 中直接执行
/workflow full-code-review --range HEAD~20
```

---

## 11. 质量模式组合公式

| 任务类型 | 推荐组合 | 关键 Step 序列 |
|----------|----------|----------------|
| 安全审计 | 多模态扫描 → 对抗式验证 → 完整性批评 | parallel + parallel + agent |
| 代码审查 | 视角多样化 → 对抗式验证 → run 汇总 | parallel + pipeline + run |
| 架构设计 | 评审团 → 多视角评分 → 综合 | parallel + parallel + agent |
| Bug 搜索 | 多模态扫描 → 对抗式验证 → 循环 | parallel + parallel + loop |
| 大规模迁移 | 扫描 → pipeline 迁移 → 验证 → 综合 | agent + pipeline + pipeline + agent |

---

## 附录 A：YAML Step 速查表

```yaml
# 顶层声明
name: workflow-name
require: [bun]               # 可选，声明外部运行时依赖（node/bun）

# 阶段标记
- phase: PhaseName

# 单 Agent（支持 when 条件，需要 require 声明）
- id: result_name
  when: "vars.count > 0"          # 可选，JS 表达式，falsy 时跳过
  agent:
    prompt: ./prompts/file.md
    label: agent-label
    phase: PhaseName          # 可选
    schema: schema_name       # 可选
    model: sonnet             # 可选

# 并行 fan-out
- id: results_array
  parallel:
    over: "${params.collection}"    # 或前序步骤 id
    item: item_var
    agent:
      prompt: "./prompts/${item_var}.md"
      label: "prefix:${item_var}"
      schema: schema_name

# 流水线处理
- id: processed
  pipeline:
    over: previous_step_id
    item: item_var
    agent:
      prompt: ./prompts/process.md
      label: "prefix:${item_var.field}"
      schema: schema_name
    merge: verdict            # 可选，自定义字段名

# JS 数据变换（需要 require: [bun]）
- id: filtered
  input: previous_step_id
  run: |
    return input.filter(item => item.active === true)
              .sort((a, b) => a.priority - b.priority)
              .slice(0, 10);

# 循环至干涸
- id: all_results
  loop:
    until_dry: 2
    max_iterations: 10
    collect: items
    dedup_by: id
    body:
      - id: new_items
        agent:
          prompt: ./prompts/generate.md
          schema: item_list
          label: "gen:${iteration}"

# 进度消息
- log: "消息 ${variable.field}"
```

## 附录 B：与 Claude Code JS Workflow 的概念映射

| Claude Code JS | Perihelion YAML | 说明 |
|----------------|-----------------|------|
| `export const meta = {…}` | YAML 顶层 `name`/`description`/`phases` | 声明方式不同 |
| `agent(prompt, {schema})` | `agent` step + `schema` 引用 | prompt 移到 .md 文件 |
| `pipeline(items, fn, merge)` | `pipeline` step + `merge: fieldname` | 声明式，可自定义字段名 |
| `parallel([() => agent()])` | `parallel` step + `over`/`item` | 声明式，无工厂函数 |
| `phase('title')` | `phase` step | 一致 |
| `log('message')` | `log` step | 一致 |
| `args.param` | `${params.param}` | 模板变量语法 |
| `budget.total/spent()` | `loop.until_budget` | 循环内置预算控制 |
| `const x = js_expr` | `run` step（JS 脚本，需 `require: [bun]`） | 完整 JS 能力，外部运行时 |
| 内联 JSON Schema | 外部 `.json` 文件 | 独立管理 |
| 内联 prompt 字符串 | 外部 `.md` 文件 | 独立管理 |

---

---

## 附录 C：设计决策与约束

### C.1 为什么所有数据操作统一用 JS（不做 Rust 内置 transform）

**排除的方案**：在 Rust 中实现内置声明式 transform（filter/flatten/sort_by 等 8 种操作）和简单 when 表达式引擎。

**决策**：统一使用 JS 脚本（通过外部 Node/Bun）处理所有数据变换和条件判断。理由：

- **表达能力**：8 种内置操作无法覆盖所有场景（如 groupBy、条件映射、字符串处理），JS 天然覆盖一切
- **维护成本**：内置操作需要逐一实现、测试、处理边界情况；JS 零维护成本
- **一致性**：数据变换和条件判断用同一种语言，不混搭 DSL
- **性能可接受**：Bun 冷启动 ~30ms，对 Agent 步骤（5-30 秒）可忽略
- **代价**：`require: [bun]` 或 `require: [node]` 变为事实上的必选项

### C.2 为什么用外部 Node/Bun 而非内嵌 rquickjs

**考虑过的方案**：acts 使用内嵌 `rquickjs`（QuickJS 引擎），带来 8.5 MB rlib + 244 新 crate。

**决策**：通过 `require: [bun]` 或 `require: [node]` 声明外部 JS 运行时，零嵌入体积。

| 维度 | rquickjs（内嵌） | 外部 Node/Bun |
|------|-----------------|---------------|
| 二进制增量 | +8.5 MB | +0 KB |
| 新增 crate | +244 | +0 |
| JS 能力 | QuickJS 子集 | 完整 Node API |
| 包管理器 | 无 | npm/bun 生态 |
| 跨平台 | ✅ 内嵌一致 | ✅ Node/Bun 均跨平台 |
| 启动延迟 | ~1ms | Bun ~30ms / Node ~200ms |

Bun 的 ~30ms 冷启动对于 Agent 步骤（通常 5-30 秒）完全可忽略。而零嵌入体积和完整 JS 生态是显著优势。

提示词统一使用独立 `.md` 文件，不提供内联模式。理由：

- **审查一致性**：所有提示词都是独立文件，便于领域专家审查和版本控制
- **复用性**：同一个 `.md` 文件可被多个 workflow 引用
- **关注点分离**：编排逻辑（YAML）和提示词内容（MD）物理隔离

### C.3 JS 子进程的跨平台注意事项

JS `run` 步骤和 `when` 条件通过子进程执行 Node/Bun，需注意以下跨平台差异：

| 行为 | Unix | Windows |
|------|------|---------|
| 进程启动 | `Command::new("bun")` | ✅ 同 |
| stdout 捕获 | ✅ UTF-8 | ✅ UTF-8（Node/Bun 均默认） |
| 退出码 | ✅ 可靠 | ✅ 可靠 |
| 临时文件 | `/tmp/` | `%TEMP%\` |

Node 和 Bun 本身跨平台一致，`run`/`when` 不经过 shell 管道（直接子进程执行 `.js` 文件），因此不受 shell 差异影响。临时文件通过 `std::env::temp_dir()` 获取平台合适的路径。

### C.4 路径处理

项目已有跨平台路径归一化（`path_to_posix()`），Windows `\` → `/`。YAML 中的路径（`./prompts/scan.md`）使用正斜杠，`Path::new()` 在所有平台上均可正确解析。Workflow executor 应复用此模式，无需额外处理。

### C.5 模板引擎边界情况

提示词 `.md` 文件中的 `{{variable}}` 模板语法（仅用于 agent prompt 替换）需处理：

| 场景 | 处理方式 |
|------|----------|
| Markdown 中的 `{{` 字面量 | 转义语法 `\{{literal}}` → `{{literal}}`（不解析） |
| 变量不存在 | 默认留空（lenient），可配置为 strict（报错） |
| CJK 字符 | 模板解析基于字节偏移，不依赖 Unicode 边界 |

### C.6 改进项追踪

| 优先级 | 改进项 | 状态 |
|--------|--------|------|
| **P0** | `require` + `run` JS 脚本执行（外部 Node/Bun） | ✅ 已纳入 v3 |
| **P0** | `when` JS 条件表达式 | ✅ 已纳入 v3 |
| **P1** | `loop` 循环模式（until_dry / until_count / until_budget） | ✅ 已纳入 v3 |
| **P1** | 自定义 merge 字段名 | ✅ 已纳入 v3 |
| **P2** | 模板引擎边界处理（转义、未定义变量） | ✅ 已记录约束 |
| ~~P0~~ | ~~Rust 内置声明式 transform~~ | ❌ 不做，统一用 JS run |
| ~~P0~~ | ~~Rust when 表达式引擎~~ | ❌ 不做，统一用 JS 表达式 |
| ~~P0~~ | ~~内联提示词~~ | ❌ 不做，统一用外部 .md 文件 |
| ~~P0~~ | ~~内嵌 rquickjs~~ | ❌ 不做，用外部 Node/Bun 替代 |

---

*生成日期：2026-05-29 | Perihelion Workflow 系统设计文档 v3*
