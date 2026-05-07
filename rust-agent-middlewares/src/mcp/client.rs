use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

use rmcp::model::{Resource, Tool};
use rmcp::service::{Peer, RoleClient, RunningService};

use super::config::{ConfigSource, McpServerConfig};
use super::transport::TransportConfig;

use super::auth_store::FileCredentialStore;
use super::oauth_flow::{OAuthFlowEvent, OAuthFlowManager};

/// MCP 客户端连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum ClientStatus {
    Connected,
    Failed(String),
    Disconnected,
    Disabled,
    /// 配置存在但从未尝试连接（不在 clients 表中，仅在 configs 表中）
    Uninitialized,
}

/// MCP 连接池初始化状态
#[derive(Debug, Clone, PartialEq)]
pub enum McpInitStatus {
    Pending,
    Initializing { connected: usize, total: usize },
    Ready { total: usize },
    Failed(String),
}

/// MCP 服务器 OAuth 授权状态
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OAuthStatus {
    /// 不使用 OAuth（stdio 传输或未配置 OAuth）
    #[default]
    None,
    /// 已授权（token 有效）
    Authorized,
    /// 需要授权（HTTP 传输且配置了 OAuth，但 token 缺失或过期）
    NeedsAuthorization,
}

/// 单个 MCP 服务器的详细信息（用于 TUI 面板展示）
#[derive(Debug)]
pub struct ServerInfo {
    pub name: String,
    pub transport_type: String,
    pub status: ClientStatus,
    pub tool_count: usize,
    pub resource_count: usize,
    /// OAuth 授权状态
    pub oauth_status: OAuthStatus,
    /// 配置来源
    pub source: Option<ConfigSource>,
    /// 服务器 URL（HTTP 传输）
    pub url: Option<String>,
    /// 插件来源标识（`"name@marketplace"`），非插件 server 为 None
    pub plugin_source: Option<String>,
}

/// 连接池级别错误
#[derive(Debug, Error)]
pub enum McpPoolError {
    #[error("MCP 服务器 \"{server}\" 连接失败: {reason}")]
    ConnectionFailed { server: String, reason: String },
    #[error("MCP 服务器 \"{server}\" 工具发现失败: {reason}")]
    ToolDiscoveryFailed { server: String, reason: String },
    #[error("MCP 服务器 \"{server}\" 未连接 (状态: {status:?})")]
    NotConnected {
        server: String,
        status: ClientStatus,
    },
    #[error("MCP 服务器 \"{server}\" 调用超时")]
    CallTimeout { server: String },
}

/// 单个 MCP 服务器的客户端句柄
#[derive(Clone)]
pub struct McpClientHandle {
    pub name: String,
    pub peer: Option<Peer<RoleClient>>,
    pub tools: Vec<Tool>,
    pub resources: Vec<Resource>,
    pub status: ClientStatus,
    pub oauth_status: OAuthStatus,
    /// 配置来源
    pub source: Option<ConfigSource>,
    /// 服务器 URL（HTTP 传输）
    pub url: Option<String>,
}

/// MCP 客户端连接池
pub struct McpClientPool {
    clients: parking_lot::RwLock<HashMap<String, Arc<McpClientHandle>>>,
    services: tokio::sync::Mutex<HashMap<String, RunningService<RoleClient, ()>>>,
    configs: parking_lot::RwLock<HashMap<String, McpServerConfig>>,
    /// 插件来源旁路表：key 为 server name（如 `"plugin:p1:srv1"`），value 为 `"name@marketplace"`
    plugin_sources: parking_lot::RwLock<HashMap<String, String>>,
}

const STDIO_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const HTTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

