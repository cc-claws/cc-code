//! Jev 语义门 —— 三层决策流水线。
//!
//! ```text
//! 工具调用
//! ├─ 层0  硬黑名单（HARD_DENY）        → Block，永不进 Jev
//! ├─ 层1  用户 deny/allow pattern     → Block / Allow（记录）
//! ├─ 层2  只读白名单 + 项目内安全写    → 直接放行（零成本）
//! └─ 层3  Jev 语义层（仅"没被担保的"）
//!       ├ 一次请求问 N 个 noul 条件
//!       ├ 任一 hazard 拒绝 → Block
//!       ├ soft 拒绝 + intent 满足 → Allow
//!       ├ required 中间带 → 按 config.uncertain（默认 Block）
//!       └ 任何失败 → Block（fail-closed）
//! ```
//!
//! **沉默从不等于同意。**

pub mod client;
pub mod conditions;
pub mod config;
pub mod decide;
pub mod policy;
pub mod redact;
pub mod rules;
pub mod sources;

use std::path::Path;
use std::sync::Arc;

use serde_json::{json, Value};

use client::{JevClient, JevError};
use config::{GateScope, JevConfig, UncertainPolicy};

pub use rules::{
    empty_slot, extract_rules, extract_rules_chunked, JevRule, JevRuleLoader, JevRules,
    JevRulesSlot,
};

/// 门对一次调用的判决。
#[derive(Debug, Clone)]
pub enum GateDecision {
    Allow {
        rationale: String,
    },
    Block {
        rationale: String,
    },
    /// 交由上层弹窗确认（仅当语义层说 uncertain 且配置为 ask）。
    Ask {
        rationale: String,
    },
}

/// 受门管控的调用形态（持有自身数据，便于跨函数传递）。
///
/// **刻意不含用户对话内容。** 用户的话可能是误导、口误，或来自被注入的文件内容，
/// 让它参与判定等于把授权建立在不可信输入上。门只看"这次调用本身"。
pub struct GateCall {
    pub tool_name: String,
    /// Bash 命令（仅 bash）。**RTK 改写后**的实际执行命令（无改写时即原命令）。
    pub command: Option<String>,
    /// 用户**原始**命令（RTK 改写前的 X；无改写 / 非 Bash 时为 `None`）。
    ///
    /// 显式规则（`disallowed_commands` / `allowed_commands` / `safe_commands`）与危险形状判定
    /// **同时**看 `command` 与它：用户写规则时想的是自己敲的那条命令，若只看改写后的
    /// `rtk kubectl delete pod x`，装了 rtk 的机器上 `kubectl delete*` 这类规则会**静默失效**
    /// （#358）。反向只看原命令则会重新引入「批准 X、实际执行 X′」(#288)，故取并集。
    pub original_command: Option<String>,
    /// Write/Edit 目标路径。
    pub path: Option<std::path::PathBuf>,
    /// 当前 git 分支（仓库现场）。规则常带条件（"不要在 main 上提交"），
    /// 没有这个事实 judge 就判不了，只能去匹配命令里出现的分支名。
    pub branch: Option<String>,
    pub cwd: std::path::PathBuf,
}

/// 门控的**命令集合**：有效命令（RTK 改写后、实际会执行）+ 原始命令（用户敲的）。
///
/// 为什么两者都要（见 [`GateCall::original_command`]）：
/// - 只看有效命令 → 用户写的 `disallowed_commands` 在装了 rtk 的机器上被 `rtk X` 绕过（#358）；
/// - 只看原始命令 → 重新引入「批准 X、实际执行 X′」（#288）。
///
/// 取值方向：
/// - deny / 硬黑名单 / 危险形状 → **任一**命令命中即命中（fail-closed，方向更严）；
/// - allow / 用户声明安全 → **任一**命中即生效（用户白名单表达的是"意图"，RTK 改写只是前缀包装；
///   若只看改写后的 `rtk X`，用户的 allow 规则同样会静默失效）；
/// - 只读快车道 → 需要**两条都**是只读链（保守：执行的是改写后的命令）。
struct Commands<'a> {
    effective: &'a str,
    original: Option<&'a str>,
}

