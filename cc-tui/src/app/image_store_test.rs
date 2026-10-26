use super::{path_for, store_image_in, temp_image_dir};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use uuid::Uuid;

#[test]
fn test_temp_image_dir_uses_system_temporary_directory() {
    assert_eq!(
        temp_image_dir(),
        std::env::temp_dir().join("cc-code-images"),
        "图片根目录必须位于系统临时目录，不依赖用户配置"
    );
}

#[test]
fn test_store_image_writes_decoded_bytes_to_session_directory() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    let bytes = b"\x89PNG\r\n\x1a\n\x00\xff";
    let path = store_image_in(
        &image_root,
        session_id,
        1,
        "image/png",
        &STANDARD.encode(bytes),
    )
    .expect("图片应成功落盘");
    assert_eq!(
        path,
        image_root.join(session_id.to_string()).join("1.png"),
        "路径必须包含会话 UUID 和图片编号"
    );
    assert_eq!(
        std::fs::read(&path).expect("返回时图片应已完整写入"),
        bytes,
        "落盘字节必须与 base64 解码结果完全一致"
    );
}

#[test]
fn test_store_image_keeps_different_identifiers_separate() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    let first = store_image_in(&image_root, session_id, 1, "image/png", "Zmlyc3Q=")
        .expect("写入第一张图片");
    let second = store_image_in(&image_root, session_id, 2, "image/png", "c2Vjb25k")
        .expect("写入第二张图片");
    assert!(first != second, "不同图片编号必须使用不同路径");
    assert_eq!(
        std::fs::read(first).expect("读取第一张图片"),
        b"first",
        "第二张图片不能覆盖第一张图片"
    );
    assert_eq!(
        std::fs::read(second).expect("读取第二张图片"),
        b"second",
        "第二张图片必须保留自己的内容"
    );
}

#[test]
fn test_store_image_keeps_different_sessions_separate() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let first = store_image_in(&image_root, Uuid::from_u128(1), 1, "image/png", "Zmlyc3Q=")
        .expect("写入第一个会话的图片");
    let second = store_image_in(&image_root, Uuid::from_u128(2), 1, "image/png", "c2Vjb25k")
        .expect("写入第二个会话的图片");
    assert!(first != second, "不同会话中的相同编号不能共用路径");
    assert_eq!(
        std::fs::read(first).expect("读取第一个会话的图片"),
        b"first",
        "其他会话不能覆盖已存图片"
    );
    assert_eq!(
        std::fs::read(second).expect("读取第二个会话的图片"),
        b"second",
        "每个会话应保存自己的图片"
    );
}

#[test]
fn test_store_image_repeated_write_is_idempotent() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    let first =
        store_image_in(&image_root, session_id, 1, "image/png", "aW1hZ2U=").expect("首次写入图片");
    let second =
        store_image_in(&image_root, session_id, 1, "image/png", "aW1hZ2U=").expect("重复写入图片");
    assert_eq!(first, second, "同一编号重复写入必须返回相同路径");
    assert_eq!(
        std::fs::read(second).expect("读取重复写入后的图片"),
        b"image",
        "重复写入不能追加或改变图片内容"
    );
    assert_eq!(
        std::fs::read_dir(image_root.join(session_id.to_string()))
            .expect("读取会话图片目录")
            .count(),
        1,
        "重复写入不能产生额外文件"
    );
}

#[test]
fn test_store_image_replacement_truncates_previous_contents() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    store_image_in(&image_root, session_id, 1, "image/png", "bG9uZyBpbWFnZQ==")
        .expect("写入较长的旧图片");
    let path = store_image_in(&image_root, session_id, 1, "image/png", "AA==")
        .expect("同一编号写入较短的新图片");
    assert_eq!(
        std::fs::read(path).expect("读取替换后的图片"),
        b"\x00",
        "替换图片必须截断旧内容，不能残留尾部字节"
    );
}

#[test]
fn test_store_image_maps_mime_types_to_fixed_extensions() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    for (image_id, (media_type, extension)) in [
        ("image/png", "png"),
        ("image/jpeg", "jpg"),
        ("image/jpg", "jpg"),
        ("image/webp", "webp"),
        ("image/gif", "gif"),
        ("image/bmp", "bmp"),
        ("application/octet-stream", "png"),
        ("image/../../outside", "png"),
        ("", "png"),
    ]
    .into_iter()
    .enumerate()
    {
        let path = store_image_in(&image_root, session_id, image_id, media_type, "aW1hZ2U=")
            .expect("各 MIME 类型应映射到固定扩展名");
        assert_eq!(
            path,
            image_root
                .join(session_id.to_string())
                .join(format!("{image_id}.{extension}")),
            "MIME 类型 {media_type} 只能影响白名单扩展名，未知类型默认 PNG"
        );
    }
}

#[test]
fn test_store_image_invalid_base64_creates_no_file() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    let result = store_image_in(&image_root, session_id, 1, "image/png", "aW1hZ2U=!");
    assert!(result.is_err(), "无效 base64 必须返回错误");
    assert!(
        !path_for(&image_root, session_id, 1, "image/png").exists(),
        "解码失败不能产生空图片文件"
    );
    assert!(!image_root.exists(), "解码失败不应创建图片目录");
}

#[test]
fn test_store_image_invalid_base64_preserves_existing_file() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    let path =
        store_image_in(&image_root, session_id, 1, "image/png", "aW1hZ2U=").expect("写入有效图片");
    let result = store_image_in(&image_root, session_id, 1, "image/png", "invalid base64!");
    assert!(result.is_err(), "无效数据覆盖已有编号时必须返回错误");
    assert_eq!(
        std::fs::read(path).expect("读取已有图片"),
        b"image",
        "解码失败不能截断或改写已有图片"
    );
}

#[test]
fn test_store_image_directory_creation_failure_returns_error() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    std::fs::write(&image_root, b"blocked").expect("用文件阻止图片目录创建");
    let result = store_image_in(&image_root, Uuid::from_u128(1), 1, "image/png", "aW1hZ2U=");
    assert!(result.is_err(), "目录创建失败必须返回错误");
    assert_eq!(
        std::fs::read(image_root).expect("读取阻挡目录创建的文件"),
        b"blocked",
        "目录创建失败不能覆盖已有文件"
    );
}

#[test]
fn test_store_image_file_write_failure_returns_error() {
    let directory = tempfile::tempdir().expect("创建隔离临时目录");
    let image_root = directory.path().join("cc-code-images");
    let session_id = Uuid::from_u128(1);
    let path = path_for(&image_root, session_id, 1, "image/png");
    std::fs::create_dir_all(&path).expect("用目录占据目标图片路径");
    let result = store_image_in(&image_root, session_id, 1, "image/png", "aW1hZ2U=");
    assert!(result.is_err(), "图片文件写入失败必须返回错误");
    assert!(path.is_dir(), "写入失败不能改变已有目录");
}
