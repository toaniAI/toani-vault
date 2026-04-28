//! 沙箱池管理

use crate::crypto::hkdf::KeyHierarchy;
use crate::tee::SharedEnclave;
use crate::tee::sandbox::{
    PoolStatus, SandboxHealth,
    browser_runtime::SandboxBrowserRuntime,
    config::{MountConfig, MountType, NsjailConfig, SandboxConfig, SandboxPoolConfig},
    error::{SandboxError, SessionError},
    nsjail::{NsjailSandbox, WarmNsjailInstance},
    repository::{NewSandboxSessionRecord, SandboxRepository, metadata_to_json, to_chrono_utc},
    session::{
        ActiveNsjailSession, RecoveredSandboxResource, RecoveredSandboxResourceKind,
        SandboxResourceRecovery, SandboxReusePolicy, SandboxSession,
    },
    types::{SandboxId, SessionContext, SessionId, SessionRequest},
};
use crate::vault::storage::CredentialVault;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
struct PoolRecoveryHandle {
    warm_instances: Arc<Mutex<VecDeque<WarmNsjailInstance>>>,
    active_sessions: Arc<RwLock<HashMap<SessionId, ActiveNsjailSession>>>,
    repository: Option<Arc<dyn SandboxRepository>>,
}

/// 沙箱池 trait
#[async_trait::async_trait]
pub trait SandboxPool: Send + Sync {
    /// 获取会话
    async fn acquire_session(
        &self,
        request: SessionRequest,
    ) -> Result<Arc<dyn SandboxSession>, SandboxError>;
    /// 释放会话
    async fn release_session(&self, session_id: SessionId) -> Result<(), SandboxError>;
    /// 获取会话
    async fn get_session(
        &self,
        session_id: SessionId,
    ) -> Result<Arc<dyn SandboxSession>, SandboxError>;
    /// 获取健康状态
    async fn health(&self) -> SandboxHealth;
    /// 关闭池
    async fn shutdown(&self) -> Result<(), SandboxError>;
    /// 转换为 Any 以便 downcast
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Nsjail 沙箱池
pub struct NsjailSandboxPool {
    /// 配置
    config: SandboxPoolConfig,
    /// 热实例队列
    warm_instances: Arc<Mutex<VecDeque<WarmNsjailInstance>>>,
    /// 活跃会话
    active_sessions: Arc<RwLock<HashMap<SessionId, ActiveNsjailSession>>>,
    /// 沙箱模板配置
    sandbox_config: SandboxConfig,
    /// 池状态
    status: Arc<RwLock<PoolStatus>>,
    /// 清理任务句柄
    cleanup_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// 持久化仓储
    repository: Option<Arc<dyn SandboxRepository>>,
    /// 凭证 Vault
    vault: Option<Arc<CredentialVault>>,
    /// 密钥层次
    key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
    /// 共享 TEE Enclave
    enclave: Option<SharedEnclave>,
    /// browser runtime 自检失败摘要
    browser_runtime_probe_error: Arc<RwLock<Option<String>>>,
}

impl NsjailSandboxPool {
    /// 创建新的沙箱池
    pub fn new(config: SandboxConfig) -> Self {
        Self::new_with_repository(config, None)
    }

    /// 创建带持久化仓储的沙箱池
    pub fn new_with_repository(
        config: SandboxConfig,
        repository: Option<Arc<dyn SandboxRepository>>,
    ) -> Self {
        Self::new_with_dependencies(config, repository, None, None, None)
    }

    pub fn new_with_dependencies(
        config: SandboxConfig,
        repository: Option<Arc<dyn SandboxRepository>>,
        vault: Option<Arc<CredentialVault>>,
        key_hierarchy: Option<Arc<RwLock<KeyHierarchy>>>,
        enclave: Option<SharedEnclave>,
    ) -> Self {
        let pool_config = config.pool.clone();

        Self {
            config: pool_config.clone(),
            warm_instances: Arc::new(Mutex::new(VecDeque::with_capacity(
                pool_config.max_warm_instances,
            ))),
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            sandbox_config: config,
            status: Arc::new(RwLock::new(PoolStatus::Initializing)),
            cleanup_handle: Arc::new(RwLock::new(None)),
            repository,
            vault,
            key_hierarchy,
            enclave,
            browser_runtime_probe_error: Arc::new(RwLock::new(None)),
        }
    }

    fn recovery_handle(&self) -> Arc<dyn SandboxResourceRecovery> {
        Arc::new(PoolRecoveryHandle {
            warm_instances: Arc::clone(&self.warm_instances),
            active_sessions: Arc::clone(&self.active_sessions),
            repository: self.repository.clone(),
        })
    }

