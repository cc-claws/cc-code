# Git 安全提示词与留名 执行计划

**设计文档:** `spec/feature_20260514_F001_git-attribution-research/spec-design.md`

---

## 改动总览

| # | Task | 文件 | 类型 |
|---|------|------|------|
| 1 | Git 安全提示词扩展 | `peri-tui/prompts/sections/04_actions.md` | 修改 |
| 2 | 模型邮箱映射表 | `peri-middlewares/src/attribution/model_email.rs` | 新增 |
| 3 | 追踪状态与字符贡献 | `peri-middlewares/src/attribution/state.rs` | 新增 |
| 4 | Middleware 实现 + mod.rs | `peri-middlewares/src/attribution/mod.rs` | 新增 |
| 5 | 中间件链注册 | `peri-tui/src/app/agent.rs`, `src/acp/agent_assembler.rs` | 修改 |

Task 1 与 Task 2-5 可并行。Task 2→3→4 有顺序依赖。

---

## Task 1: Git 安全提示词扩展

**涉及文件:** `peri-tui/prompts/sections/04_actions.md`

### 步骤

1. 在 `04_actions.md` 末尾（`## Simplicity & Surgical Changes` 内容块之后，文件末尾前）追加 Git Safety Protocol 段落。

追加内容：

```markdown
## Git Safety Protocol

- NEVER update the git config
- NEVER run destructive/irreversible git commands (push --force, hard reset, etc) unless the user explicitly requests them
- NEVER skip hooks (--no-verify, --no-gpg-sign, etc) unless the user explicitly requests it
- NEVER run force push to main/master — warn the user if they request it
- Do not commit files that likely contain secrets (.env, credentials.json, etc). Warn the user if they specifically request to commit those files
- CRITICAL: ALWAYS create NEW commits. NEVER use git commit --amend unless the user explicitly requests it
- Never use git commands with the -i flag (git rebase -i, git add -i) since they require interactive input
```

### 验证

```bash
grep "Git Safety Protocol" peri-tui/prompts/sections/04_actions.md
cargo build -p peri-tui 2>&1 | tail -3
```

---

## Task 2: 模型邮箱映射表

**涉及文件:** `peri-middlewares/src/attribution/model_email.rs`（新建）

### 步骤

1. 创建 `peri-middlewares/src/attribution/model_email.rs`
2. 实现 `MODEL_EMAIL_MAP` 常量（10 个模型族）
3. 实现 `get_attribution_email(model_name: &str) -> &str` 函数
4. 编写单元测试

关键代码：

```rust
const MODEL_EMAIL_MAP: &[(&[&str], &str)] = &[
    (&["claude"],                             "noreply@anthropic.com"),
    (&["gpt", "dall-e", "o1-", "o3-", "o4-"], "openai@claude-code-best.win"),
    (&["gemini"],                             "google-gemini@claude-code-best.win"),
    (&["grok"],                               "xai-org@claude-code-best.win"),
    (&["glm"],                                "zai-org@claude-code-best.win"),
    (&["deepseek"],                           "deepseek-ai@claude-code-best.win"),
    (&["qwen"],                               "QwenLM@claude-code-best.win"),
    (&["minimax"],                            "MiniMax-AI@claude-code-best.win"),
    (&["mimo"],                               "XiaomiMiMo@claude-code-best.win"),
    (&["kimi"],                               "MoonshotAI@claude-code-best.win"),
];

pub fn get_attribution_email(model_name: &str) -> &str {
    let lower = model_name.to_lowercase();
    for (keywords, email) in MODEL_EMAIL_MAP {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return email;
        }
    }
    "noreply@anthropic.com"
}
```

### 验证

```bash
cargo test -p peri-middlewares --lib model_email
```

**测试覆盖：** 14 个场景（每个模型族 1 个 + 大小写不敏感 + 未匹配回退 + O-series 推理模型）

---

## Task 3: 追踪状态与字符贡献计算

**依赖:** Task 2

**涉及文件:** `peri-middlewares/src/attribution/state.rs`（新建）

### 步骤

1. 创建 `state.rs`，实现 `FileContribution`、`AttributionState` 结构体
2. 实现字符级 prefix/suffix 匹配算法（`track_change`）
3. 实现 `co_authored_by()` 方法
4. 编写单元测试

### 验证

```bash
cargo test -p peri-middlewares --lib state
```

**测试覆盖：** 全新增、全删除、末尾追加、中间修改、等长替换、累积贡献、CJK 字符

---

## Task 4: Middleware 实现与模块导出

**依赖:** Task 3

**涉及文件:** `peri-middlewares/src/attribution/mod.rs`（新建）、`peri-middlewares/src/lib.rs`（修改）

### 步骤

1. 创建 `mod.rs`，实现 `GitAttributionMiddleware`
2. 实现三个钩子：
   - `before_tool`: 检测 Write/Edit→读取文件旧内容→存入 `pending_old_content`
   - `after_tool`: 检测 Write/Edit→读取文件新内容→调用 `track_change`
   - `before_agent`: 生成 Co-Authored-By System 消息注入
3. 在 `lib.rs` 中导出 `pub mod attribution;`

关键结构：

```rust
pub struct GitAttributionMiddleware {
    state: Arc<Mutex<AttributionState>>,
    pending_old_content: Arc<Mutex<HashMap<String, String>>>,
}
```

### 验证

```bash
cargo build -p peri-middlewares 2>&1 | tail -5
cargo test -p peri-middlewares --lib attribution 2>&1 | tail -5
```

---

## Task 5: 中间件链注册

**依赖:** Task 4

**涉及文件:** `peri-tui/src/app/agent.rs`、`peri-tui/src/acp/agent_assembler.rs`

### 步骤

1. 在两个注册点中，将 `GitAttributionMiddleware` 注入中间件链，位置紧接 `FilesystemMiddleware` 之后
2. 从 LLM provider 获取当前模型名称传入 middleware 构造函数

### 验证

```bash
cargo build -p peri-tui 2>&1 | tail -3
cargo test -p peri-tui 2>&1 | tail -5
```

---

## 端到端验收

1. 系统提示词包含 Git Safety Protocol 段落
2. 全部 10 个模型族邮箱映射正确
3. Write/Edit 文件后 attribution state 正确累积
4. 模型在生成 commit 时提示词包含 Co-Authored-By 指令
