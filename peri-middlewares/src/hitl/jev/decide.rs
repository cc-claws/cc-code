//! 概率 → 决策。
//!
//! 每个条件两个对称阈值：
//!   `p >= t`      → satisfied
//!   `p <= 1 - t`  → rejected
//!   中间          → 中间带
//!
//! 中间带不是要挤掉的 bug。中间带**意味着什么**取决于 mode：
//! `Required` → 升级（询问用户）；`Hazard` → 忽略（"无明显危害" ≠ "存在危害"）。
//!
//! 合成在**代码里**做，模型从不权衡多个关注点。

use crate::hitl::jev::conditions::{self, Mode, Severity};

pub type Probabilities = std::collections::HashMap<String, f32>;

/// IEEE 754 下 `1 - 0.9 = 0.0999...98`，精确比较会把 `p=0.1, t=0.9` 放进中间带。
/// 加宽的偏向**拒绝侧**——这是门应该出错的方向。
const BOUNDARY_EPSILON: f32 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionVerdict {
    Satisfied,
    Rejected,
    Uncertain,
}

#[derive(Debug, Clone)]
pub struct Observation {
    pub rule_id: String,
    pub probability: f32,
    pub threshold: f32,
    pub verdict: ConditionVerdict,
    /// 该规则的 mode 对 band 的解释结果。`Hazard` 的中间带→视为 satisfied。
    pub effective: ConditionVerdict,
    /// **未作答**：分数缺失、越界或非有限。
    ///
    /// 必须与"明确否定"区分开——官方语义是"只有每一条都明确通过才放行，
    /// 其余一律回到人工确认"。把缺失当成 0（=明确违规）会把"网关响应不完整"
    /// 伪装成"用户违规"，既错误又无从排查。
    pub unknown: bool,
}

pub fn classify_condition(probability: f32, threshold: f32) -> ConditionVerdict {
    if probability >= threshold - BOUNDARY_EPSILON {
        ConditionVerdict::Satisfied
    } else if probability <= 1.0 - threshold + BOUNDARY_EPSILON {
        ConditionVerdict::Rejected
    } else {
        ConditionVerdict::Uncertain
    }
}

/// 根据答案生成观测。**缺失/越界一律标为 `unknown`，不再当作 0（拒绝）。**
pub fn observe(rule_ids: &[String], answers: &Probabilities) -> Vec<Observation> {
    rule_ids
        .iter()
        .filter_map(|id| {
            let cond = conditions::condition_by_id(id)?;
            // 非有限值或越界概率 = 未知，不是拒绝
            let raw = answers
                .get(id)
                .copied()
                .filter(|p| p.is_finite() && (0.0..=1.0).contains(p));
            let unknown = raw.is_none();
            let p = raw.unwrap_or(0.0);
            let verdict = if unknown {
                ConditionVerdict::Uncertain
            } else {
                classify_condition(p, cond.threshold)
            };
            let effective = if cond.mode == Mode::Hazard && verdict == ConditionVerdict::Uncertain {
                ConditionVerdict::Satisfied
            } else {
                verdict
            };
            Some(Observation {
                rule_id: id.clone(),
                probability: p,
                threshold: cond.threshold,
                verdict,
                effective,
                unknown,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow { rationale: String },
    Block {
        rule: String,
        rationale: String,
    },
    Uncertain {
        rule: String,
        rationale: String,
    },
}

fn severity_of(rule_id: &str) -> Severity {
    conditions::condition_by_id(rule_id)
        .map(|c| c.severity)
        .unwrap_or(Severity::Hazard)
}

fn describe(obs: &Observation) -> String {
    let label = conditions::condition_by_id(&obs.rule_id)
        .map(|c| c.id)
        .unwrap_or(&obs.rule_id);
    format!("{label} (p={:.2})", obs.probability)
}

/// 合成条件：
/// 1. `Hazard` 严重度的**明确拒绝** → block
/// 2. 任何**未作答**（缺失/越界）→ 未决（回到人工确认）
/// 3. `Soft` 严重度的拒绝 → 未决（可恢复风险，需一次显式确认）
/// 4. 未决的 `Required` → uncertain
/// 5. 否则 allow
///
/// 官方语义：**只有每一条都明确通过才放行**（"It answers yes only when every
/// asked score clears the threshold"），其余一律回到人工确认——所以顺序里
/// "未知"排在"放行"之前，且在"明确违规"之后。
///
/// **用户对话不参与合成。** 曾经有一条"用户要求了吗"的条件可以清除 soft 拒绝，
/// 但那段对话可能是误导、口误或注入的——用它清除安全条件，等于把授权建立在
/// 不可信输入上。现在 soft 拒绝改为"需要一次显式确认"，而不是"用户说了就算"。
pub fn combine(observations: &[Observation]) -> Decision {
    let rejected: Vec<&Observation> = observations
        .iter()
        .filter(|o| o.effective == ConditionVerdict::Rejected)
        .collect();

    // 1. hazard 明确拒绝
    if let Some(first) = rejected
        .iter()
        .find(|o| severity_of(&o.rule_id) == Severity::Hazard)
    {
        return Decision::Block {
            rule: first.rule_id.clone(),
            rationale: format!("A safety condition was clearly violated. {}", describe(first)),
        };
    }

    // 2. 有没答上来的 → 不可自动放行，回到人工确认
    if let Some(first) = observations.iter().find(|o| o.unknown) {
        return Decision::Uncertain {
            rule: first.rule_id.clone(),
            rationale: format!(
                "The judge returned no usable score for {}. {}",
                first.rule_id,
                describe(first)
            ),
        };
    }

    // 3. soft 拒绝：可恢复的风险 → 不硬拒，但也不再由用户对话自动放行
    if let Some(first) = rejected.first() {
        return Decision::Uncertain {
            rule: first.rule_id.clone(),
            rationale: format!(
                "A recoverable-risk condition was violated and needs explicit confirmation. {}",
                describe(first)
            ),
        };
    }

    // 4. 未决的 required
    if let Some(first) = observations
        .iter()
        .find(|o| o.effective == ConditionVerdict::Uncertain)
    {
        return Decision::Uncertain {
            rule: first.rule_id.clone(),
            rationale: format!("A safety condition could not be decided. {}", describe(first)),
        };
    }

    // 5. allow
    let lowest = observations
        .iter()
        .min_by(|a, b| a.probability.partial_cmp(&b.probability).unwrap_or(std::cmp::Ordering::Equal));
    Decision::Allow {
        rationale: match lowest {
            Some(l) => format!(
                "No hazard was evident across {} conditions (lowest p={:.2} on {}).",
                observations.len(),
                l.probability,
                l.rule_id
            ),
            None => "No safety conditions applied.".to_string(),
        },
    }
}

#[cfg(test)]
#[path = "decide_test.rs"]
mod tests;