    /// 初始化池
    pub async fn initialize(&self) -> Result<(), SandboxError> {
        info!("Initializing NsjailSandboxPool");

        // 创建最小数量的热实例
        let min_instances = self.config.min_warm_instances;
        for i in 0..min_instances {
            match self.create_warm_instance().await {
                Ok(instance) => {
                    let mut warm_instances = self.warm_instances.lock().await;
                    warm_instances.push_back(instance);
                    debug!("Created warm instance {}/{}", i + 1, min_instances);
                }
                Err(e) => {
                    warn!("Failed to create warm instance {}: {}", i, e);
                }
            }
        }

        // 启动清理任务
        self.start_cleanup_task().await;

        if let Some(repository) = &self.repository {
            let updated = repository
                .reconcile_orphaned_active_sessions("service_startup_recovery")
                .await?;
            if updated > 0 {
                warn!(
                    updated,
                    "Recovered orphaned active sandbox sessions on startup"
                );
            }
        }

        let browser_runtime_probe_error =
            self.run_browser_runtime_probe().await.err().map(|error| {
                warn!(
                    "Browser runtime probe failed during sandbox pool initialization: {}",
                    error
                );
                error.to_string()
            });
        *self.browser_runtime_probe_error.write().await = browser_runtime_probe_error;

        *self.status.write().await = PoolStatus::Running;
        info!("NsjailSandboxPool initialized successfully");

        Ok(())
    }

    /// 从热实例获取沙箱
    async fn acquire_from_warm(&self) -> Result<Option<NsjailSandbox>, SandboxError> {
        let mut warm_instances = self.warm_instances.lock().await;

        while let Some(mut instance) = warm_instances.pop_front() {
            // 检查实例是否健康
            if !instance.is_healthy() {
                warn!(
                    "Warm instance {} is not healthy, discarding",
                    instance.info.instance_id
                );
                continue;
            }

            // 检查是否过期
            if instance.is_expired(self.config.warm_instance_ttl_secs) {
                warn!(
                    "Warm instance {} has expired, discarding",
                    instance.info.instance_id
                );
                continue;
            }

            // 实例可用
            instance.touch();
            info!("Acquired warm instance {}", instance.info.instance_id);
            return Ok(instance.sandbox);
        }

        Ok(None)
    }

    /// 创建新的热实例
    async fn create_warm_instance(&self) -> Result<WarmNsjailInstance, SandboxError> {
        let sandbox_id = SandboxId::new();
        let nsjail_config = self.create_nsjail_config();

        let mut sandbox = NsjailSandbox::new(nsjail_config);
        sandbox.start().await?;

        let pid = sandbox.pid().unwrap_or(0);

        let mut instance = WarmNsjailInstance::new(sandbox_id, pid);
        instance.sandbox = Some(sandbox);

        Ok(instance)
    }

    /// 创建 nsjail 配置
    fn create_nsjail_config(&self) -> NsjailConfig {
        let mut sandbox = self.sandbox_config.clone();
        let frontend_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend");
        if frontend_dir.exists()
            && !sandbox
                .security
                .namespace
                .mount_points
                .iter()
                .any(|mount| mount.src == frontend_dir)
        {
            sandbox.security.namespace.mount_points.push(MountConfig {
                src: frontend_dir.clone(),
                dst: frontend_dir,
                mount_type: MountType::Bind,
                read_only: true,
            });
        }

        let mut env = std::collections::HashMap::new();
        if let Some(path) = std::env::var_os("PATH") {
            env.insert("PATH".to_string(), path.to_string_lossy().to_string());
        }
        for key in [
            "CREDBRIDGE_SANDBOX_NODE_BINARY",
            "NODE_PATH",
            "LIGHTPANDA_BINARY_PATH",
            "LIGHTPANDA_DISABLE_TELEMETRY",
        ] {
            if let Some(value) = std::env::var_os(key) {
                env.insert(key.to_string(), value.to_string_lossy().to_string());
            }
        }

        NsjailConfig {
            sandbox,
            command: vec!["sleep".to_string(), "3600".to_string()], // 长时间运行的占位命令
            cwd: std::path::PathBuf::from("/"),
            env,
            // Session sandboxes can later host browser scoped processes. Keep the
            // session jail on the same relaxed browser policy so Lightpanda child
            // process creation is not killed by the base denylist.
            disable_seccomp_for_browser_runtime: true,
            enable_user_namespace: true,
            uid_map: Default::default(),
            gid_map: Default::default(),
        }
    }

