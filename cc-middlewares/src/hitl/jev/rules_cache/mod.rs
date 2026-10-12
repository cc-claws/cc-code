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

/// 单个规则来源的脱敏元数据快照。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceMeta {
    /// 来源类别（固定为 personal / project / hooks / global）
    pub source: String,
    /// 该来源是否存在
    pub present: bool,
    /// 字符数（仅在 present 为 true 时记录）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub char_count: Option<usize>,
    /// 内容 SHA-256 哈希摘要（不记录任何原文内容）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// 规则缓存索引的人类可读元数据。
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CacheMetadata {
    /// 项目根目录（本地展示路径）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    /// LLM 提供方名称
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// 提炼所用模型 ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// 各来源指纹（仅包含类别与单向摘要）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceMeta>,
}

#[derive(Clone)]
pub(super) struct RulesCache {
    directory: PathBuf,
    signature: String,
    metadata: Option<CacheMetadata>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct CacheIndex {
    version: u32,
    signature: String,
    rules_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sources: Vec<SourceMeta>,
}

impl RulesCache {
    pub(super) fn new(
        cwd: &Path,
        context: serde_json::Value,
        metadata: Option<CacheMetadata>,
    ) -> Option<Self> {
        let cwd = fs::canonicalize(cwd).ok()?;
        let project_id = digest(cwd.to_string_lossy().as_bytes());
        let signature = digest(&serde_json::to_vec(&(FORMAT_VERSION, &project_id, context)).ok()?);
        Some(Self {
            directory: cc_agent::app_home::app_home_dir_in(&dirs_next::home_dir()?)
                .join("jev")
                .join(format!("peri-{project_id}")),
            signature,
            metadata,
        })
    }

    #[cfg(test)]
    pub(super) fn new_in(
        directory: PathBuf,
        signature: String,
        metadata: Option<CacheMetadata>,
    ) -> Self {
        Self {
            directory,
            signature,
            metadata,
        }
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
        // 人类可读优先：落盘使用 pretty 格式便于查阅。
        let bytes = serde_json::to_vec_pretty(rules)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid_data("规则缓存超出大小上限"));
        }
        let rules_id = digest(&bytes);
        let (project_path, provider, model, sources) = if let Some(m) = &self.metadata {
            (
                m.project_path.clone(),
                m.provider.clone(),
                m.model.clone(),
                m.sources.clone(),
            )
        } else {
            (None, None, None, Vec::new())
        };
        let created_at = Some(chrono::Local::now().to_rfc3339());
        let index = serde_json::to_vec_pretty(&CacheIndex {
            version: FORMAT_VERSION,
            signature: self.signature.clone(),
            rules_id: rules_id.clone(),
            created_at,
            project_path,
            provider,
            model,
            sources,
        })?;
        fs::create_dir_all(&self.directory)?;
        cc_agent::fs::restrict_to_owner(&self.directory)?;
        write_atomic(&self.directory.join(format!("{rules_id}.json")), &bytes)?;
        write_atomic(&self.index_path(), &index)
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn digest_str(s: &str) -> String {
    digest(s.as_bytes())
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

#[cfg(test)]
#[path = "rules_cache_test.rs"]
mod tests;
