use super::*;
use crate::messages::MessageId;

async fn make_store() -> Result<(SqliteThreadStore, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let store = SqliteThreadStore::new(directory.path().join("context-cache.db")).await?;
    Ok((store, directory))
}

fn make_message_ids(messages: &[BaseMessage]) -> Vec<MessageId> {
    messages.iter().map(BaseMessage::id).collect()
}

#[tokio::test]
async fn test_cached_context_load_preserves_baseline_across_multiple_appends() -> Result<()> {
    let (store, _directory) = make_store().await?;
    let thread_id = store.create_thread(ThreadMeta::new("/project")).await?;
    let mut expected = vec![BaseMessage::human("first"), BaseMessage::ai("reply")];
    store.append_messages(&thread_id, &expected).await?;
    let initial = store.load_context(&thread_id).await?;
    let baseline = store.load_meta(&thread_id).await?.cached_context;
    assert!(baseline.is_some(), "首次读取仍应物化缓存");
    assert_eq!(make_message_ids(&initial), make_message_ids(&expected));
    for messages in [
        vec![
            BaseMessage::human("second"),
            BaseMessage::ai("second reply"),
        ],
        vec![BaseMessage::human("third")],
    ] {
        store.append_messages(&thread_id, &messages).await?;
        let before = store.load_meta(&thread_id).await?;
        expected.extend(messages);
        for _ in 0..2 {
            let loaded = store.load_context(&thread_id).await?;
            assert_eq!(
                make_message_ids(&loaded),
                make_message_ids(&expected),
                "多轮追加与重复读取必须保持消息完整且有序"
            );
        }
        let after = store.load_meta(&thread_id).await?;
        assert_eq!(after.cached_context, baseline, "缓存命中不能回写完整历史");
        assert_eq!(after.updated_at, before.updated_at, "读取不能改变更新时间");
    }
    Ok(())
}

