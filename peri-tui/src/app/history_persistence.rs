//! 输入历史持久化：JSON 文件按项目目录隔离存储。
//!
//! 路径：`{cwd}/.peri/history.json`
//! 格式：JSON 数组，最新在前。

use std::path::Path;

const HISTORY_FILE: &str = ".peri/history.json";
const HISTORY_TMP: &str = ".peri/history.json.tmp";

/// 从磁盘加载输入历史（最新在前）。文件不存在或解析失败返回空 Vec。
pub fn load_input_history(cwd: &str) -> Vec<String> {
    let path = Path::new(cwd).join(HISTORY_FILE);
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// 保存输入历史到磁盘（原子写入：先写 .tmp 再 rename）。静默忽略 IO 错误。
pub fn save_input_history(cwd: &str, history: &[String]) {
    let path = Path::new(cwd).join(HISTORY_FILE);
    let tmp_path = Path::new(cwd).join(HISTORY_TMP);

    // Ensure .peri directory exists
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
