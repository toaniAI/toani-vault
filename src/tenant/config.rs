//! 租户配置管理模块
//!
//! 管理租户的配置信息，包括功能开关、配额限制和自定义设置。
//! 支持 PostgreSQL 持久化存储和 Redis 缓存。
//!
//! # 配置层次
//!
//! ```text
//! TenantConfig
//! ├── feature_flags: 功能开关
//! │   ├── enable_credential_encryption
//! │   ├── enable_audit_logging
//!   │   ├── enable_token_revocation
//! │   └── ...
//! ├── quota_limits: 配额限制
//! │   ├── max_credentials
//! │   ├── max_tokens_per_user
//! │   ├── max_requests_per_minute
//! │   └── ...
//! └── settings: 自定义设置
//!     ├── token_ttl_seconds
//!     ├── audit_retention_days
//!     └── ...
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use super::TenantId;

/// 租户配置错误
#[derive(Debug, Error)]
pub enum TenantConfigError {
    #[error("租户配置未找到: {0}")]
    NotFound(TenantId),

    #[error("租户配置已存在: {0}")]
    AlreadyExists(TenantId),

    #[error("配置验证失败: {0}")]
    ValidationError(String),

    #[error("存储错误: {0}")]
    StorageError(String),

    #[error("缓存错误: {0}")]
    CacheError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),
}

/// 租户状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum TenantStatus {
    /// 待激活
    #[default]
    Pending,
    /// 活跃
    Active,
    /// 已暂停
    Suspended,
    /// 已删除
    Deleted,
}

impl TenantStatus {
    /// 检查状态是否允许操作
    pub fn allows_operations(&self) -> bool {
        matches!(self, TenantStatus::Active)
    }

    /// 检查状态是否允许修改配置
    pub fn allows_config_changes(&self) -> bool {
        matches!(self, TenantStatus::Active | TenantStatus::Pending)
    }
}

impl std::fmt::Display for TenantStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TenantStatus::Pending => write!(f, "pending"),
            TenantStatus::Active => write!(f, "active"),
            TenantStatus::Suspended => write!(f, "suspended"),
            TenantStatus::Deleted => write!(f, "deleted"),
        }
    }
}

/// 功能开关
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlags {
    /// 启用凭证加密
    #[serde(default = "default_true")]
    pub enable_credential_encryption: bool,
    /// 启用审计日志
    #[serde(default = "default_true")]
    pub enable_audit_logging: bool,
    /// 启用 Token 撤销
    #[serde(default = "default_true")]
    pub enable_token_revocation: bool,
    /// 启用多因素认证
    #[serde(default = "default_false")]
    pub enable_mfa: bool,
    /// 启用远程认证
    #[serde(default = "default_false")]
    pub enable_remote_attestation: bool,
    /// 启用凭证自动轮换
    #[serde(default = "default_false")]
    pub enable_auto_rotation: bool,
    /// 允许跨域请求
    #[serde(default = "default_false")]
    pub allow_cors: bool,
    /// 启用 IP 白名单
    #[serde(default = "default_false")]
    pub enable_ip_whitelist: bool,
    /// 启用 Webhook 通知
    #[serde(default = "default_false")]
    pub enable_webhooks: bool,
    /// 启用 SSO 集成
    #[serde(default = "default_false")]
    pub enable_sso: bool,
    /// 启用自定义加密策略
    #[serde(default = "default_false")]
    pub enable_custom_crypto: bool,
    /// 启用高级审计分析
    #[serde(default = "default_false")]
    pub enable_advanced_audit: bool,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            enable_credential_encryption: true,
            enable_audit_logging: true,
            enable_token_revocation: true,
            enable_mfa: false,
            enable_remote_attestation: false,
            enable_auto_rotation: false,
            allow_cors: false,
            enable_ip_whitelist: false,
            enable_webhooks: false,
            enable_sso: false,
            enable_custom_crypto: false,
            enable_advanced_audit: false,
        }
    }
}

impl FeatureFlags {
    /// 启用所有高级功能
    pub fn enable_all_advanced() -> Self {
        Self {
            enable_credential_encryption: true,
            enable_audit_logging: true,
            enable_token_revocation: true,
            enable_mfa: true,
            enable_remote_attestation: true,
            enable_auto_rotation: true,
            allow_cors: true,
            enable_ip_whitelist: true,
            enable_webhooks: true,
            enable_sso: true,
            enable_custom_crypto: true,
            enable_advanced_audit: true,
        }
    }

