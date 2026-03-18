//! 页面状态冻结模块
//!
//! 提供 TOCTOU (Time-of-check to time-of-use) 防护机制，
//! 在截图或审核期间冻结页面状态，防止内容被篡改。

use crate::tee::sandbox::{error::ExportError, types::SessionId};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 生成唯一的冻结令牌
fn generate_freeze_token() -> String {
    format!("freeze_{}", uuid::Uuid::new_v4())
}

/// 页面冻结状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreezeState {
    /// 未冻结
    Unfrozen,
    /// 冻结中
    Freezing,
    /// 已冻结
    Frozen,
    /// 解冻中
    Unfreezing,
}

impl std::fmt::Display for FreezeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FreezeState::Unfrozen => write!(f, "unfrozen"),
            FreezeState::Freezing => write!(f, "freezing"),
            FreezeState::Frozen => write!(f, "frozen"),
            FreezeState::Unfreezing => write!(f, "unfreezing"),
        }
    }
}

/// 冻结的页面状态快照
#[derive(Debug, Clone)]
pub struct FrozenPageState {
    /// 会话 ID
    pub session_id: SessionId,
    /// 冻结时间戳
    pub frozen_at: Instant,
    /// 冻结超时时间
    pub timeout: Duration,
    /// 页面 URL
    pub page_url: String,
    /// DOM 状态哈希（用于完整性验证）
    pub dom_hash: String,
    /// 冻结令牌（用于TOCTOU防护）
    pub freeze_token: String,
    /// 视口信息
    pub viewport: ViewportInfo,
    /// 元数据
    pub metadata: FrozenMetadata,
}

/// 视口信息
#[derive(Debug, Clone)]
pub struct ViewportInfo {
    pub width: u32,
    pub height: u32,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub device_scale_factor: f64,
}

/// 冻结元数据
#[derive(Debug, Clone, Default)]
pub struct FrozenMetadata {
    /// 页面标题
    pub title: Option<String>,
    /// 活跃元素选择器
    pub active_element: Option<String>,
    /// 焦点状态
    pub has_focus: bool,
    /// 自定义数据
    pub custom_data: std::collections::HashMap<String, String>,
}

/// 页面状态冻结器
///
/// 用于在截图或内容审核期间冻结页面状态，防止 TOCTOU 攻击。
pub struct PageStateFreezer {
    /// 会话 ID
    pub session_id: SessionId,
    /// 当前冻结状态
    state: Arc<RwLock<FreezeState>>,
    /// 冻结的状态快照
    frozen_state: Arc<RwLock<Option<FrozenPageState>>>,
    /// 冻结超时时间
    freeze_timeout: Duration,
    /// 操作锁（防止并发冻结/解冻）
    operation_lock: Arc<RwLock<()>>,
}

impl PageStateFreezer {
    /// 创建新的页面冻结器
    pub fn new(session_id: SessionId) -> Self {
        Self {
            session_id,
            state: Arc::new(RwLock::new(FreezeState::Unfrozen)),
            frozen_state: Arc::new(RwLock::new(None)),
            freeze_timeout: Duration::from_secs(30), // 默认 30 秒超时
            operation_lock: Arc::new(RwLock::new(())),
        }
    }

    /// 创建带自定义超时的页面冻结器
    pub fn with_timeout(session_id: SessionId, timeout: Duration) -> Self {
        Self {
            session_id,
            state: Arc::new(RwLock::new(FreezeState::Unfrozen)),
            frozen_state: Arc::new(RwLock::new(None)),
            freeze_timeout: timeout,
            operation_lock: Arc::new(RwLock::new(())),
        }
    }

    /// 获取当前冻结状态
    pub async fn state(&self) -> FreezeState {
        *self.state.read().await
    }

    /// 检查是否已冻结
    pub async fn is_frozen(&self) -> bool {
        matches!(self.state().await, FreezeState::Frozen)
    }

    /// 冻结页面状态
    ///
    /// # Arguments
    ///
    /// * `page_info` - 页面信息，用于创建状态快照
    ///
    /// # Returns
    ///
    /// 返回冻结的状态快照
    pub async fn freeze(&self, page_info: PageInfo) -> Result<FrozenPageState, ExportError> {
        // 获取操作锁，防止并发操作
        let _lock = self.operation_lock.write().await;

        // 检查当前状态
        let current_state = self.state.read().await;
        if matches!(*current_state, FreezeState::Frozen | FreezeState::Freezing) {
            return Err(ExportError::Freeze(format!(
                "页面已经是冻结状态: {}",
                *current_state
            )));
        }
        drop(current_state);

        // 更新状态为冻结中
        *self.state.write().await = FreezeState::Freezing;
        info!("开始冻结会话 {} 的页面状态", self.session_id);

        // 生成冻结令牌
        let freeze_token = generate_freeze_token();
        info!("为会话 {} 生成冻结令牌: {}", self.session_id, freeze_token);

        // 创建冻结状态
        let frozen_state = FrozenPageState {
            session_id: self.session_id,
            frozen_at: Instant::now(),
            timeout: self.freeze_timeout,
            page_url: page_info.url,
            dom_hash: page_info.dom_hash,
            freeze_token,
            viewport: page_info.viewport,
            metadata: page_info.metadata,
        };

        // 保存冻结状态
        *self.frozen_state.write().await = Some(frozen_state.clone());

        // 更新状态为已冻结
        *self.state.write().await = FreezeState::Frozen;

        debug!(
            "会话 {} 的页面状态已冻结，超时: {:?}",
            self.session_id, self.freeze_timeout
        );

        // 启动超时监控任务
        self.start_timeout_monitor();

        Ok(frozen_state)
    }

