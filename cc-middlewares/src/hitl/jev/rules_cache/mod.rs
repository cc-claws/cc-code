//! 完整规则的内容寻址磁盘缓存；失败或损坏按未命中处理。

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::rules::JevRules;

const FORMAT_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct RulesCache {
    directory: PathBuf,
    signature: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheIndex {
    version: u32,
    signature: String,
    rules_id: String,
}

impl RulesCache {
    pub(super) fn new(cwd: &Path, context: serde_json::Value) -> Option<Self> {
        let cwd = fs::canonicalize(cwd).ok()?;
        let project_id = digest(cwd.to_string_lossy().as_bytes());
        let signature = digest(&serde_json::to_vec(&(FORMAT_VERSION, &project_id, context)).ok()?);
        Some(Self {
            directory: cc_agent::app_home::app_home_dir_in(&dirs_next::home_dir()?)
                .join("jev")
                .join(format!("peri-{project_id}")),
            signature,
        })
    }

    pub(super) fn signature(&self) -> &str {
        &self.signature
    }

    fn index_path(&self) -> PathBuf {
        self.directory
            .join(format!("{}.index.json", self.signature))
    }

    pub(super) async fn load(&self) -> Option<Arc<JevRules>> {
        let cache = self.clone();
        match tokio::task::spawn_blocking(move || cache.read()).await {
            Ok(Ok(rules)) => Some(Arc::new(rules)),
            Ok(Err(error)) => {
                tracing::debug!(%error, "Jev 规则磁盘缓存未命中");
                None
            }
            Err(error) => {
                tracing::debug!(%error, "Jev 规则磁盘缓存读取任务失败");
                None
            }
        }
    }

    fn read(&self) -> io::Result<JevRules> {
        let index: CacheIndex = serde_json::from_slice(&read_bounded(&self.index_path())?)?;
        if index.version != FORMAT_VERSION
            || index.signature != self.signature
            || !valid_id(&index.rules_id)
        {
            return Err(invalid_data("规则缓存索引不匹配"));
        }
        // 文件名只接受内部 SHA-256，不将索引里的任意路径拼进目录。
        let bytes = read_bounded(&self.directory.join(format!("{}.json", index.rules_id)))?;
        if digest(&bytes) != index.rules_id {
            return Err(invalid_data("规则缓存内容校验失败"));
        }
        let rules: JevRules = serde_json::from_slice(&bytes)?;
        if rules.is_empty() {
            return Err(invalid_data("规则缓存为空"));
        }
        Ok(rules)
    }

    pub(super) async fn store(&self, rules: Arc<JevRules>) {
        let cache = self.clone();
        match tokio::task::spawn_blocking(move || cache.write(&rules)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::debug!(%error, "Jev 规则磁盘缓存写入失败"),
            Err(error) => tracing::debug!(%error, "Jev 规则磁盘缓存写入任务失败"),
        }
    }

    fn write(&self, rules: &JevRules) -> io::Result<()> {
        if rules.is_empty() {
            return Err(invalid_data("不缓存空规则"));
        }
        let bytes = serde_json::to_vec(rules)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid_data("规则缓存超出大小上限"));
        }
        let rules_id = digest(&bytes);
        let index = serde_json::to_vec(&CacheIndex {
            version: FORMAT_VERSION,
            signature: self.signature.clone(),
            rules_id: rules_id.clone(),
        })?;
        fs::create_dir_all(&self.directory)?;
        cc_agent::fs::restrict_to_owner(&self.directory)?;
        // 先完整发布内容，再发布索引；并发写入不会留下半份 JSON。
        write_atomic(&self.directory.join(format!("{rules_id}.json")), &bytes)?;
        write_atomic(&self.index_path(), &index)
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn invalid_data(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn read_bounded(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(invalid_data("规则缓存超出大小上限"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid_data("规则缓存超出大小上限"));
    }
    Ok(bytes)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| invalid_data("缓存目录为空"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    cc_agent::fs::restrict_to_owner(temporary.path())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
