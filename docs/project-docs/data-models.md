# CredBridge 数据模型文档

**生成日期**: 2026-03-18
**数据库**: PostgreSQL (with RLS)

---

## 核心数据类型

### CredentialType (凭证类型)

```rust
pub enum CredentialType {
    UsernamePassword,  // 用户名密码
    OAuthRefresh,      // OAuth 刷新令牌
    ApiKey,            // API 密钥
    SessionCookie,     // 会话 Cookie
    KycDocument,       // KYC 文档
}
```

---

## 主要数据结构

### CredentialMetadata (凭证元数据)

存储在数据库中的凭证元数据，不含敏感加密内容。

```rust
pub struct CredentialMetadata {
    pub credential_id: String,      // 凭证唯一ID
    pub credential_type: CredentialType,  // 凭证类型
    pub user_id_hash: String,       // 用户ID（哈希后）
    pub service_id: String,         // 服务ID
    pub tenant_id: String,          // 租户ID（RLS 隔离）
    pub created_at: String,         // 创建时间
    pub expires_at: Option<String>, // 过期时间
    pub is_deleted: bool,           // 软删除标记
    pub version: u32,               // 版本号
}
```

**数据库表**: `credentials`

| 字段 | 类型 | 约束 | 描述 |
|------|------|------|------|
| credential_id | UUID | PRIMARY KEY | 凭证唯一标识 |
| credential_type | VARCHAR | NOT NULL | 凭证类型枚举 |
| user_id_hash | VARCHAR | NOT NULL, INDEX | 用户ID哈希 |
| service_id | VARCHAR | NOT NULL, INDEX | 服务标识 |
| tenant_id | VARCHAR | NOT NULL, INDEX | 租户ID（RLS） |
| created_at | TIMESTAMP | NOT NULL | 创建时间 |
| expires_at | TIMESTAMP | NULL | 过期时间 |
| is_deleted | BOOLEAN | DEFAULT false | 软删除标记 |
| version | INTEGER | DEFAULT 1 | 版本号 |

### StoredCredential (存储的凭证)

完整的凭证存储格式，包含加密内容。

```rust
pub struct StoredCredential {
    pub metadata: CredentialMetadata,      // 元数据
    pub encrypted_payload: String,         // 加密后的载荷（Base64）
    pub version: u8,                       // 加密版本
    pub algorithm: String,                 // 加密算法
    pub kdf: String,                       // KDF 算法
}
```

**加密算法**: AES-256-GCM
**KDF**: Argon2id

### TenantConfig (租户配置)

```rust
pub struct TenantConfig {
    pub tenant_id: String,              // 租户唯一ID
    pub storage_quota: u64,             // 存储配额（字节）
    pub token_ttl_seconds: u64,         // Token 有效期（秒）
    pub audit_retention_days: u32,      // 审计日志保留期（天）
    pub is_active: bool,                // 是否启用
}
```

**默认值**:
- storage_quota: 1GB
- token_ttl_seconds: 900秒（15分钟）
- audit_retention_days: 7天
- is_active: true

**数据库表**: `tenant_configs`

---

## 审计日志模型

### AuditEvent (审计事件)

```rust
pub struct AuditEvent {
    pub id: String,                     // 事件唯一ID
    pub event_type: AuditEventType,     // 事件类型
    pub tenant_id: String,              // 租户ID
    pub user_id: Option<String>,        // 操作用户ID
    pub credential_id: Option<String>,  // 相关凭证ID
    pub timestamp: DateTime<Utc>,       // 时间戳
    pub details: serde_json::Value,     // 事件详情
    pub hash: String,                   // 事件哈希（完整性校验）
}
```

**存储**: immudb（不可篡改数据库）

### AuditEventType (审计事件类型)

| 事件类型 | 描述 |
|----------|------|
| `credential_created` | 凭证创建 |
| `credential_updated` | 凭证更新 |
| `credential_deleted` | 凭证删除 |
| `credential_decrypted` | 凭证解密 |
| `token_issued` | Token 签发 |
| `token_revoked` | Token 撤销 |
| `tenant_created` | 租户创建 |
| `tenant_updated` | 租户更新 |
| `sandbox_session_created` | 沙箱会话创建 |
| `sandbox_code_executed` | 沙箱代码执行 |

---

## Token 模型

### TokenClaims (Token 声明)

```rust
pub struct TokenClaims {
    pub sub: String,                    // 主题（租户ID）
    pub iat: i64,                       // 签发时间
    pub exp: i64,                       // 过期时间
    pub scopes: Vec<String>,            // 权限范围
    pub jti: String,                    // Token 唯一ID
}
```

**Token 格式**: PASETO v4.public

### TokenScope (Token 权限范围)

