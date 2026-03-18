//! 凭证版本历史模型
//!
//! 实现凭证版本控制功能
//! - 版本历史存储
//! - 版本回滚支持
//! - 审计追踪

use super::models::EncryptedPayload;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 凭证版本历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialVersion {
    /// 版本记录唯一 ID
    pub id: Uuid,

    /// 关联的凭证 ID
    pub credential_id: String,

    /// 版本号
    pub version: u32,

    /// 加密的凭证载荷
    pub encrypted_payload: EncryptedPayload,

    /// 变更原因
    pub change_reason: Option<String>,

    /// 变更人用户 ID
    pub changed_by: Option<String>,

    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl CredentialVersion {
    /// 创建新的版本记录
    pub fn new(
        credential_id: String,
        version: u32,
        encrypted_payload: EncryptedPayload,
        change_reason: Option<String>,
        changed_by: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            credential_id,
            version,
            encrypted_payload,
            change_reason,
            changed_by,
            created_at: Utc::now(),
        }
    }
}

/// 版本历史查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    /// 凭证 ID
    pub credential_id: String,

    /// 当前版本号
    pub current_version: u32,

    /// 版本列表
    pub versions: Vec<VersionSummary>,

    /// 总版本数
    pub total: usize,
}

/// 版本摘要（不含加密载荷）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionSummary {
    /// 版本号
    pub version: u32,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 变更人
    pub changed_by: Option<String>,

    /// 变更原因
    pub change_reason: Option<String>,
}

/// 版本详情响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDetail {
    /// 凭证 ID
    pub credential_id: String,

    /// 版本号
    pub version: u32,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 变更人
    pub changed_by: Option<String>,

    /// 变更原因
    pub change_reason: Option<String>,

    /// 元数据
    pub metadata: VersionMetadata,
}

/// 版本元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    /// 服务 ID
    pub service_id: String,

    /// 凭证类型
    pub credential_type: String,

    /// 加密算法
    pub algorithm: String,
}

/// 更新凭证请求
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCredentialRequest {
    /// 新的明文凭证内容
    pub plaintext_data: serde_json::Value,

    /// 变更原因（用于审计）
    pub change_reason: Option<String>,

    /// 期望的版本号（用于乐观锁）
    pub expected_version: Option<u32>,
}

/// 更新凭证响应
#[derive(Debug, Clone, Serialize)]
pub struct UpdateCredentialResponse {
    /// 凭证 ID
    pub credential_id: String,

    /// 新版本号
    pub version: u32,

    /// 服务 ID
    pub service_id: String,

    /// 凭证类型
    pub credential_type: String,

    /// 更新时间
    pub updated_at: String,

    /// 上一版本号
    pub previous_version: u32,
}

/// 回滚请求
#[derive(Debug, Clone, Deserialize)]
pub struct RollbackRequest {
    /// 目标版本号
    pub target_version: u32,

    /// 回滚原因
    pub reason: String,
}

/// 回滚响应
#[derive(Debug, Clone, Serialize)]
pub struct RollbackResponse {
    /// 凭证 ID
    pub credential_id: String,

    /// 回滚前版本
    pub previous_version: u32,

    /// 当前版本（回滚后创建的新版本）
    pub current_version: u32,

    /// 回滚到的源版本
    pub rollback_to_version: u32,

    /// 回滚时间
    pub rollback_at: String,

    /// 回滚原因
    pub reason: String,
}

/// 版本差异类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffType {
    /// 内容变更
    ContentChanged,
    /// 仅原因变更
    ReasonChanged,
    /// 回滚操作
    Rollback,
}

/// 版本差异
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDiff {
    /// 凭证 ID
    pub credential_id: String,

    /// 源版本
    pub from_version: u32,

    /// 目标版本
    pub to_version: u32,

    /// 差异类型
    pub diff_type: DiffType,

    /// 元数据变更
    pub metadata_changes: Vec<MetadataChange>,
}

/// 元数据变更
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataChange {
    /// 字段名
    pub field: String,

    /// 旧值
    pub old_value: Option<String>,

    /// 新值
    pub new_value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::constants;

    fn create_test_payload() -> EncryptedPayload {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        EncryptedPayload {
            version: constants::PROTOCOL_VERSION,
            algorithm: constants::ALGORITHM_AES_256_GCM.to_string(),
            kdf: constants::KDF_HKDF_SHA256.to_string(),
            nonce: URL_SAFE_NO_PAD.encode(&[0u8; constants::NONCE_LENGTH]),
            auth_tag: URL_SAFE_NO_PAD.encode(&[0u8; constants::AUTH_TAG_LENGTH]),
            ciphertext: URL_SAFE_NO_PAD.encode(&[1, 2, 3, 4, 5]),
        }
    }

    #[test]
    fn test_credential_version_creation() {
        let version = CredentialVersion::new(
            "test-credential-id".to_string(),
            1,
            create_test_payload(),
            Some("初始创建".to_string()),
            Some("user_123".to_string()),
        );

        assert_eq!(version.credential_id, "test-credential-id");
        assert_eq!(version.version, 1);
        assert_eq!(version.change_reason, Some("初始创建".to_string()));
        assert_eq!(version.changed_by, Some("user_123".to_string()));
    }

    #[test]
    fn test_version_summary_serialization() {
        let summary = VersionSummary {
            version: 2,
            created_at: Utc::now(),
            changed_by: Some("user_456".to_string()),
            change_reason: Some("密码轮换".to_string()),
        };

        let json = serde_json::to_string(&summary).unwrap();
        let parsed: VersionSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 2);
    }

    #[test]
    fn test_rollback_request_deserialization() {
        let json = r#"{"target_version": 2, "reason": "安全审计要求"}"#;
        let request: RollbackRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.target_version, 2);
        assert_eq!(request.reason, "安全审计要求");
    }

    #[test]
    fn test_update_credential_request() {
        let json = r#"{
            "plaintext_data": {"password": "new_secret"},
            "change_reason": "定期更新",
            "expected_version": 1
        }"#;
        let request: UpdateCredentialRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.change_reason, Some("定期更新".to_string()));
        assert_eq!(request.expected_version, Some(1));
    }
}
