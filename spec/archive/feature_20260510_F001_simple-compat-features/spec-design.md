# Feature: 20260510_F001 - 简单兼容特性批次

## 需求背景

COMPATIBLE.md 列出 190 个 ❌ 缺失项，其中部分特性改动量极小（< 100 行）但实用价值高。本批次聚焦**配置系统补全**和**TUI 命令交互**两个方向，挑选 6 个最简单的特性集中实现，快速提升兼容覆盖度。

## 目标

- 实现 4 项配置系统补全（C1/C2/C4/C6），对齐 Claude Code 的 CLAUDE.md 加载机制
- 实现 3 项 TUI 命令（T1/T2/T5），补全常用交互命令
- 总改动量控制在 ~300 行以内，不引入新依赖

## 方案设计

### 配置系统补全

#### C1 — CLAUDE.local.md 支持

Claude Code 支持 `./CLAUDE.local.md` 作为个人项目级配置（不入库），Peri 当前未读取此文件。

**实现位置：** `peri-middlewares/src/agents_md.rs` — `AgentsMdMiddleware::before_agent()`

**加载顺序：**
```
CLAUDE.md → .claude/CLAUDE.md → AGENTS.md → CLAUDE.local.md
```

**细节：**
- `CLAUDE.local.md` 追加到已有 CLAUDE.md 内容末尾，不单独生成 system block
- 文件不存在则静默跳过（与 CLAUDE.md 行为一致）
- 无需自动 gitignore 处理——用户自行 `.gitignore` 管理

#### C2 — `$schema` URL 字段

Claude Code 的 `settings.json` 支持 `$schema` 字段指向 JSON Schema 文档，编辑器可提供自动补全。

**实现位置：** `peri-tui/src/config/types.rs` — `PeriConfig` struct

**细节：**
```rust
#[serde(rename = "$schema")]
pub schema: Option<String>,
```
- 纯 passthrough 字段，读取时保留、写入时回写，不影响任何逻辑
- 无验证，无默认值

#### C4 — `@import` 外部文件引用

Claude Code 支持 `<!-- @import path -->` 语法在 CLAUDE.md 中引用外部文件内容。

**实现位置：** `peri-middlewares/src/agents_md.rs` — 内容后处理函数

**流程：**
```
读取 CLAUDE.md 内容
  → 正则匹配 <!-- @import path -->
  → 解析 path（相对于 CLAUDE.md 所在目录）
  → 读取引用文件内容替换占位符
  → 递归处理（深度上限 3，visited 集合防循环）
  → 返回最终内容
```

**正则：** `<!--\s*@import\s+(\S+)\s*-->`

**边界处理：**
- 引用文件不存在 → 保留原始占位符，不报错
- 超过深度上限 → 保留原始占位符，不报错
- 循环引用 → visited 集合检测，保留原始占位符
- 仅处理 CLAUDE.md / CLAUDE.local.md，AGENTS.md 不处理

#### C6 — CLAUDE.md 排除 glob

Claude Code 支持 `claudeMdExcludes` 配置项，通过 glob 模式跳过特定路径的 CLAUDE.md。

**实现位置：**
- `peri-tui/src/config/types.rs` — `PeriConfig` 新增字段
- `peri-middlewares/src/agents_md.rs` — 扫描时过滤

**配置格式：**
```json
{
  "claudeMdExcludes": ["node_modules/**", "vendor/**"]
}
```

**实现：**
```rust
// PeriConfig
pub claude_md_excludes: Option<Vec<String>>,

// AgentsMdMiddleware 扫描时
if let Some(patterns) = &config.claude_md_excludes {
    if patterns.iter().any(|p| glob_match(p, path)) {
        continue; // 跳过
    }
}
```

**注意：** AgentsMdMiddleware 位于 `peri-middlewares`（不依赖 config types），排除模式需通过 Middleware 初始化参数传入，不直接读取 PeriConfig。

### TUI 命令

#### T1 — `/effort` 命令

调整推理力度级别，控制 Thinking 模型的 reasoning effort。

**实现位置：** `peri-tui/src/command/` 新增 `effort.rs`

**命令格式：**
- `/effort` — 显示当前 effort 级别
- `/effort low` / `/effort medium` / `/effort high` — 设置级别

**行为：**
- 无参数时输出当前 effort 级别到消息区
- 有参数时更新 `App.provider.thinking.effort`，输出确认消息
- 参考 `/model <alias>` 的即时切换模式（不需要打开面板）
- 如果 thinking 未启用，提示用户先通过 `/config` 启用

**数据流：**
```
/effort high
  → CommandRegistry::dispatch("effort", args=["high"])
  → app.provider.thinking.effort = ThinkingEffort::High
  → 输出 "推理力度已设为 high"
```

#### T2 — `/rename` 命令

为当前会话设置标题，方便在 /history 面板中识别。

