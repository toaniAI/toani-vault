//! 认证领域模型
//!
//! 定义用户认证相关的核心数据结构：
//! - User: 用户实体
//! - ExternalIdentity: 外部身份提供商映射（Privy、Email等）
//! - TenantMembership: 用户-租户关系
//! - TenantInvitation: 租户邀请
//! - AuthSession: 服务端会话
//! - AuthAuditLog: 认证审计日志

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, Postgres};
use std::str::FromStr;
use uuid::Uuid;

// ============================================================================
// 用户状态枚举
// ============================================================================

/// 用户状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    /// 活跃状态
    Active,
    /// 未激活状态
    Inactive,
    /// 已暂停状态
    Suspended,
    /// 待删除状态
    PendingDeletion,
}

impl Default for UserStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl UserStatus {
    /// 检查状态是否允许认证
    pub fn allows_authentication(&self) -> bool {
        matches!(self, UserStatus::Active)
    }

    /// 检查状态是否允许操作
    pub fn allows_operations(&self) -> bool {
        matches!(self, UserStatus::Active | UserStatus::Inactive)
    }

    /// 获取状态字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            UserStatus::Active => "active",
            UserStatus::Inactive => "inactive",
            UserStatus::Suspended => "suspended",
            UserStatus::PendingDeletion => "pending_deletion",
        }
    }
}

impl std::fmt::Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// 身份提供商枚举
// ============================================================================

/// 外部身份提供商类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityProvider {
    /// Privy 身份提供商
    Privy,
    /// 邮箱认证
    Email,
    /// Google OAuth
    Google,
    /// Apple OAuth
    Apple,
    /// GitHub OAuth
    GitHub,
    /// Discord OAuth
    Discord,
    /// Twitter OAuth
    Twitter,
    /// 自定义提供商
    Custom,
}

impl IdentityProvider {
    /// 获取提供商字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            IdentityProvider::Privy => "privy",
            IdentityProvider::Email => "email",
            IdentityProvider::Google => "google",
            IdentityProvider::Apple => "apple",
            IdentityProvider::GitHub => "github",
            IdentityProvider::Discord => "discord",
            IdentityProvider::Twitter => "twitter",
            IdentityProvider::Custom => "custom",
        }
    }

    /// 是否为 OAuth 提供商
    pub fn is_oauth(&self) -> bool {
        matches!(
            self,
            IdentityProvider::Google
                | IdentityProvider::Apple
                | IdentityProvider::GitHub
                | IdentityProvider::Discord
                | IdentityProvider::Twitter
        )
    }

    /// 是否为钱包认证提供商
    pub fn is_wallet_provider(&self) -> bool {
        matches!(self, IdentityProvider::Privy)
    }
}

impl std::fmt::Display for IdentityProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for IdentityProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "privy" => Ok(IdentityProvider::Privy),
            "email" => Ok(IdentityProvider::Email),
            "google" => Ok(IdentityProvider::Google),
            "apple" => Ok(IdentityProvider::Apple),
            "github" => Ok(IdentityProvider::GitHub),
            "discord" => Ok(IdentityProvider::Discord),
            "twitter" => Ok(IdentityProvider::Twitter),
            "custom" => Ok(IdentityProvider::Custom),
            _ => Err(format!("Unknown identity provider: {s}")),
        }
    }
}

// ============================================================================
// 成员资格角色枚举
// ============================================================================

/// 租户成员角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    /// 租户所有者
    Owner,
    /// 租户管理员
    Admin,
    /// 普通成员
    Member,
    /// 只读成员
    Readonly,
}

impl Default for MembershipRole {
    fn default() -> Self {
        Self::Member
    }
}

impl MembershipRole {
    /// 获取角色权限级别（数值越大权限越高）
    pub fn permission_level(&self) -> u8 {
        match self {
            MembershipRole::Owner => 100,
            MembershipRole::Admin => 80,
            MembershipRole::Member => 50,
            MembershipRole::Readonly => 10,
        }
    }

    /// 检查是否为所有者
    pub fn is_owner(&self) -> bool {
        matches!(self, MembershipRole::Owner)
    }

    /// 检查是否有管理权限（Owner 或 Admin）
    pub fn can_manage(&self) -> bool {
        matches!(self, MembershipRole::Owner | MembershipRole::Admin)
    }

    /// 检查是否有写入权限
    pub fn can_write(&self) -> bool {
        matches!(
            self,
            MembershipRole::Owner | MembershipRole::Admin | MembershipRole::Member
        )
    }

