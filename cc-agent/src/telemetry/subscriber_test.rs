//! subscriber 默认日志路径测试

use super::default_log_path;

#[test]
fn test_default_log_path_uses_cc_code_logs_dir() {
    // Act
    let path = default_log_path("agent-tui");
    // Assert: 路径结构为 ~/.cc-code/logs/{service_name}.log
    let file_name = path.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(
        file_name, "agent-tui.log",
        "日志文件名应为 <service_name>.log"
    );
    let logs_dir = path.parent().unwrap();
    assert_eq!(
        logs_dir.file_name().unwrap().to_string_lossy(),
        "logs",
        "父目录应为 logs"
    );
    let cc_code_dir = logs_dir.parent().unwrap();
    assert_eq!(
        cc_code_dir.file_name().unwrap().to_string_lossy(),
        ".cc-code",
        "上级目录应为 .cc-code"
    );
}

#[test]
fn test_default_log_path_varies_by_service_name() {
    // Act
    let tui = default_log_path("agent-tui");
    let print = default_log_path("peri-print");
    let acp = default_log_path("cc-acp");
    // Assert: 三种运行模式的日志路径互不相同
    assert_ne!(tui, print, "不同 service_name 的日志路径应不同");
    assert_ne!(print, acp, "不同 service_name 的日志路径应不同");
    assert_eq!(
        print.file_name().unwrap().to_string_lossy(),
        "peri-print.log"
    );
    assert_eq!(acp.file_name().unwrap().to_string_lossy(), "cc-acp.log");
}

#[test]
fn test_init_tracing_creates_default_log_file() {
    // Arrange: 外部已设置 RUST_LOG_FILE 时本测试无意义（全局 subscriber 会被写到外部路径），跳过
    if std::env::var("RUST_LOG_FILE").is_ok() {
        return;
    }
    let path = default_log_path("init-tracing-test");
    let _ = std::fs::remove_file(&path);
    // Act: 初始化 tracing（全局 subscriber 进程内仅允许一次，本测试为唯一调用点）
    let _guard = super::init_tracing("init-tracing-test");
    // Assert: 默认日志目录与文件自动创建
    assert!(path.exists(), "默认日志文件应自动创建: {:?}", path);
    // 清理测试产物
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_resolve_log_writer_bad_path_falls_back_without_panic() {
    // Arrange: 拿一个「目录」当日志文件路径 → 打开必失败（Windows: 拒绝访问 / IsADirectory）
    let dir = std::env::temp_dir().join("cc-code-log-writer-probe-dir");
    std::fs::create_dir_all(&dir).unwrap();
    // Act: 旧实现用 `.expect("cannot open log file")` 在此 panic；修复后应退回 stderr、不崩
    let _writer = super::resolve_log_writer(&dir.to_string_lossy());
    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}