    /// 解冻页面状态
    pub async fn unfreeze(&self) -> Result<(), ExportError> {
        // 获取操作锁
        let _lock = self.operation_lock.write().await;

        // 检查当前状态
        let current_state = self.state.read().await;
        if !matches!(*current_state, FreezeState::Frozen) {
            return Err(ExportError::Freeze(format!(
                "页面未处于冻结状态，当前状态: {}",
                *current_state
            )));
        }
        drop(current_state);

        // 更新状态为解冻中
        *self.state.write().await = FreezeState::Unfreezing;
        info!("开始解冻会话 {} 的页面状态", self.session_id);

        // 清理冻结状态
        *self.frozen_state.write().await = None;

        // 更新状态为未冻结
        *self.state.write().await = FreezeState::Unfrozen;

        debug!("会话 {} 的页面状态已解冻", self.session_id);

        Ok(())
    }

    /// 获取冻结的状态快照
    pub async fn get_frozen_state(&self) -> Option<FrozenPageState> {
        self.frozen_state.read().await.clone()
    }

    /// 验证冻结状态是否仍然有效
    ///
    /// 检查冻结是否已超时
    pub async fn validate_freeze(&self) -> Result<(), ExportError> {
        let state = self.frozen_state.read().await;

        if let Some(ref frozen) = *state {
            let elapsed = frozen.frozen_at.elapsed();
            if elapsed > frozen.timeout {
                return Err(ExportError::Freeze(format!(
                    "冻结已超时: {:?} > {:?}",
                    elapsed, frozen.timeout
                )));
            }
            Ok(())
        } else {
            Err(ExportError::Freeze("没有活动的冻结状态".to_string()))
        }
    }

    /// 强制解冻（用于错误恢复）
    ///
    /// # Safety
    ///
    /// 此操作会强制清除冻结状态，可能导致数据不一致。
    /// 仅在紧急情况下使用。
    pub async fn force_unfreeze(&self) {
        warn!("强制解冻会话 {} 的页面状态", self.session_id);
        *self.state.write().await = FreezeState::Unfrozen;
        *self.frozen_state.write().await = None;
    }