    /// 获取角色字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            MembershipRole::Owner => "owner",
            MembershipRole::Admin => "admin",
            MembershipRole::Member => "member",
            MembershipRole::Readonly => "readonly",
        }
    }

    /// 获取默认 scopes
    ///
    /// # 映射规则（与 TokenScope::from_role 保持一致）
    /// | Role | Scopes |
    /// |------|--------|
    /// | owner | admin, tenant:*, credential:read/write/delete, sandbox:*, audit:read, members:*, invitations:*, tokens:*, users:manage, roles:manage |
    /// | admin | tenant:read/write/admin, credential:read/write/delete, sandbox:*, audit:read, members:*, invitations:*, tokens:*, users:manage |
    /// | member | tenant:read, credential:read/write, sandbox:*, audit:read, tokens:read/write |
    /// | readonly | tenant:read, credential:read, tokens:read |
    pub fn default_scopes(&self) -> Vec<String> {
        match self {
            MembershipRole::Owner => vec![
                "admin".to_string(),
                "tenant:read".to_string(),
                "tenant:write".to_string(),
                "tenant:admin".to_string(),
                "tenant:delete".to_string(),
                "credential:read".to_string(),
                "credential:write".to_string(),
                "credential:delete".to_string(),
                "sandbox:read".to_string(),
                "sandbox:write".to_string(),
                "sandbox:execute".to_string(),
                "audit:read".to_string(),
                "members:read".to_string(),
                "members:write".to_string(),
                "members:invite".to_string(),
                "invitations:read".to_string(),
                "invitations:write".to_string(),
                "tokens:read".to_string(),
                "tokens:write".to_string(),
                "tokens:revoke".to_string(),
                "users:manage".to_string(),
                "roles:manage".to_string(),
            ],
            MembershipRole::Admin => vec![
                "tenant:read".to_string(),
                "tenant:write".to_string(),
                "tenant:admin".to_string(),
                "credential:read".to_string(),
                "credential:write".to_string(),
                "credential:delete".to_string(),
                "sandbox:read".to_string(),
                "sandbox:write".to_string(),
                "sandbox:execute".to_string(),
                "audit:read".to_string(),
                "members:read".to_string(),
                "members:write".to_string(),
                "members:invite".to_string(),
                "invitations:read".to_string(),
                "invitations:write".to_string(),
                "tokens:read".to_string(),
                "tokens:write".to_string(),
                "tokens:revoke".to_string(),
                "users:manage".to_string(),
            ],
            MembershipRole::Member => vec![
                "tenant:read".to_string(),
                "credential:read".to_string(),
                "credential:write".to_string(),
                "sandbox:read".to_string(),
                "sandbox:write".to_string(),
                "sandbox:execute".to_string(),
                "audit:read".to_string(),
                "tokens:read".to_string(),
                "tokens:write".to_string(),
            ],
            MembershipRole::Readonly => vec![
                "tenant:read".to_string(),
                "credential:read".to_string(),
                "tokens:read".to_string(),
            ],
        }
    }
}

impl std::fmt::Display for MembershipRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for MembershipRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "owner" => Ok(MembershipRole::Owner),
            "admin" => Ok(MembershipRole::Admin),
            "member" => Ok(MembershipRole::Member),
            "readonly" | "read_only" | "read-only" => Ok(MembershipRole::Readonly),
            _ => Err(format!("Unknown membership role: {s}")),
        }
    }
}

// ============================================================================
// 成员资格状态枚举
// ============================================================================

/// 租户成员资格状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    /// 活跃状态
    Active,
    /// 待确认状态（等待用户接受邀请）
    Pending,
    /// 已暂停状态
    Suspended,
    /// 已失效状态（用户已离开租户）
    Inactive,
}

impl Default for MembershipStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl MembershipStatus {
    /// 检查状态是否允许访问租户资源
    pub fn allows_access(&self) -> bool {
        matches!(self, MembershipStatus::Active)
    }

    /// 获取状态字符串标识
    pub fn as_str(&self) -> &'static str {
        match self {
            MembershipStatus::Active => "active",
            MembershipStatus::Pending => "pending",
            MembershipStatus::Suspended => "suspended",
            MembershipStatus::Inactive => "inactive",
        }
    }
}

impl std::fmt::Display for MembershipStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// 成员资格来源枚举
// ============================================================================

/// 成员资格来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipSource {
    /// 通过邀请加入
    Invitation,
    /// 作为所有者创建租户时自动绑定
    OwnerCreation,
    /// 系统自动添加
    System,
    /// OAuth 同步添加
    OAuthSync,
}

impl MembershipSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MembershipSource::Invitation => "invitation",
            MembershipSource::OwnerCreation => "owner_creation",
            MembershipSource::System => "system",
            MembershipSource::OAuthSync => "oauth_sync",
        }
    }
}

// ============================================================================
// 邀请类型枚举
// ============================================================================

/// 邀请对象类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteeType {
    /// 邮箱邀请
    Email,
    /// 钱包地址邀请
    Wallet,
    /// 任意用户（通过邀请链接）
    Any,
}

impl InviteeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InviteeType::Email => "email",
            InviteeType::Wallet => "wallet",
            InviteeType::Any => "any",
        }
    }
}

// ============================================================================
// 邀请状态枚举
// ============================================================================

/// 邀请状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationStatus {
    /// 待接受
    Pending,
    /// 已接受
    Consumed,
    /// 已过期
    Expired,
    /// 已撤销
    Revoked,
}

impl Default for InvitationStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl InvitationStatus {
    /// 检查邀请是否可用
    pub fn is_available(&self) -> bool {
        matches!(self, InvitationStatus::Pending)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            InvitationStatus::Pending => "pending",
            InvitationStatus::Consumed => "consumed",
            InvitationStatus::Expired => "expired",
            InvitationStatus::Revoked => "revoked",
        }
    }
}

// ============================================================================
// MFA 状态枚举
// ============================================================================

/// MFA 验证状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MfaStatus {
    /// 待验证
    Pending,
    /// 已验证
    Verified,
    /// 不需要验证
    NotRequired,
}

impl Default for MfaStatus {
    fn default() -> Self {
        Self::NotRequired
    }
}

impl MfaStatus {
    /// 检查 MFA 是否通过
    pub fn is_passed(&self) -> bool {
        matches!(self, MfaStatus::Verified | MfaStatus::NotRequired)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MfaStatus::Pending => "pending",
            MfaStatus::Verified => "verified",
            MfaStatus::NotRequired => "not_required",
        }
    }
}

// ============================================================================
// 用户实体
// ============================================================================