impl McpClientPool {
    pub fn new_pending() -> Self {
        Self {
            clients: parking_lot::RwLock::new(HashMap::new()),
            services: tokio::sync::Mutex::new(HashMap::new()),
            configs: parking_lot::RwLock::new(HashMap::new()),
            plugin_sources: parking_lot::RwLock::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self::new_pending()
    }

    /// 查询指定 server 的插件来源标识，非插件 server 返回 None
    /// key 格式为 `"plugin_name__server_name"`，返回 `"name@marketplace"`
    pub fn plugin_source_of(&self, name: &str) -> Option<String> {
        self.plugin_sources.read().get(name).cloned()
    }

    pub async fn run_initialize(
        pool: Arc<Self>,
        cwd: &Path,
        claude_home: &Path,
        status_tx: tokio::sync::watch::Sender<McpInitStatus>,
        oauth_event_callback: Option<Box<dyn Fn(OAuthFlowEvent) + Send + Sync>>,
    ) {
        let (config, plugin_sources) = super::load_merged_config_full(cwd, claude_home);
        let connectable = config
            .mcp_servers
            .iter()
            .filter(|(_, sc)| !sc.disabled.unwrap_or(false))
            .count();
        if config.mcp_servers.is_empty() {
            let _ = status_tx.send(McpInitStatus::Ready { total: 0 });
            return;
        }

        *pool.plugin_sources.write() = plugin_sources;

        let token_store = Arc::new(FileCredentialStore::new());
        let mut oauth_manager: Option<OAuthFlowManager> =
            oauth_event_callback.map(|cb| OAuthFlowManager::new(token_store, cb));

        for (name, server_config) in &config.mcp_servers {
            pool.configs
                .write()
                .insert(name.clone(), server_config.clone());
        }
        let _ = status_tx.send(McpInitStatus::Initializing {
            connected: 0,
            total: connectable,
        });

        let mut connected = 0usize;
        for (name, server_config) in &config.mcp_servers {
            // 跳过已禁用的服务器，注册为 Disabled 状态
            if server_config.disabled.unwrap_or(false) {
                tracing::info!(server = %name, "MCP 服务器已禁用，跳过连接");
                pool.clients.write().insert(
                    name.clone(),
                    Arc::new(McpClientHandle {
                        name: name.clone(),
                        peer: None,
                        tools: vec![],
                        resources: vec![],
                        status: ClientStatus::Disabled,
                        oauth_status: OAuthStatus::default(),
                        source: server_config.source.clone(),
                        url: server_config.url.clone(),
                    }),
                );
                continue;
            }
            let transport_config = match TransportConfig::try_from(server_config) {
                Ok(tc) => tc,
                Err(e) => {
                    tracing::warn!(server = %name, error = %e, "传输层构建失败");
                    Self::insert_failed(&pool, name, format!("传输层构建失败: {e}"));
                    continue;
                }
            };
            let is_http = matches!(transport_config, TransportConfig::StreamableHttp { .. });
            let timeout = if is_http {
                HTTP_CONNECT_TIMEOUT
            } else {
                STDIO_CONNECT_TIMEOUT
            };

            let mut used_oauth = false;
            let connect_result: Result<Result<_, _>, _> = match transport_config {
                TransportConfig::Stdio {
                    ref command,
                    ref args,
                    ref env,
                } => match spawn_stdio_transport(command, args, env) {
                    Ok(transport) => {
                        tokio::time::timeout(timeout, rmcp::service::serve_client((), transport))
                            .await
                    }
                    Err(e) => {
                        Self::insert_failed(&pool, name, format!("stdio 启动失败: {e}"));
                        continue;
                    }
                },
                TransportConfig::StreamableHttp {
                    ref url,
                    ref headers,
                    ref oauth,
                } => {
                    let oauth_cfg = oauth.as_ref().cloned().or_else(|| {
                        // 没有显式 OAuth 配置的 HTTP 服务器：检查磁盘是否有已保存的凭证
                        // 如果有，用默认配置触发 OAuth 恢复流程
                        if let Some(ref mgr) = oauth_manager {
                            let token_store = mgr.token_store();
                            match tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(token_store.load_server(name))) {
                                Ok(Some(_)) => {
                                    tracing::info!(server = %name, "发现已保存的 OAuth 凭证，使用默认配置恢复");
                                    Some(super::config::OAuthConfig::default())
                                }
                                _ => None,
                            }
                        } else {
                            None
                        }
                    });
                    if let (Some(ref cfg), Some(ref mut mgr)) = (oauth_cfg, &mut oauth_manager) {
                        match mgr.run_oauth_flow(name, url, cfg).await {
                            Ok(()) => {
                                used_oauth = true;
                                if let Some(am) = mgr.get_authorization_manager(name) {
                                    tokio::time::timeout(
                                        timeout,
                                        rmcp::service::serve_client(
                                            (),
                                            build_authed_transport(url, headers, am),
                                        ),
                                    )
                                    .await
                                } else {
                                    tokio::time::timeout(
                                        timeout,
                                        rmcp::service::serve_client(
                                            (),
                                            build_http_transport(url, headers),
                                        ),
                                    )
                                    .await
                                }
                            }
                            Err(e) => {
                                // OAuth 恢复失败（如凭证过期），回退到裸连接，让 401 错误处理接管
                                tracing::warn!(server = %name, error = %e, "OAuth 恢复失败，尝试裸连接");
                                tokio::time::timeout(
                                    timeout,
                                    rmcp::service::serve_client(
                                        (),
                                        build_http_transport(url, headers),
                                    ),
                                )
                                .await
                            }
                        }
                    } else {
                        tokio::time::timeout(
                            timeout,
                            rmcp::service::serve_client((), build_http_transport(url, headers)),
                        )
                        .await
                    }
                }
            };

            match connect_result {
                Ok(Ok(rs)) => {
                    let tools = rs.list_all_tools().await.unwrap_or_default();
                    let resources = rs.list_all_resources().await.unwrap_or_default();
                    tracing::info!(server = %name, tools = tools.len(), resources = resources.len(), "MCP 连接成功");
                    let peer = rs.peer().clone();
                    let oauth_status = if used_oauth {
                        OAuthStatus::Authorized
                    } else {
                        OAuthStatus::default()
                    };
                    let handle = Arc::new(McpClientHandle {
                        name: name.clone(),
                        peer: Some(peer),
                        tools,
                        resources,
                        status: ClientStatus::Connected,
                        oauth_status,
                        source: server_config.source.clone(),
                        url: server_config.url.clone(),
                    });
                    pool.clients.write().insert(name.clone(), handle);
                    pool.services.lock().await.insert(name.clone(), rs);
                    connected += 1;
                    let _ = status_tx.send(McpInitStatus::Initializing {
                        connected,
                        total: connectable,
                    });
                }
                Ok(Err(e)) => {
                    let err_str = e.to_string();
                    tracing::warn!(server = %name, error = %err_str, "MCP 连接失败");
                    if Self::is_auth_required_error(&err_str, is_http) {
                        Self::insert_needs_auth(&pool, name, err_str);
                    } else {
                        Self::insert_failed(&pool, name, err_str);
                    }
                }
                Err(_) => {
                    Self::insert_failed(&pool, name, "连接超时".to_string());
                }
            }
        }

        if connectable > 0 && connected == 0 {
            let all_need_auth = pool
                .clients
                .read()
                .values()
                .all(|h| h.oauth_status == OAuthStatus::NeedsAuthorization);
            if all_need_auth {
                let _ = status_tx.send(McpInitStatus::Ready { total: 0 });
            } else {
                let failed: Vec<String> = pool
                    .clients
                    .read()
                    .iter()
                    .filter(|(_, h)| matches!(h.status, ClientStatus::Failed(_)))
                    .map(|(n, h)| {
                        if let ClientStatus::Failed(r) = &h.status {
                            format!("{}: {}", n, r)
                        } else {
                            n.clone()
                        }
                    })
                    .collect();
                let _ = status_tx.send(McpInitStatus::Failed(format!(
                    "{} 个服务器连接失败: {}",
                    connectable,
                    failed.join("; ")
                )));
            }
        } else {
            let _ = status_tx.send(McpInitStatus::Ready { total: connected });
        }
    }

