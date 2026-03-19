//! 沙箱池管理

use crate::tee::sandbox::{
    PoolStatus, SandboxHealth,
    config::{NsjailConfig, SandboxConfig, SandboxPoolConfig},
    error::{SandboxError, SessionError},
    nsjail::{NsjailSandbox, WarmNsjailInstance},
    session::{ActiveNsjailSession, SandboxSession},
    types::{SandboxId, SessionContext, SessionId, SessionRequest},
};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

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
}

impl NsjailSandboxPool {
    /// 创建新的沙箱池
    pub fn new(config: SandboxConfig) -> Self {
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
        }
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
        NsjailConfig {
            sandbox: self.sandbox_config.clone(),
            command: vec!["sleep".to_string(), "3600".to_string()], // 长时间运行的占位命令
            cwd: std::path::PathBuf::from("/"),
            env: std::collections::HashMap::new(),
            uid_map: Default::default(),
            gid_map: Default::default(),
        }
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
        let session = ActiveNsjailSession::new(session_id, context, sandbox);

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

        // 尝试提取 sandbox
        if let Some(sandbox) = session.take_sandbox().await {
            // session 已经从 map 中移除，不需要再修改状态
            drop(sessions); // 释放锁

            // 尝试回收
            if let Err(e) = self.recycle_sandbox(sandbox).await {
                warn!("Failed to recycle sandbox: {}", e);
            }
        } else {
            session.close().await?;
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

        let healthy =
            status == PoolStatus::Running && active_sessions < self.config.max_concurrent_sessions;

        let error = if warm_instances < self.config.min_warm_instances {
            Some(format!(
                "Insufficient warm instances: {}/{}",
                warm_instances, self.config.min_warm_instances
            ))
        } else {
            None
        };

        SandboxHealth {
            pool_status: status,
            active_sessions,
            warm_instances,
            healthy,
            error,
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
    use uuid::Uuid;

    fn create_test_pool() -> NsjailSandboxPool {
        let config = SandboxConfig::default();
        NsjailSandboxPool::new(config)
    }

    #[test]
    fn test_pool_creation() {
        let pool = create_test_pool();
        assert_eq!(pool.config.max_warm_instances, 10);
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
}
