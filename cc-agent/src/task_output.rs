//! 后台任务磁盘输出存储
//!
//! 为 Ctrl+B 后台 Shell 提供 stdout/stderr 的磁盘持久化：流式输出 channel
//! 的每个 chunk 追加写入文件，支持末尾读取（tail）和增量读取（delta），
//! 供 BackgroundTasksPanel 详情视图渲染。
//!
//! 输出路径：`{temp_dir}/cc-code-{user}/{sanitize(cwd)}/{session_id}/tasks/{hash}.output`
//!
//! 设计说明：文档原方案是 DiskOutput 内部维护 queue + 批量 drain。此处采用
//! append 直写 + 独立 writer task 的简化方案：mpsc channel 本身已是队列，
//! 每个 chunk 一次 append 写入，正确性更易保证，且后台 shell 输出频率不高，
//! 性能足够。如后续出现高频小写入场景可再加内部缓冲。

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// 单个后台任务输出文件的最大字节数（5 GB），超过后停止写入避免撑爆磁盘
pub const MAX_OUTPUT_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// 单次读取（tail/delta）的最大字节数（8 MB），避免一次性读入过大内容
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// 计算后台任务磁盘输出文件的首选路径，不创建文件。
///
/// 目录层级：`{temp_dir}/cc-code-{user}/{sanitize(cwd)}/{session_id}/tasks/{hash}.output`
/// - `user`：当前系统用户标识（`USER`/`USERNAME`），用于多用户隔离
/// - `sanitize(cwd)`：把工作目录路径转成安全的单层目录名
/// - `session_id`：会话隔离
/// - `hash`：完整任务 ID 的 SHA-256 前 8 位；写入前必须使用
///   [`reserve_task_output_path`] 预占路径并处理碰撞。
pub fn task_output_path(id: &str, cwd: &Path, session_id: &str) -> PathBuf {
    task_output_dir(cwd, session_id).join(format!("{}.output", short_task_hash(id, 0)))
}

fn task_output_dir(cwd: &Path, session_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("cc-code-{}", user_tag()))
        .join(sanitize_path_segment(cwd))
        .join(session_id)
        .join("tasks")
}

fn short_task_hash(id: &str, attempt: u32) -> String {
    let mut digest = Sha256::new();
    digest.update(id.as_bytes());
    if attempt > 0 {
        digest.update([0]);
        digest.update(attempt.to_be_bytes());
    }
    let hash = digest.finalize();
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3]
    )
}

/// 原子预占输出路径；短 hash 撞名时重新计算，避免不同任务追加到同一文件。
///
/// 预占发生 I/O 错误时回退到完整任务 ID 路径，让原有写入器继续重试。
pub fn reserve_task_output_path(id: &str, cwd: &Path, session_id: &str) -> PathBuf {
    reserve_output_in_dir(id, &task_output_dir(cwd, session_id))
}

fn reserve_output_in_dir(id: &str, dir: &Path) -> PathBuf {
    let fallback = dir.join(format!("{}.output", id));
    if let Err(error) = std::fs::create_dir_all(dir) {
        tracing::warn!(error = %error, path = %dir.display(), "预占任务输出目录失败，回退完整任务 ID");
        return fallback;
    }

    // 32 次碰撞后使用完整 ID，避免目录异常时无限循环。
    for attempt in 0..32 {
        let path = dir.join(format!("{}.output", short_task_hash(id, attempt)));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return path,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                tracing::warn!(error = %error, path = %path.display(), "预占任务输出文件失败，回退完整任务 ID");
                return fallback;
            }
        }
    }

    tracing::warn!(path = %dir.display(), "任务输出短 hash 多次碰撞，回退完整任务 ID");
    for attempt in 0u64.. {
        let path = if attempt == 0 {
            fallback.clone()
        } else {
            dir.join(format!("{}-{}.output", id, attempt))
        };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return path,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                tracing::warn!(error = %error, path = %path.display(), "预占完整任务 ID 路径失败，交由写入器重试");
                return path;
            }
        }
    }
    unreachable!("完整任务 ID 路径重试次数耗尽")
}

/// 把任意路径转成安全的单层目录名：替换文件系统非法字符为 `_`，限制长度。
fn sanitize_path_segment(path: &Path) -> String {
    let raw: String = path
        .to_string_lossy()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect();
    let trimmed = raw.trim_matches(|c: char| c == '_' || c.is_whitespace());
    if trimmed.is_empty() {
        "root".to_string()
    } else {
        trimmed.chars().take(64).collect()
    }
}

/// 当前系统用户标识，用于输出目录的多用户隔离。
fn user_tag() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "shared".to_string())
}

/// 后台任务磁盘输出：管理输出文件的写入和读取。
///
/// 写入通过 [`DiskOutput::spawn_writer`] 启动独立 tokio task，从 mpsc channel
/// 消费 chunk 追加写入文件；读取通过 [`DiskOutput::read_tail`] /
/// [`DiskOutput::read_delta`] 直接读取磁盘文件。
pub struct DiskOutput {
    path: PathBuf,
}

