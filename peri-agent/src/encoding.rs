/// Decode subprocess output bytes to UTF-8 string.
///
/// On Windows, tries UTF-8 first, falls back to GBK (the default console
/// encoding for Chinese locales). On other platforms, uses lossy UTF-8.
pub fn decode_output_bytes(bytes: &[u8]) -> String {
    #[cfg(target_os = "windows")]
    {
        if let Ok(text) = std::str::from_utf8(bytes) {
            return text.to_string();
        }
        let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
        decoded.into_owned()
    }
    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}
