//! Token Scope 权限系统
//!
//! 实现基于 RBAC + 资源级权限的 Scope 管理
//! 支持细粒度的凭证访问控制和受限 Token
//!
//! # 架构约束
//!
//! - **FR2**: 有限 Scope Token 系统
//! - **SA-003**: PASETO Token Scope 绑定
//!
//! # 核心功能
//!
//! - **Scope 枚举**: 类型安全的权限定义
//! - **Operation 枚举**: 操作类型定义
//! - **can_access**: 权限检查方法
//! - **受限 Token**: 资源级权限控制（credential_ids 白名单）
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use vault_service::token::scope::{Scope, Operation};
//!
//! // 检查 Scope 是否允许操作
//! let scope = Scope::CredentialRead;
//! assert!(scope.can_access(&Operation::ReadCredential));
//!
//! // Admin 拥有所有权限
//! let admin = Scope::Admin;
//! assert!(admin.can_access(&Operation::DeleteCredential));
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Scope 权限枚举
///
/// 定义 Token 可拥有的权限范围，符合最小权限原则
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// 读取凭证元数据
    CredentialRead,

    /// 解密凭证内容（获取明文）
    CredentialDecrypt,

    /// 创建/更新凭证
    CredentialWrite,

    /// 删除凭证
    CredentialDelete,

    /// 管理 Token（撤销/刷新）
    TokenManage,

    /// 读取审计日志
    AuditRead,

    /// 所有管理权限（超级管理员）
    Admin,
}

impl Scope {
    /// 将 Scope 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::CredentialRead => "credential:read",
            Scope::CredentialDecrypt => "credential:decrypt",
            Scope::CredentialWrite => "credential:write",
            Scope::CredentialDelete => "credential:delete",
            Scope::TokenManage => "token:manage",
            Scope::AuditRead => "audit:read",
            Scope::Admin => "admin",
        }
    }

    /// 从字符串解析 Scope
    ///
    /// # 参数
    /// - `s`: 要解析的字符串
    ///
    /// # 返回值
    /// 解析成功返回 `Some(Scope)`，失败返回 `None`
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "credential:read" => Some(Scope::CredentialRead),
            "credential:decrypt" => Some(Scope::CredentialDecrypt),
            "credential:write" => Some(Scope::CredentialWrite),
            "credential:delete" => Some(Scope::CredentialDelete),
            "token:manage" => Some(Scope::TokenManage),
            "audit:read" => Some(Scope::AuditRead),
            "admin" => Some(Scope::Admin),
            _ => None,
        }
    }

    /// 检查 Scope 是否允许执行指定操作
    ///
    /// # 权限映射规则
    /// - `credential:read`: 允许 ReadCredential
    /// - `credential:decrypt`: 允许 DecryptCredential（也隐式包含 read）
    /// - `credential:write`: 允许 CreateCredential, UpdateCredential
    /// - `credential:delete`: 允许 DeleteCredential
    /// - `token:manage`: 允许 ManageToken
    /// - `audit:read`: 允许 ReadAudit
    /// - `admin`: 允许所有操作
    ///
    /// # 参数
    /// - `operation`: 要检查的操作
    ///
    /// # 返回值
    /// `true` 如果 Scope 允许该操作
    pub fn can_access(&self, operation: &Operation) -> bool {
        match (self, operation) {
            // Admin 拥有所有权限
            (Scope::Admin, _) => true,

            // 读权限可访问读操作
            (Scope::CredentialRead, Operation::ReadCredential) => true,

            // 解密权限可访问解密操作（解密通常也需要读取）
            (Scope::CredentialDecrypt, Operation::DecryptCredential) => true,
            (Scope::CredentialDecrypt, Operation::ReadCredential) => true,

            // 写权限可访问写操作
            (Scope::CredentialWrite, Operation::CreateCredential) => true,
            (Scope::CredentialWrite, Operation::UpdateCredential) => true,

            // 删除权限可访问删除操作
            (Scope::CredentialDelete, Operation::DeleteCredential) => true,

            // Token 管理权限
            (Scope::TokenManage, Operation::ManageToken) => true,
            (Scope::TokenManage, Operation::RevokeToken) => true,
            (Scope::TokenManage, Operation::RefreshToken) => true,

            // 审计读取权限
            (Scope::AuditRead, Operation::ReadAudit) => true,

            // 其他情况不允许
            _ => false,
        }
    }

    /// 检查此 Scope 是否为凭证相关权限
    pub fn is_credential_scope(&self) -> bool {
        matches!(
            self,
            Scope::CredentialRead
                | Scope::CredentialDecrypt
                | Scope::CredentialWrite
                | Scope::CredentialDelete
                | Scope::Admin
        )
    }

    /// 检查此 Scope 是否为管理员权限
    pub fn is_admin(&self) -> bool {
        matches!(self, Scope::Admin)
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 操作类型枚举
///
/// 定义系统中可执行的操作，用于权限检查
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// 读取凭证元数据
    ReadCredential,

    /// 解密凭证内容
    DecryptCredential,

    /// 创建新凭证
    CreateCredential,

    /// 更新凭证
    UpdateCredential,

    /// 删除凭证
    DeleteCredential,

    /// 管理 Token（签发新 Token）
    ManageToken,

    /// 撤销 Token
    RevokeToken,

    /// 刷新 Token
    RefreshToken,

    /// 读取审计日志
    ReadAudit,
}