impl DiskOutput {
    /// 根据输出文件路径创建句柄。
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 输出文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 启动写入 task：消费 `rx` 中的每个 chunk，追加写入 `path` 文件。
    ///
    /// - 自动创建父目录
    /// - 累计写入达到 [`MAX_OUTPUT_BYTES`] 后停止消费（后续 chunk 丢弃）
    /// - `rx` 关闭（所有 sender drop）后 flush 并退出
    ///
    /// 返回 JoinHandle，调用方可用于等待写入完成或 abort。
    pub fn spawn_writer(path: PathBuf, mut rx: mpsc::Receiver<Vec<u8>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            // Windows CI runner 文件系统偶发瞬时失败（建目录/建文件返回瞬时错误，
            // 与 #323 同一家族问题）：重试几次再放弃。否则 writer 静默退出、
            // 文件不存在，而 exit_signal 照常触发，读盘侧撞上 NotFound flake。
            let mut file = None;
            for attempt in 0..5u32 {
                let opened = async {
                    if let Some(parent) = path.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                        .await
                }
                .await;
                match opened {
                    Ok(f) => {
                        file = Some(f);
                        break;
                    }
                    Err(e) => {
                        tracing::debug!(
                            error = %e, path = %path.display(), attempt,
                            "创建后台任务输出文件失败，重试"
                        );
                        if attempt < 4 {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                100 * (attempt + 1) as u64,
                            ))
                            .await;
                        }
                    }
                }
            }
            let mut file = match file {
                Some(f) => f,
                None => {
                    tracing::warn!(
                        path = %path.display(),
                        "多次重试后仍无法创建后台任务输出文件"
                    );
                    // 排空 rx 避免 sender 阻塞
                    while rx.recv().await.is_some() {}
                    return;
                }
            };

            let mut total: u64 = 0;
            while let Some(chunk) = rx.recv().await {
                if total >= MAX_OUTPUT_BYTES {
                    // 写满后排空 rx（丢弃 chunk），防 reader send 阻塞导致进程 stdout 管道僵死
                    continue;
                }
                let remaining = (MAX_OUTPUT_BYTES - total) as usize;
                let to_write: &[u8] = if chunk.len() <= remaining {
                    &chunk
                } else {
                    &chunk[..remaining]
                };
                if file.write_all(to_write).await.is_err() {
                    // 写错误后排空 rx（丢弃），防 reader send 阻塞
                    continue;
                }
                total += to_write.len() as u64;
            }
            let _ = file.flush().await;
        })
    }

    /// 读取文件末尾最多 `bytes` 字节（受 [`MAX_READ_BYTES`] 上限约束）。
    ///
    /// 从 `max(0, size - bytes)` 处读到真正的 EOF，不依赖预计算的文件大小，
    /// 避免与并发写入 task 的竞争。
    pub async fn read_tail(path: &Path, bytes: u64) -> Result<Vec<u8>> {
        let mut file = tokio::fs::File::open(path)
            .await
            .with_context(|| format!("打开后台任务输出文件失败: {}", path.display()))?;
        let meta = file.metadata().await.context("读取输出文件元数据失败")?;
        let size = meta.len();
        let start = size.saturating_sub(bytes.min(MAX_READ_BYTES));
        file.seek(std::io::SeekFrom::Start(start))
            .await
            .context("seek 输出文件失败")?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .await
            .context("读取输出文件末尾失败")?;
        Ok(buf)
    }

    /// 从 `from_offset` 读取到 EOF（最多 [`MAX_READ_BYTES`] 字节）。
    ///
    /// 用于详情视图的增量刷新：记录上次读取到的 offset，下次只读新字节。
    pub async fn read_delta(path: &Path, from_offset: u64) -> Result<Vec<u8>> {
        let mut file = tokio::fs::File::open(path)
            .await
            .with_context(|| format!("打开后台任务输出文件失败: {}", path.display()))?;
        let meta = file.metadata().await.context("读取输出文件元数据失败")?;
        let size = meta.len();
        if from_offset >= size {
            return Ok(Vec::new());
        }
        file.seek(std::io::SeekFrom::Start(from_offset))
            .await
            .context("seek 输出文件失败")?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .await
            .context("读取输出文件增量失败")?;
        if buf.len() as u64 > MAX_READ_BYTES {
            buf.truncate(MAX_READ_BYTES as usize);
        }
        Ok(buf)
    }

    /// 删除输出文件。文件不存在视为成功（幂等）。
    pub async fn cleanup(path: &Path) -> Result<()> {
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                Err(e).with_context(|| format!("删除后台任务输出文件失败: {}", path.display()))
            }
        }
    }
}

#[cfg(test)]
#[path = "task_output_test.rs"]
mod tests;
