//! 权限检查逻辑
//!
//! 实现细粒度的权限检查和资源级访问控制
//! 支持受限 Token（credential_ids 白名单）
//!
//! # 核心功能
//!
//! - **PermissionEngine**: 权限引擎，综合检查 Scope 和资源权限
//! - **ResourceAccess**: 资源访问控制 trait
//! - **CredentialAccess**: 凭证访问控制实现
//! - **AccessDecision**: 访问决策枚举
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use vault_service::token::permission::{
//!     PermissionEngine, CredentialAccess, AccessDecision
//! };
//!
//! // 创建权限引擎
//! let engine = PermissionEngine::new("credential:read", vec!["cred_123"])
//!     .expect("invalid scope");
//!
//! // 检查凭证访问
//! match engine.check_credential_access("cred_123") {
//!     AccessDecision::Allow => println!("允许访问"),
//!     AccessDecision::Deny(reason) => println!("拒绝访问: {}", reason),
//! }
//! ```

use super::scope::{
    Operation, RestrictedTokenContext, Scope, ScopeError, ScopeSet,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

/// 权限错误类型
#[derive(Error, Debug, Clone, PartialEq)]
pub enum PermissionError {
    #[error("权限检查失败: {0}")]
    CheckFailed(String),

    #[error("Scope 错误: {0}")]
    ScopeError(#[from] ScopeError),

    #[error("无效的 Scope 字符串: {0}")]
    InvalidScopeString(String),

    #[error("资源访问被拒绝: {resource_type} {resource_id}")]
    ResourceDenied {
        resource_type: String,
        resource_id: String,
    },

    #[error("凭证访问被拒绝: {0}")]
    CredentialDenied(String),

    #[error("操作未授权: 需要 {required}, 实际 {actual}")]
    Unauthorized {
        required: String,
        actual: String,
    },
}

/// 访问决策
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessDecision {
    /// 允许访问
    Allow,
    /// 拒绝访问（带原因）
    Deny(String),
    /// 需要额外审批
    RequireApproval(String),
}

impl AccessDecision {
    /// 检查是否允许
    pub fn is_allowed(&self) -> bool {
        matches!(self, AccessDecision::Allow)
    }

    /// 检查是否拒绝
    pub fn is_denied(&self) -> bool {
        matches!(self, AccessDecision::Deny(_))
    }

    /// 检查是否需要审批
    pub fn requires_approval(&self) -> bool {
        matches!(self, AccessDecision::RequireApproval(_))
    }

    /// 获取拒绝原因（如果是拒绝）
    pub fn denial_reason(&self) -> Option<&str> {
        match self {
            AccessDecision::Deny(reason) => Some(reason),
            _ => None,
        }
    }
}

/// 资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    /// 凭证
    Credential,
    /// Token
    Token,
    /// 审计日志
    AuditLog,
    /// 用户
    User,
    /// 租户
    Tenant,
    /// 配置
    Config,
}

impl ResourceType {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceType::Credential => "credential",
            ResourceType::Token => "token",
            ResourceType::AuditLog => "audit_log",
            ResourceType::User => "user",
            ResourceType::Tenant => "tenant",
            ResourceType::Config => "config",
        }
    }
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

use std::fmt;

/// 资源访问请求
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRequest {
    /// 资源类型
    pub resource_type: ResourceType,
    /// 资源 ID
    pub resource_id: String,
    /// 操作类型
    pub operation: Operation,
    /// 额外上下文
    pub context: AccessContext,
}

impl AccessRequest {
    /// 创建凭证访问请求
    pub fn credential(operation: Operation, credential_id: impl Into<String>) -> Self {
        Self {
            resource_type: ResourceType::Credential,
            resource_id: credential_id.into(),
            operation,
            context: AccessContext::default(),
        }
    }

    /// 创建 Token 管理请求
    pub fn token(operation: Operation, token_id: impl Into<String>) -> Self {
        Self {
            resource_type: ResourceType::Token,
            resource_id: token_id.into(),
            operation,
            context: AccessContext::default(),
        }
    }

    /// 添加上下文
    pub fn with_context(mut self, context: AccessContext) -> Self {
        self.context = context;
        self
    }
}

/// 访问上下文
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccessContext {
    /// 客户端 IP
    pub client_ip: Option<String>,
    /// 请求时间戳
    pub request_time: Option<u64>,
    /// 风险等级 (0=低, 1=中, 2=高)
    pub risk_level: Option<u8>,
    /// 额外元数据
    pub metadata: std::collections::HashMap<String, String>,
}

impl AccessContext {
    /// 创建新的访问上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置客户端 IP
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }

    /// 设置风险等级
    pub fn with_risk_level(mut self, level: u8) -> Self {
        self.risk_level = Some(level.min(2));
        self
    }

    /// 添加元数据
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// 凭证访问控制 trait
pub trait CredentialAccess {
    /// 检查是否可以访问凭证
    fn can_access_credential(&self, credential_id: &str) -> Result<(), PermissionError>;

    /// 检查是否可以执行凭证操作
    fn can_execute_on_credential(
        &self,
        operation: &Operation,
        credential_id: &str,
    ) -> Result<(), PermissionError>;

    /// 获取可访问的凭证 ID 列表
    fn accessible_credentials(&self) -> Option<&[String]>;
}

/// 权限引擎
///
/// 综合处理 Scope 权限和资源级限制
#[derive(Debug, Clone)]
pub struct PermissionEngine {
    scope_set: ScopeSet,
    restricted_context: RestrictedTokenContext,
    audit_mode: bool,
}

impl PermissionEngine {
    /// 创建新的权限引擎
    ///
    /// # 参数
    /// - `scope_string`: 空格分隔的 Scope 列表
    ///
    /// # 返回值
    /// - `Ok(PermissionEngine)`: 创建成功
    /// - `Err(PermissionError)`: Scope 字符串无效
    pub fn new(scope_string: &str) -> Result<Self, PermissionError> {
        let scope_set =
            ScopeSet::from_string(scope_string).map_err(PermissionError::ScopeError)?;

        Ok(Self {
            scope_set,
            restricted_context: RestrictedTokenContext::unrestricted(),
            audit_mode: false,
        })
    }

    /// 创建带资源限制的权限引擎
    ///
    /// # 参数
    /// - `scope_string`: 空格分隔的 Scope 列表
    /// - `credential_ids`: 允许的凭证 ID 列表
    pub fn with_restricted_credentials(
        scope_string: &str,
        credential_ids: Vec<String>,
    ) -> Result<Self, PermissionError> {
        let mut engine = Self::new(scope_string)?;
        engine.restricted_context = RestrictedTokenContext::restricted(credential_ids);
        Ok(engine)
    }

    /// 从 Scope 集合创建
    pub fn from_scope_set(scope_set: ScopeSet) -> Self {
        Self {
            scope_set,
            restricted_context: RestrictedTokenContext::unrestricted(),
            audit_mode: false,
        }
    }

    /// 启用审计模式（记录所有权限检查）
    pub fn with_audit_mode(mut self, enabled: bool) -> Self {
        self.audit_mode = enabled;
        self
    }

    /// 检查是否允许执行操作
    pub fn can_execute(&self, operation: &Operation) -> bool {
        self.scope_set.can_access(operation)
    }

    /// 检查资源访问
    ///
    /// # 参数
    /// - `request`: 访问请求
    ///
    /// # 返回值
    /// `AccessDecision` - 访问决策
    pub fn check_access(&self, request: &AccessRequest) -> AccessDecision {
        // 1. 检查 Scope 权限
        if !self.can_execute(&request.operation) {
            return AccessDecision::Deny(format!(
                "缺少 {} 权限",
                request.operation.required_scope()
            ));
        }

        // 2. 检查资源级限制
        match request.resource_type {
            ResourceType::Credential => {
                if let Err(e) = self
                    .restricted_context
                    .can_access_credential(&request.resource_id)
                {
                    return AccessDecision::Deny(e.to_string());
                }
            }
            _ => {
                // 其他资源类型的检查可以在这里扩展
            }
        }

        // 3. 风险等级检查
        if let Some(risk_level) = request.context.risk_level {
            if risk_level >= 2 {
                return AccessDecision::RequireApproval(
                    "高风险操作需要人工审批".to_string(),
                );
            }
        }

        AccessDecision::Allow
    }

    /// 快速检查凭证访问
    pub fn check_credential_access(&self, credential_id: &str) -> AccessDecision {
        let request = AccessRequest::credential(Operation::ReadCredential, credential_id);
        self.check_access(&request)
    }

    /// 检查凭证操作
    pub fn check_credential_operation(
        &self,
        operation: Operation,
        credential_id: &str,
    ) -> AccessDecision {
        let request = AccessRequest::credential(operation, credential_id);
        self.check_access(&request)
    }

    /// 获取 Scope 集合
    pub fn scopes(&self) -> &ScopeSet {
        &self.scope_set
    }

    /// 是否为受限 Token
    pub fn is_restricted(&self) -> bool {
        self.restricted_context.is_restricted()
    }

    /// 获取可访问的凭证列表
    pub fn allowed_credentials(&self) -> Option<&[String]> {
        self.restricted_context.allowed_credentials()
    }

    /// 验证是否拥有所有指定 Scope
    pub fn has_all_scopes(&self, scopes: &[Scope]) -> bool {
        self.scope_set.contains_all(scopes)
    }

    /// 验证是否拥有任意指定 Scope
    pub fn has_any_scope(&self, scopes: &[Scope]) -> bool {
        self.scope_set.contains_any(scopes)
    }

    /// 是否为管理员
    pub fn is_admin(&self) -> bool {
        self.scope_set.contains(&Scope::Admin)
    }
}

impl CredentialAccess for PermissionEngine {
    fn can_access_credential(&self, credential_id: &str) -> Result<(), PermissionError> {
        self.restricted_context
            .can_access_credential(credential_id)
            .map_err(|e| e.into())
    }

    fn can_execute_on_credential(
        &self,
        operation: &Operation,
        credential_id: &str,
    ) -> Result<(), PermissionError> {
        // 检查操作权限
        if !self.can_execute(operation) {
            return Err(PermissionError::Unauthorized {
                required: operation.required_scope().to_string(),
                actual: self.scope_set.to_string(),
            });
        }

        // 检查资源限制
        self.can_access_credential(credential_id)
    }

    fn accessible_credentials(&self) -> Option<&[String]> {
        self.restricted_context.allowed_credentials()
    }
}

/// 批量权限检查器
pub struct BatchPermissionChecker {
    engine: PermissionEngine,
    results: Vec<(AccessRequest, AccessDecision)>,
}

impl BatchPermissionChecker {
    /// 创建批量检查器
    pub fn new(engine: PermissionEngine) -> Self {
        Self {
            engine,
            results: Vec::new(),
        }
    }

    /// 添加检查请求
    pub fn add_check(&mut self, request: AccessRequest) {
        let decision = self.engine.check_access(&request);
        self.results.push((request, decision));
    }

    /// 检查所有凭证
    pub fn check_all_credentials(
        &mut self,
        operation: Operation,
        credential_ids: &[String],
    ) -> Vec<(String, AccessDecision)> {
        let mut results = Vec::new();

        for id in credential_ids {
            let request = AccessRequest::credential(operation.clone(), id.clone());
            let decision = self.engine.check_access(&request);
            results.push((id.clone(), decision));
            self.results.push((request, results.last().unwrap().1.clone()));
        }

        results
    }

    /// 获取所有结果
    pub fn results(&self) -> &[(AccessRequest, AccessDecision)] {
        &self.results
    }

    /// 获取允许的数量
    pub fn allowed_count(&self) -> usize {
        self.results.iter().filter(|(_, d)| d.is_allowed()).count()
    }

    /// 获取拒绝的数量
    pub fn denied_count(&self) -> usize {
        self.results.iter().filter(|(_, d)| d.is_denied()).count()
    }

    /// 是否全部允许
    pub fn all_allowed(&self) -> bool {
        self.results.iter().all(|(_, d)| d.is_allowed())
    }

    /// 获取被拒绝的请求
    pub fn denied_requests(&self) -> Vec<&AccessRequest> {
        self.results
            .iter()
            .filter(|(_, d)| d.is_denied())
            .map(|(r, _)| r)
            .collect()
    }
}

/// 权限策略
///
/// 定义权限检查的策略配置
#[derive(Debug, Clone)]
pub struct PermissionPolicy {
    /// 是否启用资源级限制
    pub enable_resource_restriction: bool,
    /// 是否启用风险检查
    pub enable_risk_check: bool,
    /// 高风险操作阈值
    pub high_risk_threshold: u8,
    /// 是否记录审计日志
    pub audit_logging: bool,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            enable_resource_restriction: true,
            enable_risk_check: true,
            high_risk_threshold: 2,
            audit_logging: true,
        }
    }
}