**实现位置：** `peri-tui/src/command/` 新增 `rename.rs`

**命令格式：**
- `/rename` — 显示当前标题
- `/rename <name>` — 更新标题

**行为：**
- 无参数时显示当前 ThreadMeta.title
- 有参数时调用 `thread_store.update_title(thread_id, name)`
- `update_title` 方法需在 ThreadStore trait 和 SqliteThreadStore 中新增
- 输出确认消息

**SQL：**
```sql
UPDATE threads SET title = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?
```

#### T5 — `/doctor` 健康检查

检测配置完整性，帮助排查启动问题。

**实现位置：** `peri-tui/src/command/` 新增 `doctor.rs`

**检查项：**

| 检查项 | 检测方式 | 状态 |
|--------|----------|------|
| settings.json | `Path::exists("~/.peri/settings.json")` | OK / Missing |
| API Key | `env::var("ANTHROPIC_API_KEY")` 或 `env::var("OPENAI_API_KEY")` | OK / Missing |
| Provider 配置 | `app.providers` 非空 | OK / No Provider |
| MCP 配置 | `.mcp.json` 或 settings.json 中 `mcpServers` | OK / None / Error |
| Model Alias | 至少有一个 alias 配置 | OK / No Alias |

**输出格式：**
```
Doctor 检查结果：
| 检查项 | 状态 | 详情 |
|--------|------|------|
| Settings | OK | ~/.peri/settings.json |
| API Key | OK | ANTHROPIC_API_KEY |
| Provider | OK | anthropic (claude-sonnet-4-20250514) |
| MCP | None | 未配置 MCP 服务器 |
| Model Alias | OK | opus/sonnet/haiku |
```

结果以系统消息形式添加到消息区（不发送到 LLM）。

## 实现要点

### 依赖关系

```
C2 ($schema)         — 独立，无依赖
C1 (CLAUDE.local.md) — 独立，无依赖
C6 (CLAUDE.md 排除)  — 依赖 C2（同文件 config/types.rs）
C4 (@import)         — 依赖 C1（先确定 CLAUDE.md 加载流程）
T1 (/effort)         — 独立
T2 (/rename)         — 独立（需新增 ThreadStore 方法）
T5 (/doctor)         — 独立
```

### 实施顺序建议

1. C2 ($schema) — 5 行改动
2. C1 (CLAUDE.local.md) — 20 行改动
3. C6 (CLAUDE.md 排除) — 30 行改动
4. C4 (@import) — 50 行改动
5. T2 (/rename) — 40 行改动
6. T1 (/effort) — 50 行改动
7. T5 (/doctor) — 80 行改动

### 新增依赖

无。`@import` 路径解析用 `std::path`，glob 匹配用已有的 `glob` crate。

### 不引入的设计

- `paths:` frontmatter — 复杂度较高，留后续批次
- `.claude/settings.json` 项目级配置合并 — 需要设计多层合并策略，超出简单批次范围
- `.claude/settings.local.json` — 同上

## 约束一致性

- **Middleware Chain 模式：** C1/C4/C6 均在 `AgentsMdMiddleware::before_agent()` 中处理，符合现有中间件模式
- **系统提示词段落化：** C1/C4 追加到 CLAUDE.md 内容块，由 PrependSystemMiddleware 统一处理，不破坏段落化架构
- **Command trait：** T1/T2/T5 均实现 Command trait，注册到 CommandRegistry，符合命令分发模式
- **配置类型：** C2/C6 新增字段到 PeriConfig，serde passthrough，不破坏向后兼容
- **Workspace 分层：** C6 的排除模式通过 AgentsMdMiddleware 初始化参数传入（`with_excludes(patterns: Vec<String>)`），不引入 middlewares → tui 的反向依赖

## 验收标准

- [ ] C1: `CLAUDE.local.md` 存在时内容被加载，不存在时静默跳过
- [ ] C2: `settings.json` 中 `$schema` 字段可正常读写，不报错
- [ ] C4: `<!-- @import path -->` 语法正确替换为引用文件内容
- [ ] C4: 循环引用、超深嵌套、文件不存在等边界情况不 panic
- [ ] C6: `claudeMdExcludes` glob 匹配到的 CLAUDE.md 路径被跳过
- [ ] T1: `/effort` 无参数显示当前级别，有参数切换成功
- [ ] T1: `/effort` 切换后下轮 LLM 调用使用新的 effort 级别
- [ ] T2: `/rename <name>` 更新 ThreadMeta.title 并持久化
- [ ] T2: `/history` 面板显示更新后的标题
- [ ] T5: `/doctor` 输出 5 项检查结果到消息区
- [ ] T5: 各检查项状态正确反映实际情况
- [ ] 总改动量 < 350 行
- [ ] `cargo build` 无 warning
- [ ] `cargo test` 全量通过
