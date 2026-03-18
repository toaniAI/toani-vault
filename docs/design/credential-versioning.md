# 凭证版本控制设计文档

**文档版本**: 1.0
**创建日期**: 2026-03-12
**状态**: 已批准
**关联 Epic**: EP2 Story 2.4
**P0 修复**: Task 2

---

## 1. 设计概述

### 1.1 背景

EP2 Story 2.4 凭证版本控制在 E2E 测试中被标记为**功能完全缺失**。本设计文档描述凭证版本控制系统的完整架构，支持凭证更新、版本历史查询和版本回滚。

### 1.2 设计目标

| 目标 | 描述 |
|------|------|
| 版本追溯 | 完整记录凭证的所有历史版本 |
| 安全回滚 | 支持安全、审计的版本回滚操作 |
| 审计合规 | 满足金融凭证审计要求 |
| 租户隔离 | 保持多租户数据隔离架构 |

### 1.3 设计原则

1. **增量版本模式**: 每次更新创建新版本，保留所有历史
2. **加密一致性**: 版本历史存储加密后的 payload，保持安全边界
3. **审计完整**: 记录每次变更的原因和变更人
4. **租户隔离**: 版本数据遵循 Schema-per-Tenant 隔离

---

## 2. 数据模型设计

### 2.1 VaultEntry 扩展

在现有 `VaultEntry` 结构中添加 `version` 字段：

```rust
/// src/vault/models.rs
pub struct VaultEntry {
    /// 凭证唯一 ID（UUID v7）
    pub credential_id: CredentialId,

    /// 当前版本号 (新增字段，从 1 开始递增)
    pub version: u32,

    /// 租户 ID（多租户隔离）
    pub tenant_id: TenantId,

    /// 用户 ID（哈希后存储）
    pub user_id: UserId,

    /// 服务 ID
    pub service_id: ServiceId,

    /// 凭证类型
    pub credential_type: CredentialType,

    /// 创建时间戳（Unix 秒）
    pub created_at: u64,

    /// 更新时间戳（Unix 秒）
    pub updated_at: u64,

    /// 过期时间戳（可选，Unix 秒）
    pub expires_at: Option<u64>,

    /// 加密载荷
    pub encrypted_payload: EncryptedPayload,

    /// 是否已删除（软删除）
    pub is_deleted: bool,
}
```

### 2.2 版本历史表

```sql
-- credential_versions 表结构
CREATE TABLE credential_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    credential_id UUID NOT NULL REFERENCES credentials(credential_id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    encrypted_payload JSONB NOT NULL,
    change_reason TEXT,
    changed_by UUID,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    CONSTRAINT unique_credential_version UNIQUE (credential_id, version)
);

-- 索引
CREATE INDEX idx_credential_versions_credential_id ON credential_versions(credential_id);
CREATE INDEX idx_credential_versions_created_at ON credential_versions(created_at);
CREATE INDEX idx_credential_versions_changed_by ON credential_versions(changed_by);
```

### 2.3 版本历史存储结构

```json
{
  "version": 2,
  "algorithm": "AES-256-GCM",
  "kdf": "HKDF-SHA-256",
  "nonce": "base64_encoded_nonce",
  "auth_tag": "base64_encoded_auth_tag",
  "ciphertext": "base64_encoded_ciphertext"
}
```

---

## 3. API 设计

### 3.1 更新凭证

**端点**: `PUT /api/v1/credentials/:id`

**Scope**: `credential:write`

**请求**:
```http
PUT /api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c
Authorization: Bearer <token>
Content-Type: application/json

{
  "plaintext_data": { "password": "new_secure_password" },
  "change_reason": "密码轮换 - 定期更新"
}
```

