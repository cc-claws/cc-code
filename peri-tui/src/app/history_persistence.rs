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
        // 父目录设为 0o700：input-history.json 包含用户原始提示词，
        // 可能内联 API key / 调试命令 / 粘贴的密钥，禁止同机其它账户读取（#15）
        restrict_to_owner_unix(parent);
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
    // 文件本身设为 0o600（同 #15 根因）：rename 之前设好权限，避免短暂窗口期暴露
    restrict_to_owner_unix(&tmp_path);
    let _ = std::fs::rename(&tmp_path, &path);
    restrict_to_owner_unix(&path);
}

/// Unix：把给定路径权限收回到 owner-only（文件 0o600、目录 0o700）。
/// Windows / 其它平台无对应语义，函数为 no-op。
#[cfg(unix)]
fn restrict_to_owner_unix(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let target_mode = if path.is_dir() { 0o700 } else { 0o600 };
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(target_mode));
}

#[cfg(not(unix))]
fn restrict_to_owner_unix(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    // 注意：这里不放 `use super::*;`。
    // Windows 下 restrict_to_owner_unix 是 #[cfg(not(unix))] 的空实现，
    // 整个 mod tests 在 Windows 下没有任何使用 super::* 内容的代码，
    // clippy -D unused-imports 会把 super::* 当作未使用 import 报错。

    #[cfg(unix)]
    #[test]
    fn test_restrict_to_owner_unix_file_gets_0600() {
        let dir = std::env::temp_dir();
        let file = dir.join(format!(
            "peri-history-perm-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&file, b"x").unwrap();
        super::restrict_to_owner_unix(&file);
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "文件权限应为 0o600，实际 0o{:o}",
            mode
        );
        let _ = std::fs::remove_file(&file);
    }

    #[cfg(unix)]
    #[test]
    fn test_restrict_to_owner_unix_dir_gets_0700() {
        let dir = std::env::temp_dir().join(format!(
            "peri-history-perm-dir-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        super::restrict_to_owner_unix(&dir);
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "目录权限应为 0o700，实际 0o{:o}",
            mode
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