impl PermissionPolicy {
    /// 创建宽松策略
    pub fn permissive() -> Self {
        Self {
            enable_resource_restriction: false,
            enable_risk_check: false,
            high_risk_threshold: 3,
            audit_logging: false,
        }
    }

    /// 创建严格策略
    pub fn strict() -> Self {
        Self {
            enable_resource_restriction: true,
            enable_risk_check: true,
            high_risk_threshold: 1,
            audit_logging: true,
        }
    }
}

/// 扩展的受限 Token 上下文
///
/// 支持更多限制类型的 Token 上下文
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtendedRestrictedContext {
    /// 允许的凭证 ID
    allowed_credentials: Option<HashSet<String>>,
    /// 允许的操作
    allowed_operations: Option<HashSet<Operation>>,
    /// 允许的 IP 范围
    allowed_ips: Option<Vec<String>>,
    /// 最大请求次数
    max_requests: Option<u32>,
    /// 当前请求次数
    current_requests: u32,
}

impl ExtendedRestrictedContext {
    /// 创建新的扩展受限上下文
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置允许的凭证
    pub fn with_credentials(mut self, credentials: Vec<String>) -> Self {
        self.allowed_credentials = Some(credentials.into_iter().collect());
        self
    }

    /// 设置允许的操作
    pub fn with_operations(mut self, operations: Vec<Operation>) -> Self {
        self.allowed_operations = Some(operations.into_iter().collect());
        self
    }

