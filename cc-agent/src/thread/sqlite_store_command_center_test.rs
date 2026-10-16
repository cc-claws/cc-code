#[tokio::test]
async fn test_runtime_snapshot_roundtrip_and_narrow_update() -> Result<()> {
    let (store, _dir) = make_store().await;
    let mut meta = ThreadMeta::new("/project");
    meta.model = Some("initial-model".into());
    meta.reasoning_effort = Some("medium".into());
    meta.branch = Some("feat/initial".into());
    meta.cached_context = Some("[]".into());
    let id = store.create_thread(meta).await?;
    let created = store.load_meta(&id).await?;
    assert_eq!(created.model.as_deref(), Some("initial-model"));
    assert_eq!(created.reasoning_effort.as_deref(), Some("medium"));
    assert_eq!(created.branch.as_deref(), Some("feat/initial"));
    store.update_title(&id, "保留并发标题").await?;
    store
        .update_latest_recap(&id, Some("保留回顾".into()))
        .await?;
    let before = store.load_meta(&id).await?;
    store
        .update_runtime_snapshot(&id, Some("updated-model".into()), Some("high".into()), None)
        .await?;
    let updated = store.load_meta(&id).await?;
    assert_eq!(updated.model.as_deref(), Some("updated-model"));
    assert_eq!(updated.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(updated.branch, None);
    assert_eq!(updated.title, before.title, "配置快照不能覆盖标题");
    assert_eq!(
        updated.latest_recap, before.latest_recap,
        "配置快照不能覆盖回顾"
    );
    assert_eq!(updated.cached_context, before.cached_context);
    assert_eq!(updated.updated_at, before.updated_at);
    let mut changed = updated;
    changed.branch = Some("feat/updated".into());
    store.update_meta(&id, changed).await?;
    assert_eq!(
        store.load_meta(&id).await?.branch.as_deref(),
        Some("feat/updated")
    );
    Ok(())
}

#[tokio::test]
async fn test_legacy_schema_migration_preserves_threads_and_is_idempotent() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("legacy.db");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .context("应能创建具有继承权限的旧数据库")?;
    sqlx::query(
        "CREATE TABLE threads (
            id TEXT PRIMARY KEY, title TEXT, cwd TEXT NOT NULL,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            message_count INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO threads VALUES (
            'legacy', '旧会话', '/legacy', '2026-01-01T00:00:00Z',
            '2026-01-01T00:00:00Z', 0
        )",
    )
    .execute(&pool)
    .await?;
    pool.close().await;
    let store = SqliteThreadStore::new(&db_path)
        .await
        .context("收紧权限后仍应能迁移已有数据库")?;
    store.init_schema().await?;
    let legacy = store.load_meta(&"legacy".into()).await?;
    assert_eq!(legacy.title.as_deref(), Some("旧会话"));
    assert_eq!(legacy.model, None, "旧会话模型快照应为空");
    assert_eq!(legacy.reasoning_effort, None);
    assert_eq!(legacy.branch, None);
    assert_eq!(legacy.agent_status, "active");
    assert_eq!(legacy.content_size, 0);
    assert_eq!(store.list_threads_paged(10, 0).await?.len(), 1);
    store.pool.close().await;
    let reopened = SqliteThreadStore::new(&db_path)
        .await
        .context("迁移后的数据库应能再次打开")?;
    assert_eq!(
        reopened.load_meta(&"legacy".into()).await?.title,
        legacy.title,
        "再次打开不能丢失旧会话"
    );
    Ok(())
}