    fn insert_failed(pool: &Arc<Self>, name: &str, reason: String) {
        let (source, url) = pool
            .configs
            .read()
            .get(name)
            .map(|c| (c.source.clone(), c.url.clone()))
            .unwrap_or((None, None));
        pool.clients.write().insert(
            name.to_string(),
            Arc::new(McpClientHandle {
                name: name.to_string(),
                peer: None,
                tools: vec![],
                resources: vec![],
                status: ClientStatus::Failed(reason),
                oauth_status: OAuthStatus::default(),
                source,
                url,
            }),
        );
    }

    /// 插入需要 OAuth 授权的服务器（HTTP 传输收到 401/AuthRequired 时使用）
    fn insert_needs_auth(pool: &Arc<Self>, name: &str, reason: String) {
        tracing::info!(server = %name, "HTTP 服务器需要 OAuth 授权，可在 MCP 面板按 r 键触发");
        let (source, url) = pool
            .configs
            .read()
            .get(name)
            .map(|c| (c.source.clone(), c.url.clone()))
            .unwrap_or((None, None));
        pool.clients.write().insert(
            name.to_string(),
            Arc::new(McpClientHandle {
                name: name.to_string(),
                peer: None,
                tools: vec![],
                resources: vec![],
                status: ClientStatus::Failed(reason),
                oauth_status: OAuthStatus::NeedsAuthorization,
                source,
                url,
            }),
        );
    }

    /// 检测错误是否为 HTTP 401 认证错误
    fn is_auth_required_error(error: &str, transport_is_http: bool) -> bool {
        transport_is_http && (error.contains("Auth required") || error.contains("AuthRequired"))
    }