    /// 仅启用基础功能
    pub fn basic_only() -> Self {
        Self::default()
    }

    /// 检查功能是否启用
    pub fn is_enabled(&self, feature: &str) -> bool {
        match feature {
            "credential_encryption" => self.enable_credential_encryption,
            "audit_logging" => self.enable_audit_logging,
            "token_revocation" => self.enable_token_revocation,
            "mfa" => self.enable_mfa,
            "remote_attestation" => self.enable_remote_attestation,
            "auto_rotation" => self.enable_auto_rotation,
            "cors" => self.allow_cors,
            "ip_whitelist" => self.enable_ip_whitelist,
            "webhooks" => self.enable_webhooks,
            "sso" => self.enable_sso,
            "custom_crypto" => self.enable_custom_crypto,
            "advanced_audit" => self.enable_advanced_audit,
            _ => false,
        }
    }

    /// 启用特定功能
    pub fn enable(&mut self, feature: &str) -> Result<(), String> {
        match feature {
            "credential_encryption" => self.enable_credential_encryption = true,
            "audit_logging" => self.enable_audit_logging = true,
            "token_revocation" => self.enable_token_revocation = true,
            "mfa" => self.enable_mfa = true,
            "remote_attestation" => self.enable_remote_attestation = true,
            "auto_rotation" => self.enable_auto_rotation = true,
            "cors" => self.allow_cors = true,
            "ip_whitelist" => self.enable_ip_whitelist = true,
            "webhooks" => self.enable_webhooks = true,
            "sso" => self.enable_sso = true,
            "custom_crypto" => self.enable_custom_crypto = true,
            "advanced_audit" => self.enable_advanced_audit = true,
            _ => return Err(format!("Unknown feature: {feature}")),
        }
        Ok(())
    }

    /// 禁用特定功能
    pub fn disable(&mut self, feature: &str) -> Result<(), String> {
        match feature {
            "credential_encryption" => self.enable_credential_encryption = false,
            "audit_logging" => self.enable_audit_logging = false,
            "token_revocation" => self.enable_token_revocation = false,
            "mfa" => self.enable_mfa = false,
            "remote_attestation" => self.enable_remote_attestation = false,
            "auto_rotation" => self.enable_auto_rotation = false,
            "cors" => self.allow_cors = false,
            "ip_whitelist" => self.enable_ip_whitelist = false,
            "webhooks" => self.enable_webhooks = false,
            "sso" => self.enable_sso = false,
            "custom_crypto" => self.enable_custom_crypto = false,
            "advanced_audit" => self.enable_advanced_audit = false,
            _ => return Err(format!("Unknown feature: {feature}")),
        }
        Ok(())
    }
}

/// 配额限制
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaLimits {
    /// 最大凭证数量
    #[serde(default = "default_max_credentials")]
    pub max_credentials: u64,
    /// 每用户最大 Token 数量
    #[serde(default = "default_max_tokens_per_user")]
    pub max_tokens_per_user: u64,
    /// 每分钟最大请求数
    #[serde(default = "default_max_requests_per_minute")]
    pub max_requests_per_minute: u64,
    /// 最大用户数
    #[serde(default = "default_max_users")]
    pub max_users: u64,
    /// 最大服务连接器数
    #[serde(default = "default_max_connectors")]
    pub max_connectors: u64,
    /// 最大 Webhook 数量
    #[serde(default = "default_max_webhooks")]
    pub max_webhooks: u64,
    /// 存储配额 (MB)
    #[serde(default = "default_storage_quota_mb")]
    pub storage_quota_mb: u64,
    /// 审计日志保留天数
    #[serde(default = "default_audit_retention_days")]
    pub audit_retention_days: u64,
    /// Token 最大有效期 (秒)
    #[serde(default = "default_max_token_ttl_seconds")]
    pub max_token_ttl_seconds: u64,
    /// 批量操作最大数量
    #[serde(default = "default_max_batch_size")]
    pub max_batch_size: u64,
}

fn default_max_credentials() -> u64 {
    1000
}
fn default_max_tokens_per_user() -> u64 {
    10
}
fn default_max_requests_per_minute() -> u64 {
    1000
}
fn default_max_users() -> u64 {
    100
}
fn default_max_connectors() -> u64 {
    50
}
fn default_max_webhooks() -> u64 {
    10
}
fn default_storage_quota_mb() -> u64 {
    1024
}
fn default_audit_retention_days() -> u64 {
    30
}
fn default_max_token_ttl_seconds() -> u64 {
    86400
}
fn default_max_batch_size() -> u64 {
    100
}

