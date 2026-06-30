use super::*;

fn make_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{}.output", id))
}

#[tokio::test]
async fn test_spawn_writer_多chunk合并写入并读取尾部() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let path = make_path(tmp.path(), "t1");
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    // Act
    let handle = DiskOutput::spawn_writer(path.clone(), rx);
    tx.send(b"hello ".to_vec()).await.unwrap();
    tx.send(b"world\n".to_vec()).await.unwrap();
    tx.send(b"second line\n".to_vec()).await.unwrap();
    drop(tx);
    handle.await.unwrap();
    // Assert
    let tail = DiskOutput::read_tail(&path, 1024).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&tail), "hello world\nsecond line\n");
}

#[tokio::test]
async fn test_read_tail_只读末尾n字节() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let path = make_path(tmp.path(), "t2");
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let handle = DiskOutput::spawn_writer(path.clone(), rx);
    tx.send(b"ABCDEFGHIJ".to_vec()).await.unwrap(); // 10 bytes
    drop(tx);
    handle.await.unwrap();
    // Act
    let tail = DiskOutput::read_tail(&path, 4).await.unwrap();
    // Assert
    assert_eq!(tail, b"GHIJ");
}

#[tokio::test]
async fn test_read_delta_从offset读取新字节() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let path = make_path(tmp.path(), "t3");
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let handle = DiskOutput::spawn_writer(path.clone(), rx);
    tx.send(b"0123456789".to_vec()).await.unwrap();
    drop(tx);
    handle.await.unwrap();
    // Act
    let delta = DiskOutput::read_delta(&path, 5).await.unwrap();
    // Assert
    assert_eq!(delta, b"56789");
    // offset 超过 size 返回空
    let delta = DiskOutput::read_delta(&path, 100).await.unwrap();
    assert!(delta.is_empty());
}

#[tokio::test]
async fn test_cleanup_删除文件且幂等() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let path = make_path(tmp.path(), "t4");
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let handle = DiskOutput::spawn_writer(path.clone(), rx);
    tx.send(b"data".to_vec()).await.unwrap();
    drop(tx);
    handle.await.unwrap();
    assert!(path.exists());
    // Act
    DiskOutput::cleanup(&path).await.unwrap();
    // Assert
    assert!(!path.exists());
    // 幂等：再次 cleanup 不报错
    DiskOutput::cleanup(&path).await.unwrap();
}

#[tokio::test]
async fn test_spawn_writer_自动创建多层父目录() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("a/b/c/t5.output");
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    // Act
    let handle = DiskOutput::spawn_writer(path.clone(), rx);
    tx.send(b"nested".to_vec()).await.unwrap();
    drop(tx);
    handle.await.unwrap();
    // Assert
    let tail = DiskOutput::read_tail(&path, 64).await.unwrap();
    assert_eq!(tail, b"nested");
}

#[test]
fn test_sanitize_path_segment_替换非法字符() {
    // Act
    let seg = sanitize_path_segment(Path::new("C:\\Work\\peri"));
    // Assert
    assert!(!seg.contains('\\'));
    assert!(!seg.contains(':'));
    let seg2 = sanitize_path_segment(Path::new("/work/app"));
    assert!(!seg2.contains('/'));
}

#[test]
fn test_sanitize_path_segment_空路径回退root() {
    // Act
    let seg = sanitize_path_segment(Path::new(""));
    // Assert
    assert_eq!(seg, "root");
}

#[test]
fn test_task_output_path_包含完整层级() {
    // Act
    let path = task_output_path("abc123", Path::new("/work/app"), "sess-1");
    let s = path.to_string_lossy().to_string();
    // Assert
    assert!(s.contains("peri-"), "路径应包含 peri- 前缀: {}", s);
    assert!(s.contains("abc123.output"), "路径应包含任务 id: {}", s);
    assert!(s.contains("sess-1"), "路径应包含 session id: {}", s);
    assert!(s.contains("tasks"), "路径应包含 tasks 段: {}", s);
}
