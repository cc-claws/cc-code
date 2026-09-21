//! 本机图片文件读取：按内容解码，统一生成 PNG，避免把路径文字当作图片发送。

use std::{io::Read, path::Path};

use super::paste::{encode_base64, encode_rgba_to_png, PasteImageError};

const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_RGBA_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
            )
        })
}

pub(crate) fn image_file_as_png_base64(
    path: &Path,
) -> Result<(String, usize, u32, u32), PasteImageError> {
    let (bytes, width, height) = decode_image_file(path)?;
    encode_base64(&bytes, width, height)
}

pub(super) fn decode_image_file(path: &Path) -> Result<(Vec<u8>, u32, u32), PasteImageError> {
    let io_error = |error: std::io::Error| PasteImageError::IoError(error.to_string());
    let metadata = std::fs::metadata(path).map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(PasteImageError::DecodeFailed(
            "not a regular image file or exceeds 20 MiB".into(),
        ));
    }
    let file = std::fs::File::open(path).map_err(io_error)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(PasteImageError::DecodeFailed("image exceeds 20 MiB".into()));
    }
    let decode_error = |error: image::ImageError| PasteImageError::DecodeFailed(error.to_string());
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(io_error)?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_RGBA_BYTES);
    reader.limits(limits.clone());
    let format = reader
        .format()
        .ok_or_else(|| PasteImageError::DecodeFailed("unknown image format".into()))?;
    if !matches!(
        format,
        image::ImageFormat::Png
            | image::ImageFormat::Jpeg
            | image::ImageFormat::WebP
            | image::ImageFormat::Gif
            | image::ImageFormat::Bmp
    ) {
        return Err(PasteImageError::DecodeFailed(
            "unsupported image format".into(),
        ));
    }
    let (width, height) = reader.into_dimensions().map_err(decode_error)?;
    if u64::from(width) * u64::from(height) > MAX_RGBA_BYTES / 4 {
        return Err(PasteImageError::DecodeFailed(
            "decoded image exceeds 64 MiB".into(),
        ));
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    reader.limits(limits);
    let rgba = reader.decode().map_err(decode_error)?.into_rgba8();
    let png = encode_rgba_to_png(width, height, rgba.as_raw())?;
    if png.len() as u64 > MAX_FILE_BYTES {
        return Err(PasteImageError::EncodeFailed(
            "encoded PNG exceeds 20 MiB".into(),
        ));
    }
    Ok((png, width, height))
}

#[cfg(test)]
#[path = "image_file_test.rs"]
mod tests;