impl Default for QuotaLimits {
    fn default() -> Self {
        Self {
            max_credentials: default_max_credentials(),
            max_tokens_per_user: default_max_tokens_per_user(),
            max_requests_per_minute: default_max_requests_per_minute(),
            max_users: default_max_users(),
            max_connectors: default_max_connectors(),
            max_webhooks: default_max_webhooks(),
            storage_quota_mb: default_storage_quota_mb(),
            audit_retention_days: default_audit_retention_days(),
            max_token_ttl_seconds: default_max_token_ttl_seconds(),
            max_batch_size: default_max_batch_size(),
        }
    }
}

impl QuotaLimits {
    /// 免费版配额
    pub fn free_tier() -> Self {
        Self {
            max_credentials: 100,
            max_tokens_per_user: 3,
            max_requests_per_minute: 100,
            max_users: 5,
            max_connectors: 5,
            max_webhooks: 0,
            storage_quota_mb: 100,
            audit_retention_days: 7,
            max_token_ttl_seconds: 3600,
            max_batch_size: 10,
        }
    }

    /// 专业版配额
    pub fn pro_tier() -> Self {
        Self {
            max_credentials: 10_000,
            max_tokens_per_user: 20,
            max_requests_per_minute: 10_000,
            max_users: 1000,
            max_connectors: 100,
            max_webhooks: 20,
            storage_quota_mb: 10_240,
            audit_retention_days: 90,
            max_token_ttl_seconds: 604_800,
            max_batch_size: 500,
        }
    }

    /// 企业版配额
    pub fn enterprise_tier() -> Self {
        Self {
            max_credentials: 100_000,
            max_tokens_per_user: 100,
            max_requests_per_minute: 100_000,
            max_users: 10_000,
            max_connectors: 1000,
            max_webhooks: 100,
            storage_quota_mb: 102_400,
            audit_retention_days: 365,
            max_token_ttl_seconds: 2_592_000,
            max_batch_size: 1000,
        }
    }

    /// 验证配额是否合理
    pub fn validate(&self) -> Result<(), String> {
        if self.max_credentials == 0 {
            return Err("max_credentials must be greater than 0".to_string());
        }
        if self.max_tokens_per_user == 0 {
            return Err("max_tokens_per_user must be greater than 0".to_string());
        }
        if self.max_requests_per_minute == 0 {
            return Err("max_requests_per_minute must be greater than 0".to_string());
        }
        Ok(())
    }
}

/// 租户设置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantSettings {
    /// Token 默认有效期 (秒)
    #[serde(default = "default_token_ttl")]
    pub token_ttl_seconds: u64,
    /// 会话超时时间 (秒)
    #[serde(default = "default_session_timeout")]
    pub session_timeout_seconds: u64,
    /// 最大登录失败次数
    #[serde(default = "default_max_login_attempts")]
    pub max_login_attempts: u32,
    /// 登录锁定时间 (秒)
    #[serde(default = "default_lockout_duration")]
    pub lockout_duration_seconds: u64,
    /// 密码最小长度
    #[serde(default = "default_password_min_length")]
    pub password_min_length: u32,
    /// 是否要求密码复杂度
    #[serde(default = "default_true")]
    pub require_password_complexity: bool,
    /// 是否要求 MFA
    #[serde(default = "default_false")]
    pub require_mfa: bool,
    /// 允许的回调 URL 列表
    #[serde(default)]
    pub allowed_callback_urls: Vec<String>,
    /// 时区设置
    #[serde(default = "default_timezone")]
    pub timezone: String,
    /// 语言设置
    #[serde(default = "default_language")]
    pub language: String,
    /// 自定义元数据
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

fn default_token_ttl() -> u64 {
    900
} // 15 minutes
fn default_session_timeout() -> u64 {
    3600
} // 1 hour
fn default_max_login_attempts() -> u32 {
    5
}
fn default_lockout_duration() -> u64 {
    900
} // 15 minutes
fn default_password_min_length() -> u32 {
    8
}
fn default_timezone() -> String {
    "UTC".to_string()
}
fn default_language() -> String {
    "zh-CN".to_string()
}

