//! 审计事件定义
//!
//! 定义审计日志事件结构体和枚举类型
//! 符合 FR3 审计日志系统架构约束

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// 审计事件 ID（UUID v7）
pub type AuditEventId = String;

/// 用户 ID 哈希（SHA-256）
pub type UserIdHash = String;

/// 会话 ID
pub type SessionId = String;

/// 服务标识
pub type ServiceId = String;

/// TEE MRENCLAVE 测量值
pub type TeeMrenclave = String;

/// Action Token JTI
pub type ActionTokenJti = String;

/// 审计事件严重等级
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum RiskTier {
    /// 低风险 - 常规操作
    #[default]
    Low,
    /// 中风险 - 敏感数据访问
    Medium,
    /// 高风险 - 特权操作
    High,
    /// 严重风险 - 安全相关操作
    Critical,
}

impl RiskTier {
    /// 获取风险等级的数值权重
    pub fn weight(&self) -> u8 {
        match self {
            RiskTier::Low => 1,
            RiskTier::Medium => 2,
            RiskTier::High => 4,
            RiskTier::Critical => 8,
        }
    }

    /// 检查是否需要额外审查
    pub fn requires_review(&self) -> bool {
        matches!(self, RiskTier::High | RiskTier::Critical)
    }

    /// 根据操作类型自动判断风险等级
    pub fn from_action(action: &AuditAction) -> Self {
        match action {
            AuditAction::CredentialDecrypt => RiskTier::High,
            AuditAction::CredentialAccess => RiskTier::Medium,
            AuditAction::CredentialCreate => RiskTier::Medium,
            AuditAction::CredentialUpdate => RiskTier::Medium,
            AuditAction::CredentialDelete => RiskTier::High,
            AuditAction::TokenIssue => RiskTier::Medium,
            AuditAction::TokenRevoke => RiskTier::Medium,
            AuditAction::TokenValidate => RiskTier::Low,
            AuditAction::AuditQuery => RiskTier::High,
            AuditAction::SystemConfigChange => RiskTier::Critical,
            AuditAction::TeeAttestation => RiskTier::Medium,
            AuditAction::KeyRotation => RiskTier::Critical,
            AuditAction::AdminLogin => RiskTier::Critical,
            AuditAction::FailedAuth => RiskTier::High,
        }
    }
}

impl fmt::Display for RiskTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskTier::Low => write!(f, "low"),
            RiskTier::Medium => write!(f, "medium"),
            RiskTier::High => write!(f, "high"),
            RiskTier::Critical => write!(f, "critical"),
        }
    }
}

/// 审计操作类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// 凭证解密
    CredentialDecrypt,
    /// 凭证访问（元数据读取）
    CredentialAccess,
    /// 凭证创建
    CredentialCreate,
    /// 凭证更新
    CredentialUpdate,
    /// 凭证删除
    CredentialDelete,
    /// Token 签发
    TokenIssue,
    /// Token 撤销
    TokenRevoke,
    /// Token 验证
    TokenValidate,
    /// 审计日志查询
    AuditQuery,
    /// 系统配置变更
    SystemConfigChange,
    /// TEE 远程证明
    TeeAttestation,
    /// 密钥轮换
    KeyRotation,
    /// 管理员登录
    AdminLogin,
    /// 认证失败
    FailedAuth,
}

impl fmt::Display for AuditAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AuditAction::CredentialDecrypt => "credential_decrypt",
            AuditAction::CredentialAccess => "credential_access",
            AuditAction::CredentialCreate => "credential_create",
            AuditAction::CredentialUpdate => "credential_update",
            AuditAction::CredentialDelete => "credential_delete",
            AuditAction::TokenIssue => "token_issue",
            AuditAction::TokenRevoke => "token_revoke",
            AuditAction::TokenValidate => "token_validate",
            AuditAction::AuditQuery => "audit_query",
            AuditAction::SystemConfigChange => "system_config_change",
            AuditAction::TeeAttestation => "tee_attestation",
            AuditAction::KeyRotation => "key_rotation",
            AuditAction::AdminLogin => "admin_login",
            AuditAction::FailedAuth => "failed_auth",
        };
        write!(f, "{s}")
    }
}

