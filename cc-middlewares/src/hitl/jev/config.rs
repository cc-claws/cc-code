//! Jev 网关配置。

use serde::{Deserialize, Serialize};

/// 未决（中间带）判决的默认处置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UncertainPolicy {
    /// 弹窗确认（默认）。
    ///
    /// **拿不准的事应该问人，而不是替人拒绝。** 走到这一档的是
    /// `local_scope` / `no_outward_effect` / `no_irreversible_damage` 这类
    /// "可恢复但可能有风险"的条件——恰恰是用户完全可能**确实想干**的事
    /// （例：用户明确要求删除仓库外的 `C:\tmp`）。
    ///
    /// 默认 `Deny` 会把这类事变成无法申诉的硬拦，把门变成"什么都不让干"。
    /// 没有可用确认通道时仍会退回拒绝（fail-closed）。
    #[default]
    Ask,
    /// 静默拒绝（不打断用户）。
    Deny,
    /// 信任中间带。
    Allow,
}

/// 语义层覆盖范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GateScope {
    /// 默认：确定性层能担保的走快车道，其余全部送 Jev
    #[default]
    All,
    /// 仅判定命中危险 pattern 的调用
    Matched,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JevConfig {
    /// 网关基址（如 `http://localhost:20130`）。
    pub endpoint: String,
    /// 模型名。
    pub model: String,
    /// API key 来源：环境变量名（**不硬编码密钥**）。
    pub api_key_env: String,
    /// 单次请求超时（毫秒）。超时 → fail-closed。
    ///
    /// 注意实际最坏耗时 = `timeout_ms` × 重试次数 + 退避（见 `client.rs`）。
    ///
    /// 实测依据（`e2e_hardware_address_disclosure`）：本地端点**冷启动**时单次判定
    /// 可超过 2 秒，原默认 2000ms 会直接 timeout → 无理由的 fail-closed 误拦；
    /// 热态只需 0.7–1.5 秒。故取 5000ms 覆盖冷启动。
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub uncertain: UncertainPolicy,
    #[serde(default)]
    pub gate_scope: GateScope,
    /// 用户声明的快车道（优先级高于危险 pattern）。
    #[serde(default)]
    pub safe_commands: Vec<String>,
    /// 覆盖危险 pattern 的允许列表（记录下来）。
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// 立即拒绝的命令 pattern。
    ///
    /// **只由人显式配置**。从 CLAUDE.md 提炼出的规则不写这里——提炼会出错，
    /// 而这一层是硬 Block、无申诉路径。提炼结果一律走 `policy`，交给 Jev 语义判定，
    /// 错的提炼在那里可以被上下文纠正，而不是变成无法申辩的一刀切。
    #[serde(default)]
    pub disallowed_commands: Vec<String>,
    /// 追加的受保护路径。
    #[serde(default)]
    pub protected_paths: Vec<String>,
    /// 用户策略说明（`policy_compliance` 条件用）。
    #[serde(default)]
    pub policy: String,
    /// 策略文本最大长度（脱敏后截断）。
    ///
    /// 这是**送给判定模型**的策略上限（规则由 LLM 提炼，本就很短，此处是兜底）。
    #[serde(default = "default_max_policy_len")]
    pub max_policy_len: usize,
    /// 规则来源**总体**字符上限（分块数 × 每块上限）。
    ///
    /// 刻意给得宽松：项目 `CLAUDE.md` + 个人 + 全局 + hooks 很容易超过 4 万字符，
    /// 卡太紧会静默丢掉排在后面的个人/全局规则。
    #[serde(default = "default_max_rule_source_len")]
    pub max_rule_source_len: usize,
    /// 单次提炼调用送进去的字符上限（超过则分块）。
    #[serde(default = "default_rule_chunk_len")]
    pub rule_chunk_len: usize,
    /// 最多提炼多少块。超出部分会被丢弃并记 warning。
    #[serde(default = "default_max_rule_chunks")]
    pub max_rule_chunks: usize,
    /// 规则提炼的超时（毫秒）。超时 → 本会话不携带用户策略。
    #[serde(default = "default_rule_timeout_ms")]
    pub rule_timeout_ms: u64,
    /// 命令文本最大长度（脱敏后截断）。
    #[serde(default = "default_max_command_len")]
    pub max_command_len: usize,
}