**响应**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "version": 2,
  "service_id": "schwab",
  "credential_type": "username_password",
  "updated_at": "2026-03-12T10:30:00Z",
  "previous_version": 1
}
```

**处理流程**:
```
1. 验证 Scope: credential:write
2. 验证租户隔离（用户只能更新自己的凭证）
3. 读取当前凭证（获取当前版本 N）
4. 加密新的 plaintext_data
5. 将当前版本保存到 credential_versions 表
   - credential_id, version=N, encrypted_payload, change_reason, changed_by
6. 更新 credentials 表：version = N+1, updated_at = now
7. 记录审计日志
8. 返回新凭证信息
```

### 3.2 查询版本历史

**端点**: `GET /api/v1/credentials/:id/versions`

**Scope**: `credential:read`

**请求**:
```http
GET /api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c/versions
Authorization: Bearer <token>
```

**响应**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "current_version": 5,
  "versions": [
    {
      "version": 1,
      "created_at": "2026-01-15T08:00:00Z",
      "changed_by": "user_123",
      "change_reason": "初始创建"
    },
    {
      "version": 2,
      "created_at": "2026-02-01T14:30:00Z",
      "changed_by": "user_123",
      "change_reason": "密码轮换 - 定期更新"
    },
    {
      "version": 3,
      "created_at": "2026-02-15T09:15:00Z",
      "changed_by": "user_123",
      "change_reason": "安全事件响应"
    },
    {
      "version": 4,
      "created_at": "2026-03-01T16:45:00Z",
      "changed_by": "user_123",
      "change_reason": "密码轮换 - 定期更新"
    },
    {
      "version": 5,
      "created_at": "2026-03-10T11:20:00Z",
      "changed_by": "user_123",
      "change_reason": "回滚到版本 2"
    }
  ],
  "total": 5
}
```

### 3.3 查询指定版本

**端点**: `GET /api/v1/credentials/:id/versions/:version`

**Scope**: `credential:read`

**请求**:
```http
GET /api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c/versions/2
Authorization: Bearer <token>
```

**响应**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "version": 2,
  "created_at": "2026-02-01T14:30:00Z",
  "changed_by": "user_123",
  "change_reason": "密码轮换 - 定期更新",
  "metadata": {
    "service_id": "schwab",
    "credential_type": "username_password",
    "algorithm": "AES-256-GCM"
  }
}
```

### 3.4 版本回滚

**端点**: `POST /api/v1/credentials/:id/rollback`

**Scope**: `credential:write`

**请求**:
```http
POST /api/v1/credentials/018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c/rollback
Authorization: Bearer <token>
Content-Type: application/json

{
  "target_version": 2,
  "reason": "安全审计要求恢复到版本 2"
}
```

**响应**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "previous_version": 5,
  "current_version": 6,
  "rollback_to_version": 2,
  "rollback_at": "2026-03-12T10:35:00Z",
  "reason": "安全审计要求恢复到版本 2"
}
```

---

## 4. 版本回滚流程

### 4.1 回滚算法

```
POST /api/v1/credentials/:id/rollback

输入:
  - credential_id: 凭证 ID
  - target_version: 目标版本号
  - reason: 回滚原因

输出:
  - 新创建的版本记录

流程:
1. 验证 Scope: credential:write
2. 验证租户隔离
3. 读取当前凭证，获取 current_version
4. 验证 target_version < current_version（不能回滚到未来）
5. 验证 target_version >= 1
6. 从 credential_versions 表读取目标版本的 encrypted_payload
7. 创建新版本:
   - new_version = current_version + 1
   - encrypted_payload = 目标版本的 encrypted_payload（直接复制）
   - change_reason = "rollback: " + reason
   - changed_by = 当前用户
8. 更新 credentials 表: version = new_version, updated_at = now
9. 记录审计日志: event_type = "credential_rollback"
10. 返回新创建版本信息
```

### 4.2 回滚流程图