    async fn run_browser_runtime_probe(&self) -> Result<(), SandboxError> {
        let mut sandbox = NsjailSandbox::new(self.create_nsjail_config());
        sandbox.start().await?;

        let work_dir = sandbox.working_dir();
        let runtime = match SandboxBrowserRuntime::launch(work_dir, &sandbox).await {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = sandbox.stop().await;
                return Err(error);
            }
        };
        let _ = runtime.close().await;
        sandbox.stop().await?;
        Ok(())
    }

    /// 创建会话上下文
    fn create_session_context(
        &self,
        request: &SessionRequest,
        session_id: SessionId,
        sandbox_id: SandboxId,
    ) -> SessionContext {
        let now = OffsetDateTime::now_utc();
        let expires_at = now + Duration::minutes(self.config.session_timeout_minutes as i64);

        SessionContext {
            session_id,
            sandbox_id,
            tenant_id: request.tenant_id,
            user_id: request.user_id,
            credential_id: request.credential_id,
            original_intent: request.original_intent.clone(),
            created_at: now,
            expires_at,
            last_activity_at: Arc::new(RwLock::new(now)),
        }
    }

    /// 启动清理任务
    async fn start_cleanup_task(&self) {
        let warm_instances = Arc::clone(&self.warm_instances);
        let active_sessions = Arc::clone(&self.active_sessions);
        let config = self.config.clone();
        let repository = self.repository.clone();

        let handle = tokio::spawn(async move {
            let mut ticker = interval(tokio::time::Duration::from_secs(
                config.cleanup_interval_secs,
            ));

            loop {
                ticker.tick().await;

                // 清理过期的热实例
                {
                    let mut instances = warm_instances.lock().await;
                    let before_count = instances.len();
                    instances.retain(|instance| {
                        let keep = instance.is_healthy()
                            && !instance.is_expired(config.warm_instance_ttl_secs);
                        if !keep {
                            debug!("Cleaning up warm instance {}", instance.info.instance_id);
                        }
                        keep
                    });
                    let after_count = instances.len();
                    if before_count != after_count {
                        debug!(
                            "Cleaned up {} expired warm instances",
                            before_count - after_count
                        );
                    }
                }

                // 清理过期的会话
                {
                    let sessions = active_sessions.read().await;
                    let expired_sessions: Vec<SessionId> = sessions
                        .iter()
                        .filter(|(_, session)| session.context().is_expired())
                        .map(|(id, _)| *id)
                        .collect();
                    drop(sessions);

                    for session_id in expired_sessions {
                        warn!("Session {} has expired, cleaning up", session_id);
                        let mut sessions = active_sessions.write().await;
                        if let Some(session) = sessions.remove(&session_id) {
                            // 尝试关闭会话
                            if let Err(e) = session.close().await {
                                error!("Failed to close expired session {}: {}", session_id, e);
                            }
                            if let Some(repo) = &repository {
                                let now = chrono::Utc::now();
                                if let Err(error) = repo
                                    .mark_session_terminated(
                                        session_id,
                                        "expired",
                                        Some("expired_by_cleanup_task".to_string()),
                                        now,
                                    )
                                    .await
                                {
                                    error!(
                                        "Failed to persist expired session {} status: {}",
                                        session_id, error
                                    );
                                }
                            }
                        }
                    }
                }
            }
        });

        *self.cleanup_handle.write().await = Some(handle);
    }

    /// 获取活跃会话数
    pub async fn active_session_count(&self) -> usize {
        self.active_sessions.read().await.len()
    }

    /// 获取热实例数
    pub async fn warm_instance_count(&self) -> usize {
        self.warm_instances.lock().await.len()
    }

