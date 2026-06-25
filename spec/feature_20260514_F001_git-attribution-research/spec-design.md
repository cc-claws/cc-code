# Feature: 20260514_F001 - Git 安全提示词与留名

## 需求背景

claude-code 在系统提示词和 commit 命令中注入了完整的 Git 安全协议（不更新 config、不 force-push、不含 secret 文件等），并通过 `commitAttribution.ts` 等模块实现了 AI 模型在 commit 中的署名机制（Co-Authored-By trailer + 贡献百分比追踪）。

peri 当前：

- 系统提示词中仅有通用的操作安全指导（"prefer reversible operations"、"confirm destructive actions"），缺少 git 专属规则
- 无任何 commit 署名机制——AI 生成的代码被提交后无法追溯模型来源

需要参照 claude-code 的设计，实现两个能力：

1. **Git 安全提示词**：将 git 操作的具体安全规则注入系统提示词
2. **Git 留名**：跟踪文件变更，让模型在 commit 中自动追加 Co-Authored-By trailer

## 目标

- 系统提示词中加入 git 安全协议（不更新 config、禁止 force-push/--amend、不提交 .env 等）
- 支持 Write/Edit 工具的文件变更追踪
- 模型在提交时自动生成 `Co-Authored-By: model-name <email>`（硬编码格式、不可配置）
- 全面支持所有 peri 支持的模型族（Anthropic/OpenAI/Gemini/Grok/GLM/DeepSeek/Qwen/MiniMax/Kimi/MiMo）
- 代码改动最小化（<300 行新增代码 + <20 行提示词扩展）

## 不在此范围

- 文件变更百分比统计（后续版本）
- 内置 /commit 斜杠命令（后续统一设计 SlashCommand 机制）
- PR body 中的 attribution 文本
- 公开仓库"卧底模式"（Undercover）
- Attribution trailer 持久化到 git notes

## 方案设计

### 架构概览

两个独立改动，互不依赖：

**A. Git 安全提示词**：

- 扩展 `peri-tui/prompts/sections/04_actions.md`，增加 Git Safety Protocol 段落
- 内容放入静态缓存段（01-06），参与 Anthropic prompt cache

**B. Git 留名追踪**：

- 新增 `peri-middlewares/src/attribution/` 模块
- 新增 `GitAttributionMiddleware`，hook `before_tool` / `after_tool` 追踪 Write/Edit
- 通过 middleware 的 `before_agent` 钩子注入 Co-Authored-By 指令到消息历史

```
FilesystemMiddleware (Write/Edit 工具)
  → GitAttributionMiddleware.before_tool()
    → 读取文件旧内容，存入 pending_changes
  → [Write/Edit 工具执行]
  → GitAttributionMiddleware.after_tool()
    → 读取文件新内容，计算贡献字符数
    → 更新 AttributionState

On commit (模型自主行为):
  → 模型读取系统提示词中的 Co-Authored-By 指令
  → 在 commit message 末尾追加 Co-Authored-By 行
```

### 详细设计

#### 1. 系统提示词扩展

修改 `peri-tui/prompts/sections/04_actions.md`，在现有内容后追加：

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

#### 2. Attribution 模块结构

```
peri-middlewares/src/attribution/
├── mod.rs              # 公开 API + GitAttributionMiddleware
├── model_email.rs      # MODEL_EMAIL_MAP: model 关键词 → 邮箱
└── state.rs            # AttributionState + 追踪逻辑
```

**`model_email.rs`** — 模型→邮箱映射表：

```rust
const MODEL_EMAIL_MAP: &[(&[&str], &str)] = &[
    (&["claude"],                             "noreply@anthropic.com"),
    (&["gpt", "dall-e", "o1-", "o3-", "o4-"], "openai@peri.ai"),
    (&["gemini"],                             "google-gemini@peri.ai"),
    (&["grok"],                               "xai-org@peri.ai"),
    (&["glm"],                                "zai-org@peri.ai"),
    (&["deepseek"],                           "deepseek-ai@peri.ai"),
    (&["qwen"],                               "QwenLM@peri.ai"),
    (&["minimax"],                            "MiniMax-AI@peri.ai"),
    (&["mimo"],                               "XiaomiMiMo@peri.ai"),
    (&["kimi"],                               "MoonshotAI@peri.ai"),
];

pub fn get_attribution_email(model_name: &str) -> &str {
    let lower = model_name.to_lowercase();
    for (keywords, email) in MODEL_EMAIL_MAP {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return email;
        }
    }
    "noreply@anthropic.com"  // 默认回退
}
```

