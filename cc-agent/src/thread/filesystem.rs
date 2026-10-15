use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::{collections::HashSet, path::PathBuf};
use tokio::{fs, io::AsyncWriteExt};

use crate::{
    messages::BaseMessage,
    thread::{ThreadId, ThreadMeta, ThreadStore},
};

/// 基于文件系统的 ThreadStore 实现
///
/// 目录结构：
/// ```text
/// <base_dir>/
///   index.json                 # 所有 thread 的摘要索引
///   <thread_id>/
///     meta.json                # 单个 thread 的完整元数据
///     messages.jsonl           # 消息流，每行一条 JSON
/// ```
pub struct FilesystemThreadStore {
    base_dir: PathBuf,
    index_write_lock: tokio::sync::Mutex<()>,
}

impl FilesystemThreadStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            index_write_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// 使用默认路径创建（`~/.cc-code/threads/`）
    pub fn default_path() -> Result<Self> {
        let dir = crate::app_home::app_data_dir("threads");
        Ok(Self::new(dir))
    }

    fn thread_dir(&self, id: &ThreadId) -> PathBuf {
        self.base_dir.join(id)
    }

    fn meta_path(&self, id: &ThreadId) -> PathBuf {
        self.thread_dir(id).join("meta.json")
    }

    fn messages_path(&self, id: &ThreadId) -> PathBuf {
        self.thread_dir(id).join("messages.jsonl")
    }

    fn index_path(&self) -> PathBuf {
        self.base_dir.join("index.json")
    }

    /// 读取全局 index，不存在时返回空列表
    async fn read_index(&self) -> Result<Vec<ThreadMeta>> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(vec![]);
        }
        let raw = fs::read_to_string(&path).await?;
        let metas: Vec<ThreadMeta> = serde_json::from_str(&raw)?;
        Ok(metas)
    }

    /// 将 metas 写入 index.json
    async fn write_index(&self, metas: &[ThreadMeta]) -> Result<()> {
        fs::create_dir_all(&self.base_dir).await?;
        let json = serde_json::to_string_pretty(metas)?;
        fs::write(self.index_path(), json).await?;
        Ok(())
    }

    /// 在 index 中更新或插入一条摘要
    async fn upsert_index(&self, meta: &ThreadMeta) -> Result<()> {
        let _guard = self.index_write_lock.lock().await;
        let mut metas = self.read_index().await?;
        if let Some(pos) = metas.iter().position(|m| m.id == meta.id) {
            metas[pos] = meta.clone();
        } else {
            metas.push(meta.clone());
        }
        // 按 updated_at 降序排列
        sort_thread_metadata(&mut metas);
        self.write_index(&metas).await
    }

    async fn fill_content_sizes(&self, metas: &mut [ThreadMeta]) {
        for meta in metas {
            meta.content_size = fs::metadata(self.messages_path(&meta.id))
                .await
                .map(|metadata| metadata.len())
                .unwrap_or(0);
        }
    }

    async fn list_page(
        &self,
        limit: usize,
        offset: usize,
        root_only: bool,
    ) -> Result<Vec<ThreadMeta>> {
        if limit == 0 {
            return Ok(vec![]);
        }
        let mut metas = self.read_index().await?;
        metas.retain(|meta| !meta.hidden && (!root_only || meta.is_root()));
        sort_thread_metadata(&mut metas);
        let mut page: Vec<_> = metas.into_iter().skip(offset).take(limit).collect();
        for meta in &mut page {
            meta.cached_context = None;
        }
        self.fill_content_sizes(&mut page).await;
        Ok(page)
    }
}

#[async_trait]
impl ThreadStore for FilesystemThreadStore {
    async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId> {
        let id = meta.id.clone();
        fs::create_dir_all(self.thread_dir(&id)).await?;
        let json = serde_json::to_string_pretty(&meta)?;
        fs::write(self.meta_path(&id), json).await?;
        // 创建空的 messages.jsonl
        fs::write(self.messages_path(&id), b"").await?;
        self.upsert_index(&meta).await?;
        Ok(id)
    }

    async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()> {
        if msgs.is_empty() {
            return Ok(());
        }
        let path = self.messages_path(id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .with_context(|| format!("Failed to open messages.jsonl: {}", path.display()))?;

        for msg in msgs {
            let mut line = serde_json::to_string(msg)?;
            line.push('\n');
            file.write_all(line.as_bytes()).await?;
        }
        file.flush().await?;

        // 更新 meta 的 message_count 和 updated_at
        let mut meta = self.load_meta(id).await?;
        meta.message_count += msgs.len();
        meta.updated_at = Utc::now();
        // 如果还没有标题，用第一条 Human 消息的前 50 字符作为标题
        if meta.title.is_none() {
            if let Some(title) = extract_title(msgs) {
                meta.title = Some(title);
            }
        }
        self.update_meta(id, meta).await
    }

    async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>> {
        let path = self.messages_path(id);
        if !path.exists() {
            return Ok(vec![]);
        }
        let raw = fs::read_to_string(&path).await?;
        let mut msgs = Vec::new();
        for line in raw.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let msg: BaseMessage = serde_json::from_str(line)
                .with_context(|| format!("Failed to deserialize message: {line}"))?;
            msgs.push(msg);
        }
        Ok(msgs)
    }

    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta> {
        let path = self.meta_path(id);
        let raw = fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read meta.json: {}", path.display()))?;
        let meta: ThreadMeta = serde_json::from_str(&raw)?;
        Ok(meta)
    }

    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()> {
        let json = serde_json::to_string_pretty(&meta)?;
        fs::write(self.meta_path(id), json).await?;
        self.upsert_index(&meta).await
    }

