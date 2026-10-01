//! 输入历史持久化：JSON 文件存储在用户家目录下。
//!
//! 路径：`~/.cc-code/input-history.json`（旧版 `~/.peri/` 仅回退，见 #289）
//! 格式：JSON 数组，最新在前。

use std::path::PathBuf;

const HISTORY_FILE: &str = "input-history.json";
const HISTORY_TMP: &str = "input-history.json.tmp";

fn history_path() -> Option<PathBuf> {
    Some(cc_agent::app_home::app_data_path(HISTORY_FILE))
}

fn history_tmp() -> Option<PathBuf> {
    Some(cc_agent::app_home::app_data_path(HISTORY_TMP))
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
        // 父目录设为 owner-only：input-history.json 包含用户原始提示词，
        // 可能内联 API key / 调试命令 / 粘贴的密钥，禁止同机其它账户读取（#15）
        restrict_to_owner(parent);
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
    // 文件本身设为 owner-only（同 #15 根因）：rename 之前设好权限，避免短暂窗口期暴露
    restrict_to_owner(&tmp_path);
    let _ = std::fs::rename(&tmp_path, &path);
    restrict_to_owner(&path);
}

/// 密钥/历史类路径收紧为 owner-only（Unix 0o600/0o700，Windows owner-only DACL）。
fn restrict_to_owner(path: &std::path::Path) {
    let _ = cc_agent::fs::restrict_to_owner(path);
}

#[cfg(test)]
mod tests {

    #[cfg(unix)]
    #[test]
    fn test_restrict_to_owner_file_gets_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir();
        let file = dir.join(format!(
            "peri-history-perm-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&file, b"x").unwrap();
        super::restrict_to_owner(&file);
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "文件权限应为 0o600，实际 0o{:o}", mode);
        let _ = std::fs::remove_file(&file);
    }

    #[cfg(unix)]
    #[test]
    fn test_restrict_to_owner_dir_gets_0700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "peri-history-perm-dir-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        super::restrict_to_owner(&dir);
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "目录权限应为 0o700，实际 0o{:o}", mode);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
