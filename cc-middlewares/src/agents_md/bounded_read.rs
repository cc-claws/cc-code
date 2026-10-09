//! 有界读取指引文件：大文件只读取头尾，避免先加载全文再截断。

use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use super::{TRUNCATE_HEAD_PERCENT, TRUNCATE_MARKER_SLACK, TRUNCATE_TAIL_PERCENT};

const FULL_MARKER_PREFIX: &str = "\n\n[...truncated ";
const FULL_MARKER_SUFFIX: &str = " bytes. Use file tools to read the full file.]\n\n";
const SHORT_MARKER: &str = "[...truncated]\n\n";
/// UTF-8 字符最多 4 字节，切边时多读 3 字节以验证跨边界字符。
const UTF8_BOUNDARY_LOOKAROUND: usize = 3;

/// 返回标记及头尾字节预算；小额度放不下标记时退化为仅保留头部。
///
/// 字符串截断和文件读取共用此布局，避免两条路径的比例与标记漂移。
pub(super) fn truncation_layout(
    max_bytes: usize,
    name: &str,
    total_bytes: u64,
) -> Option<(String, usize, usize)> {
    let byte_count = total_bytes.to_string();
    let description = ": kept head+tail of ";
    // 先计算长度，避免为了检查小额度而复制很长的路径名。
    let full_length = FULL_MARKER_PREFIX
        .len()
        .checked_add(name.len())?
        .checked_add(description.len())?
        .checked_add(byte_count.len())?
        .checked_add(FULL_MARKER_SUFFIX.len())?;
    let marker = if full_length <= max_bytes.saturating_sub(TRUNCATE_MARKER_SLACK)
        && max_bytes >= TRUNCATE_MARKER_SLACK
    {
        format!("{FULL_MARKER_PREFIX}{name}{description}{byte_count}{FULL_MARKER_SUFFIX}")
    } else if SHORT_MARKER.len() <= max_bytes.saturating_sub(TRUNCATE_MARKER_SLACK / 2)
        && max_bytes >= TRUNCATE_MARKER_SLACK / 2
    {
        SHORT_MARKER.to_string()
    } else {
        return None;
    };
    let budget = max_bytes - marker.len();
    let ratio = TRUNCATE_HEAD_PERCENT + TRUNCATE_TAIL_PERCENT;
    // 分开商和余数，避免极大配置值乘法溢出。
    let head_bytes =
        budget / ratio * TRUNCATE_HEAD_PERCENT + budget % ratio * TRUNCATE_HEAD_PERCENT / ratio;
    Some((marker, head_bytes, budget - head_bytes))
}

pub(super) fn read_bounded_file(path: &Path, max_bytes: usize) -> io::Result<String> {
    let mut file = File::open(path)?;
    let file_length = file.metadata()?.len();
    read_bounded_reader(
        &mut file,
        file_length,
        max_bytes,
        &super::config::slash_display(path),
    )
}

/// 小文件保留完整 UTF-8 校验；大文件读取量至多为预算加 6 字节边界验证。
fn read_bounded_reader<R: Read + Seek>(
    reader: &mut R,
    file_length: u64,
    max_bytes: usize,
    name: &str,
) -> io::Result<String> {
    if max_bytes == 0 {
        return Ok(String::new());
    }
    let max_read = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    if file_length <= max_read {
        let mut bytes = Vec::new();
        reader.take(max_read).read_to_end(&mut bytes)?;
        return String::from_utf8(bytes).map_err(invalid_utf8);
    }
    let Some((marker, head_bytes, tail_bytes)) = truncation_layout(max_bytes, name, file_length)
    else {
        return read_head(reader, max_bytes);
    };
    let head = read_head(reader, head_bytes)?;
    let tail = read_tail(reader, file_length, tail_bytes)?;
    let mut output = String::with_capacity(head.len() + marker.len() + tail.len());
    output.push_str(&head);
    output.push_str(&marker);
    output.push_str(&tail);
    Ok(output)
}

fn read_head<R: Read>(reader: &mut R, budget: usize) -> io::Result<String> {
    if budget == 0 {
        return Ok(String::new());
    }
    let limit = u64::try_from(budget.saturating_add(UTF8_BOUNDARY_LOOKAROUND)).unwrap_or(u64::MAX);
    let mut bytes = Vec::new();
    reader.take(limit).read_to_end(&mut bytes)?;
    let selected = budget.min(bytes.len());
    let kept = match std::str::from_utf8(&bytes[..selected]) {
        Ok(_) => selected,
        Err(error) if error.error_len().is_none() => {
            // 验证被切开的字符，不能把真实非法 UTF-8 当成普通切边静默丢弃。
            let start = error.valid_up_to();
            let width = utf8_character_width(bytes[start])?;
            let end = start + width;
            let character = bytes.get(start..end).ok_or_else(|| invalid_utf8(error))?;
            std::str::from_utf8(character).map_err(invalid_utf8)?;
            start
        }
        Err(error) => return Err(invalid_utf8(error)),
    };
    bytes.truncate(kept);
    String::from_utf8(bytes).map_err(invalid_utf8)
}

fn read_tail<R: Read + Seek>(
    reader: &mut R,
    file_length: u64,
    budget: usize,
) -> io::Result<String> {
    if budget == 0 {
        return Ok(String::new());
    }
    let tail_length = u64::try_from(budget).unwrap_or(u64::MAX).min(file_length);
    let selected_start = file_length - tail_length;
    let window_start = selected_start.saturating_sub(UTF8_BOUNDARY_LOOKAROUND as u64);
    let prefix_length = (selected_start - window_start) as usize;
    reader.seek(SeekFrom::Start(window_start))?;
    let mut bytes = Vec::new();
    reader
        .take(tail_length + prefix_length as u64)
        .read_to_end(&mut bytes)?;
    if prefix_length >= bytes.len() {
        return Ok(String::new());
    }
    let mut start = prefix_length;
    if is_utf8_continuation(bytes[start]) {
        // 向前找跨边界字符的起点，并验证整个字符；不能直接吞掉非法续字节。
        let mut character_start = start;
        while character_start > 0 && is_utf8_continuation(bytes[character_start]) {
            character_start -= 1;
        }
        let width = utf8_character_width(bytes[character_start])?;
        let end = character_start + width;
        let character = bytes
            .get(character_start..end)
            .ok_or_else(|| invalid_utf8("尾部切边字符不完整"))?;
        std::str::from_utf8(character).map_err(invalid_utf8)?;
        if end <= start {
            return Err(invalid_utf8("尾部切边续字节没有有效起始字符"));
        }
        start = end;
    }
    std::str::from_utf8(&bytes[start..]).map_err(invalid_utf8)?;
    bytes.drain(..start);
    String::from_utf8(bytes).map_err(invalid_utf8)
}

fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0xc0 == 0x80
}

fn utf8_character_width(byte: u8) -> io::Result<usize> {
    match byte {
        0x00..=0x7f => Ok(1),
        0xc2..=0xdf => Ok(2),
        0xe0..=0xef => Ok(3),
        0xf0..=0xf4 => Ok(4),
        _ => Err(invalid_utf8("切边处存在非法 UTF-8 起始字节")),
    }
}

fn invalid_utf8(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
#[path = "bounded_read_test.rs"]
mod tests;
