use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Thread 唯一标识符（UUID v7，按时间排序）
pub type ThreadId = String;

/// 上一轮任务完成态总结（对应 UI 的 `✻ Cooked for 25s · done 14:26` 行）。
///
/// 属于纯展示态、不进 message history，但也需要随 thread 持久化，
/// 否则 `-c`/`-r` 恢复会话时该行会丢失。命名对齐 [`AgentComm::last_task_duration`]
/// 所在的任务完成信息域（`last_task_*`），避免与 compact 域的 summary 混淆。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSummary {
    /// 完成态动词（随机过去式英文，如 "Cooked"）
    pub verb: String,
    /// 本轮耗时（毫秒）
    pub elapsed_ms: u64,
    /// 完成时刻（wall-clock）
    pub done_at: DateTime<Utc>,
}

/// Thread 元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMeta {
    pub id: ThreadId,
    /// 对话标题，可由第一条用户消息自动截取
    pub title: Option<String>,
    /// 创建时的工作目录
    pub cwd: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    /// 消息内容总字节数（由 list_threads 查询时计算）
    #[serde(default)]
    pub content_size: u64,
    /// 父 agent thread ID，None = 根 agent
    #[serde(default)]
    pub parent_thread_id: Option<String>,
    /// 快照截止消息 ID
    #[serde(default)]
    pub snapshot_at_message_id: Option<String>,
    /// true = 子 agent，不显示在主列表
    #[serde(default)]
    pub hidden: bool,
    /// cascade / independent
    #[serde(default = "default_cancel_policy")]
    pub cancel_policy: String,
    /// JSON 完整配置快照
    #[serde(default)]
    pub config: Option<String>,
    /// 物化缓存
    #[serde(default)]
    pub cached_context: Option<String>,
    /// active / done / cancelled / error
    #[serde(default = "default_agent_status")]
    pub agent_status: String,
    /// 最新会话回顾文本（`/recap` 生成，持久化以支持 `-c`/`-r` 恢复）
    #[serde(default)]
    pub latest_recap: Option<String>,
    /// 上一轮任务完成态总结（spinner 完成行，持久化以支持 `-c`/`-r` 恢复）
    #[serde(default)]
    pub last_task_summary: Option<TaskSummary>,
}

fn default_cancel_policy() -> String {
    "cascade".to_string()
}

fn default_agent_status() -> String {
    "active".to_string()
}

impl ThreadMeta {
    pub fn new(cwd: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            title: None,
            cwd: cwd.into(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            content_size: 0,
            parent_thread_id: None,
            snapshot_at_message_id: None,
            hidden: false,
            cancel_policy: default_cancel_policy(),
            config: None,
            cached_context: None,
            agent_status: default_agent_status(),
            latest_recap: None,
            last_task_summary: None,
        }
    }

    /// 是否为根 agent（无父线程）
    pub fn is_root(&self) -> bool {
        self.parent_thread_id.is_none()
    }

    /// 用于从 DB 行构建 ThreadMeta 时填充新字段的默认值
    pub fn default_for_db() -> Self {
        Self {
            id: String::new(),
            title: None,
            cwd: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 0,
            content_size: 0,
            parent_thread_id: None,
            snapshot_at_message_id: None,
            hidden: false,
            cancel_policy: default_cancel_policy(),
            config: None,
            cached_context: None,
            agent_status: default_agent_status(),
            latest_recap: None,
            last_task_summary: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_meta_default_values() {
        // new() 创建的根线程应具有正确的默认值
        let meta = ThreadMeta::new("/tmp/test");
        assert_eq!(meta.parent_thread_id, None);
        assert_eq!(meta.snapshot_at_message_id, None);
        assert!(!meta.hidden);
        assert_eq!(meta.cancel_policy, "cascade");
        assert_eq!(meta.config, None);
        assert_eq!(meta.cached_context, None);
        assert_eq!(meta.agent_status, "active");
        assert_eq!(meta.latest_recap, None);
        assert_eq!(meta.last_task_summary, None);
        assert!(meta.is_root());
    }

    #[test]
    fn test_thread_meta_is_root() {
        // parent_thread_id 为 None 时是根 agent
        let mut meta = ThreadMeta::new("/tmp/test");
        assert!(meta.is_root());

        // 设置 parent_thread_id 后不是根 agent
        meta.parent_thread_id = Some("parent-uuid".to_string());
        assert!(!meta.is_root());
    }

    #[test]
    fn test_thread_meta_deserialize_defaults() {
        // 反序列化旧格式 JSON 时新字段应使用默认值
        let json = r#"{"id":"test-id","title":null,"cwd":"/tmp","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","message_count":0,"content_size":0}"#;
        let meta: ThreadMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.parent_thread_id, None);
        assert_eq!(meta.snapshot_at_message_id, None);
        assert!(!meta.hidden);
        assert_eq!(meta.cancel_policy, "cascade");
        assert_eq!(meta.config, None);
        assert_eq!(meta.cached_context, None);
        assert_eq!(meta.agent_status, "active");
        assert_eq!(meta.latest_recap, None);
        assert_eq!(meta.last_task_summary, None);
    }

    #[test]
    fn test_task_summary_roundtrip() {
        // Arrange
        let summary = TaskSummary {
            verb: "Cooked".to_string(),
            elapsed_ms: 25_000,
            done_at: Utc::now(),
        };

        // Act
        let json = serde_json::to_string(&summary).unwrap();
        let back: TaskSummary = serde_json::from_str(&json).unwrap();

        // Assert
        assert_eq!(back, summary);
    }
}
