#[tokio::test]
async fn test_root_thread_pagination_is_global_stable_and_excludes_children() -> Result<()> {
    let dir = tempdir()?;
    let store = FilesystemThreadStore::new(dir.path());
    let timestamp = "2026-01-01T00:00:00Z".parse::<chrono::DateTime<Utc>>()?;
    for index in 0..12 {
        let mut meta = ThreadMeta::new(format!("/project-{}", index % 3));
        meta.id = format!("root-{index:02}");
        meta.updated_at = timestamp;
        store.create_thread(meta).await?;
    }
    let mut child = ThreadMeta::new("/child");
    child.parent_thread_id = Some("root-11".into());
    store.create_thread(child).await?;
    let mut hidden = ThreadMeta::new("/hidden");
    hidden.hidden = true;
    store.create_thread(hidden).await?;
    let first = store.list_root_threads_paged(10, 0).await?;
    let second = store.list_root_threads_paged(10, 10).await?;
    assert_eq!(first.len(), 10, "子线程不能占根会话分页名额");
    assert_eq!(second.len(), 2);
    assert_eq!(first[0].id, "root-11", "同时间戳分页应稳定排序");
    assert_eq!(second[0].id, "root-01");
    assert!(first.iter().any(|meta| meta.cwd == "/project-0"));
    assert!(first.iter().any(|meta| meta.cwd == "/project-1"));
    assert!(first.iter().any(|meta| meta.cwd == "/project-2"));
    assert_eq!(store.list_threads_paged(20, 0).await?.len(), 13);
    assert!(store.list_threads_paged(0, 0).await?.is_empty());
    assert!(store
        .list_root_threads_paged(10, usize::MAX)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn test_runtime_snapshot_and_lightweight_metadata_roundtrip() -> Result<()> {
    let dir = tempdir()?;
    let store = FilesystemThreadStore::new(dir.path());
    let mut meta = ThreadMeta::new("/project");
    meta.config = Some("保留配置".into());
    meta.cached_context = Some("保留上下文".into());
    let id = store.create_thread(meta).await?;
    store
        .update_runtime_snapshot(
            &id,
            Some("test-model".into()),
            Some("high".into()),
            Some("feat/agent-center".into()),
        )
        .await?;
    store
        .append_messages(&id, &[BaseMessage::human("消息内容")])
        .await?;
    let loaded = store.load_meta(&id).await?;
    assert_eq!(loaded.model.as_deref(), Some("test-model"));
    assert_eq!(loaded.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(loaded.branch.as_deref(), Some("feat/agent-center"));
    assert_eq!(loaded.config.as_deref(), Some("保留配置"));
    let metadata = store.list_threads_metadata().await?;
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].content_size, 0);
    assert_eq!(metadata[0].config, None);
    assert_eq!(metadata[0].cached_context, None);
    assert_eq!(metadata[0].model, loaded.model);
    let page = store.list_threads_paged(10, 0).await?;
    assert!(page[0].content_size > 0);
    assert_eq!(page[0].cached_context, None, "分页不能返回完整上下文缓存");
    Ok(())
}

#[tokio::test]
async fn test_delete_thread_tree_removes_hidden_descendants_messages_and_caches() -> Result<()> {
    let dir = tempdir()?;
    let store = FilesystemThreadStore::new(dir.path());
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
            "子树消息应真删"
        );
        assert!(!dir.path().join(id).exists(), "线程目录连同缓存应一并删除");
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
    let index = store.read_index().await?;
    assert_eq!(index.len(), 2, "索引不能遗留隐藏子线程");
    assert!(index.iter().all(|meta| meta.id.starts_with("other")));
    store.delete_thread_tree(&"root".into()).await?;
    assert_eq!(store.read_index().await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn test_delete_thread_tree_terminates_for_parent_cycles() -> Result<()> {
    let dir = tempdir()?;
    let store = FilesystemThreadStore::new(dir.path());
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
    assert_eq!(store.read_index().await?.len(), 1, "环外会话应保留");
    Ok(())
}

#[tokio::test]
async fn test_delete_thread_preserves_descendants_for_existing_callers() -> Result<()> {
    let dir = tempdir()?;
    let store = FilesystemThreadStore::new(dir.path());
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

#[tokio::test]
async fn test_delete_thread_tree_rejects_index_paths_outside_storage() -> Result<()> {
    let dir = tempdir()?;
    let store = FilesystemThreadStore::new(dir.path().join("threads"));
    let root = store.create_thread(ThreadMeta::new("/project")).await?;
    let mut invalid = ThreadMeta::new("/project");
    invalid.id = "../keep".into();
    invalid.parent_thread_id = Some(root.clone());
    let mut index = store.read_index().await?;
    index.push(invalid);
    store.write_index(&index).await?;
    fs::create_dir_all(dir.path().join("keep")).await?;
    fs::write(dir.path().join("keep/preserved.txt"), "保留").await?;
    assert!(
        store.delete_thread_tree(&root).await.is_err(),
        "异常路径必须拒绝"
    );
    assert!(
        store.load_meta(&root).await.is_ok(),
        "校验失败前不能开始删除"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("keep/preserved.txt")).await?,
        "保留"
    );
    assert_eq!(store.read_index().await?.len(), 2, "校验失败不能改索引");
    Ok(())
}

#[tokio::test]
async fn test_concurrent_index_upserts_preserve_unrelated_threads() -> Result<()> {
    let dir = tempdir()?;
    let store = std::sync::Arc::new(FilesystemThreadStore::new(dir.path()));
    let mut creates = Vec::new();
    for _ in 0..12 {
        let store = store.clone();
        creates.push(tokio::spawn(async move {
            store.create_thread(ThreadMeta::new("/project")).await
        }));
    }
    for create in creates {
        create.await??;
    }
    assert_eq!(
        store.read_index().await?.len(),
        12,
        "并发索引更新不能相互覆盖"
    );
    Ok(())
}