| 范围 | 描述 |
|------|------|
| `credentials:read` | 读取凭证 |
| `credentials:write` | 创建/更新凭证 |
| `credentials:delete` | 删除凭证 |
| `credentials:decrypt` | 解密凭证 |
| `audit:read` | 读取审计日志 |
| `sandbox:execute` | 执行沙箱代码 |
| `tenant:admin` | 租户管理 |

---

## 沙箱模型

### SandboxSession (沙箱会话)

```rust
pub struct SandboxSession {
    pub session_id: String,             // 会话ID
    pub tenant_id: String,              // 租户ID
    pub runtime: String,                // 运行时（python3.11, node18等）
    pub status: SandboxStatus,          // 状态
    pub attestation_report: AttestationReport,  // TEE 证明报告
    pub created_at: DateTime<Utc>,      // 创建时间
    pub expires_at: DateTime<Utc>,      // 过期时间
    pub memory_limit_mb: u32,           // 内存限制 |
    pub cpu_limit: f32,                 // CPU 限制 |
}
```

### SandboxStatus (沙箱状态)

```rust
pub enum SandboxStatus {
    Initializing,   // 初始化中
    Ready,          // 就绪
    Running,        // 运行中
    Terminated,     // 已终止
    Error,          // 错误
}
```

### SandboxOperation (沙箱操作)

```rust
pub struct SandboxOperation {
    pub operation_id: String,           // 操作ID
    pub session_id: String,             // 所属会话ID
    pub operation_type: OperationType,  // 操作类型
    pub input: serde_json::Value,       // 输入参数
    pub output: Option<serde_json::Value>,  // 输出结果
    pub status: OperationStatus,        // 状态
    pub started_at: DateTime<Utc>,      // 开始时间
    pub completed_at: Option<DateTime<Utc>>,  // 完成时间
}
```

---

## 数据库关系图

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   credentials   │     │ tenant_configs  │     │  audit_events   │
├─────────────────┤     ├─────────────────┤     ├─────────────────┤
│ credential_id   │     │ tenant_id       │◄────┤ tenant_id       │
│ credential_type │     │ storage_quota   │     │ event_type      │
│ user_id_hash    │     │ token_ttl       │     │ user_id         │
│ service_id      │     │ audit_retention │     │ credential_id ──┼──►
│ tenant_id ──────┼────►│ is_active       │     │ timestamp       │
│ created_at      │     └─────────────────┘     │ details         │
│ expires_at      │                             │ hash            │
│ is_deleted      │                             └─────────────────┘
│ version         │
└─────────────────┘

┌─────────────────┐     ┌─────────────────┐
│sandbox_sessions │     │token_blacklist  │
├─────────────────┤     ├─────────────────┤
│ session_id      │     │ token_jti       │
│ tenant_id       │     │ revoked_at      │
│ runtime         │     │ expires_at      │
│ status          │     └─────────────────┘
│ attestation     │
│ created_at      │
└─────────────────┘
```

---

## RLS (行级安全) 策略

### credentials 表 RLS 策略

```sql
-- 启用 RLS
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;

-- 租户隔离策略
CREATE POLICY tenant_isolation ON credentials
    USING (tenant_id = current_setting('app.current_tenant')::TEXT);

-- 只允许查看未删除的凭证
CREATE POLICY hide_deleted ON credentials
    USING (is_deleted = false);
```

### 设置当前租户

```sql
SET app.current_tenant = 'tenant-uuid';
```

---

## 索引设计

### credentials 表索引

```sql
-- 按租户查询
CREATE INDEX idx_credentials_tenant ON credentials(tenant_id);

-- 按用户查询
CREATE INDEX idx_credentials_user ON credentials(user_id_hash);

-- 按服务查询
CREATE INDEX idx_credentials_service ON credentials(service_id);

-- 复合索引：租户+用户
CREATE INDEX idx_credentials_tenant_user ON credentials(tenant_id, user_id_hash);

-- 复合索引：租户+服务
CREATE INDEX idx_credentials_tenant_service ON credentials(tenant_id, service_id);

-- 过期时间索引（用于清理任务）
CREATE INDEX idx_credentials_expires ON credentials(expires_at) WHERE expires_at IS NOT NULL;
```

---

## 数据流

### 凭证存储流程

```
1. 接收凭证数据
   ↓
2. 使用 AES-256-GCM 加密 payload
   ↓
3. 存储元数据到 PostgreSQL (RLS 保护)
   ↓
4. 存储加密密钥到 Vault
   ↓
5. 记录审计日志到 immudb
```

### 凭证解密流程

```
1. 验证 Token 权限
   ↓
2. 从 PostgreSQL 获取元数据（RLS 隔离）
   ↓
3. 从 Vault 获取解密密钥
   ↓
4. 使用 AES-256-GCM 解密 payload
   ↓
5. 记录解密审计日志
   ↓
6. 返回解密后的凭证
```

---

*本文档由 BMAD document-project 工作流自动生成*