**`state.rs`** — 追踪状态与计算：

```rust
pub struct FileContribution {
    pub claude_chars: usize,  // 累积贡献字符数
    pub file_hash: String,    // SHA-256 校验（可选，版本追踪用）
}

pub struct AttributionState {
    pub contributions: HashMap<String, FileContribution>,  // 相对路径 → 贡献
    pub model_name: String,
    pub email: String,
}

impl AttributionState {
    pub fn new(model_name: String) -> Self {
        let email = get_attribution_email(&model_name).to_string();
        Self { contributions: HashMap::new(), model_name, email }
    }

    /// 计算字符级贡献：前缀/后缀匹配找出实际变更区域
    pub fn track_change(
        &mut self, file_path: &str, old_content: &str, new_content: &str
    ) {
        let contribution = if old_content.is_empty() || new_content.is_empty() {
            // 新文件或全量删除
            if old_content.is_empty() { new_content.len() } else { old_content.len() }
        } else {
            // 前缀/后缀匹配找出差异化区域
            let min_len = old_content.len().min(new_content.len());
            let prefix_len = old_content.chars()
                .zip(new_content.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let suffix_len = old_content.chars().rev()
                .zip(new_content.chars().rev())
                .take_while(|(a, b)| a == b)
                .count();
            let old_changed = old_content.len().saturating_sub(prefix_len + suffix_len);
            let new_changed = new_content.len().saturating_sub(prefix_len + suffix_len);
            old_changed.max(new_changed)
        };

        let entry = self.contributions.entry(file_path.to_string()).or_insert_with(|| FileContribution {
            claude_chars: 0,
            file_hash: String::new(),
        });
        entry.claude_chars += contribution;
    }

    /// 生成 Co-Authored-By trailer 文本
    pub fn co_authored_by(&self) -> String {
        format!("Co-Authored-By: {} <{}>", self.model_name, self.email)
    }
}
```

**`mod.rs`** — Middleware 实现：

```rust
pub struct GitAttributionMiddleware {
    state: Arc<Mutex<AttributionState>>,
    // pending: before_tool 时暂存的旧内容，key 为 file_path
    pending_old_content: Arc<Mutex<HashMap<String, String>>>,
}

impl GitAttributionMiddleware {
    pub fn new(model_name: &str) -> Self { ... }
    pub fn attribution_text(&self) -> String { ... }
}

#[async_trait]
impl<S: State> Middleware<S> for GitAttributionMiddleware {
    fn name(&self) -> &str { "GitAttributionMiddleware" }

    async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
        // 仅处理 Write 和 Edit
        if tool_call.name != "Write" && tool_call.name != "Edit" {
            return Ok(tool_call.clone());
        }
        // 读取当前文件内容
        if let Some(file_path) = tool_call.input.get("file_path").and_then(|v| v.as_str()) {
            if let Ok(old_content) = tokio::fs::read_to_string(file_path).await {
                self.pending_old_content.lock().unwrap()
                    .insert(file_path.to_string(), old_content);
            }
        }
        Ok(tool_call.clone())
    }

    async fn after_tool(&self, _state: &mut S, tool_call: &ToolCall, _result: &ToolResult) -> AgentResult<()> {
        if tool_call.name != "Write" && tool_call.name != "Edit" {
            return Ok(());
        }
        let file_path = match tool_call.input.get("file_path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return Ok(()),
        };
        let old_content = self.pending_old_content.lock().unwrap().remove(file_path).unwrap_or_default();
        let new_content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(_) => return Ok(())),
        };
        self.state.lock().unwrap().track_change(file_path, &old_content, &new_content);
        Ok(())
    }

    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 注入 Co-Authored-By 指令到系统消息
        let text = format!(
            "## Git Attribution\n\nWhen creating git commits, append the following line to the commit message:\n\n```\n{}\n```\n\nThis tracks AI contributions and should always be included in commit messages for code you authored.",
            self.attribution_text()
        );
        state.add_message(BaseMessage::system(text));
        Ok(())
    }
}
```