impl Operation {
    /// 将 Operation 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            Operation::ReadCredential => "read_credential",
            Operation::DecryptCredential => "decrypt_credential",
            Operation::CreateCredential => "create_credential",
            Operation::UpdateCredential => "update_credential",
            Operation::DeleteCredential => "delete_credential",
            Operation::ManageToken => "manage_token",
            Operation::RevokeToken => "revoke_token",
            Operation::RefreshToken => "refresh_token",
            Operation::ReadAudit => "read_audit",
        }
    }

    /// 获取操作所需的最小 Scope
    ///
    /// # 返回值
    /// 返回执行此操作所需的最小 Scope
    pub fn required_scope(&self) -> Scope {
        match self {
            Operation::ReadCredential => Scope::CredentialRead,
            Operation::DecryptCredential => Scope::CredentialDecrypt,
            Operation::CreateCredential => Scope::CredentialWrite,
            Operation::UpdateCredential => Scope::CredentialWrite,
            Operation::DeleteCredential => Scope::CredentialDelete,
            Operation::ManageToken => Scope::TokenManage,
            Operation::RevokeToken => Scope::TokenManage,
            Operation::RefreshToken => Scope::TokenManage,
            Operation::ReadAudit => Scope::AuditRead,
        }
    }

    /// 检查此操作是否为凭证操作
    pub fn is_credential_operation(&self) -> bool {
        matches!(
            self,
            Operation::ReadCredential
                | Operation::DecryptCredential
                | Operation::CreateCredential
                | Operation::UpdateCredential
                | Operation::DeleteCredential
        )
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Scope 权限错误
#[derive(Error, Debug, Clone, PartialEq)]
pub enum ScopeError {
    #[error("权限不足: 需要 {required}, 实际拥有 {actual}")]
    InsufficientScope {
        required: String,
        actual: String,
    },

    #[error("无效的 Scope: {0}")]
    InvalidScope(String),

    #[error("访问被拒绝: Token 无权访问凭证 {credential_id}")]
    AccessDenied { credential_id: String },

    #[error("Token 已限制为特定凭证，无法访问其他凭证")]
    RestrictedToken,

    #[error("Scope 解析失败: {0}")]
    ParseError(String),
}

/// Scope 集合
///
/// 管理多个 Scope，提供批量权限检查
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeSet {
    scopes: Vec<Scope>,
}

impl ScopeSet {
    /// 创建空的 Scope 集合
    pub fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    /// 从 Scope 列表创建
    pub fn from_scopes(scopes: Vec<Scope>) -> Self {
        Self { scopes }
    }

    /// 从字符串解析 Scope 集合
    ///
    /// 字符串格式: 空格分隔的 scope 列表，如 "credential:read credential:write"
    pub fn from_string(s: &str) -> Result<Self, ScopeError> {
        let mut scopes = Vec::new();

        for scope_str in s.split_whitespace() {
            match Scope::from_str(scope_str) {
                Some(scope) => scopes.push(scope),
                None => {
                    return Err(ScopeError::ParseError(format!(
                        "未知的 Scope: {}",
                        scope_str
                    )))
                }
            }
        }

        Ok(Self { scopes })
    }

    /// 添加 Scope
    pub fn add(&mut self, scope: Scope) {
        if !self.scopes.contains(&scope) {
            self.scopes.push(scope);
        }
    }

    /// 检查是否包含指定 Scope
    pub fn contains(&self, scope: &Scope) -> bool {
        self.scopes.contains(scope) || self.scopes.contains(&Scope::Admin)
    }

    /// 检查是否包含任意指定 Scope
    pub fn contains_any(&self, scopes: &[Scope]) -> bool {
        scopes.iter().any(|s| self.contains(s))
    }

    /// 检查是否包含所有指定 Scope
    pub fn contains_all(&self, scopes: &[Scope]) -> bool {
        scopes.iter().all(|s| self.contains(s))
    }

    /// 检查是否允许执行指定操作
    pub fn can_access(&self, operation: &Operation) -> bool {
        // 如果有 Admin，允许所有操作
        if self.scopes.contains(&Scope::Admin) {
            return true;
        }

        // 检查是否有任何 Scope 允许该操作
        self.scopes.iter().any(|scope| scope.can_access(operation))
    }

    /// 获取所有 Scope
    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }

    /// 转换为字符串（空格分隔）
    pub fn to_string(&self) -> String {
        self.scopes
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    /// 获取 Scope 数量
    pub fn len(&self) -> usize {
        self.scopes.len()
    }
}

impl From<Vec<Scope>> for ScopeSet {
    fn from(scopes: Vec<Scope>) -> Self {
        Self::from_scopes(scopes)
    }
}

/// 受限 Token 上下文
///
/// 用于表示 Token 的资源级限制（credential_ids 白名单）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestrictedTokenContext {
    /// 允许的凭证 ID 列表
    /// None 表示无限制（可以访问所有凭证）
    /// Some(vec) 表示只能访问列表中的凭证
    allowed_credential_ids: Option<Vec<String>>,
}