    /// 设置最大请求次数限制
    pub fn with_max_requests(mut self, max: u32) -> Self {
        self.max_requests = Some(max);
        self
    }

    /// 检查是否可以访问凭证
    pub fn can_access_credential(&self, credential_id: &str) -> Result<(), PermissionError> {
        match &self.allowed_credentials {
            None => Ok(()),
            Some(allowed) => {
                if allowed.contains(credential_id) {
                    Ok(())
                } else {
                    Err(PermissionError::CredentialDenied(credential_id.to_string()))
                }
            }
        }
    }

    /// 检查是否可以执行操作
    pub fn can_execute(&self, operation: &Operation) -> Result<(), PermissionError> {
        match &self.allowed_operations {
            None => Ok(()),
            Some(allowed) => {
                if allowed.contains(operation) {
                    Ok(())
                } else {
                    Err(PermissionError::Unauthorized {
                        required: operation.to_string(),
                        actual: "restricted".to_string(),
                    })
                }
            }
        }
    }

    /// 检查请求限制
    pub fn check_rate_limit(&mut self) -> Result<(), PermissionError> {
        if let Some(max) = self.max_requests {
            if self.current_requests >= max {
                return Err(PermissionError::CheckFailed(
                    "Token 请求次数已耗尽".to_string(),
                ));
            }
        }
        self.current_requests += 1;
        Ok(())
    }

