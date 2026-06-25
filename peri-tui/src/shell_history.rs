use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::thread::ThreadId;

const SHELL_HISTORY_FILE: &str = "shell-commands.jsonl";

/// A completed shell command shown in TUI chat history.
///
/// This is intentionally separate from `BaseMessage` so shell output is restored
/// in the UI without entering Agent history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShellCommandRecord {
    pub id: String,
    pub thread_id: ThreadId,
    pub command: String,
    pub cwd: String,
    #[serde(default)]
    pub stdin: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    #[serde(default)]
    pub anchor_message_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ShellCommandStore {
    path: PathBuf,
}

impl ShellCommandStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_path() -> Result<Self> {
        let path = dirs_next::home_dir()
            .context("无法获取 home 目录")?
            .join(".peri")
            .join("threads")
            .join(SHELL_HISTORY_FILE);
        Ok(Self::new(path))
    }

    pub async fn append(&self, record: &ShellCommandRecord) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await?;
        let line = serde_json::to_string(record)?;
        file.write_all(line.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;
        Ok(())
    }

    pub async fn load_for_thread(&self, thread_id: &ThreadId) -> Result<Vec<ShellCommandRecord>> {
        let content = match tokio::fs::read_to_string(&self.path).await {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut records = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: ShellCommandRecord = serde_json::from_str(line)
                .with_context(|| format!("解析 shell 历史第 {} 行失败", idx + 1))?;
            if &record.thread_id == thread_id {
                records.push(record);
            }
        }
        records.sort_by_key(|record| (record.started_at, record.completed_at));
        Ok(records)
    }
}

#[cfg(test)]
#[path = "shell_history_test.rs"]
mod tests;
