use super::*;

fn make_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{}.output", id))
}

#[tokio::test]
async fn test_spawn_writer_merges_multiple_chunks_and_reads_tail() {
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
async fn test_read_tail_reads_only_last_n_bytes() {
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
async fn test_read_delta_reads_new_bytes_from_offset() {
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
async fn test_cleanup_deletes_file_and_is_idempotent() {
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
async fn test_spawn_writer_creates_nested_parent_directories() {
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
fn test_sanitize_path_segment_replaces_invalid_characters() {
    // Act
    let seg = sanitize_path_segment(Path::new("C:\\Work\\peri"));
    // Assert
    assert!(!seg.contains('\\'));
    assert!(!seg.contains(':'));
    let seg2 = sanitize_path_segment(Path::new("/work/app"));
    assert!(!seg2.contains('/'));
}

#[test]
fn test_sanitize_path_segment_empty_path_falls_back_to_root() {
    // Act
    let seg = sanitize_path_segment(Path::new(""));
    // Assert
    assert_eq!(seg, "root");
}

#[test]
fn test_task_output_path_contains_full_hierarchy() {
    // Act
    let id = "01a18645-2f37-7b10-a6b1-a30b66c4e020";
    let path = task_output_path(id, Path::new("/work/app"), "sess-1");
    // Assert
    assert_eq!(path.file_name().unwrap(), "fef1adc2.output");
    assert_eq!(path.parent().unwrap().file_name().unwrap(), "tasks");
    assert_eq!(
        path.ancestors().nth(2).unwrap().file_name().unwrap(),
        "sess-1"
    );
    assert_eq!(
        path.ancestors().nth(4).unwrap().file_name().unwrap(),
        format!("cc-code-{}", user_tag()).as_str()
    );
}

#[tokio::test]
async fn test_reserve_task_output_hash_collision_concurrently_keeps_output_separate() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("tasks");
    let left_id = "01a18645-2f37-7b10-a6b1-0000000036d3";
    let right_id = "01a18645-2f37-7b10-a6b1-000000008291";
    assert_eq!(short_task_hash(left_id, 0), "e2413b7c");
    assert_eq!(short_task_hash(left_id, 0), short_task_hash(right_id, 0));
    let barrier = std::sync::Barrier::new(2);
    // Act
    let (left_path, right_path) = std::thread::scope(|scope| {
        let left = scope.spawn(|| {
            barrier.wait();
            reserve_output_in_dir(left_id, &dir)
        });
        let right = scope.spawn(|| {
            barrier.wait();
            reserve_output_in_dir(right_id, &dir)
        });
        (left.join().unwrap(), right.join().unwrap())
    });
    let (left_tx, left_rx) = mpsc::channel(1);
    let (right_tx, right_rx) = mpsc::channel(1);
    let left_writer = DiskOutput::spawn_writer(left_path.clone(), left_rx);
    let right_writer = DiskOutput::spawn_writer(right_path.clone(), right_rx);
    left_tx.send(b"left".to_vec()).await.unwrap();
    right_tx.send(b"right".to_vec()).await.unwrap();
    drop((left_tx, right_tx));
    left_writer.await.unwrap();
    right_writer.await.unwrap();
    // Assert
    assert_ne!(left_path, right_path, "hash 碰撞后必须使用不同文件");
    assert_eq!(left_path.file_stem().unwrap().len(), 8);
    assert_eq!(right_path.file_stem().unwrap().len(), 8);
    assert_eq!(
        DiskOutput::read_tail(&left_path, 64).await.unwrap(),
        b"left"
    );
    assert_eq!(
        DiskOutput::read_tail(&right_path, 64).await.unwrap(),
        b"right"
    );
}

#[test]
fn test_reserve_task_output_preserves_existing_file() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let id = "01a18645-2f37-7b10-a6b1-a30b66c4e020";
    let occupied = make_path(tmp.path(), &short_task_hash(id, 0));
    std::fs::write(&occupied, b"previous output").unwrap();
    // Act
    let reserved = reserve_output_in_dir(id, tmp.path());
    // Assert
    assert_ne!(reserved, occupied);
    assert_eq!(reserved.file_stem().unwrap().len(), 8);
    assert_eq!(std::fs::read(occupied).unwrap(), b"previous output");
    assert!(reserved.is_file());
}

#[test]
fn test_reserve_task_output_uses_full_id_when_directory_creation_fails() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let blocked = tmp.path().join("not-a-directory");
    std::fs::write(&blocked, b"occupied").unwrap();
    let id = "01a18645-2f37-7b10-a6b1-a30b66c4e020";
    // Act
    let reserved = reserve_output_in_dir(id, &blocked);
    // Assert
    assert_eq!(reserved, make_path(&blocked, id));
    assert_eq!(std::fs::read(blocked).unwrap(), b"occupied");
}

#[test]
fn test_reserve_task_output_uses_unique_full_id_after_repeated_hash_collisions() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let id = "01a18645-2f37-7b10-a6b1-a30b66c4e020";
    for attempt in 0..32 {
        std::fs::write(
            make_path(tmp.path(), &short_task_hash(id, attempt)),
            b"occupied",
        )
        .unwrap();
    }
    let occupied_full = make_path(tmp.path(), id);
    std::fs::write(&occupied_full, b"previous output").unwrap();
    // Act
    let reserved = reserve_output_in_dir(id, tmp.path());
    // Assert
    assert_eq!(reserved, make_path(tmp.path(), &format!("{id}-1")));
    assert_eq!(std::fs::read(occupied_full).unwrap(), b"previous output");
    assert!(reserved.is_file());
}