/// 操作结果
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// 成功
    Success,
    /// 失败
    Failure,
    /// 拒绝（权限不足等）
    Denied,
    /// 超时
    Timeout,
    /// 异常终止
    Aborted,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Success => write!(f, "success"),
            Outcome::Failure => write!(f, "failure"),
            Outcome::Denied => write!(f, "denied"),
            Outcome::Timeout => write!(f, "timeout"),
            Outcome::Aborted => write!(f, "aborted"),
        }
    }
}

impl Outcome {
    /// 检查操作是否成功
    pub fn is_success(&self) -> bool {
        matches!(self, Outcome::Success)
    }

    /// 检查操作是否失败
    pub fn is_failure(&self) -> bool {
        !self.is_success()
    }
}

/// 脱敏参数值
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RedactedParam {
    /// SSN 脱敏
    SsnRedacted,
    /// 密码脱敏
    PasswordRedacted,
    /// API 密钥脱敏
    ApiKeyRedacted,
    /// 信用卡号脱敏
    CreditCardRedacted,
    /// 邮箱脱敏
    EmailRedacted,
    /// 电话号码脱敏
    PhoneRedacted,
    /// 地址脱敏
    AddressRedacted,
    /// 密钥脱敏
    KeyRedacted,
    /// 原始值（无需脱敏）
    Plain(String),
}

impl fmt::Display for RedactedParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedactedParam::SsnRedacted => write!(f, "[SSN_REDACTED]"),
            RedactedParam::PasswordRedacted => write!(f, "[PASSWORD_REDACTED]"),
            RedactedParam::ApiKeyRedacted => write!(f, "[API_KEY_REDACTED]"),
            RedactedParam::CreditCardRedacted => write!(f, "[CREDIT_CARD_REDACTED]"),
            RedactedParam::EmailRedacted => write!(f, "[EMAIL_REDACTED]"),
            RedactedParam::PhoneRedacted => write!(f, "[PHONE_REDACTED]"),
            RedactedParam::AddressRedacted => write!(f, "[ADDRESS_REDACTED]"),
            RedactedParam::KeyRedacted => write!(f, "[KEY_REDACTED]"),
            RedactedParam::Plain(v) => write!(f, "{v}"),
        }
    }
}

/// 审计事件条目
///
/// 不可篡改审计日志的基本单位
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    /// 事件唯一 ID（UUID v7，时间排序）
    pub id: AuditEventId,

    /// 用户 ID 哈希（SHA-256）
    pub user_id_hash: UserIdHash,

    /// 事件时间戳（UTC，Unix 时间戳毫秒）
    pub timestamp: u64,

    /// 会话 ID
    pub session_id: SessionId,

    /// 服务标识
    pub service: ServiceId,

    /// 操作类型
    pub action: AuditAction,

    /// 风险等级
    pub risk_tier: RiskTier,

    /// 操作结果
    pub outcome: Outcome,

    /// TEE MRENCLAVE 测量值
    pub tee_mrenclave: TeeMrenclave,

    /// Action Token JTI
    pub action_token_jti: ActionTokenJti,

    /// 操作参数（已脱敏）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<(String, RedactedParam)>>,

    /// 错误信息（仅失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// 客户端 IP 哈希
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip_hash: Option<String>,

    /// 用户代理哈希
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent_hash: Option<String>,
}