impl RestrictedTokenContext {
    /// 创建无限制的 Token 上下文
    pub fn unrestricted() -> Self {
        Self {
            allowed_credential_ids: None,
        }
    }

    /// 创建受限的 Token 上下文
    ///
    /// # 参数
    /// - `credential_ids`: 允许的凭证 ID 列表
    pub fn restricted(credential_ids: Vec<String>) -> Self {
        Self {
            allowed_credential_ids: Some(credential_ids),
        }
    }

    /// 检查 Token 是否可以访问指定凭证
    ///
    /// # 参数
    /// - `credential_id`: 要访问的凭证 ID
    ///
    /// # 返回值
    /// - `Ok(())`: 允许访问
    /// - `Err(ScopeError::AccessDenied)`: 拒绝访问
    pub fn can_access_credential(&self, credential_id: &str) -> Result<(), ScopeError> {
        match &self.allowed_credential_ids {
            None => Ok(()), // 无限制
            Some(allowed_ids) => {
                if allowed_ids.contains(&credential_id.to_string()) {
                    Ok(())
                } else {
                    Err(ScopeError::AccessDenied {
                        credential_id: credential_id.to_string(),
                    })
                }
            }
        }
    }

    /// 检查是否为受限 Token
    pub fn is_restricted(&self) -> bool {
        self.allowed_credential_ids.is_some()
    }

    /// 获取允许的凭证 ID 列表
    pub fn allowed_credentials(&self) -> Option<&[String]> {
        self.allowed_credential_ids.as_deref()
    }

    /// 添加允许的凭证 ID
    pub fn add_credential(&mut self, credential_id: impl Into<String>) {
        match &mut self.allowed_credential_ids {
            Some(ids) => {
                let id = credential_id.into();
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            None => {
                // 从无限制变为受限
                self.allowed_credential_ids = Some(vec![credential_id.into()]);
            }
        }
    }
}

/// 权限检查器
///
/// 综合检查 Scope 和资源级权限
pub struct PermissionChecker {
    scope_set: ScopeSet,
    restricted_context: RestrictedTokenContext,
}

impl PermissionChecker {
    /// 创建新的权限检查器
    pub fn new(scope_set: ScopeSet) -> Self {
        Self {
            scope_set,
            restricted_context: RestrictedTokenContext::unrestricted(),
        }
    }

