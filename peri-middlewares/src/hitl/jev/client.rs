//! Jev HTTP 客户端。所有失败 → `Err`（上层 fail-closed）。
//!
//! 端点是**网关**（代理到上游），实测会遇到瞬时 502/ECONNRESET，因此带有限重试；
//! 重试耗尽仍 → `Err`。

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::hitl::jev::config::JevConfig;

/// 已解析的 Jev 响应：条件 id → 概率。
#[derive(Debug, Clone)]
pub struct JevAnswers {
    pub probabilities: HashMap<String, f32>,
    pub model: String,
    pub cost: f64,
    /// 判定请求 id（审计用；网关未返回时为 None）。
    pub request_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum JevError {
    #[error("no api key configured")]
    NoKey,
    #[error("http error: {0}")]
    Http(String),
    #[error("timeout")]
    Timeout,
    #[error("malformed response: {0}")]
    Malformed(String),
}

#[derive(Deserialize)]
struct RawResponse {
    #[serde(default)]
    answers: HashMap<String, RawAnswer>,
    #[serde(default)]
    model: String,
    #[serde(default)]
    usage: Option<RawUsage>,
    /// 判定请求 id。官方建议连同 cost 一起记审计日志，便于回溯"这次是谁批的、花了多少"。
    #[serde(default)]
    id: Option<String>,
}

#[derive(Deserialize)]
struct RawAnswer {
    #[serde(default)]
    noul: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)]
    choice: Option<String>,
}

#[derive(Deserialize)]
struct RawUsage {
    #[serde(default)]
    cost: f64,
}

/// Jev 客户端。持有一个复用的 `reqwest::Client`。
pub struct JevClient {
    http: reqwest::Client,
    config: JevConfig,
}

impl JevClient {
    pub fn new(config: JevConfig) -> Result<Self, JevError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| JevError::Http(e.to_string()))?;
        Ok(Self { http, config })
    }

    pub fn config(&self) -> &JevConfig {
        &self.config
    }

    /// 发送一次判定请求。`state` 是结构化 {value, context}，`questions` 是 noul 问题表。
    pub async fn judge(
        &self,
        state: &Value,
        questions: &serde_json::Map<String, Value>,
    ) -> Result<JevAnswers, JevError> {
        let key = self.config.api_key().ok_or(JevError::NoKey)?;
        let body = json!({
            "model": self.config.model,
            "state": state,
            "questions": questions,
        });

        let mut last_err = JevError::Http("no attempt".into());
        // 2 次尝试：一次瞬时故障足够覆盖，同时把最坏耗时钉在
        // `2 × timeout_ms + 退避` 以内（超时值调大后，3 次会拖出 15 秒以上）。
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(200 * (1 << attempt))).await;
            }
            match self
                .http
                .post(self.config.systemone_url())
                .header("Authorization", format!("Bearer {key}"))
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.json::<RawResponse>().await {
                            Ok(raw) => return Ok(Self::parse(raw, questions)),
                            Err(e) => {
                                // 响应体畸形：不可重试（不是瞬时故障）
                                return Err(JevError::Malformed(e.to_string()));
                            }
                        }
                    }
                    // 4xx（除 429）不可通过重试修复 → 立即失败
                    if status.is_client_error() && status.as_u16() != 429 {
                        let text = resp.text().await.unwrap_or_default();
                        return Err(JevError::Http(format!(
                            "{status}: {}",
                            text.chars().take(200).collect::<String>()
                        )));
                    }
                    // 5xx / 429 → 可重试
                    last_err = JevError::Http(format!("{status}"));
                }
                Err(e) => {
                    last_err = if e.is_timeout() {
                        JevError::Timeout
                    } else {
                        JevError::Http(e.to_string())
                    };
                }
            }
        }
        Err(last_err)
    }

    fn parse(raw: RawResponse, questions: &serde_json::Map<String, Value>) -> JevAnswers {
        let mut probabilities = HashMap::new();
        // 只接受我们提问过的条件；缺失的**不插入**，由上层默认 0（=拒绝）
        for id in questions.keys() {
            if let Some(ans) = raw.answers.get(id) {
                if let Some(p) = ans.noul {
                    probabilities.insert(id.clone(), p);
                }
            }
        }

        // 缺失 ≠ 0。**0 是"模型确信违规"，缺失是"响应结构对不上"**。
        // 混为一谈的后果：网关响应结构一变（字段改名/换键），所有条件都按 0 处理 →
        // 拦下一切，而且 decide 会把它报告成"安全条件被明确违反"——错误且无从排查。
        let missing: Vec<&str> = questions
            .keys()
            .filter(|id| !probabilities.contains_key(*id))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            tracing::warn!(
                asked = questions.len(),
                missing = missing.len(),
                missing_ids = ?missing,
                "Jev 响应缺少部分条件答案，这些条件将按拒绝处理（可能是网关响应结构变化）"
            );
        }

        JevAnswers {
            probabilities,
            model: raw.model,
            cost: raw.usage.map(|u| u.cost).unwrap_or(0.0),
            request_id: raw.id,
        }
    }
}

#[cfg(test)]
#[path = "client_test.rs"]
mod tests;