impl AuditEntry {
    /// 创建新的审计事件条目
    ///
    /// # 参数
    /// - `user_id_hash`: 用户 ID 的 SHA-256 哈希
    /// - `session_id`: 会话 ID
    /// - `service`: 服务标识
    /// - `action`: 操作类型
    /// - `outcome`: 操作结果
    /// - `tee_mrenclave`: TEE MRENCLAVE 测量值
    /// - `action_token_jti`: Action Token JTI
    pub fn new(
        user_id_hash: impl Into<String>,
        session_id: impl Into<String>,
        service: impl Into<String>,
        action: AuditAction,
        outcome: Outcome,
        tee_mrenclave: impl Into<String>,
        action_token_jti: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::now_v7().to_string(),
            user_id_hash: user_id_hash.into(),
            timestamp: current_timestamp_millis(),
            session_id: session_id.into(),
            service: service.into(),
            action,
            risk_tier: RiskTier::from_action(&action),
            outcome,
            tee_mrenclave: tee_mrenclave.into(),
            action_token_jti: action_token_jti.into(),
            params: None,
            error_message: None,
            client_ip_hash: None,
            user_agent_hash: None,
        }
    }

    /// 添加脱敏参数
    pub fn with_param(mut self, key: impl Into<String>, value: RedactedParam) -> Self {
        let params = self.params.get_or_insert_with(Vec::new);
        params.push((key.into(), value));
        self
    }

    /// 添加多个脱敏参数
    pub fn with_params(mut self, params: Vec<(String, RedactedParam)>) -> Self {
        self.params = Some(params);
        self
    }

    /// 设置错误信息
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error_message = Some(error.into());
        self
    }

    /// 设置客户端 IP 哈希
    pub fn with_client_ip_hash(mut self, ip_hash: impl Into<String>) -> Self {
        self.client_ip_hash = Some(ip_hash.into());
        self
    }

    /// 设置用户代理哈希
    pub fn with_user_agent_hash(mut self, ua_hash: impl Into<String>) -> Self {
        self.user_agent_hash = Some(ua_hash.into());
        self
    }

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// 从 JSON 字符串反序列化
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// 计算条目的内容哈希（用于 Merkle Tree）
    pub fn content_hash(&self) -> [u8; 32] {
        use ring::digest::{SHA256, digest};
        let json = self.to_json().unwrap_or_default();
        let digest = digest(&SHA256, json.as_bytes());
        let mut hash = [0u8; 32];
        hash.copy_from_slice(digest.as_ref());
        hash
    }

    /// 检查是否为高风险操作
    pub fn is_high_risk(&self) -> bool {
        self.risk_tier.requires_review()
    }

    /// 获取事件年龄（毫秒）
    pub fn age_millis(&self) -> u64 {
        let now = current_timestamp_millis();
        now.saturating_sub(self.timestamp)
    }
}

/// PII 数据脱敏工具
pub struct PiiRedactor;