    /// 创建带资源限制的权限检查器
    pub fn with_restriction(
        scope_set: ScopeSet,
        restricted_context: RestrictedTokenContext,
    ) -> Self {
        Self {
            scope_set,
            restricted_context,
        }
    }

    /// 检查是否可以执行操作
    pub fn can_execute(&self, operation: &Operation) -> bool {
        self.scope_set.can_access(operation)
    }

    /// 检查是否可以访问凭证
    pub fn can_access_credential(&self, credential_id: &str) -> Result<(), ScopeError> {
        self.restricted_context.can_access_credential(credential_id)
    }

    /// 检查操作和凭证访问权限
    ///
    /// # 参数
    /// - `operation`: 要执行的操作
    /// - `credential_id`: 要访问的凭证 ID（可选）
    ///
    /// # 返回值
    /// - `Ok(())`: 允许执行
    /// - `Err(ScopeError)`: 拒绝执行
    pub fn check_permission(
        &self,
        operation: &Operation,
        credential_id: Option<&str>,
    ) -> Result<(), ScopeError> {
        // 1. 检查 Scope 权限
        if !self.can_execute(operation) {
            return Err(ScopeError::InsufficientScope {
                required: operation.required_scope().to_string(),
                actual: self.scope_set.to_string(),
            });
        }

        // 2. 检查资源级权限（如果是凭证操作且有 credential_id）
        if operation.is_credential_operation() {
            if let Some(id) = credential_id {
                self.can_access_credential(id)?;
            }
        }

        Ok(())
    }

    /// 获取 Scope 集合
    pub fn scopes(&self) -> &ScopeSet {
        &self.scope_set
    }

    /// 获取限制上下文
    pub fn restricted_context(&self) -> &RestrictedTokenContext {
        &self.restricted_context
    }
}

/// Scope 常量（向后兼容）
pub mod constants {
    use super::Scope;

    /// 读取凭证元数据
    pub const CREDENTIAL_READ: Scope = Scope::CredentialRead;

    /// 解密凭证内容
    pub const CREDENTIAL_DECRYPT: Scope = Scope::CredentialDecrypt;

    /// 创建/更新凭证
    pub const CREDENTIAL_WRITE: Scope = Scope::CredentialWrite;

    /// 删除凭证
    pub const CREDENTIAL_DELETE: Scope = Scope::CredentialDelete;

    /// 管理 Token
    pub const TOKEN_MANAGE: Scope = Scope::TokenManage;

    /// 读取审计日志
    pub const AUDIT_READ: Scope = Scope::AuditRead;

    /// 所有管理权限
    pub const ADMIN: Scope = Scope::Admin;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_as_str() {
        assert_eq!(Scope::CredentialRead.as_str(), "credential:read");
        assert_eq!(Scope::CredentialDecrypt.as_str(), "credential:decrypt");
        assert_eq!(Scope::CredentialWrite.as_str(), "credential:write");
        assert_eq!(Scope::CredentialDelete.as_str(), "credential:delete");
        assert_eq!(Scope::TokenManage.as_str(), "token:manage");
        assert_eq!(Scope::AuditRead.as_str(), "audit:read");
        assert_eq!(Scope::Admin.as_str(), "admin");
    }

    #[test]
    fn test_scope_from_str() {
        assert_eq!(Scope::from_str("credential:read"), Some(Scope::CredentialRead));
        assert_eq!(Scope::from_str("credential:decrypt"), Some(Scope::CredentialDecrypt));
        assert_eq!(Scope::from_str("credential:write"), Some(Scope::CredentialWrite));
        assert_eq!(Scope::from_str("credential:delete"), Some(Scope::CredentialDelete));
        assert_eq!(Scope::from_str("token:manage"), Some(Scope::TokenManage));
        assert_eq!(Scope::from_str("audit:read"), Some(Scope::AuditRead));
        assert_eq!(Scope::from_str("admin"), Some(Scope::Admin));
        assert_eq!(Scope::from_str("invalid:scope"), None);
    }