    /// 获取活跃会话快照
    pub async fn list_active_sessions(&self) -> Vec<ActiveNsjailSession> {
        self.active_sessions
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// 查询某个操作记录
    pub async fn find_operation(
        &self,
        operation_id: uuid::Uuid,
    ) -> Option<(SessionId, crate::tee::sandbox::session::OperationRecord)> {
        let sessions = self.active_sessions.read().await;
        for (session_id, session) in sessions.iter() {
            let history = session.get_operation_history().await;
            if let Some(record) = history
                .into_iter()
                .find(|item| item.operation_id == operation_id)
            {
                return Some((*session_id, record));
            }
        }
        None
    }

    /// 回收沙箱到热实例池
    ///
    /// 执行以下步骤：
    /// 1. 清理凭证环境变量
    /// 2. 重置沙箱状态
    /// 3. 验证沙箱健康
    /// 4. 重新加入热实例池
    async fn recycle_sandbox(&self, mut sandbox: NsjailSandbox) -> Result<(), SandboxError> {
        let sandbox_id = sandbox.id;
        info!("开始回收沙箱 {} 到热实例池", sandbox_id);

        // 检查沙箱是否仍在运行
        if !sandbox.is_running().await {
            warn!("沙箱 {} 已停止运行，无法回收", sandbox_id);
            return Err(SandboxError::NotRunning {
                sandbox_id: sandbox_id.into(),
            });
        }

        // 清理凭证环境
        if let Err(e) = sandbox.prepare_for_reuse().await {
            warn!("清理沙箱 {} 凭证环境失败: {}", sandbox_id, e);
            // 清理失败不阻断流程，继续尝试回收
        }

        // 验证沙箱健康状态
        match sandbox.stats().await {
            Ok(stats) => {
                debug!(
                    "沙箱 {} 健康检查通过: PID={}, 内存={}MB, FD={}",
                    sandbox_id,
                    stats.pid,
                    stats.memory_usage_bytes / 1024 / 1024,
                    stats.fd_count
                );
            }
            Err(e) => {
                warn!("沙箱 {} 健康检查失败: {}", sandbox_id, e);
                return Err(e);
            }
        }

        // 创建热实例
        let pid = sandbox.pid().unwrap_or(0);
        let mut instance = WarmNsjailInstance::new(sandbox_id, pid);
        instance.sandbox = Some(sandbox);
        instance.touch();

        // 添加到热实例池
        // 步骤1: 先检查池容量（只读，快速释放锁）
        let should_add = {
            let warm_instances = self.warm_instances.lock().await;
            warm_instances.len() < self.config.max_warm_instances
        };

        if !should_add {
            warn!("热实例池已满，无法回收沙箱 {}", sandbox_id);
            // 步骤2: 在锁外执行异步停止操作
            if let Some(mut s) = instance.sandbox {
                let _ = s.stop().await;
            }
            return Err(SandboxError::Pool("热实例池已满".to_string()));
        }

        // 步骤3: 重新获取锁添加实例（仅同步操作）
        let mut warm_instances = self.warm_instances.lock().await;
        // 双重检查：在释放锁期间可能有其他实例被添加
        if warm_instances.len() < self.config.max_warm_instances {
            warm_instances.push_back(instance);
            info!(
                "沙箱 {} 已回收到热实例池，当前热实例数: {}/{}",
                sandbox_id,
                warm_instances.len(),
                self.config.max_warm_instances
            );
        } else {
            // 池已满，释放锁后停止沙箱
            drop(warm_instances);
            warn!("热实例池已满（双重检查），无法回收沙箱 {}", sandbox_id);
            if let Some(mut s) = instance.sandbox {
                let _ = s.stop().await;
            }
            return Err(SandboxError::Pool("热实例池已满".to_string()));
        }

        Ok(())
    }
}

impl PoolRecoveryHandle {
    async fn recover_oldest_warm_instance(
        &self,
    ) -> Result<Option<RecoveredSandboxResource>, SandboxError> {
        let instance = self.warm_instances.lock().await.pop_front();
        let Some(mut instance) = instance else {
            return Ok(None);
        };

        if let Some(mut sandbox) = instance.sandbox.take()
            && let Err(error) = sandbox.stop().await
        {
            warn!(
                sandbox_id = %instance.info.instance_id,
                "Failed to stop recovered warm sandbox: {}",
                error
            );
        }

        Ok(Some(RecoveredSandboxResource {
            kind: RecoveredSandboxResourceKind::WarmInstance,
            sandbox_id: instance.info.instance_id,
            session_id: None,
        }))
    }