#[tokio::test]
async fn test_root_thread_pagination_is_global_stable_and_excludes_children() -> Result<()> {
    let (store, _dir) = make_store().await;
    let timestamp = "2026-01-01T00:00:00Z".parse::<DateTime<Utc>>()?;
    for index in 0..25 {
        let mut meta = ThreadMeta::new(format!("/project-{}", index % 3));
        meta.id = format!("root-{index:02}");
        meta.created_at = timestamp;
        meta.updated_at = timestamp;
        meta.model = Some("test-model".into());
        store.create_thread(meta).await?;
    }
    let mut child = ThreadMeta::new("/project-child");
    child.parent_thread_id = Some("root-24".into());
    store.create_thread(child).await?;
    let mut hidden = ThreadMeta::new("/project-hidden");
    hidden.hidden = true;
    store.create_thread(hidden).await?;
    let first = store.list_root_threads_paged(10, 0).await?;
    let second = store.list_root_threads_paged(10, 10).await?;
    let third = store.list_root_threads_paged(10, 20).await?;
    assert_eq!(first.len(), 10, "子线程不能占根会话的分页名额");
    assert_eq!(second.len(), 10);
    assert_eq!(third.len(), 5);
    assert_eq!(first[0].id, "root-24", "同时间戳按 id 降序稳定排序");
    assert_eq!(second[0].id, "root-14");
    assert_eq!(third[0].id, "root-04");
    assert!(first.iter().all(|meta| meta.cached_context.is_none()));
    assert!(first.iter().any(|meta| meta.cwd == "/project-0"));
    assert!(first.iter().any(|meta| meta.cwd == "/project-1"));
    assert!(first.iter().any(|meta| meta.cwd == "/project-2"));
    assert_eq!(first[0].model.as_deref(), Some("test-model"));
    assert_eq!(store.list_threads_paged(30, 0).await?.len(), 26);
    assert!(store.list_threads_paged(0, 0).await?.is_empty());
    assert!(store.list_root_threads_paged(10, 99).await?.is_empty());
    assert!(store
        .list_root_threads_paged(10, usize::MAX)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn test_paged_content_size_counts_bytes_per_thread_without_cache() -> Result<()> {
    let (store, _dir) = make_store().await;
    let mut meta = ThreadMeta::new("/project");
    meta.cached_context = Some("大量上下文".repeat(1000));
    let id = store.create_thread(meta).await?;
    let messages = [BaseMessage::human("中文消息"), BaseMessage::ai("答复")];
    let expected: usize = messages
        .iter()
        .map(|message| {
            serde_json::to_string(message)
                .expect("构造测试消息应可序列化")
                .len()
        })
        .sum();
    store.append_messages(&id, &messages).await?;
    store.create_thread(ThreadMeta::new("/empty")).await?;
    let page = store.list_threads_paged(10, 0).await?;
    let item = page
        .iter()
        .find(|meta| meta.id == id)
        .expect("分页应包含该线程");
    assert_eq!(
        item.content_size, expected as u64,
        "内容大小应按 UTF-8 字节计算"
    );
    assert_eq!(item.message_count, 2);
    assert_eq!(item.cached_context, None, "分页不应加载上下文缓存");
    assert_eq!(
        page.iter()
            .find(|meta| meta.id != id)
            .expect("空线程应保留")
            .content_size,
        0
    );
    assert_eq!(store.load_meta(&id).await?.content_size, expected as u64);
    Ok(())
}

#[tokio::test]
async fn test_metadata_listing_does_not_read_message_table_or_large_fields() -> Result<()> {
    let (store, _dir) = make_store().await;
    let mut meta = ThreadMeta::new("/project");
    meta.model = Some("test-model".into());
    meta.config = Some("large-config".repeat(1000));
    meta.cached_context = Some("large-cache".repeat(1000));
    store.create_thread(meta).await?;
    sqlx::query("DROP TABLE messages")
        .execute(&store.pool)
        .await?;
    let metadata = store.list_threads_metadata().await?;
    assert_eq!(metadata.len(), 1, "元数据统计不应依赖消息表");
    assert_eq!(metadata[0].model.as_deref(), Some("test-model"));
    assert_eq!(metadata[0].content_size, 0);
    assert_eq!(metadata[0].config, None);
    assert_eq!(metadata[0].cached_context, None);
    Ok(())
}

#[tokio::test]
async fn test_delete_thread_tree_removes_hidden_descendants_messages_and_caches() -> Result<()> {
    let (store, _dir) = make_store().await;
    for (id, parent, hidden) in [
        ("root", None, false),
        ("child", Some("root"), true),
        ("grandchild", Some("child"), true),
        ("other", None, false),
        ("other-child", Some("other"), true),
    ] {
        let mut meta = ThreadMeta::new("/project");
        meta.id = id.into();
        meta.parent_thread_id = parent.map(String::from);
        meta.hidden = hidden;
        meta.cached_context = Some(format!("cached-{id}"));
        store.create_thread(meta).await?;
        store
            .append_messages(&id.into(), &[BaseMessage::human(format!("消息-{id}"))])
            .await?;
    }
    assert!(store
        .load_meta(&"child".into())
        .await?
        .cached_context
        .is_some());
    store.delete_thread_tree(&"root".into()).await?;
    for id in ["root", "child", "grandchild"] {
        assert!(
            store.load_meta(&id.into()).await.is_err(),
            "子树元数据应真删"
        );
        assert!(
            store.load_messages(&id.into()).await?.is_empty(),
            "子树消息应级联删除"
        );
    }
    for id in ["other", "other-child"] {
        let meta = store.load_meta(&id.into()).await?;
        assert_eq!(
            meta.cached_context,
            Some(format!("cached-{id}")),
            "其他会话缓存应保留"
        );
        assert_eq!(store.load_messages(&id.into()).await?.len(), 1);
    }
    let remaining: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM threads),
                (SELECT COUNT(*) FROM messages),
                (SELECT COUNT(*) FROM threads WHERE cached_context IS NOT NULL)",
    )
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(remaining, (2, 2, 2), "不能遗留隐藏子线程、消息或上下文缓存");
    store.delete_thread_tree(&"root".into()).await?;
    assert_eq!(
        store.list_threads().await?.len(),
        1,
        "重复删除不影响其他根会话"
    );
    Ok(())
}

#[tokio::test]
async fn test_delete_thread_tree_terminates_for_parent_cycles() -> Result<()> {
    let (store, _dir) = make_store().await;
    for (id, parent) in [("first", "second"), ("second", "first")] {
        let mut meta = ThreadMeta::new("/project");
        meta.id = id.into();
        meta.parent_thread_id = Some(parent.into());
        store.create_thread(meta).await?;
    }
    store.create_thread(ThreadMeta::new("/other")).await?;
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        store.delete_thread_tree(&"first".into()),
    )
    .await??;
    assert!(store.load_meta(&"first".into()).await.is_err());
    assert!(store.load_meta(&"second".into()).await.is_err());
    assert_eq!(store.list_threads().await?.len(), 1, "环外会话应保留");
    Ok(())
}

#[tokio::test]
async fn test_delete_thread_preserves_descendants_for_existing_callers() -> Result<()> {
    let (store, _dir) = make_store().await;
    let root = store.create_thread(ThreadMeta::new("/project")).await?;
    let mut child = ThreadMeta::new("/project");
    child.parent_thread_id = Some(root.clone());
    let child = store.create_thread(child).await?;
    store.delete_thread(&root).await?;
    assert!(
        store.load_meta(&child).await.is_ok(),
        "原删除 API 仍只删除指定线程"
    );
    Ok(())
}