```
┌─────────────────────────────────────────────────────────────┐
│                    回滚请求                                   │
│              POST /credentials/:id/rollback                  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 1: 验证 Scope 和租户隔离                                 │
│  - require_scope(credential:write)                          │
│  - verify_tenant_access()                                   │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 2: 验证目标版本                                         │
│  - target_version >= 1                                      │
│  - target_version < current_version                         │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 3: 读取目标版本数据                                      │
│  SELECT encrypted_payload FROM credential_versions          │
│  WHERE credential_id = :id AND version = :target_version    │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 4: 创建新版本（current_version + 1）                    │
│  - 复制目标版本的 encrypted_payload                          │
│  - change_reason = "rollback to version X"                  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 5: 更新凭证表                                           │
│  UPDATE credentials SET version = :new_version,             │
│    updated_at = NOW() WHERE id = :id                        │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Step 6: 记录审计日志                                         │
│  event_type: credential_rollback                            │
│  event_data: {from_version, to_version, reason}             │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    返回成功响应                               │
└─────────────────────────────────────────────────────────────┘
```

### 4.3 并发控制

对于并发更新场景，使用**乐观锁**机制：

```rust
// Rust 伪代码
pub async fn update_credential(
    &self,
    credential_id: &CredentialId,
    new_payload: EncryptedPayload,
    expected_version: u32,  // 客户端提供的期望版本
    change_reason: &str,
    changed_by: &UserId,
) -> Result<VaultEntry, VaultError> {
    // 乐观锁检查
    let current = self.get_credential(credential_id).await?;

    if current.version != expected_version {
        return Err(VaultError::OptimisticLockFailed {
            expected: expected_version,
            actual: current.version,
        });
    }

    // 继续更新...
}
```

**错误响应**:
```json
{
  "error": "optimistic_lock_failed",
  "message": "版本号冲突：期望版本 3，当前版本 5。请先刷新数据后重试。",
  "expected_version": 3,
  "actual_version": 5
}
```

---

## 5. 审计日志设计

### 5.1 审计事件类型

| 事件类型 | 描述 | 记录字段 |
|----------|------|----------|
| `credential_updated` | 凭证更新 | credential_id, old_version, new_version, change_reason |
| `credential_version_viewed` | 版本历史查询 | credential_id, viewed_version |
| `credential_rollback` | 版本回滚 | credential_id, from_version, to_version, reason |

### 5.2 审计日志表结构

```sql
-- audit_logs 表扩展字段
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS event_category VARCHAR(32);
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS metadata JSONB;

-- 索引
CREATE INDEX IF NOT EXISTS idx_audit_logs_category ON audit_logs(event_category);
CREATE INDEX IF NOT EXISTS idx_audit_logs_metadata ON audit_logs USING GIN (metadata);
```

### 5.3 审计日志记录示例

```rust
// 凭证更新审计
AuditEvent {
    event_type: "credential_updated",
    event_category: "version_control",
    tenant_id: "tenant_123",
    user_id_hash: "hash_user_456",
    credential_id: "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    metadata: json!({
        "old_version": 3,
        "new_version": 4,
        "change_reason": "密码轮换 - 定期更新",
        "algorithm": "AES-256-GCM"
    }),
    ip_address: "192.168.1.100",
    user_agent: "CredBridge-SDK-Rust/1.0.0",
    created_at: "2026-03-12T10:30:00Z"
}

// 回滚审计
AuditEvent {
    event_type: "credential_rollback",
    event_category: "version_control",
    tenant_id: "tenant_123",
    user_id_hash: "hash_user_456",
    credential_id: "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    metadata: json!({
        "from_version": 5,
        "to_version": 2,
        "reason": "安全审计要求恢复"
    }),
    ip_address: "192.168.1.100",
    user_agent: "CredBridge-Web/1.0.0",
    created_at: "2026-03-12T10:35:00Z"
}
```

### 5.4 版本差异审计

版本间差异比较在**元数据级别**进行：

