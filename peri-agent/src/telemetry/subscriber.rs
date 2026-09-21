//! Tracing subscriber 初始化（基础日志输出）

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

/// Windows 下日志文件为空时写入 UTF-8 BOM，避免 PowerShell Get-Content 乱码
#[cfg(target_os = "windows")]
fn ensure_utf8_bom(path: &str) {
    let meta = std::fs::metadata(path)
        .unwrap_or_else(|_| std::fs::metadata(path).expect("cannot read log file metadata"));
    if meta.len() == 0 {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .expect("cannot open log file for BOM");
        let _ = f.write_all(b"\xEF\xBB\xBF");
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_utf8_bom(_path: &str) {}

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
        EnvFilter::new("info,peri_middlewares::mcp=warn,peri_middlewares::plugin=warn,rmcp=warn")
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

    // 输出到日志文件（append 模式，多实例/多次运行追加不覆盖）
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("cannot open log file");

    // Windows: 写入 UTF-8 BOM（文件为空时），避免 PowerShell Get-Content 乱码
    ensure_utf8_bom(&log_path);

    if is_json {
        let subscriber = Registry::default()
            .with(filter)
            .with(fmt::layer().json().with_writer(file));
        tracing::subscriber::set_global_default(subscriber)
            .expect("Unable to set global subscriber");
    } else {
        let subscriber = Registry::default()
            .with(filter)
            .with(fmt::layer().with_writer(file).with_ansi(false));
        tracing::subscriber::set_global_default(subscriber)
            .expect("Unable to set global subscriber");
    }

    TracingGuard
}

#[cfg(test)]
#[path = "subscriber_test.rs"]
mod subscriber_test;