/// 用户实体
///
/// 代表系统中的持久用户记录，与外部身份提供商解耦。
/// 用户可以有多个外部身份（如 Privy 钱包、Email、Google OAuth）。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    /// 用户唯一 ID（UUID v7）
    pub id: Uuid,

    /// 用户状态
    pub status: UserStatus,

    /// 显示名称（可选）
    pub display_name: Option<String>,

    /// 默认租户 ID（可选，用于新用户引导）
    pub default_tenant_id: Option<Uuid>,

    /// 是否已完成引导流程
    pub onboarding_completed: bool,

    /// 软删除时间（可选）
    pub deleted_at: Option<DateTime<Utc>>,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// 创建新用户
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            status: UserStatus::Active,
            display_name: None,
            default_tenant_id: None,
            onboarding_completed: false,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 设置显示名称
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self.updated_at = Utc::now();
        self
    }

    /// 设置默认租户
    pub fn with_default_tenant(mut self, tenant_id: Uuid) -> Self {
        self.default_tenant_id = Some(tenant_id);
        self.updated_at = Utc::now();
        self
    }

    /// 标记引导完成
    pub fn complete_onboarding(&mut self) {
        self.onboarding_completed = true;
        self.updated_at = Utc::now();
    }

    /// 检查用户是否活跃
    pub fn is_active(&self) -> bool {
        self.status.allows_authentication() && self.deleted_at.is_none()
    }

    /// 暂停用户
    pub fn suspend(&mut self) {
        self.status = UserStatus::Suspended;
        self.updated_at = Utc::now();
    }

    /// 激活用户
    pub fn activate(&mut self) {
        self.status = UserStatus::Active;
        self.updated_at = Utc::now();
    }

    /// 软删除用户
    pub fn soft_delete(&mut self) {
        self.status = UserStatus::PendingDeletion;
        self.deleted_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

impl Default for User {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 外部身份实体
// ============================================================================

/// 外部身份实体
///
/// 将用户与外部身份提供商（Privy、Email、OAuth 等）关联。
/// 支持钱包地址和邮箱作为身份标识。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ExternalIdentity {
    /// 身份唯一 ID（UUID v7）
    pub id: Uuid,

    /// 关联的用户 ID
    pub user_id: Uuid,

    /// 身份提供商类型
    pub provider: IdentityProvider,

    /// 提供商中的唯一标识（如 Privy did、Google user_id）
    pub provider_subject: String,

    /// 钱包地址（可选，用于 Privy 钱包认证）
    pub wallet_address: Option<String>,

    /// 邮箱地址（可选）
    pub email: Option<String>,

    /// 提供商返回的完整 Profile 数据（JSON）
    pub provider_profile: Option<JsonValue>,

    /// 是否已验证
    pub is_verified: bool,

    /// 是否为主要身份（用于登录优先级）
    pub is_primary: bool,

    /// MFA 是否已验证（来自 Privy）
    pub mfa_verified: bool,

    /// MFA 验证时间（可选）
    pub mfa_verified_at: Option<DateTime<Utc>>,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

impl ExternalIdentity {
    /// 创建新的外部身份
    pub fn new(
        user_id: Uuid,
        provider: IdentityProvider,
        provider_subject: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            user_id,
            provider,
            provider_subject: provider_subject.into(),
            wallet_address: None,
            email: None,
            provider_profile: None,
            is_verified: false,
            is_primary: false,
            mfa_verified: false,
            mfa_verified_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 设置钱包地址
    pub fn with_wallet(mut self, address: impl Into<String>) -> Self {
        self.wallet_address = Some(address.into());
        self.updated_at = Utc::now();
        self
    }

    /// 设置邮箱
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self.updated_at = Utc::now();
        self
    }

    /// 设置 Profile 数据
    pub fn with_profile(mut self, profile: JsonValue) -> Self {
        self.provider_profile = Some(profile);
        self.updated_at = Utc::now();
        self
    }

    /// 标记为已验证
    pub fn verify(&mut self) {
        self.is_verified = true;
        self.updated_at = Utc::now();
    }

    /// 设置为主要身份
    pub fn set_primary(&mut self) {
        self.is_primary = true;
        self.updated_at = Utc::now();
    }

    /// 检查是否为钱包身份
    pub fn is_wallet_identity(&self) -> bool {
        self.wallet_address.is_some() && self.provider.is_wallet_provider()
    }
}

// ============================================================================
// 租户成员资格实体
// ============================================================================

/// 租户成员资格实体
///
/// 定义用户与租户之间的关系，包括角色、权限和状态。
/// 支持邀请加入和所有者创建两种来源。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TenantMembership {
    /// 成员资格唯一 ID（UUID v7）
    pub id: Uuid,

    /// 租户 ID
    pub tenant_id: Uuid,

    /// 用户 ID
    pub user_id: Uuid,

    /// 成员角色
    pub role: MembershipRole,

    /// 成员状态
    pub status: MembershipStatus,

    /// 邀请人用户 ID（可选）
    pub invited_by: Option<Uuid>,

    /// 加入时间（接受邀请或创建时间）
    pub joined_at: Option<DateTime<Utc>>,

    /// 成员资格来源
    pub source: MembershipSource,

    /// 权限范围列表
    pub scopes: Vec<String>,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

impl TenantMembership {
    /// 创建新的成员资格（邀请方式）
    pub fn new_from_invitation(
        tenant_id: Uuid,
        user_id: Uuid,
        role: MembershipRole,
        invited_by: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id,
            user_id,
            role,
            status: MembershipStatus::Pending,
            invited_by: Some(invited_by),
            joined_at: None,
            source: MembershipSource::Invitation,
            scopes: role.default_scopes(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 创建所有者成员资格（创建租户时）
    pub fn new_owner(tenant_id: Uuid, user_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id,
            user_id,
            role: MembershipRole::Owner,
            status: MembershipStatus::Active,
            invited_by: None,
            joined_at: Some(now),
            source: MembershipSource::OwnerCreation,
            scopes: MembershipRole::Owner.default_scopes(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 接受邀请（激活成员资格）
    pub fn accept_invitation(&mut self) {
        self.status = MembershipStatus::Active;
        self.joined_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    /// 更新角色
    pub fn update_role(&mut self, role: MembershipRole) {
        self.role = role;
        self.scopes = role.default_scopes();
        self.updated_at = Utc::now();
    }

    /// 添加权限范围
    pub fn add_scope(&mut self, scope: impl Into<String>) {
        self.scopes.push(scope.into());
        self.updated_at = Utc::now();
    }

    /// 移除权限范围
    pub fn remove_scope(&mut self, scope: &str) {
        self.scopes.retain(|s| s != scope);
        self.updated_at = Utc::now();
    }

    /// 检查是否拥有某个权限
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(&scope.to_string())
    }

    /// 检查是否有管理权限
    pub fn can_manage(&self) -> bool {
        self.status.allows_access() && self.role.can_manage()
    }

    /// 检查是否为所有者
    pub fn is_owner(&self) -> bool {
        self.role.is_owner()
    }

    /// 暂停成员
    pub fn suspend(&mut self) {
        self.status = MembershipStatus::Suspended;
        self.updated_at = Utc::now();
    }

    /// 激活成员
    pub fn activate(&mut self) {
        self.status = MembershipStatus::Active;
        self.updated_at = Utc::now();
    }

    /// 离开租户
    pub fn leave(&mut self) {
        self.status = MembershipStatus::Inactive;
        self.updated_at = Utc::now();
    }
}

// ============================================================================
// 租户邀请实体
// ============================================================================

/// 租户邀请实体
///
/// 用于邀请用户加入租户。支持邮箱邀请、钱包邀请和开放邀请链接。
/// 使用 Token 哈希验证邀请有效性，防止泄露。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TenantInvitation {
    /// 邀请唯一 ID（UUID v7）
    pub id: Uuid,

    /// 租户 ID
    pub tenant_id: Uuid,

    /// 被邀请者的角色
    pub role: MembershipRole,

    /// 被邀请者类型
    pub invitee_type: InviteeType,

    /// 被邀请者邮箱（可选）
    pub invitee_email: Option<String>,

    /// 被邀请者钱包地址（可选）
    pub invitee_wallet: Option<String>,

    /// 邀请 Token 的哈希值（用于验证）
    /// 使用 SHA-256 哈希，原始 Token 不存储
    pub token_hash: String,

    /// 创建者用户 ID
    pub created_by: Uuid,

    /// 过期时间
    pub expires_at: DateTime<Utc>,

    /// 消费时间（被接受的时间）
    pub consumed_at: Option<DateTime<Utc>>,

    /// 消费者用户 ID（接受邀请的用户）
    pub consumed_by: Option<Uuid>,

    /// 邀请状态
    pub status: InvitationStatus,

    /// 最大使用次数（默认为 1）
    pub max_uses: i32,

    /// 已使用次数
    pub use_count: i32,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

impl TenantInvitation {
    /// 创建新的邀请（邮箱邀请）
    pub fn new_email_invitation(
        tenant_id: Uuid,
        role: MembershipRole,
        invitee_email: impl Into<String>,
        created_by: Uuid,
        expires_hours: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id,
            role,
            invitee_type: InviteeType::Email,
            invitee_email: Some(invitee_email.into()),
            invitee_wallet: None,
            token_hash: String::new(), // 需要在调用方设置
            created_by,
            expires_at: now + chrono::Duration::hours(expires_hours),
            consumed_at: None,
            consumed_by: None,
            status: InvitationStatus::Pending,
            max_uses: 1,
            use_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// 创建新的邀请（钱包邀请）
    pub fn new_wallet_invitation(
        tenant_id: Uuid,
        role: MembershipRole,
        invitee_wallet: impl Into<String>,
        created_by: Uuid,
        expires_hours: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id,
            role,
            invitee_type: InviteeType::Wallet,
            invitee_email: None,
            invitee_wallet: Some(invitee_wallet.into()),
            token_hash: String::new(),
            created_by,
            expires_at: now + chrono::Duration::hours(expires_hours),
            consumed_at: None,
            consumed_by: None,
            status: InvitationStatus::Pending,
            max_uses: 1,
            use_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// 创建开放邀请链接（任意用户可接受）
    pub fn new_open_invitation(
        tenant_id: Uuid,
        role: MembershipRole,
        created_by: Uuid,
        expires_hours: i64,
        max_uses: i32,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id,
            role,
            invitee_type: InviteeType::Any,
            invitee_email: None,
            invitee_wallet: None,
            token_hash: String::new(),
            created_by,
            expires_at: now + chrono::Duration::hours(expires_hours),
            consumed_at: None,
            consumed_by: None,
            status: InvitationStatus::Pending,
            max_uses,
            use_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// 设置 Token 哈希
    pub fn with_token_hash(mut self, hash: impl Into<String>) -> Self {
        self.token_hash = hash.into();
        self
    }

    /// 检查邀请是否有效（可用且未过期）
    pub fn is_valid(&self) -> bool {
        self.status.is_available() && self.expires_at > Utc::now() && self.use_count < self.max_uses
    }

    /// 检查邀请是否已过期
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// 消费邀请
    pub fn consume(&mut self, consumed_by: Uuid) {
        self.use_count += 1;
        self.consumed_at = Some(Utc::now());
        self.consumed_by = Some(consumed_by);
        self.updated_at = Utc::now();

        if self.use_count >= self.max_uses {
            self.status = InvitationStatus::Consumed;
        }
    }

    /// 撤销邀请
    pub fn revoke(&mut self) {
        self.status = InvitationStatus::Revoked;
        self.updated_at = Utc::now();
    }

    /// 标记为过期
    pub fn mark_expired(&mut self) {
        self.status = InvitationStatus::Expired;
        self.updated_at = Utc::now();
    }
}

// ============================================================================
// 认证会话实体
// ============================================================================

/// 认证会话实体
///
/// 服务端会话追踪记录，包含用户身份、活跃租户、MFA 状态等信息。
/// 使用 Token 哈希验证会话有效性，支持会话撤销。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthSession {
    /// 会话唯一 ID（UUID v7）
    pub id: Uuid,

    /// 用户 ID
    pub user_id: Uuid,

    /// 会话 Token 的哈希值（用于验证）
    pub session_token_hash: String,

    /// 关联的外部身份 ID（可选）
    pub identity_id: Option<Uuid>,

    /// 当前活跃的成员资格 ID（可选）
    pub active_membership_id: Option<Uuid>,

    /// MFA 验证状态
    pub mfa_status: MfaStatus,

    /// MFA 验证时间（可选）
    pub mfa_verified_at: Option<DateTime<Utc>>,

    /// 用户代理字符串（可选）
    pub user_agent: Option<String>,

    /// IP 地址（可选）
    pub ip_address: Option<String>,

    /// 过期时间
    pub expires_at: DateTime<Utc>,

    /// 最后活跃时间
    pub last_active_at: DateTime<Utc>,

    /// 撤销时间（可选）
    pub revoked_at: Option<DateTime<Utc>>,

    /// 撤销原因（可选）
    pub revoked_reason: Option<String>,

    /// 创建时间
    pub created_at: DateTime<Utc>,

    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

impl AuthSession {
    /// 创建新会话
    pub fn new(user_id: Uuid, session_token_hash: impl Into<String>, ttl_seconds: i64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            user_id,
            session_token_hash: session_token_hash.into(),
            identity_id: None,
            active_membership_id: None,
            mfa_status: MfaStatus::NotRequired,
            mfa_verified_at: None,
            user_agent: None,
            ip_address: None,
            expires_at: now + chrono::Duration::seconds(ttl_seconds),
            last_active_at: now,
            revoked_at: None,
            revoked_reason: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 设置关联身份
    pub fn with_identity(mut self, identity_id: Uuid) -> Self {
        self.identity_id = Some(identity_id);
        self
    }

    /// 设置活跃成员资格
    pub fn with_active_membership(mut self, membership_id: Uuid) -> Self {
        self.active_membership_id = Some(membership_id);
        self
    }

    /// 设置用户代理
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// 设置 IP 地址
    pub fn with_ip_address(mut self, ip_address: impl Into<String>) -> Self {
        self.ip_address = Some(ip_address.into());
        self
    }

    /// 设置 MFA 状态
    pub fn set_mfa_status(&mut self, status: MfaStatus) {
        self.mfa_status = status;
        if status == MfaStatus::Verified {
            self.mfa_verified_at = Some(Utc::now());
        }
        self.updated_at = Utc::now();
    }

    /// 检查会话是否有效
    pub fn is_valid(&self) -> bool {
        self.revoked_at.is_none() && self.expires_at > Utc::now()
    }

    /// 检查会话是否已过期
    pub fn is_expired(&self) -> bool {
        self.expires_at <= Utc::now()
    }

    /// 检查会话是否已撤销
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    /// 检查 MFA 是否已通过
    pub fn is_mfa_passed(&self) -> bool {
        self.mfa_status.is_passed()
    }

    /// 更新活跃时间
    pub fn touch(&mut self) {
        self.last_active_at = Utc::now();
        self.updated_at = Utc::now();
    }

    /// 延长会话有效期
    pub fn extend(&mut self, additional_seconds: i64) {
        self.expires_at += chrono::Duration::seconds(additional_seconds);
        self.updated_at = Utc::now();
    }

    /// 撤销会话
    pub fn revoke(&mut self, reason: impl Into<String>) {
        self.revoked_at = Some(Utc::now());
        self.revoked_reason = Some(reason.into());
        self.updated_at = Utc::now();
    }
}

// ============================================================================
// Service Account 与 API Token 元数据
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAccountStatus {
    Active,
    Inactive,
}

impl Default for ServiceAccountStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl ServiceAccountStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServiceAccountStatus::Active => "active",
            ServiceAccountStatus::Inactive => "inactive",
        }
    }

    pub fn can_issue_tokens(&self) -> bool {
        matches!(self, ServiceAccountStatus::Active)
    }
}

impl std::fmt::Display for ServiceAccountStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ServiceAccountStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "inactive" => Ok(Self::Inactive),
            _ => Err(format!("Unknown service account status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServiceAccount {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub role: String,
    pub scope_ceiling: Vec<String>,
    pub status: ServiceAccountStatus,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl ServiceAccount {
    pub fn new(tenant_id: Uuid, name: impl Into<String>, created_by: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            tenant_id,
            name: name.into(),
            description: None,
            role: "service_account".to_string(),
            scope_ceiling: Vec::new(),
            status: ServiceAccountStatus::Active,
            created_by,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_scope_ceiling(mut self, scopes: Vec<String>) -> Self {
        self.scope_ceiling = scopes;
        self
    }

    pub fn can_issue_tokens(&self) -> bool {
        self.deleted_at.is_none() && self.status.can_issue_tokens()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiTokenType {
    UserAccessToken,
    ServiceAccountToken,
}

impl ApiTokenType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiTokenType::UserAccessToken => "user_access_token",
            ApiTokenType::ServiceAccountToken => "service_account_token",
        }
    }
}

impl std::fmt::Display for ApiTokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ApiTokenType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user_access_token" => Ok(Self::UserAccessToken),
            "service_account_token" => Ok(Self::ServiceAccountToken),
            _ => Err(format!("Unknown api token type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiTokenSubjectType {
    User,
    ServiceAccount,
}

impl ApiTokenSubjectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiTokenSubjectType::User => "user",
            ApiTokenSubjectType::ServiceAccount => "service_account",
        }
    }
}

impl std::fmt::Display for ApiTokenSubjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ApiTokenSubjectType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(Self::User),
            "service_account" => Ok(Self::ServiceAccount),
            _ => Err(format!("Unknown api token subject type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiTokenMetadata {
    pub id: String,
    pub token_kind: String,
    pub token_type: ApiTokenType,
    pub subject_type: ApiTokenSubjectType,
    pub subject_id: Uuid,
    pub tenant_id: Uuid,
    pub issued_from: String,
    pub token_plane: String,
    pub session_id: Option<Uuid>,
    pub membership_id: Option<Uuid>,
    pub token_name: Option<String>,
    pub token_prefix: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub scopes: Vec<String>,
    pub credential_ids: Vec<String>,
    pub binding_handles: Vec<String>,
    pub issued_membership_role_snapshot: Option<String>,
    pub permission_source: Option<String>,
    pub created_via: Option<String>,
    pub revoked_reason: Option<String>,
    pub oauth_client_id: Option<String>,
    pub oauth_grant_type: Option<String>,
    pub oauth_subject_mode: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl ApiTokenMetadata {
    pub fn new(
        id: impl Into<String>,
        token_type: ApiTokenType,
        subject_type: ApiTokenSubjectType,
        subject_id: Uuid,
        tenant_id: Uuid,
        issued_from: impl Into<String>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.into(),
            token_kind: "user_access_token".to_string(),
            token_type,
            subject_type,
            subject_id,
            tenant_id,
            issued_from: issued_from.into(),
            token_plane: "management".to_string(),
            session_id: None,
            membership_id: None,
            token_name: None,
            token_prefix: None,
            display_name: None,
            description: None,
            scopes: Vec::new(),
            credential_ids: Vec::new(),
            binding_handles: Vec::new(),
            issued_membership_role_snapshot: None,
            permission_source: None,
            created_via: None,
            revoked_reason: None,
            oauth_client_id: None,
            oauth_grant_type: None,
            oauth_subject_mode: None,
            expires_at,
            revoked_at: None,
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    pub fn with_credential_ids(mut self, credential_ids: Vec<String>) -> Self {
        self.credential_ids = credential_ids;
        self
    }

    pub fn with_binding_handles(mut self, binding_handles: Vec<String>) -> Self {
        self.binding_handles = binding_handles;
        self
    }

    pub fn with_token_plane(mut self, token_plane: impl Into<String>) -> Self {
        self.token_plane = token_plane.into();
        self
    }

    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    pub fn with_token_kind(mut self, token_kind: impl Into<String>) -> Self {
        self.token_kind = token_kind.into();
        self
    }

    pub fn with_token_name(mut self, token_name: impl Into<String>) -> Self {
        let token_name = token_name.into();
        self.token_name = Some(token_name.clone());
        if self.display_name.is_none() {
            self.display_name = Some(token_name);
        }
        self
    }

    pub fn with_token_prefix(mut self, token_prefix: impl Into<String>) -> Self {
        self.token_prefix = Some(token_prefix.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_membership_id(mut self, membership_id: Uuid) -> Self {
        self.membership_id = Some(membership_id);
        self
    }

    pub fn with_permission_source(mut self, permission_source: impl Into<String>) -> Self {
        self.permission_source = Some(permission_source.into());
        self
    }

    pub fn with_created_via(mut self, created_via: impl Into<String>) -> Self {
        self.created_via = Some(created_via.into());
        self
    }

    pub fn with_membership_role_snapshot(mut self, role: impl Into<String>) -> Self {
        self.issued_membership_role_snapshot = Some(role.into());
        self
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) {
        self.revoked_at = Some(at);
    }

    pub fn with_revoked_reason(mut self, revoked_reason: impl Into<String>) -> Self {
        self.revoked_reason = Some(revoked_reason.into());
        self
    }

    pub fn mark_used(&mut self, at: DateTime<Utc>) {
        self.last_used_at = Some(at);
    }

    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none() && self.expires_at > Utc::now()
    }
}

// ============================================================================
// 认证审计日志实体
// ============================================================================

/// 认证审计日志类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthEventType {
    /// 用户创建
    UserCreated,
    /// 用户登录
    UserLogin,
    /// 用户注销
    UserLogout,
    /// 会话创建
    SessionCreated,
    /// 会话撤销
    SessionRevoked,
    /// 会话过期
    SessionExpired,
    /// 外部身份绑定
    IdentityLinked,
    /// 外部身份验证
    IdentityVerified,
    /// 租户成员加入
    MemberJoined,
    /// 租户成员离开
    MemberLeft,
    /// 租户成员角色变更
    MemberRoleChanged,
    /// 邀请创建
    InvitationCreated,
    /// 邀请接受
    InvitationAccepted,
    /// 邀请撤销
    InvitationRevoked,
    /// MFA 验证成功
    MfaVerified,
    /// MFA 验证失败
    MfaFailed,
    /// 认证失败
    AuthenticationFailed,
}

impl AuthEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthEventType::UserCreated => "user_created",
            AuthEventType::UserLogin => "user_login",
            AuthEventType::UserLogout => "user_logout",
            AuthEventType::SessionCreated => "session_created",
            AuthEventType::SessionRevoked => "session_revoked",
            AuthEventType::SessionExpired => "session_expired",
            AuthEventType::IdentityLinked => "identity_linked",
            AuthEventType::IdentityVerified => "identity_verified",
            AuthEventType::MemberJoined => "member_joined",
            AuthEventType::MemberLeft => "member_left",
            AuthEventType::MemberRoleChanged => "member_role_changed",
            AuthEventType::InvitationCreated => "invitation_created",
            AuthEventType::InvitationAccepted => "invitation_accepted",
            AuthEventType::InvitationRevoked => "invitation_revoked",
            AuthEventType::MfaVerified => "mfa_verified",
            AuthEventType::MfaFailed => "mfa_failed",
            AuthEventType::AuthenticationFailed => "authentication_failed",
        }
    }

    pub fn is_login_flow_event(&self) -> bool {
        matches!(
            self,
            AuthEventType::UserCreated | AuthEventType::UserLogin | AuthEventType::SessionCreated
        )
    }
}

/// 认证审计日志实体
///
/// 记录所有认证相关事件，用于安全审计和合规追踪。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuthAuditLog {
    /// 日志唯一 ID（UUID v7）
    pub id: Uuid,

    /// 事件类型
    pub event_type: AuthEventType,

    /// 相关用户 ID（可选）
    pub user_id: Option<Uuid>,

    /// 相关会话 ID（可选）
    pub session_id: Option<Uuid>,

    /// 相关租户 ID（可选）
    pub tenant_id: Option<Uuid>,

    /// 相关成员资格 ID（可选）
    pub membership_id: Option<Uuid>,

    /// 相关身份 ID（可选）
    pub identity_id: Option<Uuid>,

    /// 相关邀请 ID（可选）
    pub invitation_id: Option<Uuid>,

    /// 事件详情数据（JSON）
    pub details: Option<JsonValue>,

    /// 用户代理字符串（可选）
    pub user_agent: Option<String>,

    /// IP 地址（可选）
    pub ip_address: Option<String>,

    /// 事件结果（成功/失败）
    pub success: bool,

    /// 错误消息（可选）
    pub error_message: Option<String>,

    /// 事件时间
    pub created_at: DateTime<Utc>,
}

impl AuthAuditLog {
    /// 创建审计日志
    pub fn new(event_type: AuthEventType) -> Self {
        Self {
            id: Uuid::now_v7(),
            event_type,
            user_id: None,
            session_id: None,
            tenant_id: None,
            membership_id: None,
            identity_id: None,
            invitation_id: None,
            details: None,
            user_agent: None,
            ip_address: None,
            success: true,
            error_message: None,
            created_at: Utc::now(),
        }
    }

    /// 设置用户 ID
    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// 设置会话 ID
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// 设置租户 ID
    pub fn with_tenant(mut self, tenant_id: Uuid) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// 设置成员资格 ID
    pub fn with_membership(mut self, membership_id: Uuid) -> Self {
        self.membership_id = Some(membership_id);
        self
    }

    /// 设置身份 ID
    pub fn with_identity(mut self, identity_id: Uuid) -> Self {
        self.identity_id = Some(identity_id);
        self
    }

    /// 设置邀请 ID
    pub fn with_invitation(mut self, invitation_id: Uuid) -> Self {
        self.invitation_id = Some(invitation_id);
        self
    }

    /// 设置详情数据
    pub fn with_details(mut self, details: JsonValue) -> Self {
        self.details = Some(details);
        self
    }

    /// 设置用户代理
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// 设置 IP 地址
    pub fn with_ip_address(mut self, ip_address: impl Into<String>) -> Self {
        self.ip_address = Some(ip_address.into());
        self
    }

    /// 标记为失败
    pub fn mark_failed(mut self, error_message: impl Into<String>) -> Self {
        self.success = false;
        self.error_message = Some(error_message.into());
        self
    }
}

// ============================================================================
// 辅助类型
// ============================================================================

/// Privy 认证响应数据
///
/// 从 Privy 认证服务返回的用户信息。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PrivyAuthResponse {
    /// Privy 用户 ID（did）
    pub did: String,

    /// 钱包地址（可选）
    pub wallet_address: Option<String>,

    /// 邮箱地址（可选）
    pub email: Option<String>,

    /// 用户显示名称（可选）
    pub name: Option<String>,

    /// Profile 数据
    pub profile: Option<JsonValue>,

    /// 是否为新用户
    pub is_new_user: bool,
}

/// 创建用户请求
#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserRequest {
    /// 显示名称（可选）
    pub display_name: Option<String>,
}

/// 创建会话请求
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionRequest {
    /// 会话 TTL（秒）
    pub ttl_seconds: Option<i64>,
    /// 用户代理字符串
    pub user_agent: Option<String>,
    /// IP 地址
    pub ip_address: Option<String>,
}

// ============================================================================
// SQLx trait implementations
// ====================================================================================

// UserStatus SQLx implementations
impl std::str::FromStr for UserStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(UserStatus::Active),
            "inactive" => Ok(UserStatus::Inactive),
            "suspended" => Ok(UserStatus::Suspended),
            "pending_deletion" => Ok(UserStatus::PendingDeletion),
            _ => Err(format!("Unknown user status: {s}")),
        }
    }
}

impl sqlx::Type<Postgres> for UserStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for UserStatus {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for UserStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

// IdentityProvider SQLx implementations
impl sqlx::Type<Postgres> for IdentityProvider {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for IdentityProvider {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for IdentityProvider {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

// MembershipRole SQLx implementations
impl sqlx::Type<Postgres> for MembershipRole {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for MembershipRole {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for MembershipRole {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

impl sqlx::Type<Postgres> for ServiceAccountStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for ServiceAccountStatus {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for ServiceAccountStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

impl sqlx::Type<Postgres> for ApiTokenType {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for ApiTokenType {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for ApiTokenType {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

impl sqlx::Type<Postgres> for ApiTokenSubjectType {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for ApiTokenSubjectType {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for ApiTokenSubjectType {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

// MembershipStatus SQLx implementations
impl std::str::FromStr for MembershipStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(MembershipStatus::Active),
            "pending" => Ok(MembershipStatus::Pending),
            "suspended" => Ok(MembershipStatus::Suspended),
            "inactive" => Ok(MembershipStatus::Inactive),
            _ => Err(format!("Unknown membership status: {s}")),
        }
    }
}

impl sqlx::Type<Postgres> for MembershipStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for MembershipStatus {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for MembershipStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

// MembershipSource SQLx implementations
impl std::fmt::Display for MembershipSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for MembershipSource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "invitation" => Ok(MembershipSource::Invitation),
            "owner_creation" => Ok(MembershipSource::OwnerCreation),
            "system" => Ok(MembershipSource::System),
            "oauth_sync" => Ok(MembershipSource::OAuthSync),
            _ => Err(format!("Unknown membership source: {s}")),
        }
    }
}

impl sqlx::Type<Postgres> for MembershipSource {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for MembershipSource {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for MembershipSource {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

// InviteeType SQLx implementations
impl std::fmt::Display for InviteeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for InviteeType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "email" => Ok(InviteeType::Email),
            "wallet" => Ok(InviteeType::Wallet),
            "any" => Ok(InviteeType::Any),
            _ => Err(format!("Unknown invitee type: {s}")),
        }
    }
}

impl sqlx::Type<Postgres> for InviteeType {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for InviteeType {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for InviteeType {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

// InvitationStatus SQLx implementations
impl std::str::FromStr for InvitationStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(InvitationStatus::Pending),
            "consumed" => Ok(InvitationStatus::Consumed),
            "expired" => Ok(InvitationStatus::Expired),
            "revoked" => Ok(InvitationStatus::Revoked),
            _ => Err(format!("Unknown invitation status: {s}")),
        }
    }
}

impl sqlx::Type<Postgres> for InvitationStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for InvitationStatus {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for InvitationStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

// MfaStatus SQLx implementations
impl std::fmt::Display for MfaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for MfaStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(MfaStatus::Pending),
            "verified" => Ok(MfaStatus::Verified),
            "not_required" => Ok(MfaStatus::NotRequired),
            _ => Err(format!("Unknown MFA status: {s}")),
        }
    }
}

impl sqlx::Type<Postgres> for MfaStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        sqlx::postgres::PgTypeInfo::with_name("text")
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        *ty == sqlx::postgres::PgTypeInfo::with_name("text")
            || *ty == sqlx::postgres::PgTypeInfo::with_name("varchar")
    }
}

impl<'r> sqlx::Decode<'r, Postgres> for MfaStatus {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Postgres>>::decode(value)?;
        Self::from_str(s).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as sqlx::error::BoxDynError
        })
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for MfaStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync + 'static>> {
        <&str as sqlx::Encode<Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::new();
        assert!(user.is_active());
        assert!(!user.onboarding_completed);
        assert!(user.display_name.is_none());
    }

    #[test]
    fn test_user_status_checks() {
        let active = UserStatus::Active;
        assert!(active.allows_authentication());
        assert!(active.allows_operations());

        let suspended = UserStatus::Suspended;
        assert!(!suspended.allows_authentication());
        assert!(!suspended.allows_operations());
    }

    #[test]
    fn test_membership_role_permissions() {
        let owner = MembershipRole::Owner;
        assert!(owner.is_owner());
        assert!(owner.can_manage());
        assert!(owner.can_write());
        assert_eq!(owner.permission_level(), 100);

        let admin = MembershipRole::Admin;
        assert!(!admin.is_owner());
        assert!(admin.can_manage());
        assert!(admin.can_write());

        let member = MembershipRole::Member;
        assert!(!member.is_owner());
        assert!(!member.can_manage());
        assert!(member.can_write());

        let readonly = MembershipRole::Readonly;
        assert!(!readonly.is_owner());
        assert!(!readonly.can_manage());
        assert!(!readonly.can_write());
    }

    #[test]
    fn test_membership_creation() {
        let membership = TenantMembership::new_owner(Uuid::nil(), Uuid::nil());
        assert!(membership.is_owner());
        assert!(membership.status.allows_access());
        assert!(membership.can_manage());
    }

    #[test]
    fn test_invitation_validity() {
        let invitation = TenantInvitation::new_email_invitation(
            Uuid::nil(),
            MembershipRole::Member,
            "test@example.com",
            Uuid::nil(),
            24, // 24 hours
        );
        assert!(invitation.is_valid());
        assert!(!invitation.is_expired());
    }

    #[test]
    fn test_session_validity() {
        let session = AuthSession::new(Uuid::nil(), "test_hash", 3600);
        assert!(session.is_valid());
        assert!(!session.is_expired());
        assert!(!session.is_revoked());
    }

    #[test]
    fn test_identity_provider_checks() {
        let privy = IdentityProvider::Privy;
        assert!(privy.is_wallet_provider());
        assert!(!privy.is_oauth());

        let google = IdentityProvider::Google;
        assert!(google.is_oauth());
        assert!(!google.is_wallet_provider());
    }

    #[test]
    fn test_default_scopes() {
        let owner_scopes = MembershipRole::Owner.default_scopes();
        assert!(owner_scopes.contains(&"tenant:delete".to_string()));
        assert!(owner_scopes.contains(&"admin".to_string()));

        let member_scopes = MembershipRole::Member.default_scopes();
        assert!(member_scopes.contains(&"credential:read".to_string()));
        assert!(!member_scopes.contains(&"tenant:admin".to_string()));
    }

    #[test]
    fn test_audit_log_creation() {
        let log = AuthAuditLog::new(AuthEventType::UserLogin)
            .with_user(Uuid::nil())
            .with_ip_address("127.0.0.1");
        assert!(log.success);
        assert!(log.user_id.is_some());
        assert!(log.ip_address.is_some());
    }

    #[test]
    fn test_login_flow_event_detection() {
        assert!(AuthEventType::UserCreated.is_login_flow_event());
        assert!(AuthEventType::UserLogin.is_login_flow_event());
        assert!(AuthEventType::SessionCreated.is_login_flow_event());
        assert!(!AuthEventType::InvitationCreated.is_login_flow_event());
        assert!(!AuthEventType::SessionRevoked.is_login_flow_event());
    }

    #[test]
    fn test_service_account_creation() {
        let service_account = ServiceAccount::new(Uuid::nil(), "ci-bot", Uuid::nil())
            .with_scope_ceiling(vec!["credential:read".to_string()]);
        assert!(service_account.can_issue_tokens());
        assert_eq!(service_account.role, "service_account");
        assert_eq!(
            service_account.scope_ceiling,
            vec!["credential:read".to_string()]
        );
    }

    #[test]
    fn test_api_token_metadata_lifecycle() {
        let mut metadata = ApiTokenMetadata::new(
            "token-123",
            ApiTokenType::UserAccessToken,
            ApiTokenSubjectType::User,
            Uuid::nil(),
            Uuid::nil(),
            "session",
            Utc::now() + chrono::Duration::minutes(15),
        )
        .with_scopes(vec!["credential:read".to_string()]);
        assert!(metadata.is_active());
        metadata.revoke(Utc::now());
        assert!(!metadata.is_active());
    }
}
