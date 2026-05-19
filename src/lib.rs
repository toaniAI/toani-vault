//! CredBridge - AI 原生零信任凭证保险库系统
//!
//! 专为 AI Agent 设计的 TEE（可信执行环境）凭证保险库，
//! 采用四层密钥层次架构（L0-L3）实现零知识安全存储。
//!
//! # 架构概览
//!
//! ```text
//! L0: SGX Sealing Key（硬件根密钥）
//!   │ HKDF-Extract
//!   ▼
//! L1: Enclave Master Key（TEE 内派生）
//!   │ HKDF-Expand(tenant_id + user_id)
//!   ▼
//! L2: User Vault Key（每用户独立）
//!   │ HKDF-Expand(credential_id + purpose)
//!   ▼
//! L3: Credential Encryption Key（每条凭证独立）
//!   │ AES-256-GCM
//!   ▼
//! Encrypted Credential
//! ```

pub mod api;
pub mod audit;
pub mod auth;
pub mod config;
pub mod connector;
pub mod crypto;
pub mod models;
pub mod oauth_broker;
pub mod services;
pub mod tee;
pub mod tenant;
pub mod token;
pub mod utils;
pub mod vault;

// 重新导出核心类型
pub use crypto::{
    CredentialKey, CryptoError, EnclaveMasterKey, EncryptedBlob, HardwareRootKey, KeyHandle,
    KeyHierarchy, KeyPurpose, UserVaultKey, constant_time, constants, decrypt_credential,
    encrypt_credential, key_derivation, key_rotation,
};

pub use config::{
    ConfigError, PrivyConfig, TEE_DEBUG_ENV, TEE_MODE_ENV, TEE_PCS_BASE_URL_ENV, TeeRuntimeConfig,
    TeeRuntimeMode,
};

pub use tee::{
    Enclave, EnclaveState, SelfCheckItem, SelfCheckStatus, SharedEnclave, StartupReadiness,
    TeeCapabilities, TeeRuntimeError, TeeType, detect_tee_capabilities, driver_verify, upgrade,
    validate_runtime_requirements,
};

// 重新导出 Token 模块
pub use token::{
    DEFAULT_TOKEN_TTL_SECONDS, PasetoKey, PasetoToken, TokenClaims, TokenError,
    TokenValidationResult, scopes,
};

// 重新导出 Vault 模块
pub use vault::{
    CredentialId, CredentialVault, EncryptedPayload, ServiceId, TenantId, UserId, VaultEntry,
    VaultError,
    backend::{
        VaultBackendError, VaultHealthStatus, VaultStorageBackend, VaultStorageBackendBuilder,
        check_vault_health,
    },
    client::{VaultClientError, VaultConfig, VaultCredentialData, VaultKvClient},
    postgres::{PostgresBackendError, PostgresStorageBackend},
};

// 重新导出审计模块
pub use audit::{
    AuditAction, AuditEntry, AuditFilter, AuditLogChain, AuditRecorder, AuditReport, AuditStats,
    MemoryAuditStorage, Outcome, PiiRedactor, RecorderError, RedactedParam, RiskTier,
    SignedAuditEntry, SigningKeyPair, create_memory_storage, hash_user_id, is_high_risk_action,
};

// 重新导出 Connector 模块
pub use connector::{
    Connector, ConnectorConfig, ConnectorError, ConnectorRegistry, ConnectorResult, NopConnector,
    ValidatedParams, ValidationError,
    http::{HttpConnector, HttpConnectorConfig},
    timeout::{TimeoutConfig, TimeoutError, TimeoutWrapper},
    validator::{CompositeValidator, SchemaValidator, ValidationRule, ValidatorBuilder},
};

// 重新导出 Auth 模块
pub use auth::{
    AuthAuditLog, AuthError, AuthEventType, AuthService, AuthServiceImpl, AuthSession,
    CreateSessionRequest, CreateUserRequest, ExternalIdentity, IdentityProvider, InvitationStatus,
    InviteeType, MembershipRole, MembershipSource, MembershipStatus, MfaStatus, PrivyAuthResponse,
    TenantInvitation, TenantMembership, User, UserStatus, create_owner_membership,
};

/// 库版本
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 初始化 CredBridge
pub fn init() {
    // 初始化日志、配置等
}