impl PiiRedactor {
    /// 对值进行 SSN 脱敏
    pub fn redact_ssn<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::SsnRedacted
    }

    /// 对值进行密码脱敏
    pub fn redact_password<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::PasswordRedacted
    }

    /// 对值进行 API 密钥脱敏
    pub fn redact_api_key<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::ApiKeyRedacted
    }

    /// 对值进行信用卡号脱敏
    pub fn redact_credit_card<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::CreditCardRedacted
    }

    /// 对值进行邮箱脱敏
    pub fn redact_email<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::EmailRedacted
    }

    /// 对值进行电话脱敏
    pub fn redact_phone<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::PhoneRedacted
    }

    /// 对值进行地址脱敏
    pub fn redact_address<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::AddressRedacted
    }

    /// 对值进行密钥脱敏
    pub fn redact_key<T: fmt::Display>(_value: T) -> RedactedParam {
        RedactedParam::KeyRedacted
    }

    /// 保留原始值（无需脱敏）
    pub fn plain<T: fmt::Display>(value: T) -> RedactedParam {
        RedactedParam::Plain(value.to_string())
    }

    /// 根据参数名称自动判断脱敏类型
    pub fn auto_redact(key: &str, value: impl fmt::Display) -> RedactedParam {
        let key_lower = key.to_lowercase();

        if key_lower.contains("ssn") || key_lower.contains("social") {
            Self::redact_ssn(value)
        } else if key_lower.contains("password")
            || key_lower.contains("passwd")
            || key_lower.contains("pwd")
        {
            Self::redact_password(value)
        } else if key_lower.contains("api_key")
            || key_lower.contains("apikey")
            || key_lower.contains("secret")
        {
            Self::redact_api_key(value)
        } else if key_lower.contains("credit_card")
            || key_lower.contains("card_number")
            || key_lower.contains("ccv")
        {
            Self::redact_credit_card(value)
        } else if key_lower.contains("email") || key_lower.contains("mail") {
            Self::redact_email(value)
        } else if key_lower.contains("phone")
            || key_lower.contains("mobile")
            || key_lower.contains("tel")
        {
            Self::redact_phone(value)
        } else if key_lower.contains("address") || key_lower.contains("addr") {
            Self::redact_address(value)
        } else if key_lower.contains("key")
            || key_lower.contains("private")
            || key_lower.contains("token")
        {
            Self::redact_key(value)
        } else {
            Self::plain(value)
        }
    }
}

/// 获取当前 Unix 时间戳（毫秒）
fn current_timestamp_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_millis() as u64
}

