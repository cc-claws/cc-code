//! Tracing subscriber 初始化（基础日志输出）

use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};

pub struct TracingGuard;

impl Drop for TracingGuard {
    fn drop(&mut self) {
        // 无需特殊清理逻辑
    }
}

/// 计算默认日志文件路径：`~/.cc-code/logs/{service_name}.log`
///
/// 跨平台统一写入用户主目录下的固定位置，避免被系统临时目录清理导致日志丢失。
pub fn default_log_path(service_name: &str) -> std::path::PathBuf {
    let home = dirs_next::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".cc-code")
        .join("logs")
        .join(format!("{service_name}.log"))
}

/// Windows 下日志文件为空时写入 UTF-8 BOM，避免 PowerShell Get-Content 乱码。
///
/// 失败静默忽略——BOM 只是显示优化，不应导致启动失败。
#[cfg(target_os = "windows")]
fn ensure_utf8_bom(path: &str) {
    use std::io::Write;
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() == 0 {
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path) {
            let _ = f.write_all(b"\xEF\xBB\xBF");
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_utf8_bom(_path: &str) {}

/// 解析日志输出 writer：优先打开文件（append 模式），失败**退回 stderr 而非 panic**。
///
/// 日志不可用（如日志文件 ACL 损坏导致 `PermissionDenied`）时不应阻断程序启动。
fn resolve_log_writer(log_path: &str) -> BoxMakeWriter {
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        Ok(file) => {
            // Windows: 写入 UTF-8 BOM（文件为空时），避免 PowerShell Get-Content 乱码
            ensure_utf8_bom(log_path);
            BoxMakeWriter::new(file)
        }
        Err(e) => {
            eprintln!(
                "warning: 无法打开日志文件 {log_path}: {e}；日志改输出到 stderr（程序继续运行）"
            );
            BoxMakeWriter::new(std::io::stderr)
        }
    }
}

/// 初始化 tracing，输出到日志文件（TUI 模式下避免干扰界面）
///
/// 日志路径优先级：
/// 1. `RUST_LOG_FILE` 环境变量指定的路径
/// 2. 默认 `~/.cc-code/logs/{service_name}.log`（目录不存在时自动创建）
pub fn init_tracing(service_name: &str) -> TracingGuard {
    // 根据 RUST_LOG_FORMAT 环境变量决定输出格式
    let is_json = std::env::var("RUST_LOG_FORMAT").as_deref() == Ok("json");

    // 检查是否配置了日志文件
    let log_file = std::env::var("RUST_LOG_FILE").ok();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // 默认 info 级别，但 MCP 和插件模块设为 warn（避免连接日志干扰）
        EnvFilter::new("info,cc_middlewares::mcp=warn,cc_middlewares::plugin=warn,rmcp=warn")
    });

    // 计算日志文件路径：RUST_LOG_FILE 优先，否则用 ~/.cc-code/logs/ 默认路径
    let log_path = match log_file {
        Some(path) => path,
        None => {
            let default_path = default_log_path(service_name);
            if let Some(dir) = default_path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            default_path.to_string_lossy().to_string()
        }
    };

    // 打开日志文件（append 模式，多实例/多次运行追加不覆盖）。
    //
    // **打不开不 panic**：日志无法写入不应让整个程序崩溃——例如日志文件 ACL 损坏时，
    // 新进程追加打开会得到 `PermissionDenied`，旧代码用 `.expect()` 会直接 panic 掉
    // 整个 TUI（表现为启动即崩、`cc-code` 完全不可用）。失败时退回 stderr，进程继续运行。
    let writer = resolve_log_writer(&log_path);

    // `set_global_default` 在 subscriber 已设置时会返回 Err（如重复初始化）；
    // 同样不应 panic，降级为提示。
    let set_err = if is_json {
        tracing::subscriber::set_global_default(
            Registry::default()
                .with(filter)
                .with(fmt::layer().json().with_writer(writer)),
        )
        .err()
    } else {
        tracing::subscriber::set_global_default(
            Registry::default()
                .with(filter)
                .with(fmt::layer().with_writer(writer).with_ansi(false)),
        )
        .err()
    };
    if let Some(e) = set_err {
        eprintln!("warning: 全局 tracing subscriber 已设置，本次文件日志层未生效：{e}");
    }

    TracingGuard
}

#[cfg(test)]
#[path = "subscriber_test.rs"]
mod subscriber_test;
