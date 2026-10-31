use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use cc_acp::provider::{PeriConfig, ProviderConfig, ProviderModels};
use cc_agent::messages::MessageId;
use cc_agent::thread::{ThreadId, ThreadMeta, ThreadStore};
use cc_middlewares::hitl::shared_mode::{PermissionMode, SharedPermissionMode};

use super::*;

// 计数直接落在 ThreadStore 边界，并可暂停读取以验证恢复期间的锁和并发更新。
struct MockContextThreadStore {
    meta: parking_lot::RwLock<Option<ThreadMeta>>,
    history: parking_lot::RwLock<Vec<BaseMessage>>,
    context_reads: AtomicUsize,
    meta_reads: AtomicUsize,
    block_context_reads: AtomicBool,
    context_read_started: tokio::sync::Notify,
    continue_context_read: tokio::sync::Notify,
}

#[async_trait]
impl ThreadStore for MockContextThreadStore {
    async fn create_thread(&self, _meta: ThreadMeta) -> anyhow::Result<ThreadId> {
        unreachable!("恢复历史不应创建线程")
    }

    async fn append_messages(&self, _id: &ThreadId, _msgs: &[BaseMessage]) -> anyhow::Result<()> {
        unreachable!("恢复历史不应追加消息")
    }

    async fn load_messages(&self, _id: &ThreadId) -> anyhow::Result<Vec<BaseMessage>> {
        unreachable!("恢复历史应通过 load_context 读取")
    }

    async fn load_meta(&self, id: &ThreadId) -> anyhow::Result<ThreadMeta> {
        self.meta_reads.fetch_add(1, Ordering::Relaxed);
        self.meta
            .read()
            .as_ref()
            .filter(|meta| &meta.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("测试线程不存在"))
    }

    async fn update_meta(&self, _id: &ThreadId, _meta: ThreadMeta) -> anyhow::Result<()> {
        unreachable!("恢复历史不应更新元数据")
    }

    async fn list_threads(&self) -> anyhow::Result<Vec<ThreadMeta>> {
        unreachable!("恢复历史不应列举线程")
    }

    async fn list_threads_paged(
        &self,
        _limit: usize,
        _offset: usize,
    ) -> anyhow::Result<Vec<ThreadMeta>> {
        unreachable!("恢复历史不应分页列举线程")
    }

    async fn list_root_threads_paged(
        &self,
        _limit: usize,
        _offset: usize,
    ) -> anyhow::Result<Vec<ThreadMeta>> {
        unreachable!("恢复历史不应分页列举根线程")
    }

    async fn list_threads_metadata(&self) -> anyhow::Result<Vec<ThreadMeta>> {
        unreachable!("恢复历史不应列举元数据")
    }

    async fn delete_thread(&self, _id: &ThreadId) -> anyhow::Result<()> {
        unreachable!("恢复历史不应删除线程")
    }

    async fn load_context(&self, _id: &ThreadId) -> anyhow::Result<Vec<BaseMessage>> {
        self.context_reads.fetch_add(1, Ordering::Relaxed);
        if self.block_context_reads.load(Ordering::Relaxed) {
            self.context_read_started.notify_one();
            self.continue_context_read.notified().await;
        }
        Ok(self.history.read().clone())
    }

    async fn list_child_threads(&self, _id: &ThreadId) -> anyhow::Result<Vec<ThreadMeta>> {
        unreachable!("恢复历史不应列举子线程")
    }

    async fn list_session_threads(&self, _id: &ThreadId) -> anyhow::Result<Vec<ThreadMeta>> {
        unreachable!("恢复历史不应列举会话线程")
    }

    async fn update_thread_status(&self, _id: &ThreadId, _status: &str) -> anyhow::Result<()> {
        unreachable!("恢复历史不应更新线程状态")
    }

    async fn invalidate_context_cache(&self, _id: &ThreadId) -> anyhow::Result<()> {
        unreachable!("恢复历史不应清除上下文缓存")
    }