    /// 启动超时监控任务
    fn start_timeout_monitor(&self) {
        let state = self.state.clone();
        let frozen_state = self.frozen_state.clone();
        let session_id = self.session_id;
        let timeout = self.freeze_timeout;

        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;

            // 检查是否仍然是冻结状态
            let current = *state.read().await;
            if matches!(current, FreezeState::Frozen) {
                warn!(
                    "会话 {} 的页面冻结已超时 ({:?})，自动解冻",
                    session_id, timeout
                );
                *state.write().await = FreezeState::Unfrozen;
                *frozen_state.write().await = None;
            }
        });
    }

    /// 更新冻结元数据
    pub async fn update_metadata(&self, key: String, value: String) -> Result<(), ExportError> {
        let mut state = self.frozen_state.write().await;

        if let Some(ref mut frozen) = *state {
            frozen.metadata.custom_data.insert(key, value);
            Ok(())
        } else {
            Err(ExportError::Freeze("没有活动的冻结状态".to_string()))
        }
    }

    /// 获取冻结令牌
    ///
    /// 用于验证冻结状态的真实性，防止TOCTOU攻击
    pub async fn get_freeze_token(&self) -> Option<String> {
        self.frozen_state
            .read()
            .await
            .as_ref()
            .map(|s| s.freeze_token.clone())
    }

    /// 验证 DOM 完整性
    ///
    /// 将当前 DOM hash 与冻结时保存的 hash 进行对比，
    /// 检测页面内容是否在冻结后被篡改（TOCTOU防护）
    ///
    /// # Arguments
    ///
    /// * `current_hash` - 当前页面的 DOM hash
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - DOM 完整性验证通过
    /// * `Ok(false)` - DOM 已被篡改，可能存在TOCTOU攻击
    /// * `Err(ExportError)` - 验证过程中发生错误
    pub async fn verify_dom_integrity(&self, current_hash: &str) -> Result<bool, ExportError> {
        // 首先验证冻结状态是否有效
        self.validate_freeze().await?;

        let frozen_state = self.frozen_state.read().await;

        if let Some(ref frozen) = *frozen_state {
            let matches = frozen.dom_hash == current_hash;

            if matches {
                debug!("会话 {} 的 DOM 完整性验证通过", self.session_id);
            } else {
                warn!(
                    "会话 {} 的 DOM 完整性验证失败！冻结时 hash: {}, 当前 hash: {}",
                    self.session_id, frozen.dom_hash, current_hash
                );

                // 记录安全审计日志
                info!(
                    security_event = "toctou_violation_detected",
                    session_id = %self.session_id,
                    expected_hash = %frozen.dom_hash,
                    actual_hash = %current_hash,
                    "检测到潜在的TOCTOU攻击"
                );
            }

            Ok(matches)
        } else {
            Err(ExportError::Freeze(
                "没有活动的冻结状态，无法验证DOM完整性".to_string(),
            ))
        }
    }

    /// 使用令牌验证冻结状态的真实性
    ///
    /// 验证提供的令牌是否与冻结时生成的令牌匹配
    ///
    /// # Arguments
    ///
    /// * `token` - 要验证的冻结令牌
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - 令牌验证通过
    /// * `Ok(false)` - 令牌不匹配，可能存在伪造请求
    /// * `Err(ExportError)` - 验证过程中发生错误
    pub async fn verify_freeze_token(&self, token: &str) -> Result<bool, ExportError> {
        // 首先验证冻结状态是否有效
        self.validate_freeze().await?;

        let frozen_state = self.frozen_state.read().await;

        if let Some(ref frozen) = *frozen_state {
            let matches = frozen.freeze_token == token;

            if !matches {
                warn!("会话 {} 的冻结令牌验证失败！", self.session_id);

                // 记录安全审计日志
                info!(
                    security_event = "invalid_freeze_token",
                    session_id = %self.session_id,
                    "收到无效的冻结令牌"
                );
            }

            Ok(matches)
        } else {
            Err(ExportError::Freeze(
                "没有活动的冻结状态，无法验证令牌".to_string(),
            ))
        }
    }
}

/// 页面信息
#[derive(Debug, Clone)]
pub struct PageInfo {
    /// 页面 URL
    pub url: String,
    /// DOM 状态哈希
    pub dom_hash: String,
    /// 视口信息
    pub viewport: ViewportInfo,
    /// 元数据
    pub metadata: FrozenMetadata,
}

/// 冻结守卫
///
/// 当守卫被丢弃时自动解冻页面
pub struct FreezeGuard {
    freezer: Arc<PageStateFreezer>,
    auto_unfreeze: bool,
}

impl FreezeGuard {
    /// 创建新的冻结守卫
    pub fn new(freezer: Arc<PageStateFreezer>) -> Self {
        Self {
            freezer,
            auto_unfreeze: true,
        }
    }

    /// 禁用自动解冻
    pub fn disable_auto_unfreeze(&mut self) {
        self.auto_unfreeze = false;
    }

    /// 手动解冻
    pub async fn unfreeze(self) -> Result<(), ExportError> {
        self.freezer.unfreeze().await
    }
}

impl Drop for FreezeGuard {
    fn drop(&mut self) {
        if self.auto_unfreeze {
            // 由于 Drop 不能是 async，我们需要使用阻塞方式
            // 或者发送消息给后台任务
            let freezer = self.freezer.clone();
            tokio::spawn(async move {
                if let Err(e) = freezer.unfreeze().await {
                    warn!("自动解冻失败: {}", e);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_page_info() -> PageInfo {
        PageInfo {
            url: "https://example.com".to_string(),
            dom_hash: "abc123".to_string(),
            viewport: ViewportInfo {
                width: 1920,
                height: 1080,
                scroll_x: 0.0,
                scroll_y: 0.0,
                device_scale_factor: 1.0,
            },
            metadata: FrozenMetadata::default(),
        }
    }

    #[tokio::test]
    async fn test_freezer_creation() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);

        assert!(!freezer.is_frozen().await);
        assert_eq!(freezer.state().await, FreezeState::Unfrozen);
    }

    #[tokio::test]
    async fn test_freeze_and_unfreeze() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);
        let page_info = create_test_page_info();

        // 冻结
        let frozen = freezer.freeze(page_info).await.unwrap();
        assert_eq!(frozen.session_id, session_id);
        assert!(freezer.is_frozen().await);

        // 验证状态
        let state = freezer.get_frozen_state().await;
        assert!(state.is_some());

        // 解冻
        freezer.unfreeze().await.unwrap();
        assert!(!freezer.is_frozen().await);
        assert!(freezer.get_frozen_state().await.is_none());
    }

    #[tokio::test]
    async fn test_double_freeze_error() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);
        let page_info = create_test_page_info();