#### 3. 中间件链注册

在 TUI 的中间件链中注册 `GitAttributionMiddleware`，放在 `FilesystemMiddleware` 之后：

```
FilesystemMiddleware  ← Write/Edit 工具提供
GitAttributionMiddleware  ← hook after_tool 追踪文件变更
```

由于 `GitAttributionMiddleware` 不提供新的工具（`collect_tools` 返回空），它只作为观察者存在。

#### 4. 提示词注入位置

Co-Authored-By 指令通过 `before_agent` 注入为 System 消息（不在 `04_actions.md` 中），原因：

- 模型名称是运行时动态确定的（取决于用户选择的模型），无法静态嵌入 04_actions.md
- System 消息天然在 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 之后，不影响 prompt cache

### 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `peri-tui/prompts/sections/04_actions.md` | 修改 | 追加 Git Safety Protocol 段落 |
| `peri-middlewares/src/attribution/mod.rs` | 新增 | GitAttributionMiddleware + 公开 API |
| `peri-middlewares/src/attribution/model_email.rs` | 新增 | 模型→邮箱映射表 |
| `peri-middlewares/src/attribution/state.rs` | 新增 | AttributionState + 追踪逻辑 |
| `peri-middlewares/src/attribution/mod.rs` | 新增 | 模块入口 + pub use |
| `peri-middlewares/src/lib.rs` | 修改 | 导出 attribution 模块 |
| `peri-tui/src/app/agent.rs` 或中间件注册处 | 修改 | 注册 GitAttributionMiddleware |

### 关键决策记录

1. **邮箱域名使用 `@peri.ai`**：因 GitHub 组织不支持 Co-Authored-By，使用自有域名构造虚拟邮箱（参照 claude-code 的 `@claude-code-best.win`）
2. **字符级贡献计算使用 prefix/suffix 匹配**：比简单的 `Math.abs(len_diff)` 更准确，能处理等长替换（如 `"Esc"` → `"esc"`）
3. **不计算百分比**（本迭代）：用户需要的最小实现是署名，百分比统计可后续加
4. **提示词注入使用 `before_agent` 钩子**：比扩展静态段落更灵活，支持动态模型名

### 前缀缓存稳定性评估

**Task 1（04_actions.md 追加）**：`04_actions.md` 在 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 之前（静态缓存段），追加内容会触发一次性的 cache miss（部署后首请求），之后新内容稳定恢复命中。属于任何静态 prompt 修改的预期行为。

**Task 4（before_agent 注入 System 消息）**：

- System 消息在 `DYNAMIC_BOUNDARY` 之后，天然不参与 prefix cache —— ✅ 安全
- 使用 `add_message`（尾部追加），非 `prepend_message`（头部插入），不会触发 prefix 偏移 —— ✅ 安全

**已知附带问题**：

| 问题 | 说明 | 对策 |
|------|------|------|
| 跨轮次消息累积 | `before_agent` 每次 `execute()` 调用一次，多轮对话中 "Git Attribution" System 消息会重复注入，浪费 token | `before_agent` 中检测历史上是否已有相同内容 System 消息，若已存在则跳过 |
| 注入位置偏后 | executor 中 `add_message(user_msg)` 在 `before_agent` 之前执行，导致 Co-Authored-By 指令在用户消息之后 | 可接受。Anthropic API 允许 System 块位于任意位置，缓存安全优先 |

### 测试点

- `model_email.rs`: 所有关键词→邮箱的单元测试
- `state.rs`: prefix/suffix 匹配的字符贡献计算测试（含 UTF-8/CJK）
- `GitAttributionMiddleware`: Write/Edit 工具的 before_tool/after_tool 集成测试
- 确保多次修改同一文件的累积贡献正确
- 确保非文件操作工具不触发追踪