fn default_timeout_ms() -> u64 {
    5000
}
fn default_max_policy_len() -> usize {
    12000
}
fn default_max_rule_source_len() -> usize {
    120_000
}
fn default_rule_chunk_len() -> usize {
    // 4 万：覆盖绝大多数项目的全部规则来源（实测 cc-code 合计约 3 万字符、
    // acme_order 约 1.5 万），典型情况仍只**一次**提炼调用；超出才分块。
    40_000
}
fn default_max_rule_chunks() -> usize {
    4
}
fn default_rule_timeout_ms() -> u64 {
    // 3 万：单块（默认 4 万字符上限）在实测网关上耗时 18–73 秒，
    // 原 15 秒会把**每一次**提炼都变成超时 → 规则静默失效、缓存永不落盘。
    // 这里给的是首试时限，超时后代码会再放宽 2 倍重试一次。
    30_000
}
fn default_max_command_len() -> usize {
    4000
}

impl Default for JevConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:20130".to_string(),
            model: "openrouter/typesafe/jev-1.13".to_string(),
            api_key_env: "JEV_API_KEY".to_string(),
            timeout_ms: default_timeout_ms(),
            uncertain: UncertainPolicy::default(),
            gate_scope: GateScope::default(),
            safe_commands: Vec::new(),
            allowed_commands: Vec::new(),
            disallowed_commands: Vec::new(),
            protected_paths: Vec::new(),
            policy: String::new(),
            max_policy_len: default_max_policy_len(),
            max_rule_source_len: default_max_rule_source_len(),
            rule_chunk_len: default_rule_chunk_len(),
            max_rule_chunks: default_max_rule_chunks(),
            rule_timeout_ms: default_rule_timeout_ms(),
            max_command_len: default_max_command_len(),
        }
    }
}

impl JevConfig {
    /// 从环境变量读取 API key。
    pub fn api_key(&self) -> Option<String> {
        std::env::var(&self.api_key_env)
            .ok()
            .filter(|k| !k.is_empty())
    }

    /// 从环境变量覆盖默认配置。
    ///
    /// - `JEV_ENDPOINT` — 网关基址
    /// - `JEV_MODEL` — 模型名
    /// - `JEV_API_KEY` — API key（默认 `api_key_env`）
    /// - `JEV_TIMEOUT_MS` — 超时
    /// - `JEV_UNCERTAIN` — `deny` / `ask` / `allow`
    /// - `JEV_SCOPE` — `all` / `matched`
    /// - `JEV_MAX_POLICY_LEN` — 策略文本最大长度
    /// - `JEV_RULE_TIMEOUT_MS` — CLAUDE.md 规则提炼超时
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(v) = std::env::var("JEV_ENDPOINT") {
            if !v.trim().is_empty() {
                cfg.endpoint = v;
            }
        }
        if let Ok(v) = std::env::var("JEV_MODEL") {
            if !v.trim().is_empty() {
                cfg.model = v;
            }
        }
        if let Ok(v) = std::env::var("JEV_TIMEOUT_MS") {
            if let Ok(ms) = v.parse::<u64>() {
                cfg.timeout_ms = ms;
            }
        }
        if let Ok(v) = std::env::var("JEV_UNCERTAIN") {
            cfg.uncertain = match v.trim().to_lowercase().as_str() {
                "ask" => UncertainPolicy::Ask,
                "allow" => UncertainPolicy::Allow,
                _ => UncertainPolicy::Deny,
            };
        }
        if let Ok(v) = std::env::var("JEV_SCOPE") {
            cfg.gate_scope = match v.trim().to_lowercase().as_str() {
                "matched" => GateScope::Matched,
                _ => GateScope::All,
            };
        }
        if let Ok(v) = std::env::var("JEV_MAX_POLICY_LEN") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.max_policy_len = n;
            }
        }
        if let Ok(v) = std::env::var("JEV_MAX_RULE_SOURCE_LEN") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.max_rule_source_len = n;
            }
        }
        if let Ok(v) = std::env::var("JEV_RULE_CHUNK_LEN") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.rule_chunk_len = n;
            }
        }
        if let Ok(v) = std::env::var("JEV_MAX_RULE_CHUNKS") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.max_rule_chunks = n;
            }
        }
        if let Ok(v) = std::env::var("JEV_RULE_TIMEOUT_MS") {
            if let Ok(ms) = v.parse::<u64>() {
                cfg.rule_timeout_ms = ms;
            }
        }
        cfg
    }

    /// `{endpoint}/v1/systemone`
    pub fn systemone_url(&self) -> String {
        format!("{}/v1/systemone", self.endpoint.trim_end_matches('/'))
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