    async fn list_threads(&self) -> Result<Vec<ThreadMeta>> {
        let mut metas = self.read_index().await?;
        // 排除 hidden 的子 agent
        metas.retain(|m| !m.hidden);
        sort_thread_metadata(&mut metas);
        // 计算 content_size（从 messages.jsonl 文件大小）
        self.fill_content_sizes(&mut metas).await;
        Ok(metas)
    }

    async fn list_threads_paged(&self, limit: usize, offset: usize) -> Result<Vec<ThreadMeta>> {
        self.list_page(limit, offset, false).await
    }

    async fn list_root_threads_paged(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ThreadMeta>> {
        self.list_page(limit, offset, true).await
    }

    async fn list_threads_metadata(&self) -> Result<Vec<ThreadMeta>> {
        let mut metas = self.read_index().await?;
        metas.retain(|meta| !meta.hidden);
        sort_thread_metadata(&mut metas);
        for meta in &mut metas {
            meta.content_size = 0;
            meta.config = None;
            meta.cached_context = None;
        }
        Ok(metas)
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        let _guard = self.index_write_lock.lock().await;
        let dir = self.thread_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).await?;
        }
        let mut metas = self.read_index().await?;
        metas.retain(|m| m.id != *id);
        self.write_index(&metas).await
    }

    async fn delete_thread_tree(&self, id: &ThreadId) -> Result<()> {
        let _guard = self.index_write_lock.lock().await;
        let mut metas = self.read_index().await?;
        let mut descendants = HashSet::from([id.clone()]);
        let mut pending = vec![id.clone()];
        while let Some(parent) = pending.pop() {
            for meta in &metas {
                if meta.parent_thread_id.as_ref() == Some(&parent)
                    && descendants.insert(meta.id.clone())
                {
                    pending.push(meta.id.clone());
                }
            }
        }
        // 先核对全部目标，再开始不可恢复的删除，避免异常索引越过存储根目录。
        let mut directories = Vec::new();
        for descendant in &descendants {
            let mut components = std::path::Path::new(descendant).components();
            anyhow::ensure!(
                matches!(components.next(), Some(std::path::Component::Normal(_)))
                    && components.next().is_none(),
                "Invalid thread directory: {descendant}"
            );
            let directory = self.thread_dir(descendant);
            match fs::symlink_metadata(&directory).await {
                Ok(_) => {
                    let root = fs::canonicalize(&self.base_dir).await?;
                    let resolved = fs::canonicalize(&directory).await?;
                    anyhow::ensure!(
                        resolved != root && resolved.starts_with(&root),
                        "Thread directory is outside storage root: {descendant}"
                    );
                    directories.push(directory);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        for directory in directories {
            fs::remove_dir_all(directory).await?;
        }
        metas.retain(|meta| !descendants.contains(&meta.id));
        self.write_index(&metas).await
    }

    async fn load_context(&self, thread_id: &ThreadId) -> Result<Vec<BaseMessage>> {
        // 文件系统实现暂不支持祖先链，直接加载自身消息
        self.load_messages(thread_id).await
    }

    async fn list_child_threads(&self, parent_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        let metas = self.read_index().await?;
        Ok(metas
            .into_iter()
            .filter(|m| m.parent_thread_id.as_deref() == Some(parent_id.as_str()))
            .collect())
    }

    async fn list_session_threads(&self, root_id: &ThreadId) -> Result<Vec<ThreadMeta>> {
        // 文件系统实现：简单过滤 parent_chain 包含 root_id 的 thread
        let metas = self.read_index().await?;
        let mut result = vec![];
        for m in &metas {
            if m.id == *root_id {
                result.push(m.clone());
            }
        }
        // 简单 BFS 查找子孙
        let mut to_check: Vec<String> = result.iter().map(|m| m.id.clone()).collect();
        while let Some(pid) = to_check.pop() {
            for m in &metas {
                if m.parent_thread_id.as_deref() == Some(pid.as_str()) {
                    to_check.push(m.id.clone());
                    result.push(m.clone());
                }
            }
        }
        Ok(result)
    }

    async fn update_thread_status(&self, id: &ThreadId, status: &str) -> Result<()> {
        let mut meta = self.load_meta(id).await?;
        meta.agent_status = status.to_string();
        meta.updated_at = Utc::now();
        self.update_meta(id, meta).await
    }

    async fn invalidate_context_cache(&self, thread_id: &ThreadId) -> Result<()> {
        let mut meta = self.load_meta(thread_id).await?;
        meta.cached_context = None;
        self.update_meta(thread_id, meta).await
    }

    async fn delete_messages(
        &self,
        _thread_id: &ThreadId,
        _message_ids: &[crate::messages::MessageId],
    ) -> Result<()> {
        Ok(())
    }
}

fn sort_thread_metadata(metas: &mut [ThreadMeta]) {
    metas.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.id.cmp(&left.id))
    });
}

/// 从消息列表中提取标题（取第一条 Human 消息的前 50 字符）
fn extract_title(msgs: &[BaseMessage]) -> Option<String> {
    use crate::messages::{ContentBlock, MessageContent};

    for msg in msgs {
        if let BaseMessage::Human { content, .. } = msg {
            let text = match content {
                MessageContent::Text(t) => t.to_string(),
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_ref())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                MessageContent::Raw(_) => continue,
            };
            let title: String = text.chars().take(50).collect();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    include!("filesystem_test.rs");
    include!("filesystem_command_center_test.rs");
}