/// 计算用户 ID 的 SHA-256 哈希
pub fn hash_user_id(user_id: &str) -> String {
    use ring::digest::{SHA256, digest};
    let digest = digest(&SHA256, user_id.as_bytes());
    hex::encode(digest.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_tier_weight() {
        assert_eq!(RiskTier::Low.weight(), 1);
        assert_eq!(RiskTier::Medium.weight(), 2);
        assert_eq!(RiskTier::High.weight(), 4);
        assert_eq!(RiskTier::Critical.weight(), 8);
    }

    #[test]
    fn test_risk_tier_requires_review() {
        assert!(!RiskTier::Low.requires_review());
        assert!(!RiskTier::Medium.requires_review());
        assert!(RiskTier::High.requires_review());
        assert!(RiskTier::Critical.requires_review());
    }

    #[test]
    fn test_risk_tier_from_action() {
        assert_eq!(
            RiskTier::from_action(&AuditAction::TokenValidate),
            RiskTier::Low
        );
        assert_eq!(
            RiskTier::from_action(&AuditAction::CredentialAccess),
            RiskTier::Medium
        );
        assert_eq!(
            RiskTier::from_action(&AuditAction::CredentialDecrypt),
            RiskTier::High
        );
        assert_eq!(
            RiskTier::from_action(&AuditAction::KeyRotation),
            RiskTier::Critical
        );
    }

    #[test]
    fn test_outcome_display() {
        assert_eq!(Outcome::Success.to_string(), "success");
        assert_eq!(Outcome::Failure.to_string(), "failure");
        assert_eq!(Outcome::Denied.to_string(), "denied");
    }

    #[test]
    fn test_outcome_is_success() {
        assert!(Outcome::Success.is_success());
        assert!(!Outcome::Failure.is_success());
        assert!(!Outcome::Denied.is_success());
    }

    #[test]
    fn test_redacted_param_display() {
        assert_eq!(RedactedParam::SsnRedacted.to_string(), "[SSN_REDACTED]");
        assert_eq!(
            RedactedParam::PasswordRedacted.to_string(),
            "[PASSWORD_REDACTED]"
        );
        assert_eq!(RedactedParam::Plain("test".to_string()).to_string(), "test");
    }

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry::new(
            "user_hash_123",
            "session_456",
            "vault-service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_abc",
            "jti_xyz",
        );

        assert!(!entry.id.is_empty());
        assert_eq!(entry.user_id_hash, "user_hash_123");
        assert_eq!(entry.session_id, "session_456");
        assert_eq!(entry.service, "vault-service");
        assert_eq!(entry.action, AuditAction::CredentialDecrypt);
        assert_eq!(entry.outcome, Outcome::Success);
        assert_eq!(entry.risk_tier, RiskTier::High);
        assert_eq!(entry.tee_mrenclave, "mrenclave_abc");
        assert_eq!(entry.action_token_jti, "jti_xyz");
        assert!(entry.timestamp > 0);
    }

    #[test]
    fn test_audit_entry_with_params() {
        let entry = AuditEntry::new(
            "user_hash_123",
            "session_456",
            "vault-service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_abc",
            "jti_xyz",
        )
        .with_param(
            "credential_id",
            RedactedParam::Plain("cred_123".to_string()),
        )
        .with_param("ssn", RedactedParam::SsnRedacted);

        let params = entry.params.as_ref().unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].0, "credential_id");
        assert_eq!(params[1].0, "ssn");
    }

    #[test]
    fn test_audit_entry_json_roundtrip() {
        let entry = AuditEntry::new(
            "user_hash_123",
            "session_456",
            "vault-service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_abc",
            "jti_xyz",
        )
        .with_param("test", RedactedParam::Plain("value".to_string()));

        let json = entry.to_json().unwrap();
        let deserialized = AuditEntry::from_json(&json).unwrap();

        assert_eq!(entry.id, deserialized.id);
        assert_eq!(entry.user_id_hash, deserialized.user_id_hash);
        assert_eq!(entry.action, deserialized.action);
        assert_eq!(entry.outcome, deserialized.outcome);
    }

    #[test]
    fn test_audit_entry_content_hash() {
        let entry = AuditEntry::new(
            "user_hash_123",
            "session_456",
            "vault-service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave_abc",
            "jti_xyz",
        );

        let hash = entry.content_hash();
        assert_eq!(hash.len(), 32);
        // 相同内容的条目应有不同 ID，因此哈希不同
    }

    #[test]
    fn test_pii_redactor_auto() {
        assert!(matches!(
            PiiRedactor::auto_redact("ssn", "123-45-6789"),
            RedactedParam::SsnRedacted
        ));
        assert!(matches!(
            PiiRedactor::auto_redact("password", "secret123"),
            RedactedParam::PasswordRedacted
        ));
        assert!(matches!(
            PiiRedactor::auto_redact("email", "test@example.com"),
            RedactedParam::EmailRedacted
        ));
        assert!(matches!(
            PiiRedactor::auto_redact("api_key", "abc123"),
            RedactedParam::ApiKeyRedacted
        ));
        assert!(matches!(
            PiiRedactor::auto_redact("normal_param", "normal_value"),
            RedactedParam::Plain(_)
        ));
    }

    #[test]
    fn test_hash_user_id() {
        let hash1 = hash_user_id("user_123");
        let hash2 = hash_user_id("user_123");
        let hash3 = hash_user_id("user_456");

        assert_eq!(hash1, hash2); // 相同输入产生相同哈希
        assert_ne!(hash1, hash3); // 不同输入产生不同哈希
        assert_eq!(hash1.len(), 64); // SHA-256 哈希为 64 个十六进制字符
    }

    #[test]
    fn test_audit_entry_is_high_risk() {
        let low_risk = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            AuditAction::TokenValidate,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        assert!(!low_risk.is_high_risk());

        let high_risk = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Success,
            "mrenclave",
            "jti",
        );
        assert!(high_risk.is_high_risk());
    }

    #[test]
    fn test_audit_entry_with_error() {
        let entry = AuditEntry::new(
            "user_hash",
            "session",
            "service",
            AuditAction::CredentialDecrypt,
            Outcome::Failure,
            "mrenclave",
            "jti",
        )
        .with_error("Decryption failed: invalid key");

        assert_eq!(
            entry.error_message.unwrap(),
            "Decryption failed: invalid key"
        );
    }
}