    #[test]
    fn test_scope_can_access_read() {
        let read_scope = Scope::CredentialRead;

        // credential:read 可以访问 ReadCredential
        assert!(read_scope.can_access(&Operation::ReadCredential));

        // credential:read 不能访问其他操作
        assert!(!read_scope.can_access(&Operation::DecryptCredential));
        assert!(!read_scope.can_access(&Operation::CreateCredential));
        assert!(!read_scope.can_access(&Operation::DeleteCredential));
        assert!(!read_scope.can_access(&Operation::ManageToken));
    }

    #[test]
    fn test_scope_can_access_decrypt() {
        let decrypt_scope = Scope::CredentialDecrypt;

        // credential:decrypt 可以访问 DecryptCredential 和 ReadCredential
        assert!(decrypt_scope.can_access(&Operation::DecryptCredential));
        assert!(decrypt_scope.can_access(&Operation::ReadCredential));

        // 不能访问其他操作
        assert!(!decrypt_scope.can_access(&Operation::CreateCredential));
        assert!(!decrypt_scope.can_access(&Operation::DeleteCredential));
    }

    #[test]
    fn test_scope_can_access_write() {
        let write_scope = Scope::CredentialWrite;

        // credential:write 可以访问 CreateCredential 和 UpdateCredential
        assert!(write_scope.can_access(&Operation::CreateCredential));
        assert!(write_scope.can_access(&Operation::UpdateCredential));

        // 不能访问其他操作
        assert!(!write_scope.can_access(&Operation::ReadCredential));
        assert!(!write_scope.can_access(&Operation::DeleteCredential));
    }

    #[test]
    fn test_scope_can_access_delete() {
        let delete_scope = Scope::CredentialDelete;

        // credential:delete 可以访问 DeleteCredential
        assert!(delete_scope.can_access(&Operation::DeleteCredential));

        // 不能访问其他操作
        assert!(!delete_scope.can_access(&Operation::ReadCredential));
        assert!(!delete_scope.can_access(&Operation::CreateCredential));
    }

    #[test]
    fn test_scope_can_access_admin() {
        let admin_scope = Scope::Admin;

        // admin 拥有所有权限
        assert!(admin_scope.can_access(&Operation::ReadCredential));
        assert!(admin_scope.can_access(&Operation::DecryptCredential));
        assert!(admin_scope.can_access(&Operation::CreateCredential));
        assert!(admin_scope.can_access(&Operation::UpdateCredential));
        assert!(admin_scope.can_access(&Operation::DeleteCredential));
        assert!(admin_scope.can_access(&Operation::ManageToken));
        assert!(admin_scope.can_access(&Operation::RevokeToken));
        assert!(admin_scope.can_access(&Operation::RefreshToken));
        assert!(admin_scope.can_access(&Operation::ReadAudit));
    }

    #[test]
    fn test_scope_display() {
        assert_eq!(Scope::CredentialRead.to_string(), "credential:read");
        assert_eq!(Scope::Admin.to_string(), "admin");
    }

    #[test]
    fn test_scope_is_credential_scope() {
        assert!(Scope::CredentialRead.is_credential_scope());
        assert!(Scope::CredentialDecrypt.is_credential_scope());
        assert!(Scope::CredentialWrite.is_credential_scope());
        assert!(Scope::CredentialDelete.is_credential_scope());
        assert!(Scope::Admin.is_credential_scope());

        assert!(!Scope::TokenManage.is_credential_scope());
        assert!(!Scope::AuditRead.is_credential_scope());
    }

    #[test]
    fn test_scope_is_admin() {
        assert!(Scope::Admin.is_admin());
        assert!(!Scope::CredentialRead.is_admin());
        assert!(!Scope::TokenManage.is_admin());
    }

    #[test]
    fn test_scope_set_from_string() {
        let scope_set = ScopeSet::from_string("credential:read credential:write").unwrap();
        assert!(scope_set.contains(&Scope::CredentialRead));
        assert!(scope_set.contains(&Scope::CredentialWrite));
        assert!(!scope_set.contains(&Scope::CredentialDelete));
    }

