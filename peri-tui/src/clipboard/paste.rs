//! 图片粘贴多层 fallback 实现
//!
//! 决策逻辑（参照 openai/codex `clipboard_paste.rs`）：
//!
//! 1. **file_list 优先**：macOS Finder 等文件管理器复制文件时，剪贴板里是
//!    文件路径而非图像数据。优先尝试 file_list，找到第一个能识别的图片文件
//!    解码后输出 PNG。
//! 2. **图像数据**：Chrome 截图、Win+Shift+S 等场景，剪贴板是 RGBA buffer，
//!    直接 `clipboard.get_image()` 走第二条路径。
//! 3. **WSL PowerShell fallback**：WSL 下 arboard 访问不到 Windows 剪贴板，
//!    通过 powershell.exe Get-Clipboard -Format Image 把图保存到临时 PNG，
//!    再把 Windows 路径转 WSL 路径读取。

#[cfg(target_os = "linux")]
use std::path::PathBuf;

use anyhow::Result;
use base64::Engine as _;

#[derive(Debug, Clone)]
pub enum PasteImageError {
    ClipboardUnavailable(String),
    NoImage(String),
    DecodeFailed(String),
    EncodeFailed(String),
    IoError(String),
}

impl std::fmt::Display for PasteImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasteImageError::ClipboardUnavailable(msg) => {
                write!(f, "clipboard unavailable: {msg}")
            }
            PasteImageError::NoImage(msg) => write!(f, "no image on clipboard: {msg}"),
            PasteImageError::DecodeFailed(msg) => write!(f, "could not decode image: {msg}"),
            PasteImageError::EncodeFailed(msg) => write!(f, "could not encode image: {msg}"),
            PasteImageError::IoError(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl std::error::Error for PasteImageError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedImageFormat {
    Png,
    Other,
}

/// 抓取剪贴板图片，输出 PNG base64 + 尺寸 + 字节数。
///
/// 三层 fallback：
/// 1. file_list（Finder 复制文件场景）
/// 2. get_image（截图等场景）
/// 3. WSL PowerShell fallback（仅 Linux + 检测到 WSL 环境）
pub fn paste_image_as_png_base64() -> Result<(String, usize, u32, u32), PasteImageError> {
    let (png_bytes, w, h) = match try_file_list_or_image_data() {
        Ok((bytes, w, h)) => (bytes, w, h),
        Err(e) => {
            #[cfg(target_os = "linux")]
            {
                if let Some((bytes, w, h)) = try_wsl_clipboard_fallback(&e)? {
                    return encode_base64(&bytes, w, h);
                }
            }
            return Err(e);
        }
    };
    encode_base64(&png_bytes, w, h)
}

fn encode_base64(
    png_bytes: &[u8],
    w: u32,
    h: u32,
) -> Result<(String, usize, u32, u32), PasteImageError> {
    let size = png_bytes.len();
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    Ok((b64, size, w, h))
}

/// 路径 1+2：先试 file_list，再试 get_image。
#[cfg(not(target_os = "android"))]
fn try_file_list_or_image_data() -> Result<(Vec<u8>, u32, u32), PasteImageError> {
    // macOS 下抑制 arboard 初始化 NSPasteboard 的 stderr 污染
    let _guard = crate::clipboard::SuppressStderr::new();
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| PasteImageError::ClipboardUnavailable(e.to_string()))?;

    // 路径 1：file_list（Finder 等文件复制场景，剪贴板里是文件路径）
    if let Ok(files) = cb.get().file_list() {
        for f in files.into_iter() {
            if let Ok((bytes, w, h)) = decode_png_file(&f) {
                return Ok((bytes, w, h));
            }
        }
    }

    // 路径 2：get_image（截图等场景，剪贴板里是 RGBA buffer）
    let img = cb
        .get_image()
        .map_err(|e| PasteImageError::NoImage(e.to_string()))?;
    let w = img.width as u32;
    let h = img.height as u32;
    let png_bytes = encode_rgba_to_png(w, h, img.bytes.as_ref())?;
    Ok((png_bytes, w, h))
}

#[cfg(target_os = "android")]
fn try_file_list_or_image_data() -> Result<(Vec<u8>, u32, u32), PasteImageError> {
    Err(PasteImageError::ClipboardUnavailable(
        "clipboard image paste is unsupported on Android".into(),
    ))
}

/// 用 png crate 解码文件，再重编码为标准化 PNG。
fn decode_png_file(path: &std::path::Path) -> Result<(Vec<u8>, u32, u32), PasteImageError> {
    let file = std::fs::File::open(path)
        .map_err(|e| PasteImageError::IoError(format!("open {}: {e}", path.display())))?;
    let reader = std::io::BufReader::new(file);
    let decoder = png::Decoder::new(reader);
    let mut reader = decoder.read_info().map_err(|e| {
        PasteImageError::DecodeFailed(format!("png decode {}: {e}", path.display()))
    })?;

    let (w, h) = (reader.info().width, reader.info().height);
    // Allocate buffer based on output buffer size hint
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    reader
        .next_frame(&mut buf)
        .map_err(|e| PasteImageError::DecodeFailed(format!("png read frame: {e}")))?;

    // 用标准化 RGBA 重编码
    let info = reader.info();
    let color_type = info.color_type;
    let bit_depth = info.bit_depth;
    let rgba = match (color_type, bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => buf,
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            // RGB → RGBA
            let mut rgba = Vec::with_capacity(buf.len() / 3 * 4);
            for chunk in buf.chunks_exact(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            rgba
        }
        _ => {
            return Err(PasteImageError::DecodeFailed(format!(
                "unsupported png color/depth in {}: {:?}/{:?}",
                path.display(),
                color_type,
                bit_depth
            )));
        }
    };

    let png_bytes = encode_rgba_to_png(w, h, &rgba)?;
    Ok((png_bytes, w, h))
}