    async fn delete_messages(&self, _id: &ThreadId, _ids: &[MessageId]) -> anyhow::Result<()> {
        unreachable!("恢复历史不应删除消息")
    }
}

fn make_context_thread_store(
    meta: ThreadMeta,
    history: Vec<BaseMessage>,
) -> Arc<MockContextThreadStore> {
    Arc::new(MockContextThreadStore {
        meta: parking_lot::RwLock::new(Some(meta)),
        history: parking_lot::RwLock::new(history),
        context_reads: AtomicUsize::new(0),
        meta_reads: AtomicUsize::new(0),
        block_context_reads: AtomicBool::new(false),
        context_read_started: tokio::sync::Notify::new(),
        continue_context_read: tokio::sync::Notify::new(),
    })
}

fn make_stdio_context(thread_store: Arc<dyn ThreadStore>) -> StdioContext {
    let mut peri_config = PeriConfig::default();
    peri_config.config.active_provider_id = "test".to_string();
    peri_config.config.providers = vec![ProviderConfig {
        id: "test".to_string(),
        provider_type: "openai".to_string(),
        api_key: "sk-test".to_string(),
        models: ProviderModels {
            sonnet: "gpt-4o".to_string(),
            ..Default::default()
        },
        ..Default::default()
    }];
    let provider = cc_tui::app::agent::LlmProvider::from_config(&peri_config).unwrap();
    StdioContext {
        provider: parking_lot::RwLock::new(provider),
        peri_config: parking_lot::RwLock::new(peri_config),
        permission_mode: SharedPermissionMode::new(PermissionMode::Bypass),
        cron_scheduler: Arc::new(parking_lot::Mutex::new(
            cc_middlewares::cron::CronScheduler::new(tokio::sync::mpsc::unbounded_channel().0),
        )),
        mcp_pool: None,
        channel_state: None,
        plugin_skill_dirs: Vec::new(),
        plugin_agent_dirs: Vec::new(),
        hook_groups: Vec::new(),
        plugin_lsp_servers: Vec::new(),
        tool_search_index: Arc::new(cc_middlewares::tool_search::ToolSearchIndex::new()),
        shared_tools: Arc::default(),
        sessions: parking_lot::RwLock::default(),
        thread_store,
        langfuse_session: None,
    }
}

fn make_session_info(meta: &ThreadMeta, history: Vec<BaseMessage>) -> SessionInfo {
    SessionInfo {
        session_id: meta.id.clone(),
        thread_id: meta.id.clone(),
        cwd: meta.cwd.clone(),
        history,
        cancel_token: None,
        background_registry: Arc::default(),
        frozen: None,
        agent_pool: Arc::default(),
        prompt_lock: Arc::default(),
        background_results: None,
    }
}

#[tokio::test]
async fn test_resume_session_reuses_nonempty_history_and_frozen_data() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let meta = ThreadMeta::new(tmp.path().to_string_lossy().to_string());
    let id = meta.id.clone();
    let store = make_context_thread_store(meta, vec![BaseMessage::human("已恢复的历史")]);
    let ctx = make_stdio_context(store.clone());
    resume_session(&ctx, &id).await?;
    let frozen_prompt = ctx.sessions.read()[&id]
        .frozen
        .as_ref()
        .expect("新恢复的会话应初始化冻结数据")
        .system_prompt
        .clone();
    ctx.sessions
        .write()
        .get_mut(&id)
        .expect("会话应存在")
        .history
        .push(BaseMessage::ai("内存中的新回答"));
    *store.history.write() = vec![BaseMessage::human("存储中的不同历史")];
    resume_session(&ctx, &id).await?;
    assert_eq!(
        store.context_reads.load(Ordering::Relaxed),
        1,
        "非空历史重复恢复不应读取上下文"
    );
    assert_eq!(
        store.meta_reads.load(Ordering::Relaxed),
        2,
        "重复恢复仍需确认线程存在"
    );
    let sessions = ctx.sessions.read();
    assert_eq!(sessions[&id].history.len(), 2, "内存历史不能被存储快照覆盖");
    assert_eq!(sessions[&id].history[1].content(), "内存中的新回答");
    assert_eq!(
        sessions[&id]
            .frozen
            .as_ref()
            .expect("冻结信息应保留")
            .system_prompt,
        frozen_prompt
    );
    Ok(())
}