impl Default for TenantSettings {
    fn default() -> Self {
        Self {
            token_ttl_seconds: default_token_ttl(),
            session_timeout_seconds: default_session_timeout(),
            max_login_attempts: default_max_login_attempts(),
            lockout_duration_seconds: default_lockout_duration(),
            password_min_length: default_password_min_length(),
            require_password_complexity: true,
            require_mfa: false,
            allowed_callback_urls: Vec::new(),
            timezone: default_timezone(),
            language: default_language(),
            metadata: HashMap::new(),
        }
    }
}

/// 租户配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    /// 功能开关
    #[serde(default)]
    pub feature_flags: FeatureFlags,
    /// 配额限制
    #[serde(default)]
    pub quota_limits: QuotaLimits,
    /// 租户设置
    #[serde(default)]
    pub settings: TenantSettings,
    /// 配置版本（用于乐观锁）
    #[serde(default)]
    pub version: u64,
    /// 最后更新时间
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    /// 最后更新者
    #[serde(default)]
    pub updated_by: Option<String>,
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            feature_flags: FeatureFlags::default(),
            quota_limits: QuotaLimits::default(),
            settings: TenantSettings::default(),
            version: 1,
            updated_at: Some(Utc::now()),
            updated_by: None,
        }
    }
}

impl TenantConfig {
    /// 创建免费版配置
    pub fn free_tier() -> Self {
        Self {
            feature_flags: FeatureFlags::basic_only(),
            quota_limits: QuotaLimits::free_tier(),
            settings: TenantSettings::default(),
            version: 1,
            updated_at: Some(Utc::now()),
            updated_by: None,
        }
    }

    /// 创建专业版配置
    pub fn pro_tier() -> Self {
        Self {
            feature_flags: FeatureFlags {
                enable_mfa: true,
                enable_webhooks: true,
                ..FeatureFlags::default()
            },
            quota_limits: QuotaLimits::pro_tier(),
            settings: TenantSettings::default(),
            version: 1,
            updated_at: Some(Utc::now()),
            updated_by: None,
        }
    }

    /// 创建企业版配置
    pub fn enterprise_tier() -> Self {
        Self {
            feature_flags: FeatureFlags::enable_all_advanced(),
            quota_limits: QuotaLimits::enterprise_tier(),
            settings: TenantSettings {
                require_mfa: true,
                require_password_complexity: true,
                ..TenantSettings::default()
            },
            version: 1,
            updated_at: Some(Utc::now()),
            updated_by: None,
        }
    }

    /// 更新配置
    pub fn update(&mut self, updater_id: impl Into<String>) {
        self.version += 1;
        self.updated_at = Some(Utc::now());
        self.updated_by = Some(updater_id.into());
    }

    /// 验证配置
    pub fn validate(&self) -> Result<(), String> {
        self.quota_limits.validate()?;
        Ok(())
    }

    /// 合并部分配置更新
    pub fn merge(&mut self, partial: PartialTenantConfig, updater_id: impl Into<String>) {
        if let Some(flags) = partial.feature_flags {
            self.feature_flags = flags;
        }
        if let Some(quotas) = partial.quota_limits {
            self.quota_limits = quotas;
        }
        if let Some(settings) = partial.settings {
            self.settings = settings;
        }
        self.update(updater_id);
    }
}

/// 部分租户配置（用于更新）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialTenantConfig {
    pub feature_flags: Option<FeatureFlags>,
    pub quota_limits: Option<QuotaLimits>,
    pub settings: Option<TenantSettings>,
}

/// 租户配置存储 trait
#[async_trait]
pub trait TenantConfigStore: Send + Sync {
    /// 获取租户配置
    async fn get_config(&self, tenant_id: &TenantId) -> Result<TenantConfig, TenantConfigError>;

    /// 保存租户配置
    async fn save_config(
        &self,
        tenant_id: &TenantId,
        config: &TenantConfig,
    ) -> Result<(), TenantConfigError>;

    /// 删除租户配置
    async fn delete_config(&self, tenant_id: &TenantId) -> Result<(), TenantConfigError>;

    /// 检查配置是否存在
    async fn config_exists(&self, tenant_id: &TenantId) -> Result<bool, TenantConfigError>;

    /// 获取所有租户配置
    async fn list_configs(&self) -> Result<Vec<(TenantId, TenantConfig)>, TenantConfigError>;
}

