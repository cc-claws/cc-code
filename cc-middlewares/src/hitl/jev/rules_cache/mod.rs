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
    /// 对应规则文件的实际路径或模式
    pub path: String,
    /// 该来源是否存在
    pub present: bool,
    /// 内容 SHA-256 哈希摘要（不记录任何原文内容）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// 规则缓存的人类可读元数据。
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

/// 单文件归一的完整规则缓存（包含元数据与提炼规则正文）。
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CachedRulesFile {
    pub version: u32,
    pub signature: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceMeta>,
    #[serde(default)]
    pub rules: Vec<super::rules::JevRule>,
    #[serde(default)]
    pub protected_paths: Vec<String>,
}

impl CachedRulesFile {
    pub fn to_jev_rules(&self) -> JevRules {
        JevRules {
            rules: self.rules.clone(),
            protected_paths: self.protected_paths.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct RulesCache {
    directory: PathBuf,
    signature: String,
    metadata: Option<CacheMetadata>,
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
        let jev_root = cc_agent::app_home::app_home_dir_in(&dirs_next::home_dir()?).join("jev");

        // 人性化目录命名：{项目名}-{8位短哈希}
        let folder_name = cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .map(|s| sanitize_project_name(&s))
            .unwrap_or_else(|| "project".to_string());
        let short_hash = &project_id[..8];
        let directory = jev_root.join(format!("{folder_name}-{short_hash}"));

        Some(Self {
            directory,
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

    fn file_path(&self) -> PathBuf {
        self.directory.join(format!("{}.json", self.signature))
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
        let target = self.file_path();
        let bytes = read_bounded(&target)?;
        let file: CachedRulesFile = serde_json::from_slice(&bytes)?;
        if file.version != FORMAT_VERSION || file.signature != self.signature {
            return Err(invalid_data("规则缓存签名或版本不匹配"));
        }
        let rules = file.to_jev_rules();
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
        let file_content = CachedRulesFile {
            version: FORMAT_VERSION,
            signature: self.signature.clone(),
            created_at,
            project_path,
            provider,
            model,
            sources,
            rules: rules.rules.clone(),
            protected_paths: rules.protected_paths.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&file_content)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(invalid_data("规则缓存超出大小上限"));
        }
        fs::create_dir_all(&self.directory)?;
        cc_agent::fs::restrict_to_owner(&self.directory)?;
        write_atomic(&self.file_path(), &bytes)
    }
}

pub(super) fn sanitize_project_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "project".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn digest_str(s: &str) -> String {
    digest(s.as_bytes())
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
