//! 启动后的版本更新检查：查一次 GitHub release，有新版就在欢迎卡提示。
//!
//! 与 [`crate::update`] 的分工：那个模块负责真正下载安装，这里只做只读探测。
//!
//! 三条硬约束：
//!
//! - **不阻塞 UI**：探测在 `tokio::spawn` 的后台任务里跑，结果经 oneshot 回传，
//!   事件循环用 [`UpdateChecker::poll`] 收取（同 `GitBranchPoller` 的形态）。
//! - **必须超时**：整个请求（含连接、TLS、body）压在 [`REQUEST_TIMEOUT`] 内。
//!   没有超时的话，断网环境下 DNS/握手会挂很久，后台任务一直占着接收器，
//!   欢迎卡就永远等不到结果。
//! - **失败静默**：任何失败（无网、限流、JSON 变体）都只是一句 `tracing::debug`，
//!   绝不弹错——版本提示属于锦上添花，不该干扰正常使用。
//!
//! 缓存：同一进程内只查一次。用户在一次运行里升不升级都得重启，重复查询没有意义。

use std::time::Duration;

use serde::Deserialize;

/// 整个更新检查请求的总超时。
///
/// 取 3s：比 TCP+TLS 握手的最坏情况宽裕，又不至于让断网用户盯着一个永不出现的
/// 提示。超时按整个请求计，不设 connect / read 分段。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// release 列表地址。安装脚本同样按 `npm-v*` 前缀挑 tag（见 `scripts/install.ps1`），
/// 这里保持一致，避免「欢迎卡说有新版、`cc-code update` 却装到别的版本」。
const RELEASES_URL: &str = "https://api.github.com/repos/cc-claws/cc-code/releases?per_page=30";

/// 从 release 列表里挑出最新版本的结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateStatus {
    /// 探测进行中，欢迎卡不显示任何更新提示。
    #[default]
    Unknown,
    /// 已是最新（或本地版本比线上还新，通常是本地自建）。
    UpToDate,
    /// 有更新：`latest` 是去掉 `npm-v` 前缀的版本号。
    Available { latest: String },
}

/// GitHub release 的最小反序列化视图——只取挑版本要用的字段。
#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    prerelease: bool,
}

/// 版本比较结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Newer,
    Same,
    Older,
}

/// 把 `npm-v0.6.171` 解析成 `(0, 6, 171)`；非 `npm-v` 前缀或缺段返回 `None`。
///
/// 允许 `-alpha` / `-beta` 这类后缀：剥掉预发布标识后只按三段数字比较。
fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let rest = tag.strip_prefix("npm-v")?;
    let core = rest.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    // 只接受三段：`0.6` 或 `0.6.1.2` 都视为异常 tag，避免误判。
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 比较两个版本号。
fn compare_versions(latest: Option<(u64, u64, u64)>, current: &str) -> Verdict {
    let Some(latest) = latest else {
        return Verdict::Older;
    };
    let Some(current) = parse_version(&format!("npm-v{current}")) else {
        return Verdict::Older;
    };
    match latest.cmp(&current) {
        std::cmp::Ordering::Greater => Verdict::Newer,
        std::cmp::Ordering::Equal => Verdict::Same,
        std::cmp::Ordering::Less => Verdict::Older,
    }
}

/// 从 release 列表原始 JSON 里挑出可用的最新版本 tag。
///
/// 规则对齐安装脚本：只认 `npm-v*`，`prerelease` 跳过，`draft` 不返回（GitHub
/// 的 releases 接口对未授权请求本就不含 draft）。用 `max` 而非「取第一个」——
/// 接口顺序不保证严格按版本号降序，按语义值挑最稳。
fn pick_latest_tag(releases: &[Release]) -> Option<String> {
    releases
        .iter()
        .filter(|release| !release.prerelease)
        .filter_map(|release| parse_version(&release.tag_name).map(|v| (v, &release.tag_name)))
        .max_by_key(|(version, _)| *version)
        .map(|(_, tag)| tag.clone())
}

