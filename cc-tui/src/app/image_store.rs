//! 图片附件临时存储：同步写完后返回路径，文件生命周期交给系统临时目录。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use uuid::Uuid;

/// 将图片写入固定的会话临时目录；解码或写盘失败时返回错误。
///
/// 同一会话、同一编号与 MIME 映射到相同路径，重复写入会替换原文件。
pub(super) fn store_image(
    session_id: Uuid,
    image_id: usize,
    media_type: &str,
    base64_data: &str,
) -> Result<PathBuf> {
    store_image_in(
        &temp_image_dir(),
        session_id,
        image_id,
        media_type,
        base64_data,
    )
}

fn temp_image_dir() -> PathBuf {
    std::env::temp_dir().join("cc-code-images")
}

fn path_for(image_root: &Path, session_id: Uuid, image_id: usize, media_type: &str) -> PathBuf {
    let extension = match media_type {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        _ => "png",
    };
    image_root
        .join(session_id.to_string())
        .join(format!("{image_id}.{extension}"))
}

// 根目录只供模块内部与隔离测试使用，不接收用户提供的路径。
fn store_image_in(
    image_root: &Path,
    session_id: Uuid,
    image_id: usize,
    media_type: &str,
    base64_data: &str,
) -> Result<PathBuf> {
    // 先完整解码，避免无效数据创建空文件或截断已存图片。
    let bytes = STANDARD
        .decode(base64_data)
        .context("Failed to decode image attachment base64")?;
    let session_dir = image_root.join(session_id.to_string());
    std::fs::create_dir_all(&session_dir).with_context(|| {
        format!(
            "Failed to create image attachment directory {}",
            session_dir.display()
        )
    })?;
    let path = path_for(image_root, session_id, image_id, media_type);
    std::fs::write(&path, bytes)
        .with_context(|| format!("Failed to write image attachment {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
#[path = "image_store_test.rs"]
mod tests;
