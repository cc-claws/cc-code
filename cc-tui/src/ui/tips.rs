//! Spinner tips 调度（对齐 Claude Code 2.1.300 的 tips 机制，见 issue #511）。
//!
//! 模型：
//! - 每条 tip 有稳定 id、文案、会话冷却（`cooldown_sessions`）与优先级；
//! - 每轮最多挑一条：轮开始时挑选并**扣留**，轮结束时才发布（下一轮 spinner 下方展示）；
//! - 挑选 = 在冷却已满足的候选中取「距上次展示间隔会话数最长」者（从未展示最优先），
//!   间隔相同按优先级、再按注册顺序；
//! - 展示历史按会话计数持久化到 `~/.cc-code/spinner-tips-history.json`；
//! - 渲染层另有时间覆盖 tip（本轮运行 >30min → /clear 提示），优先级高于调度 tip。

use std::collections::HashMap;
use std::path::PathBuf;

use cc_acp::provider::config::{SpinnerTipEntry, SpinnerTipsOverride};
use serde::{Deserialize, Serialize};

/// 内置 tip 数量（i18n key `tip-0`..`tip-17`，中英双语）。
pub const BUILTIN_TIP_COUNT: usize = 18;
/// 内置 tip 的会话冷却（取 CC 内置 tip 的众数 10）。
pub const BUILTIN_COOLDOWN_SESSIONS: u64 = 10;
/// 时间覆盖 tip 阈值：本轮运行超过 30 分钟提示 /clear（对齐 CC 的 1_800_000ms）。
pub const CLEAR_TIP_THRESHOLD_MS: u64 = 1_800_000;

/// 一条可被调度的 tip。
#[derive(Debug, Clone, PartialEq)]
pub struct TipCandidate {
    /// 稳定 id（冷却与历史的键）
    pub id: String,
    /// 展示文案
    pub text: String,
    /// 展示后至少间隔多少个会话才可再展示
    pub cooldown_sessions: u64,
    /// 优先级（间隔并列时高者先出）
    pub priority: i64,
    /// 渲染前缀覆盖（缺省 "Tip"）
    pub label: Option<String>,
}

/// 单条 tip 的展示记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TipShownRecord {
    /// 上次展示时的会话序号
    pub last_shown_session: u64,
    /// 终身展示次数
    pub shown_count: u64,
}

/// 展示历史（持久化 JSON）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TipHistory {
    /// 已启动的会话计数（每次 TUI 启动 +1，即当前会话序号）
    #[serde(default)]
    pub session_counter: u64,
    /// 各 tip 的展示记录
    #[serde(default)]
    pub shown: HashMap<String, TipShownRecord>,
}

