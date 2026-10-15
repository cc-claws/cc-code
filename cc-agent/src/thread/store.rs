use anyhow::Result;
use async_trait::async_trait;

use super::types::{TaskSummary, ThreadId, ThreadMeta};
use crate::messages::BaseMessage;

#[async_trait]
pub trait ThreadStore: Send + Sync {
    /// 创建新 thread，返回分配的 ThreadId
    async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId>;

    /// 追加消息到指定 thread（追加写，不覆盖）
    async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()>;

    /// 追加单条消息到指定 thread（默认实现复用 append_messages）
    async fn append_message(&self, id: &ThreadId, message: BaseMessage) -> Result<()> {
        self.append_messages(id, &[message]).await
    }

    /// 加载指定 thread 的全部消息
    async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>>;

    /// 加载指定 thread 的元数据
    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta>;

    /// 更新指定 thread 的元数据
    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()>;

    /// 列举所有 thread 元数据，按 updated_at 降序（不含 hidden 的子 agent）
    async fn list_threads(&self) -> Result<Vec<ThreadMeta>>;

    /// 分页列举可见线程，按 updated_at、id 降序；limit 为 0 时返回空列表。
    async fn list_threads_paged(&self, limit: usize, offset: usize) -> Result<Vec<ThreadMeta>>;

    /// 分页列举可见根线程，先排除子线程再应用 limit / offset。
    async fn list_root_threads_paged(&self, limit: usize, offset: usize)
        -> Result<Vec<ThreadMeta>>;

    /// 轻量列举全部可见线程，用于全局计数、分组和标题过滤，不读取消息内容。
    ///
    /// content_size 为 0，config / cached_context 为 None；需要完整元数据时使用 load_meta。
    async fn list_threads_metadata(&self) -> Result<Vec<ThreadMeta>>;

    /// 删除指定 thread（包含消息和元数据）
    async fn delete_thread(&self, id: &ThreadId) -> Result<()>;

    /// 永久删除指定 thread 及全部子孙（含 hidden），一并清除消息和上下文缓存。
    /// 默认实现保留自定义后端兼容性；支持线程树的后端应覆盖为整棵树删除。
    async fn delete_thread_tree(&self, id: &ThreadId) -> Result<()> {
        self.delete_thread(id).await
    }

    /// 更新指定 thread 的标题
    async fn update_title(&self, id: &ThreadId, title: &str) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.title = Some(title.to_string());
        self.update_meta(id, meta).await
    }

    /// 更新会话运行配置快照；SQLite 后端使用窄更新，避免覆盖并发标题与回顾写入。
    async fn update_runtime_snapshot(
        &self,
        id: &ThreadId,
        model: Option<String>,
        reasoning_effort: Option<String>,
        branch: Option<String>,
    ) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.model = model;
        meta.reasoning_effort = reasoning_effort;
        meta.branch = branch;
        self.update_meta(id, meta).await
    }

    /// 仅更新 latest_recap 字段（窄更新，避免触碰 cached_context 等大字段）。
    ///
    /// 默认实现走 load_meta + update_meta；存储后端应优先 override 为单列 UPDATE。
    async fn update_latest_recap(&self, id: &ThreadId, recap: Option<String>) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.latest_recap = recap;
        self.update_meta(id, meta).await
    }

    /// 仅更新 last_task_summary 字段（窄更新，避免触碰 cached_context 等大字段）。
    ///
    /// 默认实现走 load_meta + update_meta；存储后端应优先 override 为单列 UPDATE。
    async fn update_last_task_summary(
        &self,
        id: &ThreadId,
        summary: Option<TaskSummary>,
    ) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.last_task_summary = summary;
        self.update_meta(id, meta).await
    }

    /// 加载 thread 的完整上下文（含祖先链 + 缓存）
    async fn load_context(&self, thread_id: &ThreadId) -> Result<Vec<BaseMessage>>;

    /// 列举指定父 thread 的直接子 thread
    async fn list_child_threads(&self, parent_id: &ThreadId) -> Result<Vec<ThreadMeta>>;

    /// 递归列举以 root_id 为根的所有 thread（含自身）
    async fn list_session_threads(&self, root_id: &ThreadId) -> Result<Vec<ThreadMeta>>;

    /// 更新 thread 的 agent_status 字段
    async fn update_thread_status(&self, id: &ThreadId, status: &str) -> Result<()>;

    /// 清除 thread 的 cached_context
    async fn invalidate_context_cache(&self, thread_id: &ThreadId) -> Result<()>;

    /// 按 message_id 列表精确删除消息，并刷新 cached_context。
    async fn delete_messages(
        &self,
        thread_id: &ThreadId,
        message_ids: &[crate::messages::MessageId],
    ) -> Result<()>;
}