fn encode_rgba_to_png(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, PasteImageError> {
    let mut png_bytes: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| PasteImageError::EncodeFailed(format!("png header: {e}")))?;
        writer
            .write_image_data(rgba)
            .map_err(|e| PasteImageError::EncodeFailed(format!("png write: {e}")))?;
    }
    Ok(png_bytes)
}

/// WSL fallback：通过 powershell.exe Get-Clipboard -Format Image 把剪贴板图存到临时 PNG。
#[cfg(target_os = "linux")]
fn try_wsl_clipboard_fallback(
    error: &PasteImageError,
) -> Result<Option<(Vec<u8>, u32, u32)>, PasteImageError> {
    use PasteImageError::{ClipboardUnavailable, NoImage};

    if !is_probably_wsl() || !matches!(error, ClipboardUnavailable(_) | NoImage(_)) {
        return Ok(None);
    }

    tracing::debug!("attempting Windows PowerShell clipboard fallback for image");
    let Some(win_path) = try_dump_windows_clipboard_image() else {
        return Ok(None);
    };
    let Some(mapped_path) = convert_windows_path_to_wsl(&win_path) else {
        return Ok(None);
    };

    match decode_png_file(&mapped_path) {
        Ok(tuple) => Ok(Some(tuple)),
        Err(_) => Ok(None),
    }
}

#[cfg(target_os = "linux")]
fn is_probably_wsl() -> bool {
    if let Ok(version) = std::fs::read_to_string("/proc/version") {
        let lower = version.to_lowercase();
        if lower.contains("microsoft") || lower.contains("wsl") {
            return true;
        }
    }
    std::env::var_os("WSL_DISTRO_NAME").is_some() || std::env::var_os("WSL_INTEROP").is_some()
}

#[cfg(target_os = "linux")]
fn convert_windows_path_to_wsl(input: &str) -> Option<PathBuf> {
    if input.starts_with("\\\\") {
        return None;
    }
    let drive_letter = input.chars().next()?.to_ascii_lowercase();
    if !drive_letter.is_ascii_lowercase() {
        return None;
    }
    if input.get(1..2) != Some(":") {
        return None;
    }
    let mut result = PathBuf::from(format!("/mnt/{drive_letter}"));
    for component in input
        .get(2..)?
        .trim_start_matches(['\\', '/'])
        .split(['\\', '/'])
        .filter(|c| !c.is_empty())
    {
        result.push(component);
    }
    Some(result)
}

#[cfg(target_os = "linux")]
fn try_dump_windows_clipboard_image() -> Option<String> {
    let script = r#"[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $img = Get-Clipboard -Format Image; if ($img -ne $null) { $p=[System.IO.Path]::GetTempFileName(); $p = [System.IO.Path]::ChangeExtension($p,'png'); $img.Save($p,[System.Drawing.Imaging.ImageFormat]::Png); Write-Output $p } else { exit 1 }"#;

    for cmd in ["powershell.exe", "pwsh", "powershell"] {
        match std::process::Command::new(cmd)
            .args(["-NoProfile", "-Command", script])
            .output()
        {
            Ok(output) if output.status.success() => {
                let win_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !win_path.is_empty() {
                    tracing::debug!("{} saved clipboard image to {}", cmd, win_path);
                    return Some(win_path);
                }
            }
            Ok(_) => {
                tracing::debug!("{} returned non-zero status", cmd);
            }
            Err(err) => {
                tracing::debug!("{} not executable: {}", cmd, err);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn wsl_路径转换_支持_c_盘() {
        assert_eq!(
            convert_windows_path_to_wsl(r"C:\Users\Alice\file.png"),
            Some(PathBuf::from("/mnt/c/Users/Alice/file.png"))
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn wsl_路径转换_小写盘符() {
        assert_eq!(
            convert_windows_path_to_wsl(r"d:\temp\x.png"),
            Some(PathBuf::from("/mnt/d/temp/x.png"))
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn wsl_路径转换_拒绝_unc() {
        assert_eq!(
            convert_windows_path_to_wsl(r"\\server\share\file.png"),
            None
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn wsl_路径转换_拒绝无盘符() {
        assert_eq!(convert_windows_path_to_wsl("/tmp/file.png"), None);
    }

    #[test]
    fn encode_rgba_输出有效_png_头部() {
        let rgba = vec![
            255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255,
        ];
        let png = encode_rgba_to_png(2, 2, &rgba).unwrap();
        // PNG magic number
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }

    #[test]
    fn paste_error_显示格式() {
        let e = PasteImageError::NoImage("empty".into());
        assert_eq!(format!("{e}"), "no image on clipboard: empty");
    }

    #[test]
    fn encode_base64_返回_size_宽高() {
        // 1×1 PNG (smallest valid)
        let png = encode_rgba_to_png(1, 1, &[255, 0, 0, 255]).unwrap();
        let (b64, sz, w, h) = encode_base64(&png, 1, 1).unwrap();
        assert_eq!(sz, png.len());
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        // base64 decode → 原始字节
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        assert_eq!(decoded, png);
    }
}