    pub async fn reconnect(
        self: &Arc<Self>,
        server_name: &str,
        oauth_event_callback: Option<Box<dyn Fn(OAuthFlowEvent) + Send + Sync>>,
    ) -> Result<(), McpPoolError> {
        let server_config = self
            .configs
            .read()
            .get(server_name)
            .cloned()
            .ok_or_else(|| McpPoolError::NotConnected {
                server: server_name.to_string(),
                status: ClientStatus::Disconnected,
            })?;

        if let Some(mut svc) = self.services.lock().await.remove(server_name) {
            let _ = svc.close_with_timeout(SHUTDOWN_TIMEOUT).await;
        }
        self.clients.write().remove(server_name);

        let tc = TransportConfig::try_from(&server_config).map_err(|e| {
            McpPoolError::ConnectionFailed {
                server: server_name.to_string(),
                reason: format!("传输层构建失败: {e}"),
            }
        })?;
        let is_http = matches!(tc, TransportConfig::StreamableHttp { .. });
        let timeout = if is_http {
            HTTP_CONNECT_TIMEOUT
        } else {
            STDIO_CONNECT_TIMEOUT
        };

        let mut used_oauth = false;
        let result = match &tc {
            TransportConfig::Stdio { command, args, env } => {
                match spawn_stdio_transport(command, args, env) {
                    Ok(t) => {
                        tokio::time::timeout(timeout, rmcp::service::serve_client((), t)).await
                    }
                    Err(e) => {
                        Self::insert_failed(self, server_name, format!("stdio 失败: {e}"));
                        return Err(McpPoolError::ConnectionFailed {
                            server: server_name.to_string(),
                            reason: format!("stdio 失败: {e}"),
                        });
                    }
                }
            }
            TransportConfig::StreamableHttp {
                url,
                headers,
                oauth,
            } => {
                // 与 run_initialize 一致：检查磁盘是否有已保存的 OAuth 凭证
                let oauth_cfg = oauth.as_ref().cloned().or_else(|| {
                    let token_store = Arc::new(FileCredentialStore::new());
                    match tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(token_store.load_server(server_name))
                    }) {
                        Ok(Some(_)) => {
                            tracing::info!(server = %server_name, "发现已保存的 OAuth 凭证，使用默认配置恢复");
                            Some(super::config::OAuthConfig::default())
                        }
                        _ => None,
                    }
                });
                let has_oauth = oauth_cfg.is_some();
                if let (Some(cfg), Some(cb)) = (oauth_cfg, oauth_event_callback) {
                    let ts = Arc::new(FileCredentialStore::new());
                    let mut mgr = OAuthFlowManager::new(ts, cb);
                    match mgr.run_oauth_flow(server_name, url, &cfg).await {
                        Ok(()) => {
                            used_oauth = true;
                            if let Some(am) = mgr.get_authorization_manager(server_name) {
                                tokio::time::timeout(
                                    timeout,
                                    rmcp::service::serve_client(
                                        (),
                                        build_authed_transport(url, headers, am),
                                    ),
                                )
                                .await
                            } else {
                                tokio::time::timeout(
                                    timeout,
                                    rmcp::service::serve_client(
                                        (),
                                        build_http_transport(url, headers),
                                    ),
                                )
                                .await
                            }
                        }
                        Err(e) => {
                            tracing::warn!(server = %server_name, error = %e, "OAuth 恢复失败，尝试裸连接");
                            tokio::time::timeout(
                                timeout,
                                rmcp::service::serve_client((), build_http_transport(url, headers)),
                            )
                            .await
                        }
                    }
                } else if has_oauth {
                    // 有 OAuth 配置但没有 callback（非 TUI 场景），直接裸连接
                    used_oauth = true;
                    tokio::time::timeout(
                        timeout,
                        rmcp::service::serve_client((), build_http_transport(url, headers)),
                    )
                    .await
                } else {
                    tokio::time::timeout(
                        timeout,
                        rmcp::service::serve_client((), build_http_transport(url, headers)),
                    )
                    .await
                }
            }
        };

        match result {
            Ok(Ok(rs)) => {
                let tools =
                    rs.list_all_tools()
                        .await
                        .map_err(|e| McpPoolError::ToolDiscoveryFailed {
                            server: server_name.to_string(),
                            reason: e.to_string(),
                        })?;
                let resources = rs.list_all_resources().await.unwrap_or_default();
                let peer = rs.peer().clone();
                let oauth_status = if used_oauth {
                    OAuthStatus::Authorized
                } else {
                    OAuthStatus::default()
                };
                self.clients.write().insert(
                    server_name.to_string(),
                    Arc::new(McpClientHandle {
                        name: server_name.to_string(),
                        peer: Some(peer),
                        tools,
                        resources,
                        status: ClientStatus::Connected,
                        oauth_status,
                        source: server_config.source.clone(),
                        url: server_config.url.clone(),
                    }),
                );
                self.services
                    .lock()
                    .await
                    .insert(server_name.to_string(), rs);
                Ok(())
            }
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if Self::is_auth_required_error(&err_str, is_http) {
                    Self::insert_needs_auth(self, server_name, err_str.clone());
                } else {
                    Self::insert_failed(self, server_name, err_str.clone());
                }
                Err(McpPoolError::ConnectionFailed {
                    server: server_name.to_string(),
                    reason: err_str,
                })
            }
            Err(_) => {
                let msg = "连接超时";
                Self::insert_failed(self, server_name, msg.to_string());
                Err(McpPoolError::ConnectionFailed {
                    server: server_name.to_string(),
                    reason: msg.to_string(),
                })
            }
        }
    }

    pub async fn start_oauth_flow(
        self: &Arc<Self>,
        server_name: &str,
        oauth_event_callback: Box<dyn Fn(OAuthFlowEvent) + Send + Sync>,
    ) -> Result<(), McpPoolError> {
        let cfg = self
            .configs
            .read()
            .get(server_name)
            .cloned()
            .ok_or_else(|| McpPoolError::NotConnected {
                server: server_name.to_string(),
                status: ClientStatus::Disconnected,
            })?;
        let url = cfg.url.as_deref().unwrap_or("").to_string();
        // 使用显式 OAuth 配置，或对 HTTP 服务器回退到默认配置（启用 DCR 自动发现）
        let oauth_cfg = match cfg.oauth.as_ref().filter(|o| o.is_enabled()) {
            Some(explicit) => explicit.clone(),
            None => {
                if cfg.url.is_none() {
                    return Err(McpPoolError::ConnectionFailed {
                        server: server_name.to_string(),
                        reason: "仅 HTTP 传输支持 OAuth".to_string(),
                    });
                }
                super::config::OAuthConfig::default()
            }
        };
        let ts = Arc::new(FileCredentialStore::new());
        let mut mgr = OAuthFlowManager::new(ts, oauth_event_callback);
        mgr.run_oauth_flow(server_name, &url, &oauth_cfg)
            .await
            .map_err(|e| McpPoolError::ConnectionFailed {
                server: server_name.to_string(),
                reason: format!("OAuth 授权失败: {e}"),
            })?;

        // 从 OAuth 流程中提取 AuthorizationManager，用于构建认证传输层
        let auth_manager = mgr.get_authorization_manager(server_name).ok_or_else(|| {
            McpPoolError::ConnectionFailed {
                server: server_name.to_string(),
                reason: "OAuth 授权完成但无法提取 AuthorizationManager".to_string(),
            }
        })?;

        // 关闭旧连接
        if let Some(mut svc) = self.services.lock().await.remove(server_name) {
            let _ = svc.close_with_timeout(SHUTDOWN_TIMEOUT).await;
        }
        self.clients.write().remove(server_name);

        // 使用认证传输层重新连接
        let headers = cfg.headers.clone().unwrap_or_default();
        let result = tokio::time::timeout(
            HTTP_CONNECT_TIMEOUT,
            rmcp::service::serve_client((), build_authed_transport(&url, &headers, auth_manager)),
        )
        .await;

        match result {
            Ok(Ok(rs)) => {
                let tools =
                    rs.list_all_tools()
                        .await
                        .map_err(|e| McpPoolError::ToolDiscoveryFailed {
                            server: server_name.to_string(),
                            reason: e.to_string(),
                        })?;
                let resources = rs.list_all_resources().await.unwrap_or_default();
                let peer = rs.peer().clone();
                let handle = Arc::new(McpClientHandle {
                    name: server_name.to_string(),
                    peer: Some(peer),
                    tools,
                    resources,
                    status: ClientStatus::Connected,
                    oauth_status: OAuthStatus::Authorized,
                    source: cfg.source.clone(),
                    url: cfg.url.clone(),
                });
                self.clients.write().insert(server_name.to_string(), handle);
                self.services
                    .lock()
                    .await
                    .insert(server_name.to_string(), rs);
                Ok(())
            }
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if Self::is_auth_required_error(&err_str, true) {
                    Self::insert_needs_auth(self, server_name, err_str.clone());
                } else {
                    Self::insert_failed(self, server_name, err_str.clone());
                }
                Err(McpPoolError::ConnectionFailed {
                    server: server_name.to_string(),
                    reason: err_str,
                })
            }
            Err(_) => {
                let msg = "连接超时".to_string();
                Self::insert_failed(self, server_name, msg.clone());
                Err(McpPoolError::ConnectionFailed {
                    server: server_name.to_string(),
                    reason: msg,
                })
            }
        }
    }

    /// 清除指定服务器的 OAuth 凭证并断开连接
    pub async fn clear_oauth(self: &Arc<Self>, server_name: &str) -> Result<(), McpPoolError> {
        // 1. 清除 token 文件中的凭证
        let store = FileCredentialStore::new();
        let _ = store.clear_server(server_name).await;

        // 2. 关闭连接
        if let Some(mut svc) = self.services.lock().await.remove(server_name) {
            let _ = svc.close_with_timeout(SHUTDOWN_TIMEOUT).await;
        }

        // 3. 更新 handle 为 NeedsAuthorization
        let (source, url) = self
            .configs
            .read()
            .get(server_name)
            .map(|c| (c.source.clone(), c.url.clone()))
            .unwrap_or((None, None));
        self.clients.write().insert(
            server_name.to_string(),
            Arc::new(McpClientHandle {
                name: server_name.to_string(),
                peer: None,
                tools: vec![],
                resources: vec![],
                status: ClientStatus::Failed("OAuth credentials cleared".to_string()),
                oauth_status: OAuthStatus::NeedsAuthorization,
                source,
                url,
            }),
        );

        Ok(())
    }

    pub async fn remove_server(self: &Arc<Self>, server_name: &str) {
        self.clients.write().remove(server_name);
        if let Some(mut svc) = self.services.lock().await.remove(server_name) {
            let _ = svc.close_with_timeout(SHUTDOWN_TIMEOUT).await;
        }
        self.configs.write().remove(server_name);
    }

    /// 将服务器标记为 Disabled：关闭连接但保留 config 和 handle（用于面板展示）
    pub async fn set_disabled(self: &Arc<Self>, server_name: &str) {
        // 关闭实际连接
        if let Some(mut svc) = self.services.lock().await.remove(server_name) {
            let _ = svc.close_with_timeout(SHUTDOWN_TIMEOUT).await;
        }
        // 更新 handle 为 Disabled 状态（保留 config 引用）
        let (source, url) = self
            .configs
            .read()
            .get(server_name)
            .map(|c| (c.source.clone(), c.url.clone()))
            .unwrap_or((None, None));
        self.clients.write().insert(
            server_name.to_string(),
            Arc::new(McpClientHandle {
                name: server_name.to_string(),
                peer: None,
                tools: vec![],
                resources: vec![],
                status: ClientStatus::Disabled,
                oauth_status: OAuthStatus::default(),
                source,
                url,
            }),
        );
    }

    pub fn server_infos(&self) -> Vec<ServerInfo> {
        self.clients
            .read()
            .values()
            .map(|h| ServerInfo {
                name: h.name.clone(),
                transport_type: if h.url.is_some() { "http" } else { "stdio" }.to_string(),
                status: h.status.clone(),
                tool_count: h.tools.len(),
                resource_count: h.resources.len(),
                oauth_status: h.oauth_status.clone(),
                source: h.source.clone(),
                url: h.url.clone(),
                plugin_source: self.plugin_source_of(&h.name),
            })
            .collect()
    }

    /// 返回所有 MCP 服务器信息（合并 configs + clients）
    ///
    /// config 中有但 clients 中没有的 server 会被标记为 Uninitialized。
    /// 这覆盖了连接失败后被移除、运行时新增配置、以及 disabled 后被清理等场景。
    pub fn all_server_infos(&self) -> Vec<ServerInfo> {
        let clients = self.clients.read();
        let configs = self.configs.read();

        let mut result: Vec<ServerInfo> = Vec::new();

        // 先遍历 clients 表中的所有条目
        for h in clients.values() {
            result.push(ServerInfo {
                name: h.name.clone(),
                transport_type: if h.url.is_some() { "http" } else { "stdio" }.to_string(),
                status: h.status.clone(),
                tool_count: h.tools.len(),
                resource_count: h.resources.len(),
                oauth_status: h.oauth_status.clone(),
                source: h.source.clone(),
                url: h.url.clone(),
                plugin_source: self.plugin_source_of(&h.name),
            });
        }

        // 遍历 configs，补充 clients 中不存在的条目（标记为 Uninitialized）
        for (name, sc) in configs.iter() {
            if !clients.contains_key(name) {
                result.push(ServerInfo {
                    name: name.clone(),
                    transport_type: if sc.url.is_some() { "http" } else { "stdio" }.to_string(),
                    status: ClientStatus::Uninitialized,
                    tool_count: 0,
                    resource_count: 0,
                    oauth_status: OAuthStatus::default(),
                    source: sc.source.clone(),
                    url: sc.url.clone(),
                    plugin_source: self.plugin_source_of(name),
                });
            }
        }

        result
    }

    pub fn get_tools(&self, name: &str) -> Vec<Tool> {
        self.clients
            .read()
            .get(name)
            .map(|h| h.tools.clone())
            .unwrap_or_default()
    }
    pub fn get_resources(&self, name: &str) -> Vec<Resource> {
        self.clients
            .read()
            .get(name)
            .map(|h| h.resources.clone())
            .unwrap_or_default()
    }
    pub fn get_client(&self, name: &str) -> Option<Arc<McpClientHandle>> {
        self.clients.read().get(name).cloned()
    }
    pub fn get_all_clients(&self) -> Vec<Arc<McpClientHandle>> {
        self.clients
            .read()
            .values()
            .filter(|c| matches!(c.status, ClientStatus::Connected))
            .cloned()
            .collect()
    }
    pub fn has_resources(&self) -> bool {
        self.clients
            .read()
            .values()
            .any(|c| matches!(c.status, ClientStatus::Connected) && !c.resources.is_empty())
    }
    pub fn resource_summary(&self) -> String {
        self.clients
            .read()
            .values()
            .filter(|c| matches!(c.status, ClientStatus::Connected) && !c.resources.is_empty())
            .map(|c| {
                format!(
                    "- server \"{}\": {} ({} resources)",
                    c.name,
                    c.resources
                        .iter()
                        .map(|r| r.raw.uri.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    c.resources.len()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub async fn shutdown(&self) {
        let names: Vec<String> = self.clients.read().keys().cloned().collect();
        for name in &names {
            if let Some(c) = self.clients.write().get_mut(name) {
                if matches!(c.status, ClientStatus::Connected) {
                    tracing::info!(server = %name, "关闭连接");
                }
                let h = Arc::make_mut(c);
                h.status = ClientStatus::Disconnected;
                h.peer = None;
            }
        }
        for (_name, mut svc) in self.services.lock().await.drain() {
            let _ = svc.close_with_timeout(SHUTDOWN_TIMEOUT).await;
        }
    }

    pub async fn initialize(
        cwd: &Path,
        claude_home: &Path,
        oauth_event_callback: Option<Box<dyn Fn(OAuthFlowEvent) + Send + Sync>>,
    ) -> Self {
        let (config, plugin_sources) = super::load_merged_config_full(cwd, claude_home);
        let pool = Arc::new(Self::new_pending());
        *pool.plugin_sources.write() = plugin_sources;
        let token_store = Arc::new(FileCredentialStore::new());
        let mut oauth_manager: Option<OAuthFlowManager> =
            oauth_event_callback.map(|cb| OAuthFlowManager::new(token_store, cb));

        for (name, sc) in &config.mcp_servers {
            pool.configs.write().insert(name.clone(), sc.clone());
        }

        for (name, server_config) in &config.mcp_servers {
            // 跳过已禁用的服务器，注册为 Disabled 状态
            if server_config.disabled.unwrap_or(false) {
                tracing::info!(server = %name, "MCP 服务器已禁用，跳过连接");
                pool.clients.write().insert(
                    name.clone(),
                    Arc::new(McpClientHandle {
                        name: name.clone(),
                        peer: None,
                        tools: vec![],
                        resources: vec![],
                        status: ClientStatus::Disabled,
                        oauth_status: OAuthStatus::default(),
                        source: server_config.source.clone(),
                        url: server_config.url.clone(),
                    }),
                );
                continue;
            }
            let tc = match TransportConfig::try_from(server_config) {
                Ok(tc) => tc,
                Err(e) => {
                    Self::insert_failed(&pool, name, format!("传输层构建失败: {e}"));
                    continue;
                }
            };
            let is_http = matches!(tc, TransportConfig::StreamableHttp { .. });
            let timeout = if is_http {
                HTTP_CONNECT_TIMEOUT
            } else {
                STDIO_CONNECT_TIMEOUT
            };

            let mut used_oauth = false;
            let connect_result = match tc {
                TransportConfig::Stdio {
                    ref command,
                    ref args,
                    ref env,
                } => match spawn_stdio_transport(command, args, env) {
                    Ok(t) => {
                        tokio::time::timeout(timeout, rmcp::service::serve_client((), t)).await
                    }
                    Err(e) => {
                        Self::insert_failed(&pool, name, format!("stdio 失败: {e}"));
                        continue;
                    }
                },
                TransportConfig::StreamableHttp {
                    ref url,
                    ref headers,
                    ref oauth,
                } => {
                    let oauth_cfg = oauth.as_ref().cloned().or_else(|| {
                        if let Some(ref mgr) = oauth_manager {
                            let token_store = mgr.token_store();
                            match tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(token_store.load_server(name))) {
                                Ok(Some(_)) => {
                                    tracing::info!(server = %name, "发现已保存的 OAuth 凭证，使用默认配置恢复");
                                    Some(super::config::OAuthConfig::default())
                                }
                                _ => None,
                            }
                        } else {
                            None
                        }
                    });
                    if let (Some(ref cfg), Some(ref mut mgr)) = (oauth_cfg, &mut oauth_manager) {
                        match mgr.run_oauth_flow(name, url, cfg).await {
                            Ok(()) => {
                                used_oauth = true;
                                if let Some(am) = mgr.get_authorization_manager(name) {
                                    tokio::time::timeout(
                                        timeout,
                                        rmcp::service::serve_client(
                                            (),
                                            build_authed_transport(url, headers, am),
                                        ),
                                    )
                                    .await
                                } else {
                                    tokio::time::timeout(
                                        timeout,
                                        rmcp::service::serve_client(
                                            (),
                                            build_http_transport(url, headers),
                                        ),
                                    )
                                    .await
                                }
                            }
                            Err(e) => {
                                tracing::warn!(server = %name, error = %e, "OAuth 恢复失败，尝试裸连接");
                                tokio::time::timeout(
                                    timeout,
                                    rmcp::service::serve_client(
                                        (),
                                        build_http_transport(url, headers),
                                    ),
                                )
                                .await
                            }
                        }
                    } else {
                        tokio::time::timeout(
                            timeout,
                            rmcp::service::serve_client((), build_http_transport(url, headers)),
                        )
                        .await
                    }
                }
            };

            match connect_result {
                Ok(Ok(rs)) => {
                    let tools = rs.list_all_tools().await.unwrap_or_default();
                    let resources = rs.list_all_resources().await.unwrap_or_default();
                    let peer = rs.peer().clone();
                    let oauth_status = if used_oauth {
                        OAuthStatus::Authorized
                    } else {
                        OAuthStatus::default()
                    };
                    pool.clients.write().insert(
                        name.clone(),
                        Arc::new(McpClientHandle {
                            name: name.clone(),
                            peer: Some(peer),
                            tools,
                            resources,
                            status: ClientStatus::Connected,
                            oauth_status,
                            source: server_config.source.clone(),
                            url: server_config.url.clone(),
                        }),
                    );
                    pool.services.lock().await.insert(name.clone(), rs);
                }
                Ok(Err(e)) => {
                    let err_str = e.to_string();
                    if Self::is_auth_required_error(&err_str, is_http) {
                        Self::insert_needs_auth(&pool, name, err_str);
                    } else {
                        Self::insert_failed(&pool, name, err_str);
                    }
                }
                Err(_) => {
                    Self::insert_failed(&pool, name, "连接超时".into());
                }
            }
        }

        Arc::try_unwrap(pool).unwrap_or_else(|arc| {
            let p = arc.as_ref();
            Self {
                clients: parking_lot::RwLock::new(p.clients.read().clone()),
                services: tokio::sync::Mutex::new(HashMap::new()),
                configs: parking_lot::RwLock::new(p.configs.read().clone()),
                plugin_sources: parking_lot::RwLock::new(p.plugin_sources.read().clone()),
            }
        })
    }
}

fn spawn_stdio_transport(
    command: &str,
    args: &[String],
    env: &HashMap<String, String>,
) -> std::io::Result<rmcp::transport::child_process::TokioChildProcess> {
    use std::process::Stdio;

    // 使用 builder 模式以获取 stderr 句柄
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args).envs(env);

    let builder = rmcp::transport::child_process::TokioChildProcess::builder(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let (child_process, stderr_opt) = builder.spawn()?;

    // 启动后台任务消费 stderr 并记录到 tracing
    if let Some(stderr) = stderr_opt {
        let cmd_name = command.to_string();
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(
                    command = %cmd_name,
                    stderr = %line,
                    "MCP 子进程 stderr"
                );
            }
        });
    }

    Ok(child_process)
}