    async fn recover_oldest_active_session(
        &self,
        current_session_id: SessionId,
    ) -> Result<Option<RecoveredSandboxResource>, SandboxError> {
        let session_candidates: Vec<(SessionId, ActiveNsjailSession)> = {
            let sessions = self.active_sessions.read().await;
            sessions
                .iter()
                .filter(|(session_id, _)| **session_id != current_session_id)
                .map(|(session_id, session)| (*session_id, session.clone()))
                .collect()
        };

        let mut ranked_candidates = Vec::new();
        for (session_id, session) in session_candidates {
            if session.is_executing().await {
                continue;
            }
            ranked_candidates.push((session.last_activity_at().await, session_id));
        }
        ranked_candidates.sort_by_key(|(last_activity_at, _)| *last_activity_at);

        for (_, session_id) in ranked_candidates {
            let session = {
                let mut sessions = self.active_sessions.write().await;
                sessions.remove(&session_id)
            };
            let Some(session) = session else {
                continue;
            };

            if session.is_executing().await {
                self.active_sessions
                    .write()
                    .await
                    .insert(session_id, session);
                continue;
            }

            let sandbox_id = session.context().sandbox_id;
            if let Some(mut sandbox) = session.take_sandbox().await {
                if let Err(error) = sandbox.stop().await {
                    warn!(
                        session_id = %session_id,
                        sandbox_id = %sandbox_id,
                        "Failed to stop recovered active sandbox: {}",
                        error
                    );
                }
            } else if let Err(error) = session.close().await {
                warn!(
                    session_id = %session_id,
                    sandbox_id = %sandbox_id,
                    "Failed to close recovered active session: {}",
                    error
                );
            }

            if let Some(repository) = &self.repository {
                repository
                    .mark_session_terminated(
                        session_id,
                        "terminated",
                        Some("resource_recovery_evicted_lru_session".to_string()),
                        chrono::Utc::now(),
                    )
                    .await?;
            }

            return Ok(Some(RecoveredSandboxResource {
                kind: RecoveredSandboxResourceKind::ActiveSession,
                sandbox_id,
                session_id: Some(session_id),
            }));
        }

        Ok(None)
    }
}

#[async_trait::async_trait]
impl SandboxResourceRecovery for PoolRecoveryHandle {
    async fn recover_sandbox_resources(
        &self,
        current_session_id: SessionId,
    ) -> Result<Option<RecoveredSandboxResource>, SandboxError> {
        if let Some(recovered) = self.recover_oldest_warm_instance().await? {
            return Ok(Some(recovered));
        }

        self.recover_oldest_active_session(current_session_id).await
    }
}

#[async_trait::async_trait]
impl SandboxPool for NsjailSandboxPool {
    async fn acquire_session(
        &self,
        request: SessionRequest,
    ) -> Result<Arc<dyn SandboxSession>, SandboxError> {
        // 检查是否达到最大会话数
        let active_count = self.active_session_count().await;
        if active_count >= self.config.max_concurrent_sessions {
            return Err(SandboxError::Session(SessionError::MaxSessionsReached {
                max: self.config.max_concurrent_sessions,
            }));
        }

        // 尝试从热实例获取沙箱
        let sandbox = match self.acquire_from_warm().await? {
            Some(s) => s,
            None => {
                // 创建新的沙箱
                info!("No warm instance available, creating new sandbox");
                let nsjail_config = self.create_nsjail_config();
                let mut sandbox = NsjailSandbox::new(nsjail_config);
                sandbox.start().await?;
                sandbox
            }
        };

        // 创建会话
        let session_id = SessionId::new();
        let context = self.create_session_context(&request, session_id, sandbox.id);
        let mut session = ActiveNsjailSession::new_with_dependencies(
            session_id,
            context.clone(),
            sandbox,
            self.repository.clone(),
            self.vault.clone(),
            self.key_hierarchy.clone(),
            self.enclave.clone(),
        );
        session.set_resource_recovery(self.recovery_handle());

        if let Some(repository) = &self.repository {
            repository
                .create_session(NewSandboxSessionRecord {
                    session_id,
                    sandbox_id: context.sandbox_id.into(),
                    tenant_id: request.tenant_id,
                    created_by: request.user_id,
                    credential_id: request.credential_id,
                    original_intent: request.original_intent.clone(),
                    started_at: to_chrono_utc(context.created_at),
                    expires_at: to_chrono_utc(context.expires_at),
                    metadata: metadata_to_json(request.metadata.clone()),
                })
                .await?;
        }

        // 存储会话
        let session_arc: Arc<dyn SandboxSession> = Arc::new(session.clone());
        self.active_sessions
            .write()
            .await
            .insert(session_id, session);

        info!(
            "Session {} acquired for tenant {}",
            session_id, request.tenant_id
        );

        Ok(session_arc)
    }

    async fn release_session(&self, session_id: SessionId) -> Result<(), SandboxError> {
        let mut sessions = self.active_sessions.write().await;

        let session = sessions
            .remove(&session_id)
            .ok_or_else(|| SessionError::not_found(session_id.into()))?;

        // 尝试提取 sandbox。reuse policy 必须在 shutdown_runtime 之后读取，
        // 否则 runtime 关闭失败时新增的 taint 状态会被提前快照而丢失。
        if let Some(mut sandbox) = session.take_sandbox().await {
            let reuse_policy = session.sandbox_reuse_policy().await;
            // session 已经从 map 中移除，不需要再修改状态
            drop(sessions); // 释放锁

            if let Some(repository) = &self.repository {
                repository
                    .mark_session_terminated(
                        session_id,
                        "terminated",
                        Some("released_by_api".to_string()),
                        chrono::Utc::now(),
                    )
                    .await?;
            }

            if reuse_policy == SandboxReusePolicy::DestroyAfterUse {
                info!(
                    session_id = %session_id,
                    sandbox_id = %sandbox.id,
                    "Destroying sandbox instead of recycling because browser runtime was tainted"
                );
                if let Err(error) = sandbox.stop().await {
                    warn!("Failed to stop tainted sandbox {}: {}", sandbox.id, error);
                }
            } else if let Err(e) = self.recycle_sandbox(sandbox).await {
                warn!("Failed to recycle sandbox: {}", e);
            }
        } else {
            session.close().await?;
            if let Some(repository) = &self.repository {
                repository
                    .mark_session_terminated(
                        session_id,
                        "terminated",
                        Some("closed_without_sandbox".to_string()),
                        chrono::Utc::now(),
                    )
                    .await?;
            }
        }

        info!("Session {} released", session_id);
        Ok(())
    }