/// 纯函数版判定：给定 release 列表 JSON 与本地版本，算出状态。
///
/// 抽出来是为了让测试不依赖网络，也方便断言「限流 / 空列表 / 异常 tag」的行为。
fn evaluate(releases: &[Release], current_version: &str) -> UpdateStatus {
    let Some(latest) = pick_latest_tag(releases) else {
        // 没有任何可识别的 `npm-v*` release：当作已是最新，不显示更新提示。
        return UpdateStatus::UpToDate;
    };
    match compare_versions(parse_version(&latest), current_version) {
        Verdict::Newer => UpdateStatus::Available {
            latest: latest.strip_prefix("npm-v").unwrap_or(&latest).to_string(),
        },
        _ => UpdateStatus::UpToDate,
    }
}

/// 异步执行一次更新检查；失败一律折成 [`UpdateStatus::Unknown`]。
async fn fetch_status(current_version: &str) -> UpdateStatus {
    let client = match reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        // GitHub API 要求带 UA，缺省会被 403。
        .user_agent(concat!("cc-code/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::debug!(%error, "更新检查：HTTP 客户端构造失败");
            return UpdateStatus::Unknown;
        }
    };

    let response = match client.get(RELEASES_URL).send().await {
        Ok(response) => response,
        Err(error) => {
            // 无网 / 超时 / TLS 失败都走这里。debug 级别：不打扰用户。
            tracing::debug!(%error, "更新检查：请求失败");
            return UpdateStatus::Unknown;
        }
    };

    if !response.status().is_success() {
        // 403（限流）最常见；不额外提示，下个版本启动时会自然重试。
        tracing::debug!(status = %response.status(), "更新检查：非成功响应");
        return UpdateStatus::Unknown;
    }

    let releases: Vec<Release> = match response.json().await {
        Ok(releases) => releases,
        Err(error) => {
            tracing::debug!(%error, "更新检查：响应解析失败");
            return UpdateStatus::Unknown;
        }
    };

    evaluate(&releases, current_version)
}

/// 更新检查的全部运行时状态：当前结果 + 在途任务 + 一次性触发标记。
#[derive(Default)]
pub struct UpdateChecker {
    status: UpdateStatus,
    /// 结果回传通道；`None` 表示没有在途任务。
    rx: Option<tokio::sync::oneshot::Receiver<UpdateStatus>>,
    /// 本进程是否已经发起过检查——只查一次。
    started: bool,
}

impl UpdateChecker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 欢迎卡读取的当前状态。探测未完成时返回 [`UpdateStatus::Unknown`]。
    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    /// 测试专用：直接注入一个结果，免去真实网络请求。
    #[cfg(test)]
    pub(crate) fn set_status_for_test(&mut self, status: UpdateStatus) {
        self.status = status;
        // 同时标记为「已查过」，避免后续 `request_if_due` 真的触网。
        self.started = true;
        self.rx = None;
    }

    /// 按需发起一次检查（每个进程只发一次），必须在 tokio runtime 内调用。
    ///
    /// `enabled` 为 false 时直接跳过：测试与离线构建不该产生真实网络请求。
    pub fn request_if_due(&mut self, enabled: bool) {
        if self.started || !enabled || self.rx.is_some() {
            return;
        }
        self.started = true;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.rx = Some(rx);
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        tokio::spawn(async move {
            // 接收端可能在 App 退出时先被丢弃，send 失败属正常情况。
            let _ = tx.send(fetch_status(&current_version).await);
        });
    }

    /// 收取已完成的结果；返回 true 表示状态有变化（需要重绘）。
    ///
    /// 任务失败（sender 被 drop）时同样退出「在途」状态，避免永远挂起。
    pub fn poll(&mut self) -> bool {
        let Some(rx) = self.rx.as_mut() else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(status) => Some(status),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return false,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(UpdateStatus::Unknown),
        };
        self.rx = None;
        match result {
            Some(status) => {
                let changed = status != self.status;
                self.status = status;
                changed
            }
            None => false,
        }
    }
}

#[cfg(test)]
#[path = "update_check_test.rs"]
mod tests;