```rust
// 版本差异结构
pub struct VersionDiff {
    pub credential_id: String,
    pub from_version: u32,
    pub to_version: u32,
    pub diff_type: DiffType,
    pub metadata_changes: Vec<MetadataChange>,
}

pub enum DiffType {
    ContentChanged,      // 加密内容变更
    ReasonChanged,       // 仅原因变更
    Rollback,            // 回滚操作
}

pub struct MetadataChange {
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

// 注意：由于加密特性，无法比较明文差异
// 只能报告"内容已变更"，无法报告具体变更了什么
```

---

## 6. 使用示例

### 6.1 cURL 示例

#### 创建凭证

```bash
curl -X POST https://api.credbridge.io/api/v1/credentials \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "service_id": "github",
    "credential_type": "api_key",
    "plaintext_data": { "api_key": "ghp_xxxxxxxxxxxx" }
  }'
```

#### 更新凭证

```bash
curl -X PUT https://api.credbridge.io/api/v1/credentials/$CREDENTIAL_ID \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "plaintext_data": { "api_key": "ghp_yyyyyyyyyyyy" },
    "change_reason": "密钥轮换 - 安全事件响应"
  }'
```

#### 查询版本历史

```bash
curl -X GET https://api.credbridge.io/api/v1/credentials/$CREDENTIAL_ID/versions \
  -H "Authorization: Bearer $TOKEN"
```

#### 查询指定版本

```bash
curl -X GET https://api.credbridge.io/api/v1/credentials/$CREDENTIAL_ID/versions/2 \
  -H "Authorization: Bearer $TOKEN"
```

#### 版本回滚

```bash
curl -X POST https://api.credbridge.io/api/v1/credentials/$CREDENTIAL_ID/rollback \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "target_version": 1,
    "reason": "恢复初始密钥"
  }'
```

### 6.2 TypeScript SDK 示例

```typescript
import { CredBridgeClient } from '@credbridge/sdk';

const client = new CredBridgeClient({
  apiUrl: 'https://api.credbridge.io',
  token: process.env.CRED_TOKEN,
});

// 创建凭证
const credential = await client.credentials.create({
  serviceId: 'github',
  credentialType: 'api_key',
  plaintextData: { api_key: 'ghp_xxxxxxxxxxxx' },
});

// 更新凭证（创建版本 2）
const updated = await client.credentials.update(credential.id, {
  plaintextData: { api_key: 'ghp_yyyyyyyyyyyy' },
  changeReason: '密钥轮换',
});

// 查询版本历史
const versions = await client.credentials.getVersions(credential.id);
console.log(`当前版本：${versions.currentVersion}`);

// 查询指定版本详情
const version2 = await client.credentials.getVersion(credential.id, 2);

// 版本回滚
const rollback = await client.credentials.rollback(credential.id, {
  targetVersion: 1,
  reason: '恢复初始密钥',
});
console.log(`已回滚到版本：${rollback.currentVersion}`);
```

### 6.3 Rust SDK 示例

```rust
use credbridge::{CredBridgeClient, CredentialType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = CredBridgeClient::builder()
        .api_url("https://api.credbridge.io")
        .token(std::env::var("CRED_TOKEN")?)
        .build()?;

    // 创建凭证
    let credential = client
        .credentials()
        .create()
        .service_id("github")
        .credential_type(CredentialType::ApiKey)
        .plaintext_data(serde_json::json!({
            "api_key": "ghp_xxxxxxxxxxxx"
        }))
        .send()
        .await?;

    // 更新凭证
    let updated = client
        .credentials()
        .update(&credential.id)
        .plaintext_data(serde_json::json!({
            "api_key": "ghp_yyyyyyyyyyyy"
        }))
        .change_reason("密钥轮换")
        .send()
        .await?;
    println!("更新到版本：{}", updated.version);

    // 查询版本历史
    let versions = client.credentials().get_versions(&credential.id).await?;
    println!("历史版本数：{}", versions.total);

    // 版本回滚
    let rollback = client
        .credentials()
        .rollback(&credential.id, 1)
        .reason("恢复初始密钥")
        .send()
        .await?;
    println!("回滚后版本：{}", rollback.current_version);

    Ok(())
}
```