#[tokio::test]
async fn test_resume_session_reloads_existing_empty_history() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let meta = ThreadMeta::new(tmp.path().to_string_lossy().to_string());
    let id = meta.id.clone();
    let store = make_context_thread_store(meta.clone(), Vec::new());
    let ctx = make_stdio_context(store.clone());
    ctx.sessions
        .write()
        .insert(id.clone(), make_session_info(&meta, Vec::new()));
    resume_session(&ctx, &id).await?;
    assert!(
        ctx.sessions.read()[&id].history.is_empty(),
        "首次读取可以返回空历史"
    );
    *store.history.write() = vec![BaseMessage::human("稍后持久化的历史")];
    resume_session(&ctx, &id).await?;
    assert_eq!(
        store.context_reads.load(Ordering::Relaxed),
        2,
        "空历史重复恢复必须再次读取上下文"
    );
    assert_eq!(
        ctx.sessions.read()[&id].history[0].content(),
        "稍后持久化的历史"
    );
    assert!(
        ctx.sessions.read()[&id].frozen.is_none(),
        "已有会话的冻结数据初始化语义应保持不变"
    );
    Ok(())
}

#[tokio::test]
async fn test_resume_session_revalidates_missing_thread_with_nonempty_history() -> anyhow::Result<()>
{
    let tmp = tempfile::tempdir()?;
    let meta = ThreadMeta::new(tmp.path().to_string_lossy().to_string());
    let id = meta.id.clone();
    let store = make_context_thread_store(meta, vec![BaseMessage::human("内存历史")]);
    let ctx = make_stdio_context(store.clone());
    resume_session(&ctx, &id).await?;
    *store.meta.write() = None;
    let result = resume_session(&ctx, &id).await;
    assert!(result.is_err(), "线程删除后必须拒绝重复恢复");
    assert_eq!(
        store.context_reads.load(Ordering::Relaxed),
        1,
        "元数据验证失败不应读取上下文"
    );
    assert_eq!(ctx.sessions.read()[&id].history[0].content(), "内存历史");
    Ok(())
}

#[tokio::test]
async fn test_resume_session_keeps_history_filled_during_context_load() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let meta = ThreadMeta::new(tmp.path().to_string_lossy().to_string());
    let id = meta.id.clone();
    let store = make_context_thread_store(meta.clone(), vec![BaseMessage::human("较早的存储快照")]);
    store.block_context_reads.store(true, Ordering::Relaxed);
    let ctx = Arc::new(make_stdio_context(store.clone()));
    ctx.sessions
        .write()
        .insert(id.clone(), make_session_info(&meta, Vec::new()));
    let resume_task = {
        let ctx = ctx.clone();
        let id = id.clone();
        tokio::spawn(async move { resume_session(&ctx, &id).await })
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        store.context_read_started.notified(),
    )
    .await?;
    {
        let mut sessions = ctx
            .sessions
            .try_write()
            .expect("等待存储读取时不能持有会话锁");
        sessions.get_mut(&id).expect("会话应存在").history =
            vec![BaseMessage::human("并发更新后的历史")];
    }
    store.continue_context_read.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(10), resume_task).await???;
    assert_eq!(store.context_reads.load(Ordering::Relaxed), 1);
    assert_eq!(
        ctx.sessions.read()[&id].history[0].content(),
        "并发更新后的历史",
        "写锁内必须复核，不能用较早快照覆盖并发历史"
    );
    Ok(())
}