    async fn get_session(
        &self,
        session_id: SessionId,
    ) -> Result<Arc<dyn SandboxSession>, SandboxError> {
        let sessions = self.active_sessions.read().await;

        let session = sessions
            .get(&session_id)
            .ok_or_else(|| SessionError::not_found(session_id.into()))?;

        // 检查是否过期
        if session.context().is_expired() {
            return Err(SessionError::expired(session_id.into()).into());
        }

        Ok(Arc::new(session.clone()))
    }

    async fn health(&self) -> SandboxHealth {
        let status = *self.status.read().await;
        let active_sessions = self.active_session_count().await;
        let warm_instances = self.warm_instance_count().await;
        let mut process_health_summaries = Vec::new();

        {
            let sessions = self.active_sessions.read().await;
            for (session_id, session) in sessions.iter() {
                if let Some(health) = session.sandbox_process_health().await
                    && !health.is_healthy()
                {
                    process_health_summaries
                        .push(format!("active session {session_id}: {}", health.summary()));
                }
            }
        }

        {
            let instances = self.warm_instances.lock().await;
            for instance in instances.iter() {
                if let Some(sandbox) = instance.sandbox.as_ref()
                    && let Some(health) = sandbox.process_health()
                    && !health.is_healthy()
                {
                    process_health_summaries.push(format!(
                        "warm instance {}: {}",
                        instance.info.instance_id,
                        health.summary()
                    ));
                }
            }
        }

        let process_health_issues = process_health_summaries.len();
        let browser_runtime_probe_error = self.browser_runtime_probe_error.read().await.clone();
        let healthy = status == PoolStatus::Running
            && active_sessions < self.config.max_concurrent_sessions
            && process_health_issues == 0
            && browser_runtime_probe_error.is_none();

        let mut errors = Vec::new();
        if warm_instances < self.config.min_warm_instances {
            errors.push(format!(
                "Insufficient warm instances: {}/{}",
                warm_instances, self.config.min_warm_instances
            ));
        }
        errors.extend(process_health_summaries.iter().cloned());
        if let Some(probe_error) = &browser_runtime_probe_error {
            errors.push(format!("browser runtime probe failed: {probe_error}"));
        }

        SandboxHealth {
            pool_status: status,
            active_sessions,
            warm_instances,
            healthy,
            error: if errors.is_empty() {
                None
            } else {
                Some(errors.join("; "))
            },
            browser_runtime_probe_error,
            process_health_issues,
            process_health_summaries,
        }
    }