fn build_http_transport(
    url: &str,
    headers: &HashMap<String, String>,
) -> rmcp::transport::StreamableHttpClientTransport<reqwest::Client> {
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    let mut config = StreamableHttpClientTransportConfig::with_uri(url);
    let mut custom_headers = std::collections::HashMap::new();
    for (key, value) in headers {
        match reqwest::header::HeaderName::try_from(key.as_str()) {
            Ok(name) => match reqwest::header::HeaderValue::from_str(value) {
                Ok(val) => {
                    custom_headers.insert(name, val);
                }
                Err(e) => {
                    tracing::warn!(header = %key, error = %e, "header 值无效");
                }
            },
            Err(e) => {
                tracing::warn!(header = %key, error = %e, "header 名称无效");
            }
        }
    }
    if !custom_headers.is_empty() {
        config = config.custom_headers(custom_headers);
    }
    rmcp::transport::StreamableHttpClientTransport::with_client(reqwest::Client::new(), config)
}

fn build_authed_transport(
    url: &str,
    headers: &HashMap<String, String>,
    auth_manager: rmcp::transport::auth::AuthorizationManager,
) -> rmcp::transport::StreamableHttpClientTransport<
    rmcp::transport::auth::AuthClient<reqwest::Client>,
> {
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
    let mut config = StreamableHttpClientTransportConfig::with_uri(url);
    let mut custom_headers = std::collections::HashMap::new();
    for (key, value) in headers {
        match reqwest::header::HeaderName::try_from(key.as_str()) {
            Ok(name) => match reqwest::header::HeaderValue::from_str(value) {
                Ok(val) => {
                    custom_headers.insert(name, val);
                }
                Err(e) => {
                    tracing::warn!(header = %key, error = %e, "header 值无效");
                }
            },
            Err(e) => {
                tracing::warn!(header = %key, error = %e, "header 名称无效");
            }
        }
    }
    if !custom_headers.is_empty() {
        config = config.custom_headers(custom_headers);
    }
    let auth_client = rmcp::transport::auth::AuthClient::new(reqwest::Client::new(), auth_manager);
    rmcp::transport::StreamableHttpClientTransport::with_client(auth_client, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_get_all_clients_filters_disconnected() {
        let pool = McpClientPool::new_empty();
        assert!(pool.get_all_clients().is_empty());
    }
    #[test]
    fn test_pool_has_no_resources() {
        assert!(!McpClientPool::new_empty().has_resources());
    }
    #[test]
    fn test_resource_summary_empty() {
        assert!(McpClientPool::new_empty().resource_summary().is_empty());
    }
    #[test]
    fn test_client_status_equality() {
        assert_eq!(ClientStatus::Connected, ClientStatus::Connected);
        assert_ne!(
            ClientStatus::Failed("a".into()),
            ClientStatus::Failed("b".into())
        );
    }
    #[test]
    fn test_mcp_init_status_equality() {
        assert_eq!(McpInitStatus::Pending, McpInitStatus::Pending);
        assert_eq!(
            McpInitStatus::Initializing {
                connected: 1,
                total: 2
            },
            McpInitStatus::Initializing {
                connected: 1,
                total: 2
            }
        );
        assert_ne!(
            McpInitStatus::Ready { total: 3 },
            McpInitStatus::Ready { total: 4 }
        );
    }
    #[test]
    fn test_new_pending_creates_empty_pool() {
        let pool = McpClientPool::new_pending();
        assert!(pool.clients.read().is_empty());
    }
    #[test]
    fn test_server_infos_empty_pool() {
        assert!(McpClientPool::new_pending().server_infos().is_empty());
    }
    #[tokio::test]
    async fn test_insert_failed() {
        let pool = Arc::new(McpClientPool::new_pending());
        McpClientPool::insert_failed(&pool, "s", "err".into());
        assert_eq!(
            pool.server_infos()[0].status,
            ClientStatus::Failed("err".into())
        );
    }
    #[tokio::test]
    async fn test_remove_server() {
        let pool = Arc::new(McpClientPool::new_pending());
        pool.clients.write().insert(
            "a".into(),
            Arc::new(McpClientHandle {
                name: "a".into(),
                peer: None,
                tools: vec![],
                resources: vec![],
                status: ClientStatus::Connected,
                oauth_status: OAuthStatus::default(),
                source: None,
                url: None,
            }),
        );
        pool.remove_server("a").await;
        assert!(pool.server_infos().is_empty());
    }
    #[tokio::test]
    async fn test_get_tools_resources() {
        let pool = McpClientPool::new_pending();
        pool.clients.write().insert(
            "s".into(),
            Arc::new(McpClientHandle {
                name: "s".into(),
                peer: None,
                tools: vec![],
                resources: vec![],
                status: ClientStatus::Connected,
                oauth_status: OAuthStatus::default(),
                source: None,
                url: None,
            }),
        );
        assert!(pool.get_tools("s").is_empty());
        assert!(pool.get_tools("x").is_empty());
    }

    #[test]
    fn test_plugin_source_of_empty_pool_returns_none() {
        let pool = McpClientPool::new_pending();
        assert!(pool.plugin_source_of("any").is_none());
    }

    #[test]
    fn test_plugin_source_of_after_write_returns_value() {
        let pool = McpClientPool::new_pending();
        pool.plugin_sources
            .write()
            .insert("p1__srv1".to_string(), "p1@marketplace_a".to_string());
        assert_eq!(
            pool.plugin_source_of("p1__srv1"),
            Some("p1@marketplace_a".to_string())
        );
    }

    #[test]
    fn test_plugin_source_of_nonexistent_returns_none() {
        let pool = McpClientPool::new_pending();
        pool.plugin_sources
            .write()
            .insert("p1__srv1".to_string(), "p1@alpha".to_string());
        assert!(pool.plugin_source_of("nonexistent").is_none());
    }
}