    #[test]
    fn test_scope_set_from_string_invalid() {
        let result = ScopeSet::from_string("invalid:scope");
        assert!(matches!(result, Err(ScopeError::ParseError(_))));
    }

    #[test]
    fn test_scope_set_can_access() {
        let scope_set = ScopeSet::from_string("credential:read credential:write").unwrap();

        assert!(scope_set.can_access(&Operation::ReadCredential));
        assert!(scope_set.can_access(&Operation::CreateCredential));
        assert!(!scope_set.can_access(&Operation::DeleteCredential));
    }

    #[test]
    fn test_restricted_token_unrestricted() {
        let context = RestrictedTokenContext::unrestricted();

        assert!(!context.is_restricted());
        assert!(context.can_access_credential("any_id").is_ok());
    }

    #[test]
    fn test_restricted_token_restricted() {
        let context = RestrictedTokenContext::restricted(vec![
            "cred_123".to_string(),
            "cred_456".to_string(),
        ]);

        assert!(context.is_restricted());
        assert!(context.can_access_credential("cred_123").is_ok());
        assert!(context.can_access_credential("cred_456").is_ok());

        let result = context.can_access_credential("cred_999");
        assert!(matches!(result, Err(ScopeError::AccessDenied { .. })));
    }

    #[test]
    fn test_permission_checker() {
        let scope_set = ScopeSet::from_string("credential:read").unwrap();
        let checker = PermissionChecker::new(scope_set);

        assert!(checker.can_execute(&Operation::ReadCredential));
        assert!(!checker.can_execute(&Operation::CreateCredential));

        // 检查凭证访问
        assert!(checker.can_access_credential("any_id").is_ok()); // 无限制
    }

    #[test]
    fn test_permission_checker_restricted() {
        let scope_set = ScopeSet::from_string("credential:read").unwrap();
        let restricted = RestrictedTokenContext::restricted(vec!["cred_123".to_string()]);
        let checker = PermissionChecker::with_restriction(scope_set, restricted);

        // 可以执行读操作
        assert!(checker.can_execute(&Operation::ReadCredential));

        // 可以访问允许的凭证
        assert!(checker.check_permission(&Operation::ReadCredential, Some("cred_123")).is_ok());

        // 不能访问其他凭证
        let result = checker.check_permission(&Operation::ReadCredential, Some("cred_999"));
        assert!(matches!(result, Err(ScopeError::AccessDenied { .. })));
    }

    #[test]
    fn test_permission_checker_insufficient_scope() {
        let scope_set = ScopeSet::from_string("credential:read").unwrap();
        let checker = PermissionChecker::new(scope_set);

        let result = checker.check_permission(&Operation::CreateCredential, None);
        assert!(matches!(result, Err(ScopeError::InsufficientScope { .. })));
    }

    #[test]
    fn test_operation_required_scope() {
        assert_eq!(
            Operation::ReadCredential.required_scope(),
            Scope::CredentialRead
        );
        assert_eq!(
            Operation::DecryptCredential.required_scope(),
            Scope::CredentialDecrypt
        );
        assert_eq!(
            Operation::CreateCredential.required_scope(),
            Scope::CredentialWrite
        );
        assert_eq!(
            Operation::DeleteCredential.required_scope(),
            Scope::CredentialDelete
        );
        assert_eq!(Operation::ManageToken.required_scope(), Scope::TokenManage);
        assert_eq!(Operation::ReadAudit.required_scope(), Scope::AuditRead);
    }

    #[test]
    fn test_constants() {
        use constants::*;

        assert_eq!(CREDENTIAL_READ, Scope::CredentialRead);
        assert_eq!(CREDENTIAL_DECRYPT, Scope::CredentialDecrypt);
        assert_eq!(CREDENTIAL_WRITE, Scope::CredentialWrite);
        assert_eq!(CREDENTIAL_DELETE, Scope::CredentialDelete);
        assert_eq!(TOKEN_MANAGE, Scope::TokenManage);
        assert_eq!(AUDIT_READ, Scope::AuditRead);
        assert_eq!(ADMIN, Scope::Admin);
    }
}