    async fn shutdown(&self) -> Result<(), SandboxError> {
        info!("Shutting down NsjailSandboxPool");

        *self.status.write().await = PoolStatus::Shutdown;

        // 停止清理任务
        if let Some(handle) = self.cleanup_handle.write().await.take() {
            handle.abort();
        }

        // 关闭所有活跃会话
        let sessions: Vec<_> = self.active_sessions.write().await.drain().collect();
        for (session_id, session) in sessions {
            if let Err(e) = session.close().await {
                error!(
                    "Failed to close session {} during shutdown: {}",
                    session_id, e
                );
            }
        }

        // 清理热实例
        let warm_instances: Vec<_> = self.warm_instances.lock().await.drain(..).collect();
        for instance in warm_instances {
            if let Some(mut sandbox) = instance.sandbox
                && let Err(e) = sandbox.stop().await
            {
                error!(
                    "Failed to stop warm instance {}: {}",
                    instance.info.instance_id, e
                );
            }
        }

        info!("NsjailSandboxPool shutdown complete");
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Drop for NsjailSandboxPool {
    fn drop(&mut self) {
        // 异步清理在 shutdown 方法中处理
        // 这里只记录日志
        info!("NsjailSandboxPool dropping");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::sandbox::config::SandboxConfig;
    use crate::tee::sandbox::repository::{
        CompleteSandboxOperationRecord, NewSandboxOperationRecord, NewSandboxSessionRecord,
        SandboxOperationRecord, SandboxRepository, SandboxSessionRecord,
    };
    use crate::tee::sandbox::types::{SandboxId, SessionStatus};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use time::OffsetDateTime;
    use tokio::sync::Mutex as TokioMutex;
    use uuid::Uuid;

    fn create_test_pool() -> NsjailSandboxPool {
        let config = SandboxConfig::default();
        NsjailSandboxPool::new(config)
    }

    #[derive(Default)]
    struct MockSandboxRepository {
        reconcile_calls: AtomicUsize,
        terminated_sessions: TokioMutex<Vec<SessionId>>,
    }

    #[async_trait]
    impl SandboxRepository for MockSandboxRepository {
        async fn create_session(
            &self,
            _record: NewSandboxSessionRecord,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_terminated(
            &self,
            session_id: SessionId,
            _status: &'static str,
            _reason: Option<String>,
            _terminated_at: chrono::DateTime<Utc>,
        ) -> Result<(), SandboxError> {
            self.terminated_sessions.lock().await.push(session_id);
            Ok(())
        }

        async fn update_session_status(
            &self,
            _session_id: SessionId,
            _status: &'static str,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_operation_started(
            &self,
            _session_id: SessionId,
            _operation_id: Uuid,
            _started_at: chrono::DateTime<Utc>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn mark_session_operation_finished(
            &self,
            _session_id: SessionId,
            _last_error_summary: Option<String>,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn create_operation(
            &self,
            _record: NewSandboxOperationRecord,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn complete_operation(
            &self,
            _record: CompleteSandboxOperationRecord,
        ) -> Result<(), SandboxError> {
            Ok(())
        }

        async fn reconcile_orphaned_active_sessions(
            &self,
            _recovery_reason: &str,
        ) -> Result<u64, SandboxError> {
            self.reconcile_calls.fetch_add(1, Ordering::SeqCst);
            Ok(0)
        }

        async fn list_sessions_by_tenant(
            &self,
            _tenant_id: Uuid,
        ) -> Result<Vec<SandboxSessionRecord>, SandboxError> {
            Ok(Vec::new())
        }

        async fn get_session_by_id(
            &self,
            _tenant_id: Uuid,
            _session_id: SessionId,
        ) -> Result<Option<SandboxSessionRecord>, SandboxError> {
            Ok(None)
        }

        async fn get_operation_by_id(
            &self,
            _tenant_id: Uuid,
            _operation_id: Uuid,
        ) -> Result<Option<SandboxOperationRecord>, SandboxError> {
            Ok(None)
        }
    }

    #[test]
    fn test_pool_creation() {
        let pool = create_test_pool();
        assert_eq!(pool.config.max_warm_instances, 10);
    }

    fn create_test_session_with_activity(last_activity_at: OffsetDateTime) -> ActiveNsjailSession {
        let sandbox = NsjailSandbox::new(crate::tee::sandbox::config::NsjailConfig {
            sandbox: SandboxConfig::default(),
            command: vec!["sleep".to_string(), "60".to_string()],
            cwd: std::path::PathBuf::from("/"),
            env: Default::default(),
            disable_seccomp_for_browser_runtime: false,
            enable_user_namespace: true,
            uid_map: Default::default(),
            gid_map: Default::default(),
        });
        let session_id = SessionId::new();
        let context = SessionContext {
            session_id,
            sandbox_id: SandboxId::new(),
            tenant_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            credential_id: Uuid::new_v4(),
            original_intent: "test".to_string(),
            created_at: last_activity_at,
            expires_at: last_activity_at + Duration::minutes(30),
            last_activity_at: Arc::new(RwLock::new(last_activity_at)),
        };

        ActiveNsjailSession::new(session_id, context, sandbox)
    }

    #[test]
    fn test_pool_nsjail_config_uses_browser_runtime_seccomp_policy() {
        let pool = create_test_pool();
        let config = pool.create_nsjail_config();

        assert!(config.disable_seccomp_for_browser_runtime);
        assert!(config.enable_user_namespace);

        let args = config.to_args();
        let seccomp_idx = args
            .iter()
            .position(|arg| arg == "--seccomp_string")
            .expect("pool nsjail config should include seccomp policy");
        let seccomp_policy = &args[seccomp_idx + 1];
        assert!(!seccomp_policy.contains("    execve\n"));
        assert!(!seccomp_policy.contains("    execveat\n"));
        assert!(!seccomp_policy.contains("    fork\n"));
        assert!(!seccomp_policy.contains("    vfork\n"));
        assert!(!seccomp_policy.contains("    clone\n"));
    }

    #[tokio::test]
    async fn test_health_check() {
        let pool = create_test_pool();
        let health = pool.health().await;

        // 未初始化时状态应为 Initializing
        assert_eq!(health.pool_status, PoolStatus::Initializing);
        assert_eq!(health.active_sessions, 0);
        assert_eq!(health.warm_instances, 0);
    }

    #[test]
    fn test_create_session_context() {
        let pool = create_test_pool();
        let request = SessionRequest {
            tenant_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            credential_id: Uuid::new_v4(),
            original_intent: "查询投资组合".to_string(),
            metadata: None,
        };

        let session_id = SessionId::new();
        let sandbox_id = SandboxId::new();
        let context = pool.create_session_context(&request, session_id, sandbox_id);

        assert_eq!(context.session_id, session_id);
        assert_eq!(context.sandbox_id, sandbox_id);
        assert_eq!(context.original_intent, "查询投资组合");
    }

    #[tokio::test]
    async fn test_initialize_reconciles_orphaned_sessions_when_repository_present() {
        let config = SandboxConfig::default();
        let repository = Arc::new(MockSandboxRepository::default());
        let pool = NsjailSandboxPool::new_with_repository(config, Some(repository.clone()));

        pool.initialize().await.unwrap();

        assert_eq!(repository.reconcile_calls.load(Ordering::SeqCst), 1);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_resource_recovery_prefers_oldest_warm_instance() {
        let pool = create_test_pool();
        let warm_sandbox_id = SandboxId::new();
        let mut warm_instance = WarmNsjailInstance::new(warm_sandbox_id, 0);
        warm_instance.sandbox = None;
        pool.warm_instances.lock().await.push_back(warm_instance);

        let session = create_test_session_with_activity(OffsetDateTime::now_utc());
        let session_id = session.id;
        pool.active_sessions
            .write()
            .await
            .insert(session_id, session);

        let recovered = pool
            .recovery_handle()
            .recover_sandbox_resources(SessionId::new())
            .await
            .expect("recovery should succeed")
            .expect("warm instance should be reclaimed");

        assert_eq!(recovered.kind, RecoveredSandboxResourceKind::WarmInstance);
        assert_eq!(recovered.sandbox_id, warm_sandbox_id);
        assert!(recovered.session_id.is_none());
        assert!(pool.active_sessions.read().await.contains_key(&session_id));
    }

    #[tokio::test]
    async fn test_resource_recovery_falls_back_to_oldest_idle_active_session() {
        let repository = Arc::new(MockSandboxRepository::default());
        let pool = NsjailSandboxPool::new_with_repository(
            SandboxConfig::default(),
            Some(repository.clone()),
        );
        let now = OffsetDateTime::now_utc();

        let oldest = create_test_session_with_activity(now - Duration::minutes(10));
        oldest.set_status_for_test(SessionStatus::Ready).await;
        let oldest_id = oldest.id;
        let oldest_sandbox_id = oldest.context().sandbox_id;

        let newer = create_test_session_with_activity(now - Duration::minutes(1));
        newer.set_status_for_test(SessionStatus::Paused).await;
        let newer_id = newer.id;

        let executing = create_test_session_with_activity(now - Duration::minutes(20));
        executing
            .set_status_for_test(SessionStatus::Executing)
            .await;
        let executing_id = executing.id;

        let mut sessions = pool.active_sessions.write().await;
        sessions.insert(oldest_id, oldest);
        sessions.insert(newer_id, newer);
        sessions.insert(executing_id, executing);
        drop(sessions);

        let recovered = pool
            .recovery_handle()
            .recover_sandbox_resources(SessionId::new())
            .await
            .expect("recovery should succeed")
            .expect("active session should be reclaimed");

        assert_eq!(recovered.kind, RecoveredSandboxResourceKind::ActiveSession);
        assert_eq!(recovered.sandbox_id, oldest_sandbox_id);
        assert_eq!(recovered.session_id, Some(oldest_id));
        assert!(!pool.active_sessions.read().await.contains_key(&oldest_id));
        assert!(pool.active_sessions.read().await.contains_key(&newer_id));
        assert!(
            pool.active_sessions
                .read()
                .await
                .contains_key(&executing_id)
        );
        assert_eq!(
            repository.terminated_sessions.lock().await.as_slice(),
            &[oldest_id]
        );
    }

    #[tokio::test]
    async fn test_resource_recovery_skips_executing_active_sessions() {
        let pool = create_test_pool();
        let session = create_test_session_with_activity(OffsetDateTime::now_utc());
        let session_id = session.id;
        session.set_status_for_test(SessionStatus::Executing).await;
        pool.active_sessions
            .write()
            .await
            .insert(session_id, session);

        let recovered = pool
            .recovery_handle()
            .recover_sandbox_resources(SessionId::new())
            .await
            .expect("recovery should succeed");

        assert!(recovered.is_none());
        assert!(pool.active_sessions.read().await.contains_key(&session_id));
    }
}