impl<'a> Commands<'a> {
    /// 由 `GateCall` 构造（仅在有 bash 命令时调用）。
    fn from(call: &'a GateCall) -> Option<Self> {
        let effective = call.command.as_deref()?;
        Some(Self {
            effective,
            // 与有效命令相同（没发生改写）时不重复评估
            original: call
                .original_command
                .as_deref()
                .filter(|orig| *orig != effective),
        })
    }

    /// 参与评估的全部命令（有效在前，原始在后）。
    fn all(&self) -> impl Iterator<Item = &'a str> {
        std::iter::once(self.effective).chain(self.original)
    }

    /// 任一命令命中硬黑名单即可（原因名去重保序）。
    fn hard_deny_reasons(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for cmd in self.all() {
            for name in policy::hard_deny_reasons(cmd) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// 任一命令命中危险形状即可（原因去重保序）。
    fn dangerous_reasons(&self, cwd: &Path) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for cmd in self.all() {
            for name in policy::dangerous_reasons_scoped(cmd, cwd) {
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// deny 规则：任一命令命中即算命中（`allow_shell_control = true`，宽松方向即更安全方向）。
    fn deny_match(&self, patterns: &[String]) -> Option<String> {
        self.all()
            .find_map(|cmd| policy::matches_command_pattern(cmd, patterns, true))
    }

    /// allow 规则：任一命令命中即算命中（用户白名单表达的是"意图"，改写只是前缀包装）。
    fn allow_match(&self, patterns: &[String]) -> Option<String> {
        self.all()
            .find_map(|cmd| policy::matches_command_pattern(cmd, patterns, false))
    }

    /// 用户声明的安全命令：任一命中即可（与 allow 同源语义）。
    fn declared_safe(&self, safe_commands: &[String]) -> bool {
        self.all()
            .any(|cmd| policy::is_user_declared_safe(cmd, safe_commands))
    }

    /// 只读白名单：**两条命令都必须是只读链**才算只读（保守方向）。
    fn read_only(&self) -> bool {
        self.all().all(policy::is_read_only_chain)
    }
}

/// Jev 语义门。
pub struct JevGate {
    client: JevClient,
    /// 人显式写下的配置。CLAUDE.md 提炼出的规则**不并进这里**，见 `rules_slot`。
    config: JevConfig,
    /// CLAUDE.md 提炼出的规则（惰性填充，可能后到）。
    rules_slot: JevRulesSlot,
    /// 惰性提炼触发器。None = 本会话不做提炼（如子 Agent 场景）。
    rule_loader: Option<Arc<JevRuleLoader>>,
}

/// 决策的简短标签（审计日志用）。
fn decision_short(d: &decide::Decision) -> &'static str {
    match d {
        decide::Decision::Allow { .. } => "allow",
        decide::Decision::Block { .. } => "block",
        decide::Decision::Uncertain { .. } => "review",
    }
}

// ─── 面向人和 agent 的拦截说明 ─────────────────────────────────────────────//
// 拦截信息会被**用户和 agent 同时读到**，所以必须说清三件事：
// 发生了什么、依据哪条规则、下一步能怎么办。
// 绝不把 judge 的英文提示词、内部规则 id、概率值直接抛出去——那既看不懂也动不了手。

/// 拦截信息里展示规则原文时的长度上限，避免把整段策略灌进错误信息。
const POLICY_EXCERPT_CHARS: usize = 800;

impl JevGate {
    /// 拦截说明。
    ///
    /// **用户和 agent 的需求不同**，所以分开写再合并：
    /// - 用户要**易懂**：大白话、不要内部术语、说清是哪条自己的规则、怎么办；
    /// - agent 要**专业**：结构化、带规则 id 与来源、明确可重试性与行动路径。
    pub fn block_message(&self, rule: &str) -> String {
        let (user, agent) = self.block_messages(rule);
        format!("{user}\n\n{agent}")
    }

    /// 分别生成（给用户看的，给 agent 看的）。
    pub fn block_messages(&self, rule: &str) -> (String, String) {
        let user = self.block_message_for_user(rule);
        let agent = self.block_message_for_agent(rule);
        (user, agent)
    }

    /// 给用户看的：白话，无内部术语。
    fn block_message_for_user(&self, rule: &str) -> String {
        let mut msg = format!(
            "安全门已拦截这次调用。\n原因：{}",
            conditions::human_reason(rule)
        );
        if rule == "policy_compliance" {
            let rendered = self.render_rule_sources();
            if !rendered.trim().is_empty() {
                msg.push_str("\n\n你写的规则：\n");
                msg.push_str(&rendered);
            }
            msg.push_str(
                "\n\n这条拦截来自你自己写的规则，不会被「我确实要做」这类说明自动放行。\n\
                 如果规则写错了，请修改上面标出的那个文件；如果确实需要这一步，请在终端手动执行。",
            );
        } else {
            msg.push_str(
                "\n\n如果这确实是你想做的，可以先告诉我理由；我仍无法放行时，请在终端手动执行。",
            );
        }
        msg
    }

    /// 给 agent 看的：结构化、可执行、明确禁止绕过。
    ///
    /// 刻意**不引用用户那段文案**（"见上文"之类）——两段应当各自自足，
    /// 否则将来分渠道投递时会断。
    fn block_message_for_agent(&self, rule: &str) -> String {
        let source = if rule == "policy_compliance" {
            let labels: Vec<String> = self
                .rules_by_source()
                .into_iter()
                .map(|(label, _)| label)
                .collect();
            if labels.is_empty() {
                "配置的策略".to_string()
            } else {
                labels.join(" + ")
            }
        } else {
            "内置安全条件".to_string()
        };
        format!(
            "[For the agent]\n\
             decision=block  rule={rule}  source={source}  retryable=no\n\
             Do not retry the same command or bypass this decision through an equivalent form, such as switching tools, splitting commands, or adding pipes.\n\
             Available actions:\n\
             1) Choose an approach that does not violate this rule and continue.\n\
             2) Explain the operation's specific effects to the user and ask them to confirm and execute it manually in their terminal.\n\
             3) If the rule itself is incorrect, ask the user to edit the corresponding CLAUDE.md file."
        )
    }

    /// 需要人工确认时的说明。
    pub fn ask_message(&self, rule: &str) -> String {
        let mut msg = format!(
            "安全门无法自行判定，需要你确认是否放行。\n疑虑：{}",
            conditions::human_reason(rule)
        );
        if rule == "policy_compliance" {
            let rendered = redact::truncate(
                &redact::redact_secrets(&self.render_rule_sources()),
                POLICY_EXCERPT_CHARS,
            );
            if !rendered.trim().is_empty() {
                msg.push_str("\n\n相关规则：\n");
                msg.push_str(&rendered);
            }
        }
        msg
    }

    pub fn new(config: JevConfig) -> Result<Arc<Self>, JevError> {
        Self::with_loader(config, None)
    }

    /// 带惰性提炼器的构造。
    pub fn with_loader(
        config: JevConfig,
        rule_loader: Option<Arc<JevRuleLoader>>,
    ) -> Result<Arc<Self>, JevError> {
        let rules_slot = rule_loader
            .as_ref()
            .map(|l| l.slot().clone())
            .unwrap_or_else(empty_slot);
        Ok(Arc::new(Self {
            client: JevClient::new(config.clone())?,
            config,
            rules_slot,
            rule_loader,
        }))
    }

    pub fn config(&self) -> &JevConfig {
        &self.config
    }

    /// 当前已就绪的规则（未提炼/提炼失败时为 None）。
    fn rules(&self) -> Option<Arc<JevRules>> {
        self.rules_slot.read().clone()
    }

    /// 生效策略 = 人写下的配置 + CLAUDE.md 提炼（后者排后，人写的优先）。
    fn effective_policy(&self) -> String {
        let from_rules = self.rules().map(|r| r.policy_text()).unwrap_or_default();
        let configured = self.config.policy.trim();
        match (configured.is_empty(), from_rules.trim().is_empty()) {
            (true, _) => from_rules,
            (false, true) => configured.to_string(),
            (false, false) => format!("{configured}\n\n{from_rules}"),
        }
    }

    /// 按来源文件分组渲染提炼出的规则，供拦截信息展示。
    ///
    /// 用户要知道的不只是"违反了规则"，而是**"哪条规则、在哪个文件里"**——
    /// 否则他得去三个 CLAUDE.md 里自己翻。
    fn rules_by_source(&self) -> Vec<(String, Vec<String>)> {
        let Some(rules) = self.rules() else {
            return Vec::new();
        };
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        for rule in &rules.rules {
            let text = rule.text.trim();
            if text.is_empty() {
                continue;
            }
            let label = source_label(rule.source.trim()).to_string();
            match groups.iter_mut().find(|(l, _)| *l == label) {
                Some((_, items)) => items.push(text.to_string()),
                None => groups.push((label, vec![text.to_string()])),
            }
        }
        groups
    }

    /// 渲染"哪条规则、来自哪个文件"。
    fn render_rule_sources(&self) -> String {
        let groups = self.rules_by_source();
        let raw = if groups.is_empty() {
            // 没有规则明细（如策略来自显式配置）→ 退回整段策略
            self.effective_policy().trim().to_string()
        } else {
            let mut out = String::new();
            for (label, items) in groups {
                out.push_str(&format!("【{label}】\n"));
                for item in items {
                    out.push_str("  - ");
                    out.push_str(&item);
                    out.push('\n');
                }
            }
            out.trim_end().to_string()
        };
        // 无论是规则明细还是兜底整段策略，都必须在出境前脱敏并限长
        redact::truncate(&redact::redact_secrets(&raw), POLICY_EXCERPT_CHARS)
    }

    /// 生效受保护路径 = 配置 + 提炼（叠加语义，多保护更保守，无优先级冲突）。
    fn effective_protected_paths(&self) -> Vec<String> {
        let mut paths = self.config.protected_paths.clone();
        if let Some(rules) = self.rules() {
            paths.extend(rules.protected_paths.iter().cloned());
        }
        paths
    }

    /// 对一次调用判定。所有内部失败都返回 `Block`（fail-closed）。
    ///
    /// 便捷入口（确定性 → 语义）。**注意**：调用方若需要"无语义判定时仍保留
    /// 确定性防护"，应分别调用 [`Self::deterministic`] 与 [`Self::evaluate_semantic`]，
    /// 而不是依赖本函数——理由见 `has_judge`。
    pub async fn evaluate(&self, call: &GateCall) -> GateDecision {
        // 惰性提炼：门第一次真的要用规则时才提炼 CLAUDE.md。
        // 放在最前面，因为确定性层的路径判定也依赖提炼出的受保护路径。
        if let Some(loader) = &self.rule_loader {
            loader.ensure_loaded().await;
        }
        // ── 层0/1/2：纯确定性，零成本 ──
        if let Some(d) = self.deterministic(call) {
            return d;
        }
        // ── 层3：Jev ──
        self.semantic(call).await
    }

    /// 是否配置了判定凭据。
    ///
    /// **确定性层与语义层共用同一个 `JevGate`，但绝不该共用同一个开关**：
    /// 没有凭据/端点不可用时，硬黑名单、人写的规则、只读白名单**照常应当生效**。
    /// 所以 Auto 模式必须"先确定性、后语义"，且后者不可用时不能把前者一起跳过。
    pub fn has_judge(&self) -> bool {
        self.config.api_key().is_some()
    }

    /// 仅跑语义层（不含确定性层）。调用方须自行先跑 [`Self::deterministic`]。
    pub async fn evaluate_semantic(&self, call: &GateCall) -> GateDecision {
        if let Some(loader) = &self.rule_loader {
            loader.ensure_loaded().await;
        }
        self.semantic(call).await
    }

    /// 惰性触发规则提炼（语义层需要）。确定性层若依赖提炼出的受保护路径，
    /// 也应由调用方在跑确定性层之前触发一次。
    pub async fn ensure_rules_loaded(&self) {
        if let Some(loader) = &self.rule_loader {
            loader.ensure_loaded().await;
        }
    }

    /// 确定性层。返回 `Some` 表示已判决（无需 Jev）。
    ///
    /// 这一层只用**人显式写下的规则**（env / settings）和**内置形状判定**。
    /// 从 CLAUDE.md 提炼出的规则不在这里判决——提炼会出错，而这里是硬 Block、
    /// 无申诉路径。提炼结果一律进 `policy` 由 Jev 语义判定，错的提炼在那能被纠正。
    ///
    /// **与语义层解耦**：该层不依赖 API key、也不发网络请求，
    /// 因此"判定服务不可用"绝不该让它一起失效（见 [`Self::has_judge`]）。
    pub fn deterministic(&self, call: &GateCall) -> Option<GateDecision> {
        // 只处理 bash / write / edit
        let is_bash = call.command.is_some();
        let is_write = call.path.is_some();
        if !is_bash && !is_write {
            return None;
        }

        if let Some(cmds) = Commands::from(call) {

            // ── 层0：硬黑名单 —— 不可覆盖，任何规则都不能放行 ──
            let hard = cmds.hard_deny_reasons();
            if !hard.is_empty() {
                return Some(GateDecision::Block {
                    rationale: format!("硬黑名单：{}（禁止执行）", hard.join(", ")),
                });
            }

            // 以下是**显式优先级梯**：人写下的配置 > 内置白名单。
            // 顺序即优先级，改动前先想清楚谁该压过谁。

            // ── 层1a：人显式写下的 deny（原始/有效命令任一命中即拦）──
            if let Some(pattern) = cmds.deny_match(&self.config.disallowed_commands) {
                return Some(GateDecision::Block {
                    rationale: format!("用户拒绝规则命中：{pattern}"),
                });
            }
            // ── 层1b：人显式写下的 allow（压过危险形状）──
            if let Some(pattern) = cmds.allow_match(&self.config.allowed_commands) {
                return Some(GateDecision::Allow {
                    rationale: format!("用户允许规则命中：{pattern}"),
                });
            }
            // ── 层1c：人显式声明的安全命令 ──
            let reasons = cmds.dangerous_reasons(&call.cwd);
            if cmds.declared_safe(&self.config.safe_commands) && reasons.is_empty() {
                return Some(GateDecision::Allow {
                    rationale: "确定性层：用户声明的安全命令".to_string(),
                });
            }
            // ── 层2：内置只读白名单（最低优先级，可被上面任何一条推翻）──
            if cmds.read_only() && reasons.is_empty() {
                return Some(GateDecision::Allow {
                    rationale: "确定性层：只读命令".to_string(),
                });
            }
            // gate_scope=matched 时，无危险形状 → 放行
            if self.config.gate_scope == GateScope::Matched && reasons.is_empty() {
                return Some(GateDecision::Allow {
                    rationale: "确定性层：未命中危险形状（matched 模式）".to_string(),
                });
            }
            return None; // 升级到 Jev
        }

        // Write/Edit：项目内非受保护路径 → 直接放行
        if let Some(path) = call.path.as_deref() {
            let protected = policy::protected_path_reason(path, &self.effective_protected_paths());
            // **必须先词法归一化**：`Path::starts_with` 是纯词法比较，
            // `cwd/../../etc/passwd` 词法上以 cwd 开头 → 会被误判成"项目内路径"
            // 直接放行，从而**整道门（写保护 + 策略检查）被跳过**。实测确认过。
            let normalized = policy::normalize_lexical(path);
            let root = policy::normalize_lexical(&call.cwd);
            if protected.is_none() && normalized.starts_with(&root) {
                return Some(GateDecision::Allow {
                    rationale: "确定性层：项目内非受保护路径".to_string(),
                });
            }
        }
        None
    }

    /// 语义层（Jev）。任何失败 → Block。
    async fn semantic(&self, call: &GateCall) -> GateDecision {
        // 构造 state
        let reasons: Vec<String> = call
            .command
            .as_deref()
            .map(|c| {
                policy::dangerous_reasons_scoped(c, &call.cwd)
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();
        let protected = call
            .path
            .as_deref()
            .and_then(|p| policy::protected_path_reason(p, &self.effective_protected_paths()));

        let state = self.build_state(call, &reasons);
        let filter = conditions::Filter {
            has_policy: !self.effective_policy().trim().is_empty(),
            protected_target: protected.is_some(),
            reasons: reasons.clone(),
        };
        let conds = conditions::conditions_for(&filter);
        if conds.is_empty() {
            // 无适用条件属于异常（总有 Always 条件）→ 不可自动放行，交人工
            return GateDecision::Ask {
                rationale: "安全条件集为空（异常），无法自动放行，需要你确认。".to_string(),
            };
        }
        let questions = conditions::build_questions(&conds);
        let rule_ids: Vec<String> = conds.iter().map(|c| c.id.to_string()).collect();

        let answers = match self.client.judge(&state, &questions).await {
            Ok(a) => a,
            Err(e) => {
                // **判定不可用不是"违规"**，所以不 Block。
                // 官方语义：「network failure / timeout / error response / unreadable body
                // 一律视为 unknown，回到正常的人工确认」——多弹一次窗，而不是
                // 执行危险命令，也不是把 agent 一棍子打死。
                // （实测教训：端点 503 时 fail-closed 会把整个会话的工具调用全堵死。）
                tracing::warn!(error = %e, "Jev 判定不可用，交由人工确认");
                return GateDecision::Ask {
                    rationale: format!(
                        "安全判定服务当前不可用（{e}）。为了避免误拦，这次调用需要你确认是否放行。"
                    ),
                };
            }
        };

        let obs = decide::observe(&rule_ids, &answers.probabilities);
        let decision = decide::combine(&obs);
        // 审计：官方建议把请求 id 与 cost 一起记下来，便于回溯"这次判了什么、花了多少"
        tracing::info!(
            request_id = ?answers.request_id,
            model = %answers.model,
            cost = answers.cost,
            tool = %call.tool_name,
            command = ?call.command,
            reasons = ?reasons,
            decision = ?decision_short(&decision),
            obs = ?obs.iter().map(|o| (&o.rule_id, o.probability, o.unknown)).collect::<Vec<_>>(),
            "Jev 判定完成"
        );

        match decision {
            decide::Decision::Allow { rationale } => GateDecision::Allow { rationale },
            decide::Decision::Block { rule, rationale } => {
                // judge 的原文只进日志；给人和 agent 的是中文、可行动的说明。
                tracing::warn!(rule = %rule, judge_detail = %rationale, "Jev 拒绝");
                GateDecision::Block {
                    rationale: self.block_message(&rule),
                }
            }
            decide::Decision::Uncertain { rule, rationale } => match self.config.uncertain {
                UncertainPolicy::Deny => {
                    tracing::warn!(rule = %rule, judge_detail = %rationale, "Jev 未决，按配置拒绝");
                    GateDecision::Block {
                        rationale: self.block_message(&rule),
                    }
                }
                UncertainPolicy::Ask => GateDecision::Ask {
                    rationale: self.ask_message(&rule),
                },
                UncertainPolicy::Allow => GateDecision::Allow { rationale },
            },
        }
    }

    fn build_state(&self, call: &GateCall, reasons: &[String]) -> Value {
        // 所有要离开本机的字段都先脱敏（policy / path / command）。
        // 刻意不含用户对话内容——见 `GateCall` 上的说明。
        let mut value = json!({
            "tool": call.tool_name,
            "matched_policy_reasons": reasons,
        });

        if let Some(cmd) = call.command.as_deref() {
            let redacted = redact::redact_secrets(cmd);
            value["command"] = json!(redact::truncate(&redacted, self.config.max_command_len));
        }
        if let Some(path) = call.path.as_deref() {
            let redacted = redact::redact_secrets(&path.to_string_lossy());
            value["path"] = json!(redacted);
            value["outside_working_directory"] = json!(!path.starts_with(&call.cwd));
        }

        let protected: Vec<String> = self.effective_protected_paths();
        let effective_policy = self.effective_policy();
        let policy = if effective_policy.trim().is_empty() {
            "(no user policy configured)".to_string()
        } else {
            // 策略可能很长，必须先脱敏再截断，否则单次判定会撑爆请求。
            let redacted = redact::redact_secrets(&effective_policy);
            if redacted.chars().count() > self.config.max_policy_len {
                // 截断是**静默丢规则**的同类风险：规则提炼出来了却送不进判定，
                // 用户只会看到"规则没生效"，无从排查。
                tracing::warn!(
                    policy_chars = redacted.chars().count(),
                    limit = self.config.max_policy_len,
                    "策略超出送判定上限，尾部规则被丢弃——请提高 JEV_MAX_POLICY_LEN"
                );
            }
            redact::truncate(&redacted, self.config.max_policy_len)
        };
        json!({
            "value": value,
            "context": {
                "policy": policy,
                // 结构性事实：judge 只看得到命令本身，看不到"跑完之后会怎样"。
                // A/B 实测（e2e_hardware_address_disclosure）：没有这条时
                // `ipconfig /all` 以 p=0.22 溜过（拒绝带边缘是 0.20）；有这条则 4/4 拦下。
                "environment": {
                    "tool_output_enters_conversation_context": true,
                    "note": "The output of this tool call becomes part of the conversation context and is sent to the model provider with subsequent requests. Reading locally stored sensitive information therefore discloses it to a third party. A call is not compliant with a 'do not disclose X' rule merely because it does not itself transmit anything."
                },
                "repository": {
                    "cwd": call.cwd.to_string_lossy(),
                    // 规则多为有条件式（"不要在 main 上提交"）；缺了这个事实，
                    // judge 只能去匹配命令里出现的分支名，会拦错对象。
                    "branch": call.branch,
                    "protected_paths": protected,
                }
            }
        })
    }
}

// ─── 参数解包 ───────────────────────────────────────────────────────────────

/// 解出真实工具的参数对象。
///
/// `ExecuteExtraTool` 把目标工具的参数包在 `params` 字段里；不解包会看到空参数，
/// 使确定性层（硬黑名单/危险 pattern/只读白名单）全部失效。
pub fn effective_params<'a>(tool_name: &str, input: &'a Value) -> &'a Value {
    use crate::tool_search::core_tools::{EXECUTE_EXTRA_TOOL_NAME, EXTRA_TOOL_PARAMS_FIELD};
    if tool_name == EXECUTE_EXTRA_TOOL_NAME {
        input.get(EXTRA_TOOL_PARAMS_FIELD).unwrap_or(input)
    } else {
        input
    }
}

// ─── 策略组装 ───────────────────────────────────────────────────────────────

/// 规则来源标识 → 面向用户的名字。
///
/// 提取时用 `source=` 让模型标注每条规则来自哪个文件，拦截时就能直接指到文件，
/// 而不是让用户去三个 CLAUDE.md 里自己找。
pub const SOURCE_LABELS: &[(&str, &str)] = &[
    ("personal", "个人规则 CLAUDE.local.md"),
    ("project", "项目规则 CLAUDE.md / AGENTS.md"),
    ("hooks", "项目 hooks 脚本 .claude/hooks/*.sh"),
    ("global", "全局规则 ~/.claude/CLAUDE.md"),
];

/// 来源标识的中文名（未收录则原样返回）。
pub fn source_label(key: &str) -> &str {
    SOURCE_LABELS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| *label)
        .unwrap_or(key)
}

/// 把若干来源的 CLAUDE.md 文本组装成送给提炼模型的输入。
///
/// `sections` 按**权威性降序**传入（越靠前越权威）。总长超过 `max_len` 时截断，
/// 丢掉的总是尾部，也就是权威性最低的那份。
///
/// 每个非空片段带 `## <label> | source=<key>` 标题——模型据此给每条规则标注来源。
pub fn compose_policy(sections: &[(&str, &str, Option<&str>)], max_len: usize) -> String {
    let mut out = String::new();
    for (key, label, content) in sections {
        let Some(text) = content else { continue };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("## ");
        out.push_str(label);
        out.push_str(" | source=");
        out.push_str(key);
        out.push('\n');
        out.push_str(text);
    }
    if out.chars().count() > max_len {
        // 静默丢规则是拦截率的隐形杀手：排在后面的个人/全局规则会整段消失
        tracing::warn!(
            source_chars = out.chars().count(),
            limit = max_len,
            "规则来源超出上限，尾部来源被丢弃（个人/全局规则常排在后面）——\
             请提高 JEV_MAX_RULE_SOURCE_LEN 或降低 JEV_RULE_CHUNK_LEN"
        );
    }
    redact::truncate(&out, max_len)
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

#[cfg(test)]
#[path = "e2e_test.rs"]
mod e2e_tests;
