//! 输入历史持久化：JSON 文件存储在用户家目录下。
//!
//! 路径：`~/.peri/input-history.json`
//! 格式：JSON 数组，最新在前。

use std::path::PathBuf;

const HISTORY_FILE: &str = "input-history.json";
const HISTORY_TMP: &str = "input-history.json.tmp";

fn history_path() -> Option<PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".peri").join(HISTORY_FILE))
}

fn history_tmp() -> Option<PathBuf> {
    dirs_next::home_dir().map(|h| h.join(".peri").join(HISTORY_TMP))
}

/// 从磁盘加载输入历史（最新在前）。文件不存在或解析失败返回空 Vec。
pub fn load_input_history() -> Vec<String> {
    let path = match history_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// 保存输入历史到磁盘（原子写入：先写 .tmp 再 rename）。静默忽略 IO 错误。
pub fn save_input_history(history: &[String]) {
    let path = match history_path() {
        Some(p) => p,
        None => return,
    };
    let tmp_path = match history_tmp() {
        Some(p) => p,
        None => return,
    };

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Serialize
    let json = match serde_json::to_string(history) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Atomic write
    if std::fs::write(&tmp_path, json).is_err() {
        return;
    }
    let _ = std::fs::rename(&tmp_path, &path);
}