    /// 获取剩余请求次数
    pub fn remaining_requests(&self) -> Option<u32> {
        self.max_requests.map(|max| {
            if self.current_requests >= max {
                0
            } else {
                max - self.current_requests
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_access_decision() {
        let allow = AccessDecision::Allow;
        assert!(allow.is_allowed());
        assert!(!allow.is_denied());

        let deny = AccessDecision::Deny("test reason".to_string());
        assert!(!deny.is_allowed());
        assert!(deny.is_denied());
        assert_eq!(deny.denial_reason(), Some("test reason"));

        let approval = AccessDecision::RequireApproval("需要审批".to_string());
        assert!(approval.requires_approval());
    }

    #[test]
    fn test_permission_engine_new() {
        let engine = PermissionEngine::new("credential:read");
        assert!(engine.is_ok());

        let engine = PermissionEngine::new("invalid:scope");
        assert!(engine.is_err());
    }

    #[test]
    fn test_permission_engine_can_execute() {
        let engine = PermissionEngine::new("credential:read credential:write").unwrap();

        assert!(engine.can_execute(&Operation::ReadCredential));
        assert!(engine.can_execute(&Operation::CreateCredential));
        assert!(!engine.can_execute(&Operation::DeleteCredential));
        assert!(!engine.can_execute(&Operation::ManageToken));
    }

    #[test]
    fn test_permission_engine_check_credential_access() {
        let engine = PermissionEngine::with_restricted_credentials(
            "credential:read",
            vec!["cred_123".to_string()],
        )
        .unwrap();

        // 允许访问白名单中的凭证
        let decision = engine.check_credential_access("cred_123");
        assert!(decision.is_allowed());

        // 拒绝访问白名单外的凭证
        let decision = engine.check_credential_access("cred_999");
        assert!(decision.is_denied());
    }

    #[test]
    fn test_permission_engine_check_access_with_risk() {
        let engine = PermissionEngine::new("credential:read").unwrap();

        let request = AccessRequest::credential(Operation::ReadCredential, "cred_123")
            .with_context(AccessContext::new().with_risk_level(2));

        let decision = engine.check_access(&request);
        assert!(decision.requires_approval());
    }

    #[test]
    fn test_permission_engine_is_admin() {
        let admin = PermissionEngine::new("admin").unwrap();
        assert!(admin.is_admin());

        let user = PermissionEngine::new("credential:read").unwrap();
        assert!(!user.is_admin());
    }

    #[test]
    fn test_batch_permission_checker() {
        let engine = PermissionEngine::with_restricted_credentials(
            "credential:read",
            vec!["cred_1".to_string(), "cred_2".to_string()],
        )
        .unwrap();

        let mut checker = BatchPermissionChecker::new(engine);

        let results = checker.check_all_credentials(
            Operation::ReadCredential,
            &[
                "cred_1".to_string(),
                "cred_2".to_string(),
                "cred_3".to_string(), // 不在白名单中
            ],
        );

        assert_eq!(results.len(), 3);
        assert!(results[0].1.is_allowed()); // cred_1
        assert!(results[1].1.is_allowed()); // cred_2
        assert!(results[2].1.is_denied()); // cred_3

        assert_eq!(checker.allowed_count(), 2);
        assert_eq!(checker.denied_count(), 1);
        assert!(!checker.all_allowed());
    }

    #[test]
    fn test_extended_restricted_context() {
        let context = ExtendedRestrictedContext::new()
            .with_credentials(vec!["cred_123".to_string()])
            .with_operations(vec![Operation::ReadCredential]);

        // 可以访问允许的凭证
        assert!(context.can_access_credential("cred_123").is_ok());

        // 不能访问其他凭证
        assert!(context.can_access_credential("cred_999").is_err());

        // 可以执行允许的操作
        assert!(context.can_execute(&Operation::ReadCredential).is_ok());

        // 不能执行其他操作
        assert!(context.can_execute(&Operation::CreateCredential).is_err());
    }

    #[test]
    fn test_permission_policy() {
        let default = PermissionPolicy::default();
        assert!(default.enable_resource_restriction);
        assert!(default.enable_risk_check);
        assert_eq!(default.high_risk_threshold, 2);

        let permissive = PermissionPolicy::permissive();
        assert!(!permissive.enable_resource_restriction);

        let strict = PermissionPolicy::strict();
        assert_eq!(strict.high_risk_threshold, 1);
    }

    #[test]
    fn test_credential_access_trait() {
        let engine = PermissionEngine::with_restricted_credentials(
            "credential:read",
            vec!["cred_123".to_string()],
        )
        .unwrap();

        // 使用 trait 方法
        assert!(engine.can_access_credential("cred_123").is_ok());
        assert!(engine.can_access_credential("cred_999").is_err());

        assert!(engine
            .can_execute_on_credential(&Operation::ReadCredential, "cred_123")
            .is_ok());
        assert!(engine
            .can_execute_on_credential(&Operation::CreateCredential, "cred_123")
            .is_err()); // 没有写权限
    }
}