impl TipHistory {
    /// 从文件加载；文件不存在或解析失败时返回空历史（不阻塞启动）。
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 写回文件（尽力而为，失败静默——历史丢失只影响 tip 轮换公平性）。
    pub fn save(&self, path: &std::path::Path) {
        if let Ok(json) = serde_json::to_string(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// 距上次展示的会话间隔；从未展示返回 `u64::MAX`（最优先）。
    pub fn sessions_since_shown(&self, id: &str) -> u64 {
        match self.shown.get(id) {
            None => u64::MAX,
            Some(rec) => self.session_counter.saturating_sub(rec.last_shown_session),
        }
    }
}

/// 历史文件路径：与 settings.json 同目录。
pub fn history_path() -> PathBuf {
    crate::config::config_path().with_file_name("spinner-tips.json")
}

/// 内置 tip 候选：文案走 i18n（`tip-N`），id 与 key 相同。
pub fn builtin_candidates(lc: &crate::i18n::LcRegistry) -> Vec<TipCandidate> {
    (0..BUILTIN_TIP_COUNT)
        .map(|i| {
            let key = format!("tip-{i}");
            TipCandidate {
                text: lc.tr(&key),
                id: key,
                cooldown_sessions: BUILTIN_COOLDOWN_SESSIONS,
                priority: 0,
                label: None,
            }
        })
        .collect()
}

/// 从自定义配置构造候选：内联条目 + `tips_file` 外部文件条目。
///
/// 外部文件格式与 CC 一致：条目数组，或 `{ "tips": [...] }` 对象。
/// 文件缺失/解析失败时静默跳过该来源。字符串条目以文案本身为 id（与 CC 的去重口径一致）。
pub fn override_candidates(cfg: &SpinnerTipsOverride) -> Vec<TipCandidate> {
    let mut entries: Vec<SpinnerTipEntry> = cfg.tips.clone();
    if let Some(file) = &cfg.tips_file {
        if let Ok(text) = std::fs::read_to_string(file) {
            if let Ok(mut from_file) = serde_json::from_str::<Vec<SpinnerTipEntry>>(&text) {
                entries.append(&mut from_file);
            } else if let Ok(obj) = serde_json::from_str::<SpinnerTipsOverride>(&text) {
                entries.extend(obj.tips);
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter_map(|e| {
            let cand = match e {
                SpinnerTipEntry::Text(text) => TipCandidate {
                    id: text.clone(),
                    text,
                    cooldown_sessions: 0,
                    priority: 0,
                    label: cfg.label.clone(),
                },
                SpinnerTipEntry::Detailed {
                    id,
                    text,
                    cooldown_sessions,
                    priority,
                } => TipCandidate {
                    id,
                    text,
                    cooldown_sessions,
                    priority: priority.unwrap_or(0),
                    label: cfg.label.clone(),
                },
            };
            // 同 id 只保留首条（对齐 CC 的 override 校验）
            if cand.text.is_empty() || !seen.insert(cand.id.clone()) {
                None
            } else {
                Some(cand)
            }
        })
        .collect()
}

/// 挑选下一条要展示的 tip（对齐 CC 的「最久未展示优先」）：
///
/// 冷却未满足（间隔 < cooldown）者剔除；其余按间隔降序、优先级降序、注册顺序取第一。
pub fn select_tip(candidates: &[TipCandidate], history: &TipHistory) -> Option<TipCandidate> {
    let mut best: Option<(&TipCandidate, u64)> = None;
    for cand in candidates {
        let since = history.sessions_since_shown(&cand.id);
        if since < cand.cooldown_sessions {
            continue;
        }
        let replace = match best {
            None => true,
            Some((b, b_since)) => {
                since > b_since || (since == b_since && cand.priority > b.priority)
            }
        };
        if replace {
            best = Some((cand, since));
        }
    }
    best.map(|(c, _)| c.clone())
}

/// tip 调度器：持有历史与文件路径，负责会话计数与展示记录。
#[derive(Debug)]
pub struct TipScheduler {
    history: TipHistory,
    history_path: PathBuf,
}

impl TipScheduler {
    /// 加载历史（不推进会话计数；由 [`Self::begin_session`] 在启动时调用一次）。
    pub fn new(history_path: PathBuf) -> Self {
        let history = TipHistory::load(&history_path);
        Self {
            history,
            history_path,
        }
    }

    /// 标记一次新会话启动：会话序号 +1 并落盘。
    pub fn begin_session(&mut self) {
        self.history.session_counter += 1;
        self.history.save(&self.history_path);
    }

    /// 当前会话序号。
    pub fn session_index(&self) -> u64 {
        self.history.session_counter
    }

    /// 暴露历史（供测试与诊断）。
    pub fn history(&self) -> &TipHistory {
        &self.history
    }

    /// 从候选集中挑选本轮 tip。
    pub fn pick(&self, candidates: &[TipCandidate]) -> Option<TipCandidate> {
        select_tip(candidates, &self.history)
    }

    /// 记录一次展示并落盘。
    pub fn record_shown(&mut self, id: &str) {
        let session = self.history.session_counter;
        let rec = self
            .history
            .shown
            .entry(id.to_string())
            .or_insert(TipShownRecord {
                last_shown_session: session,
                shown_count: 0,
            });
        rec.last_shown_session = session;
        rec.shown_count += 1;
        self.history.save(&self.history_path);
    }
}

impl Default for TipScheduler {
    fn default() -> Self {
        Self::new(history_path())
    }
}

/// 每轮 tip 状态机：挑选 → 扣留 → 轮次结束发布（对齐 CC 的 hold/publish 语义）。
#[derive(Debug, Default)]
pub struct SpinnerTipState {
    /// 已发布、当前展示的 tip
    current: Option<TipCandidate>,
    /// 本轮已挑好、待发布
    held: Option<TipCandidate>,
    /// 本轮是否已挑过（每轮最多一条）
    picked_this_turn: bool,
}

impl SpinnerTipState {
    /// 轮开始：挑选一条并扣留（本轮已挑过则忽略）。
    pub fn on_turn_start(&mut self, pick: impl FnOnce() -> Option<TipCandidate>) {
        if self.picked_this_turn {
            return;
        }
        self.picked_this_turn = true;
        self.held = pick();
    }

    /// 轮结束：发布扣留的 tip（无候选时清空当前展示，与 CC 一致）。
    /// 返回被发布的 tip id（供调度器记录展示历史）。
    pub fn on_turn_end(&mut self) -> Option<String> {
        self.picked_this_turn = false;
        let held = self.held.take();
        let id = held.as_ref().map(|t| t.id.clone());
        self.current = held;
        id
    }

    /// 当前展示的 tip。
    pub fn current(&self) -> Option<&TipCandidate> {
        self.current.as_ref()
    }
}

#[cfg(test)]
mod tests {
    include!("tips_test.rs");
}
