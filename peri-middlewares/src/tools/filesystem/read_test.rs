    #[tokio::test]
    async fn test_read_file_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        std::fs::write(&path, "hello\nworld").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "file.txt"}))
            .await
            .unwrap();
        assert!(
            result.contains("1\thello"),
            "should contain line 1: {result}"
        );
        assert!(
            result.contains("2\tworld"),
            "should contain line 2: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "nonexistent.txt"}))
            .await
            .unwrap();
        assert!(
            result.contains("File not found"),
            "should report not found: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_offset_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lines.txt");
        std::fs::write(&path, "L1\nL2\nL3\nL4\nL5").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "lines.txt", "offset": 2, "limit": 2}))
            .await
            .unwrap();
        // offset=2 → starts at index 2 (L3), limit=2 → L3 and L4
        assert!(result.contains("3\tL3"), "should contain line 3: {result}");
        assert!(result.contains("4\tL4"), "should contain line 4: {result}");
        assert!(!result.contains("L1"), "should not contain L1");
        assert!(!result.contains("L5"), "should not contain L5");
    }

    #[tokio::test]
    async fn test_read_file_binary_extension() {
        let dir = tempfile::tempdir().unwrap();
        // Binary extension check happens before file read, no need to create the file
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "image.png"}))
            .await
            .unwrap();
        assert!(
            result.contains("BINARY FILE DETECTED"),
            "should detect binary: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abs.txt");
        std::fs::write(&path, "absolute").unwrap();
        let tool = ReadFileTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"file_path": path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(
            result.contains("absolute"),
            "should read via absolute path: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_offset_exceeds_length() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("short.txt"), "one\ntwo").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "short.txt", "offset": 999}))
            .await
            .unwrap();
        assert!(
            result.contains("exceeds file length"),
            "offset 超出文件长度应返回错误而非 panic: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_file_too_large() {
        let dir = tempfile::tempdir().unwrap();
        // 创建一个超过 MAX_FILE_SIZE 的稀疏文件
        let large_path = dir.path().join("huge.txt");
        let f = std::fs::File::create(&large_path).unwrap();
        f.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(f);
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "huge.txt"}))
            .await
            .unwrap();
        assert!(
            result.contains("File too large"),
            "超大文件应返回 File too large 错误: {result}"
        );
    }

    #[test]
    fn test_description_extended() {
        let tool = ReadFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(
            desc.contains("Error handling:"),
            "description 应包含 Error handling 段落"
        );
        assert!(desc.contains("line numbers"), "description 应提及行号格式");
        assert!(
            desc.len() > 200,
            "description 应为扩展后的多段落文本，长度 > 200 字符"
        );
    }

    #[test]
    #[allow(non_snake_case)]
    fn test_tool_name_is_Read() {
        let tool = ReadFileTool::new("/tmp");
        assert_eq!(tool.name(), "Read");
    }

    #[tokio::test]
    async fn test_pdf_with_pages_returns_placeholder() {
        let tool = ReadFileTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"file_path": "test.pdf", "pages": "1-5"}))
            .await
            .unwrap();
        assert!(
            result.contains("PDF READING NOT YET SUPPORTED"),
            "should return placeholder: {result}"
        );
    }

    #[tokio::test]
    async fn test_pdf_without_pages_returns_binary() {
        let tool = ReadFileTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"file_path": "test.pdf"}))
            .await
            .unwrap();
        assert!(
            result.contains("BINARY FILE DETECTED"),
            "should return binary: {result}"
        );
    }

    #[tokio::test]
    async fn test_read_crlf_file_strips_cr() {
        // CRLF 文件输出不应包含 \r
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("crlf.txt"), "line1\r\nline2\r\nline3\r\n").unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": "crlf.txt"}))
            .await
            .unwrap();
        assert!(!result.contains('\r'), "CRLF 文件输出不应包含 \\r: {result}");
        assert!(result.contains("1\tline1"), "应包含 line1: {result}");
        assert!(result.contains("2\tline2"), "应包含 line2: {result}");
    }

    // ─── 图片多模态读取测试 ──────────────────────────────────────────────

    /// 最小合法 PNG 文件（1x1 透明像素）
    fn minimal_png() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, // RGBA
            0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
            0x78, 0x9C, 0x62, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0xE5,
            0x27, 0xDE, 0xFC,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND
            0xAE, 0x42, 0x60, 0x82,
        ]
    }

    #[tokio::test]
    async fn test_invoke_content_image_png() {
        // invoke_content 读取 PNG 图片应返回包含 Image block 的 ToolContent
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.png");
        std::fs::write(&img_path, minimal_png()).unwrap();

        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke_content(serde_json::json!({"file_path": img_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(
            result.output.contains("image/png"),
            "output 摘要应包含 media type: {}",
            result.output
        );
        assert!(
            result.content.is_some(),
            "图片文件应返回结构化 content"
        );
    }

    #[tokio::test]
    async fn test_invoke_content_image_jpg() {
        // invoke_content 读取 JPEG 图片应返回包含 Image block 的 ToolContent
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("photo.jpg");
        // 最小合法 JPEG（SOI + EOI markers）
        std::fs::write(&img_path, [0xFF, 0xD8, 0xFF, 0xD9]).unwrap();

        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke_content(serde_json::json!({"file_path": img_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(
            result.output.contains("image/jpeg"),
            "output 摘要应包含 image/jpeg: {}",
            result.output
        );
        assert!(result.content.is_some(), "JPEG 应返回结构化 content");
    }

    #[tokio::test]
    async fn test_invoke_content_text_file_no_content() {
        // invoke_content 读取文本文件应返回 content=None（纯文本路径）
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke_content(serde_json::json!({"file_path": "hello.txt"}))
            .await
            .unwrap();

        assert!(
            result.content.is_none(),
            "文本文件不应返回结构化 content"
        );
        assert!(
            result.output.contains("hello world"),
            "文本文件的 output 应包含文件内容: {}",
            result.output
        );
    }

    #[tokio::test]
    async fn test_invoke_image_fallback_binary_detected() {
        // invoke（非 invoke_content）读取图片仍返回 BINARY FILE DETECTED
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.png");
        std::fs::write(&img_path, minimal_png()).unwrap();

        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"file_path": img_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(
            result.contains("BINARY FILE DETECTED"),
            "invoke 读取图片应回退为 BINARY FILE DETECTED: {result}"
        );
    }

    #[tokio::test]
    async fn test_invoke_content_image_not_found() {
        // invoke_content 读取不存在的图片应返回 File not found
        let dir = tempfile::tempdir().unwrap();
        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke_content(serde_json::json!({"file_path": "nonexistent.png"}))
            .await
            .unwrap();

        assert!(
            result.output.contains("File not found"),
            "不存在的图片文件应返回 File not found: {}",
            result.output
        );
        assert!(
            result.content.is_none(),
            "不存在的图片不应有结构化 content"
        );
    }

    #[tokio::test]
    async fn test_invoke_content_image_too_large() {
        // 超过 MAX_IMAGE_SIZE 的图片应返回错误
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("huge.png");
        let f = std::fs::File::create(&img_path).unwrap();
        f.set_len(MAX_IMAGE_SIZE + 1).unwrap();
        drop(f);

        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke_content(serde_json::json!({"file_path": img_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(
            result.output.contains("Image too large"),
            "超大图片应返回 Image too large: {}",
            result.output
        );
        assert!(
            result.content.is_none(),
            "超大图片不应有结构化 content"
        );
    }

    #[tokio::test]
    async fn test_invoke_content_webp_image() {
        // invoke_content 支持 WebP 图片
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.webp");
        // 最小 WebP 头：RIFF + WEBP
        std::fs::write(&img_path, b"RIFF\x00\x00\x00\x00WEBP").unwrap();

        let tool = ReadFileTool::new(dir.path().to_str().unwrap());
        let result = tool
            .invoke_content(serde_json::json!({"file_path": img_path.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(
            result.output.contains("image/webp"),
            "WebP 图片的 output 应包含 image/webp: {}",
            result.output
        );
        assert!(result.content.is_some(), "WebP 应返回结构化 content");
    }

    #[tokio::test]
    async fn test_ico_tiff_still_binary() {
        // ico 和 tiff 不在多模态支持范围内，仍走二进制检测
        let tool = ReadFileTool::new("/tmp");
        for ext in &["ico", "tiff"] {
            let result = tool
                .invoke(serde_json::json!({"file_path": format!("test.{ext}")}))
                .await
                .unwrap();
            assert!(
                result.contains("BINARY FILE DETECTED"),
                "{ext} 文件应返回 BINARY FILE DETECTED: {result}"
            );
        }
    }