/// 内存租户配置存储（用于测试）
#[derive(Clone)]
pub struct MemoryTenantConfigStore {
    configs: std::sync::Arc<tokio::sync::RwLock<HashMap<String, TenantConfig>>>,
}

impl MemoryTenantConfigStore {
    pub fn new() -> Self {
        Self {
            configs: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemoryTenantConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TenantConfigStore for MemoryTenantConfigStore {
    async fn get_config(&self, tenant_id: &TenantId) -> Result<TenantConfig, TenantConfigError> {
        let configs = self.configs.read().await;
        configs
            .get(tenant_id.as_str())
            .cloned()
            .ok_or_else(|| TenantConfigError::NotFound(tenant_id.clone()))
    }

    async fn save_config(
        &self,
        tenant_id: &TenantId,
        config: &TenantConfig,
    ) -> Result<(), TenantConfigError> {
        let mut configs = self.configs.write().await;
        configs.insert(tenant_id.as_str().to_string(), config.clone());
        Ok(())
    }

    async fn delete_config(&self, tenant_id: &TenantId) -> Result<(), TenantConfigError> {
        let mut configs = self.configs.write().await;
        configs
            .remove(tenant_id.as_str())
            .ok_or_else(|| TenantConfigError::NotFound(tenant_id.clone()))?;
        Ok(())
    }

    async fn config_exists(&self, tenant_id: &TenantId) -> Result<bool, TenantConfigError> {
        let configs = self.configs.read().await;
        Ok(configs.contains_key(tenant_id.as_str()))
    }

    async fn list_configs(&self) -> Result<Vec<(TenantId, TenantConfig)>, TenantConfigError> {
        let configs = self.configs.read().await;
        Ok(configs
            .iter()
            .map(|(id, config)| (TenantId::from(id.clone()), config.clone()))
            .collect())
    }
}

/// 租户配置管理器
pub struct TenantConfigManager<S: TenantConfigStore> {
    store: S,
    cache: Option<std::sync::Arc<dyn TenantConfigCache>>,
}

impl<S: TenantConfigStore> TenantConfigManager<S> {
    /// 创建新的配置管理器
    pub fn new(store: S) -> Self {
        Self { store, cache: None }
    }

    /// 启用缓存
    pub fn with_cache(mut self, cache: std::sync::Arc<dyn TenantConfigCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 获取租户配置
    pub async fn get_config(
        &self,
        tenant_id: &TenantId,
    ) -> Result<TenantConfig, TenantConfigError> {
        // 先尝试从缓存获取
        if let Some(cache) = &self.cache
            && let Some(config) = cache.get(tenant_id).await
        {
            return Ok(config);
        }

        // 从存储获取
        let config = self.store.get_config(tenant_id).await?;

        // 写入缓存
        if let Some(cache) = &self.cache {
            cache.set(tenant_id, &config).await;
        }

        Ok(config)
    }

    /// 创建租户配置
    pub async fn create_config(
        &self,
        tenant_id: &TenantId,
        config: TenantConfig,
    ) -> Result<TenantConfig, TenantConfigError> {
        // 检查是否已存在
        if self.store.config_exists(tenant_id).await? {
            return Err(TenantConfigError::AlreadyExists(tenant_id.clone()));
        }

        // 验证配置
        config
            .validate()
            .map_err(TenantConfigError::ValidationError)?;

        // 保存配置
        self.store.save_config(tenant_id, &config).await?;

        // 写入缓存
        if let Some(cache) = &self.cache {
            cache.set(tenant_id, &config).await;
        }

        Ok(config)
    }

    /// 更新租户配置
    pub async fn update_config(
        &self,
        tenant_id: &TenantId,
        partial: PartialTenantConfig,
        updater_id: impl Into<String>,
    ) -> Result<TenantConfig, TenantConfigError> {
        let mut config = self.store.get_config(tenant_id).await?;
        config.merge(partial, updater_id);

        // 验证配置
        config
            .validate()
            .map_err(TenantConfigError::ValidationError)?;

        // 保存配置
        self.store.save_config(tenant_id, &config).await?;

        // 更新缓存
        if let Some(cache) = &self.cache {
            cache.set(tenant_id, &config).await;
        }

        Ok(config)
    }

    /// 删除租户配置
    pub async fn delete_config(&self, tenant_id: &TenantId) -> Result<(), TenantConfigError> {
        self.store.delete_config(tenant_id).await?;

        // 删除缓存
        if let Some(cache) = &self.cache {
            cache.delete(tenant_id).await;
        }

        Ok(())
    }

    /// 检查配置是否存在
    pub async fn config_exists(&self, tenant_id: &TenantId) -> Result<bool, TenantConfigError> {
        self.store.config_exists(tenant_id).await
    }

    /// 获取存储
    pub fn store(&self) -> &S {
        &self.store
    }
}

/// 租户配置缓存 trait
#[async_trait]
pub trait TenantConfigCache: Send + Sync {
    /// 获取配置
    async fn get(&self, tenant_id: &TenantId) -> Option<TenantConfig>;

    /// 设置配置
    async fn set(&self, tenant_id: &TenantId, config: &TenantConfig);

    /// 删除配置
    async fn delete(&self, tenant_id: &TenantId);

    /// 清空缓存
    async fn clear(&self);
}

/// Redis 租户配置缓存实现
///
/// 使用 Redis String 类型存储序列化后的 TenantConfig，
/// key 格式为 `credbridge:tenant:config:{tenant_id}`。
pub struct RedisTenantConfigCache {
    client: redis::Client,
    ttl_seconds: u64,
}

impl RedisTenantConfigCache {
    /// 创建 Redis 缓存实例
    ///
    /// # Arguments
    /// * `redis_url` - Redis 连接字符串，如 "redis://127.0.0.1:6379"
    /// * `ttl_seconds` - 缓存 TTL（秒），推荐 300（5 分钟）
    pub fn new(redis_url: &str, ttl_seconds: u64) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        Ok(Self {
            client,
            ttl_seconds,
        })
    }

    /// 构建 Redis key
    fn cache_key(tenant_id: &TenantId) -> String {
        format!("credbridge:tenant:config:{}", tenant_id.as_str())
    }
}

#[async_trait]
impl TenantConfigCache for RedisTenantConfigCache {
    async fn get(&self, tenant_id: &TenantId) -> Option<TenantConfig> {
        let mut conn = self.client.get_multiplexed_async_connection().await.ok()?;
        let key = Self::cache_key(tenant_id);
        let data: Option<String> = conn.get(&key).await.ok()?;
        data.and_then(|s| serde_json::from_str(&s).ok())
    }

    async fn set(&self, tenant_id: &TenantId, config: &TenantConfig) {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(e) => {
                log::error!("[TENANT-CONFIG] Failed to get Redis connection: {e}");
                return;
            }
        };
        let key = Self::cache_key(tenant_id);
        match serde_json::to_string(config) {
            Ok(data) => {
                // 确保 TTL 至少为 1 秒，避免 ttl_seconds=0 时 Redis 写入静默丢弃
                let ttl = if self.ttl_seconds == 0 {
                    log::warn!("[TENANT-CONFIG] Redis TTL 为 0，默认使用 1 秒");
                    1u64
                } else {
                    self.ttl_seconds
                };
                let _: Result<(), _> = conn.set_ex(&key, &data, ttl).await;
            }
            Err(e) => {
                log::error!("[TENANT-CONFIG] Failed to serialize tenant config: {e}");
            }
        }
    }

    async fn delete(&self, tenant_id: &TenantId) {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(_) => return,
        };
        let key = Self::cache_key(tenant_id);
        let _: Result<(), _> = conn.del(&key).await;
    }

    async fn clear(&self) {
        // 使用 SCAN 迭代删除所有 credbridge:tenant:config:* 键，避免 KEYS 阻塞
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(c) => c,
            Err(e) => {
                log::error!("[TENANT-CONFIG] Failed to get Redis connection for clear: {e}");
                return;
            }
        };
        let pattern = "credbridge:tenant:config:*";
        let mut cursor: u64 = 0;
        loop {
            let scan_result: redis::RedisResult<(u64, Vec<String>)> = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await;

            match scan_result {
                Ok((next_cursor, keys)) => {
                    if !keys.is_empty() {
                        let deleted: Result<(), _> = conn.del(keys).await;
                        if let Err(e) = deleted {
                            log::error!("[TENANT-CONFIG] Failed to delete keys: {e}");
                        }
                    }
                    cursor = next_cursor;
                    if cursor == 0 {
                        break;
                    }
                }
                Err(e) => {
                    log::error!("[TENANT-CONFIG] SCAN command failed: {e}");
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_flags_default() {
        let flags = FeatureFlags::default();
        assert!(flags.enable_credential_encryption);
        assert!(flags.enable_audit_logging);
        assert!(flags.enable_token_revocation);
        assert!(!flags.enable_mfa);
    }

    #[test]
    fn test_feature_flags_enable_disable() {
        let mut flags = FeatureFlags::default();

        assert!(flags.is_enabled("audit_logging"));
        flags.disable("audit_logging").unwrap();
        assert!(!flags.is_enabled("audit_logging"));

        flags.enable("mfa").unwrap();
        assert!(flags.is_enabled("mfa"));

        assert!(flags.enable("unknown_feature").is_err());
    }

    #[test]
    fn test_quota_limits_validation() {
        let valid = QuotaLimits::default();
        assert!(valid.validate().is_ok());

        let invalid = QuotaLimits {
            max_credentials: 0,
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_quota_limits_tiers() {
        let free = QuotaLimits::free_tier();
        assert_eq!(free.max_credentials, 100);

        let pro = QuotaLimits::pro_tier();
        assert_eq!(pro.max_credentials, 10_000);

        let enterprise = QuotaLimits::enterprise_tier();
        assert_eq!(enterprise.max_credentials, 100_000);
    }

    #[test]
    fn test_tenant_config_update() {
        let mut config = TenantConfig::default();
        assert_eq!(config.version, 1);

        config.update("user_123");
        assert_eq!(config.version, 2);
        assert_eq!(config.updated_by, Some("user_123".to_string()));
        assert!(config.updated_at.is_some());
    }

    #[test]
    fn test_tenant_config_merge() {
        let mut config = TenantConfig::default();

        let partial = PartialTenantConfig {
            feature_flags: Some(FeatureFlags {
                enable_mfa: true,
                ..Default::default()
            }),
            quota_limits: None,
            settings: None,
        };

        config.merge(partial, "admin");
        assert!(config.feature_flags.enable_mfa);
        assert_eq!(config.version, 2);
    }

    #[test]
    fn test_tenant_config_tiers() {
        let free = TenantConfig::free_tier();
        assert!(!free.feature_flags.enable_mfa);
        assert_eq!(free.quota_limits.max_credentials, 100);

        let pro = TenantConfig::pro_tier();
        assert!(pro.feature_flags.enable_mfa);
        assert_eq!(pro.quota_limits.max_credentials, 10_000);

        let enterprise = TenantConfig::enterprise_tier();
        assert!(enterprise.feature_flags.enable_sso);
        assert!(enterprise.settings.require_mfa);
    }

    #[tokio::test]
    async fn test_memory_tenant_config_store() {
        let store = MemoryTenantConfigStore::new();
        let tenant_id = TenantId::new();

        // 初始不存在
        assert!(!store.config_exists(&tenant_id).await.unwrap());

        // 保存配置
        let config = TenantConfig::default();
        store.save_config(&tenant_id, &config).await.unwrap();

        // 存在
        assert!(store.config_exists(&tenant_id).await.unwrap());

        // 获取配置
        let retrieved = store.get_config(&tenant_id).await.unwrap();
        assert_eq!(retrieved.version, config.version);

        // 删除配置
        store.delete_config(&tenant_id).await.unwrap();
        assert!(!store.config_exists(&tenant_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_tenant_config_manager() {
        let store = MemoryTenantConfigStore::new();
        let manager = TenantConfigManager::new(store);
        let tenant_id = TenantId::new();

        // 创建配置
        let config = TenantConfig::pro_tier();
        let created = manager.create_config(&tenant_id, config).await.unwrap();
        assert_eq!(created.quota_limits.max_credentials, 10_000);

        // 获取配置
        let retrieved = manager.get_config(&tenant_id).await.unwrap();
        assert_eq!(retrieved.version, 1);

        // 更新配置
        let partial = PartialTenantConfig {
            feature_flags: Some(FeatureFlags::enable_all_advanced()),
            ..Default::default()
        };
        let updated = manager
            .update_config(&tenant_id, partial, "admin")
            .await
            .unwrap();
        assert!(updated.feature_flags.enable_sso);
        assert_eq!(updated.version, 2);

        // 删除配置
        manager.delete_config(&tenant_id).await.unwrap();
        assert!(manager.get_config(&tenant_id).await.is_err());
    }
}
