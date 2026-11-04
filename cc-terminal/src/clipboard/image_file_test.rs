use super::*;
use base64::Engine as _;

fn make_image(path: &Path, format: image::ImageFormat) {
    let image = image::RgbImage::from_pixel(2, 3, image::Rgb([200, 100, 50]));
    image.save_with_format(path, format).expect("构造图片文件");
}

#[test]
fn test_image_file_common_formats_encode_real_png() {
    let dir = tempfile::tempdir().unwrap();
    for (extension, format) in [
        ("PNG", image::ImageFormat::Png),
        ("jpg", image::ImageFormat::Jpeg),
        ("jpeg", image::ImageFormat::Jpeg),
        ("webp", image::ImageFormat::WebP),
        ("gif", image::ImageFormat::Gif),
        ("bmp", image::ImageFormat::Bmp),
    ] {
        let path = dir.path().join(format!("中文 图片.{extension}"));
        make_image(&path, format);
        assert!(is_image_path(&path));
        let (base64, size, width, height) = image_file_as_png_base64(&path).expect("图片应可读取");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64)
            .expect("应为有效 base64");
        assert_eq!(bytes.len(), size);
        assert_eq!((width, height), (2, 3));
        assert_eq!(
            image::guess_format(&bytes).expect("应能识别图片"),
            image::ImageFormat::Png
        );
        assert!(
            image::load_from_memory(&bytes).is_ok(),
            "附件必须包含可解码图片，而不是路径文字"
        );
    }
}

#[test]
fn test_image_file_grayscale_png_supported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("灰度.png");
    image::GrayImage::from_pixel(2, 2, image::Luma([127]))
        .save(&path)
        .expect("构造灰度 PNG");
    assert!(
        image_file_as_png_base64(&path).is_ok(),
        "灰度 PNG 不应被旧 RGB/RGBA 限制拒绝"
    );
}

#[test]
fn test_image_file_rejects_missing_directory_corrupt_and_oversized() {
    let dir = tempfile::tempdir().unwrap();
    assert!(image_file_as_png_base64(dir.path()).is_err());
    assert!(image_file_as_png_base64(&dir.path().join("missing.png")).is_err());
    let corrupt = dir.path().join("corrupt.jpg");
    std::fs::write(&corrupt, b"not an image").expect("构造坏图片");
    assert!(image_file_as_png_base64(&corrupt).is_err());
    let large = dir.path().join("large.png");
    std::fs::File::create(&large)
        .expect("构造超大文件")
        .set_len(MAX_FILE_BYTES + 1)
        .expect("设置文件大小");
    assert!(image_file_as_png_base64(&large).is_err());
    assert!(!is_image_path(Path::new("notes.txt")));
}

#[test]
fn test_image_file_decodes_content_not_extension() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("图片.jpg");
    make_image(&path, image::ImageFormat::Png);
    assert!(
        image_file_as_png_base64(&path).is_ok(),
        "解码格式应由文件内容决定"
    );
}