---

## 7. 安全考虑

### 7.1 加密边界

- 版本历史存储**加密后的 payload**，不解密
- 回滚时直接复制加密数据，保持加密边界
- 仅用户有权在自己的会话中解密查看

### 7.2 权限控制

| 操作 | 所需 Scope | 说明 |
|------|------------|------|
| 更新凭证 | `credential:write` | 创建新版本 |
| 查询版本列表 | `credential:read` | 查看元数据 |
| 查询指定版本 | `credential:read` | 查看元数据 |
| 版本回滚 | `credential:write` | 创建回滚版本 |

### 7.3 租户隔离

- 版本数据存储在租户 Schema 内
- RLS 策略确保跨租户访问被拒绝
- 审计日志记录租户上下文

---

## 8. 性能考虑

### 8.1 索引策略

```sql
-- credential_versions 表索引
CREATE INDEX idx_credential_versions_credential_id ON credential_versions(credential_id);
CREATE INDEX idx_credential_versions_created_at ON credential_versions(created_at);
CREATE INDEX idx_credential_versions_changed_by ON credential_versions(changed_by);

-- 复合索引（常用查询）
CREATE INDEX idx_credential_versions_lookup
  ON credential_versions(credential_id, version DESC);
```

### 8.2 查询优化

```sql
-- 获取最新版本（使用索引）
SELECT * FROM credential_versions
WHERE credential_id = :id
ORDER BY version DESC
LIMIT 1;

-- 获取版本范围（使用索引）
SELECT * FROM credential_versions
WHERE credential_id = :id
  AND version BETWEEN 1 AND 10
ORDER BY version ASC;
```

### 8.3 存储估算

单个凭证版本大小估算：
- `encrypted_payload`: ~500 字节（JSON 格式）
- `metadata`: ~200 字节
- 索引开销：~100 字节
- **单版本总计**: ~800 字节

假设平均每凭证 10 个版本：
- **10,000 凭证**: ~80 MB
- **100,000 凭证**: ~800 MB

---

## 9. 迁移计划

### 9.1 数据库迁移

执行迁移脚本 `migrations/YYYYMMDDHHMMSS_add_credential_versioning.sql`

### 9.2 代码迁移

1. 更新 `src/vault/models.rs` - 添加 `version` 字段
2. 新增 `src/api/credentials_versioning.rs` - 版本 API
3. 更新 `src/vault/storage.rs` - 版本存储逻辑
4. 新增审计日志记录点

### 9.3 向后兼容

- 旧版本客户端仍可工作（version 默认为 1）
- API 端点保持向后兼容
- 渐进式部署策略

---

## 10. 验收标准

| 标准 | 状态 |
|------|------|
| 数据模型设计完整 | ✅ |
| API 设计符合 RESTful | ✅ |
| 版本历史可追溯 | ✅ |
| 回滚机制安全 | ✅ |
| 审计日志完整 | ✅ |
| 并发控制实现 | ✅ |
| 租户隔离保持 | ✅ |

---

## 附录 A: 相关文件

- `src/vault/models.rs` - VaultEntry 模型
- `src/api/credentials.rs` - 凭证 API
- `docker/scripts/init-postgres.sql` - 数据库初始化
- `migrations/YYYYMMDDHHMMSS_add_credential_versioning.sql` - 版本控制迁移

## 附录 B: 术语表

| 术语 | 定义 |
|------|------|
| VaultEntry | 凭证存储条目 |
| EncryptedPayload | 加密载荷（含 nonce, auth_tag, ciphertext） |
| Optimistic Lock | 乐观锁（版本号并发控制） |
| RLS | Row Level Security（行级安全） |

---

**文档结束**