#[tokio::test]
async fn test_cached_context_load_uses_own_tail_after_ancestor_prefix() -> Result<()> {
    let (store, _directory) = make_store().await?;
    let parent_id = store.create_thread(ThreadMeta::new("/project")).await?;
    let parent_messages = vec![
        BaseMessage::human("parent"),
        BaseMessage::ai("parent reply"),
    ];
    store.append_messages(&parent_id, &parent_messages).await?;
    let mut child_meta = ThreadMeta::new("/project");
    child_meta.parent_thread_id = Some(parent_id.clone());
    let child_id = store.create_thread(child_meta).await?;
    let own_messages = vec![BaseMessage::human("child"), BaseMessage::ai("child reply")];
    store.append_messages(&child_id, &own_messages).await?;
    let mut expected = parent_messages;
    expected.extend(own_messages);
    // 直接构造已物化缓存，隔离本次命中路径与旧版祖先截止点约定。
    store.save_context_cache(&child_id, &expected).await?;
    store
        .append_messages(&parent_id, &[BaseMessage::human("later parent message")])
        .await?;
    let new_message = BaseMessage::human("next child message");
    store
        .append_messages(&child_id, std::slice::from_ref(&new_message))
        .await?;
    expected.push(new_message);
    for _ in 0..2 {
        let loaded = store.load_context(&child_id).await?;
        assert_eq!(
            make_message_ids(&loaded),
            make_message_ids(&expected),
            "祖先前缀不能占用自身增量游标，父线程新增消息也不能混入"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_cached_context_load_reads_own_messages_after_ancestor_only_cache() -> Result<()> {
    let (store, _directory) = make_store().await?;
    let parent_id = store.create_thread(ThreadMeta::new("/project")).await?;
    let parent_messages = vec![
        BaseMessage::human("parent"),
        BaseMessage::ai("parent reply"),
    ];
    let mut child_meta = ThreadMeta::new("/project");
    child_meta.parent_thread_id = Some(parent_id.clone());
    child_meta.cached_context = Some(serde_json::to_string(&parent_messages)?);
    let child_id = store.create_thread(child_meta).await?;
    let initial = store.load_context(&child_id).await?;
    assert_eq!(
        make_message_ids(&initial),
        make_message_ids(&parent_messages)
    );
    let own_messages = vec![BaseMessage::human("child"), BaseMessage::ai("child reply")];
    store.append_messages(&child_id, &own_messages).await?;
    // 父消息落库晚于子消息，祖先 rowid 不能用作子线程的游标。
    store.append_messages(&parent_id, &parent_messages).await?;
    let mut expected = parent_messages;
    expected.extend(own_messages);
    for _ in 0..2 {
        let loaded = store.load_context(&child_id).await?;
        assert_eq!(
            make_message_ids(&loaded),
            make_message_ids(&expected),
            "仅含祖先的缓存应从自身第一条消息读取"
        );
    }
    store
        .delete_messages(&parent_id, &make_message_ids(&expected[..2]))
        .await?;
    let loaded_after_parent_cleanup = store.load_context(&child_id).await?;
    assert_eq!(
        make_message_ids(&loaded_after_parent_cleanup),
        make_message_ids(&expected),
        "父线程清理后必须保留已物化的祖先快照"
    );
    Ok(())
}

#[tokio::test]
async fn test_cached_context_load_ignores_duplicate_message_identifiers() -> Result<()> {
    let (store, _directory) = make_store().await?;
    let thread_id = store.create_thread(ThreadMeta::new("/project")).await?;
    let mut expected = vec![BaseMessage::human("first"), BaseMessage::ai("reply")];
    store.append_messages(&thread_id, &expected).await?;
    let _ = store.load_context(&thread_id).await?;
    let first_new = BaseMessage::human("second");
    store
        .append_messages(
            &thread_id,
            &[
                expected[1].clone(),
                first_new.clone(),
                expected[0].clone(),
                first_new.clone(),
            ],
        )
        .await?;
    expected.push(first_new.clone());
    let first_loaded = store.load_context(&thread_id).await?;
    assert_eq!(make_message_ids(&first_loaded), make_message_ids(&expected));
    let second_new = BaseMessage::ai("second reply");
    store
        .append_messages(&thread_id, &[first_new, second_new.clone()])
        .await?;
    expected.push(second_new);
    for _ in 0..2 {
        let loaded = store.load_context(&thread_id).await?;
        assert_eq!(
            make_message_ids(&loaded),
            make_message_ids(&expected),
            "INSERT OR IGNORE 不能造成增量重复或遗漏"
        );
    }
    assert_eq!(store.load_meta(&thread_id).await?.message_count, 4);
    Ok(())
}

#[tokio::test]
async fn test_cached_context_load_does_not_restore_deleted_messages() -> Result<()> {
    let (store, _directory) = make_store().await?;
    let thread_id = store.create_thread(ThreadMeta::new("/project")).await?;
    let messages = vec![
        BaseMessage::human("keep"),
        BaseMessage::ai("remove reply"),
        BaseMessage::human("remove tail"),
    ];
    store.append_messages(&thread_id, &messages).await?;
    let _ = store.load_context(&thread_id).await?;
    store
        .delete_messages(&thread_id, &make_message_ids(&messages[1..]))
        .await?;
    assert!(
        store.load_meta(&thread_id).await?.cached_context.is_none(),
        "删除后必须沿用缓存失效语义"
    );
    let new_message = BaseMessage::human("replacement");
    store
        .append_messages(&thread_id, std::slice::from_ref(&new_message))
        .await?;
    let expected = vec![messages[0].clone(), new_message];
    for _ in 0..2 {
        let loaded = store.load_context(&thread_id).await?;
        assert_eq!(
            make_message_ids(&loaded),
            make_message_ids(&expected),
            "删除的缓存尾消息不能复活，重新追加的消息不能漏载"
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_cached_context_hit_performs_no_updates() -> Result<()> {
    let (store, _directory) = make_store().await?;
    let thread_id = store.create_thread(ThreadMeta::new("/project")).await?;
    let mut expected = vec![BaseMessage::human("cached")];
    store.append_messages(&thread_id, &expected).await?;
    let _ = store.load_context(&thread_id).await?;
    let new_message = BaseMessage::ai("incremental");
    store
        .append_messages(&thread_id, std::slice::from_ref(&new_message))
        .await?;
    expected.push(new_message);
    let before = store.load_meta(&thread_id).await?;
    // 用触发器阻止 UPDATE，确定性验证读取路径不依赖时间精度或隐藏写入。
    sqlx::query(
        "CREATE TRIGGER reject_context_hit_updates BEFORE UPDATE ON threads
         BEGIN SELECT RAISE(ABORT, '缓存命中必须只读'); END",
    )
    .execute(&store.pool)
    .await?;
    for _ in 0..2 {
        let loaded = store.load_context(&thread_id).await?;
        assert_eq!(make_message_ids(&loaded), make_message_ids(&expected));
    }
    let after = store.load_meta(&thread_id).await?;
    assert_eq!(after.cached_context, before.cached_context, "不得重写缓存");
    assert_eq!(after.updated_at, before.updated_at, "不得重写更新时间");
    Ok(())
}