        // 第一次冻结
        freezer.freeze(page_info.clone()).await.unwrap();

        // 第二次冻结应该失败
        let result = freezer.freeze(page_info).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unfreeze_without_freeze() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);

        // 未冻结时解冻应该失败
        let result = freezer.unfreeze().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_force_unfreeze() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);
        let page_info = create_test_page_info();

        // 冻结
        freezer.freeze(page_info).await.unwrap();
        assert!(freezer.is_frozen().await);

        // 强制解冻
        freezer.force_unfreeze().await;
        assert!(!freezer.is_frozen().await);
    }

    #[tokio::test]
    async fn test_update_metadata() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);
        let page_info = create_test_page_info();

        // 冻结前更新应该失败
        let result = freezer
            .update_metadata("key".to_string(), "value".to_string())
            .await;
        assert!(result.is_err());

        // 冻结
        freezer.freeze(page_info).await.unwrap();

        // 冻结后更新应该成功
        freezer
            .update_metadata("key".to_string(), "value".to_string())
            .await
            .unwrap();

        let state = freezer.get_frozen_state().await.unwrap();
        assert_eq!(
            state.metadata.custom_data.get("key"),
            Some(&"value".to_string())
        );
    }

    #[tokio::test]
    async fn test_validate_freeze() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::with_timeout(session_id, Duration::from_secs(1));
        let page_info = create_test_page_info();

        // 未冻结时验证应该失败
        let result = freezer.validate_freeze().await;
        assert!(result.is_err());

        // 冻结
        freezer.freeze(page_info).await.unwrap();

        // 立即验证应该成功
        let result = freezer.validate_freeze().await;
        assert!(result.is_ok());

        // 等待超时
        tokio::time::sleep(Duration::from_secs(2)).await;

        // 超时后验证应该失败
        let result = freezer.validate_freeze().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_freeze_token_generation() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);
        let page_info = create_test_page_info();

        // 未冻结时获取令牌应该返回 None
        let token = freezer.get_freeze_token().await;
        assert!(token.is_none());

        // 冻结
        let frozen = freezer.freeze(page_info).await.unwrap();

        // 验证冻结令牌已生成
        assert!(!frozen.freeze_token.is_empty());
        assert!(frozen.freeze_token.starts_with("freeze_"));

        // 获取冻结令牌
        let token = freezer.get_freeze_token().await;
        assert!(token.is_some());
        assert_eq!(token.unwrap(), frozen.freeze_token);
    }

    #[tokio::test]
    async fn test_verify_freeze_token() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);
        let page_info = create_test_page_info();

        // 未冻结时验证应该失败
        let result = freezer.verify_freeze_token("invalid_token").await;
        assert!(result.is_err());

        // 冻结
        let frozen = freezer.freeze(page_info).await.unwrap();
        let correct_token = frozen.freeze_token.clone();

        // 验证正确的令牌
        let valid = freezer.verify_freeze_token(&correct_token).await.unwrap();
        assert!(valid);

        // 验证错误的令牌
        let valid = freezer.verify_freeze_token("wrong_token").await.unwrap();
        assert!(!valid);
    }

    #[tokio::test]
    async fn test_verify_dom_integrity() {
        let session_id = SessionId::new();
        let freezer = PageStateFreezer::new(session_id);
        let page_info = create_test_page_info();

        // 未冻结时验证应该失败
        let result = freezer.verify_dom_integrity("any_hash").await;
        assert!(result.is_err());

        // 冻结
        freezer.freeze(page_info.clone()).await.unwrap();

        // 验证正确的 DOM hash
        let valid = freezer
            .verify_dom_integrity(&page_info.dom_hash)
            .await
            .unwrap();
        assert!(valid);

        // 验证错误的 DOM hash（模拟TOCTOU攻击）
        let valid = freezer
            .verify_dom_integrity("tampered_hash_123")
            .await
            .unwrap();
        assert!(!valid);
    }

    #[tokio::test]
    async fn test_dom_integrity_with_freeze_guard() {
        use std::sync::Arc;

        let session_id = SessionId::new();
        let freezer = Arc::new(PageStateFreezer::new(session_id));
        let page_info = create_test_page_info();

        // 冻结并创建守卫
        let frozen = freezer.freeze(page_info.clone()).await.unwrap();
        let _guard = FreezeGuard::new(freezer.clone());

        // 在冻结状态下验证DOM完整性
        let valid = freezer
            .verify_dom_integrity(&page_info.dom_hash)
            .await
            .unwrap();
        assert!(valid);

        // 获取冻结令牌
        let token = freezer.get_freeze_token().await;
        assert_eq!(token.unwrap(), frozen.freeze_token);
    }
}
