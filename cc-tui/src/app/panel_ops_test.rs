use super::App;
use crate::thread::ThreadMeta;

#[tokio::test]
async fn test_headless_storage_preserves_sibling_files_and_isolates_instances() -> anyhow::Result<()>
{
    let sibling = tempfile::tempdir()?;
    let nested = sibling.path().join("threads/session");
    std::fs::create_dir_all(&nested)?;
    let path = nested.join("meta.json");
    std::fs::write(&path, "兄弟临时目录中的会话")?;
    let (first, _first_handle) = App::new_headless(80, 24).await;
    let id = first
        .services
        .thread_store
        .create_thread(ThreadMeta::new("/first"))
        .await?;
    let (second, _second_handle) = App::new_headless(80, 24).await;
    assert_eq!(
        std::fs::read_to_string(&path)?,
        "兄弟临时目录中的会话",
        "创建 Headless App 不能收紧共享 Temp 目录并破坏兄弟文件权限"
    );
    assert_eq!(
        first.services.thread_store.load_meta(&id).await?.cwd,
        "/first"
    );
    assert!(
        second.services.thread_store.load_meta(&id).await.is_err(),
        "不同 Headless App 的数据库必须相互隔离"
    );
    Ok(())
}
