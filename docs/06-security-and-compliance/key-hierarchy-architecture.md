# 四层密钥层次架构

ToaniVault 实现了四层密钥层次架构，确保密钥材料的安全派生、存储和使用。

## 目录

- [密钥层次结构](#密钥层次结构)
- [密钥派生与 HKDF 算法](#密钥派生与-hkdf-算法)
- [密钥密封与存储](#密钥密封与存储)
- [密钥轮换](#密钥轮换)
- [密钥清理](#密钥清理)
- [相关文档](#相关文档)

---

## 密钥层次结构

### 四层架构概览

```
L0: Hardware Root Key (SGX Sealing Key)
  │ HKDF-Extract
  ▼
L1: Enclave Master Key (Enclave 内派生)
  │ HKDF-Expand(tenant_id + user_id)
  ▼
L2: User Vault Key (每用户独立，缓存 5 分钟)
  │ HKDF-Expand(credential_id + purpose)
  ▼
L3: Credential Encryption Key (每条凭证独立)
  │ AES-256-GCM
  ▼
Encrypted Credential
```

### 各层级详解

#### L0: Hardware Root Key（硬件根密钥）

**来源**: Intel SGX Sealing Key

**特性**:

- 从 SGX 硬件获取，基于 Enclave 的 MRENCLAVE/MRSIGNER
- 永不出 Enclave 边界
- 用于派生 L1 Master Key
- Enclave 重启后仍可恢复（相同 MRSIGNER）

**获取方式**:

```rust
use vault_service::tee::sealing::SealingService;

let sealing_service = SealingService::new();
let sealing_key = sealing_service.get_sealing_key()
    .expect("Failed to get sealing key");
```

#### L1: Enclave Master Key（Enclave 主密钥）

**来源**: 从 L0 Sealing Key 通过 HKDF-Extract 派生

**特性**:

- Enclave 初始化时派生
- 生命周期与 Enclave 相同
- 用于派生所有 L2 User Vault Keys
- 可选密封存储（绑定到 MRSIGNER）

**派生流程**:

```rust
use vault_service::crypto::hkdf::Hkdf;

// 从 Sealing Key 派生 Master Key
let hkdf = Hkdf::new(Some(salt), &sealing_key)?;
let mut master_key = vec![0u8; 32];
hkdf.expand(b" ToaniVault Master Key", &mut master_key)?;
```

#### L2: User Vault Key（用户保险库密钥）

**来源**: 从 L1 Master Key 通过 HKDF-Expand 派生

**派生路径**: `HKDF-Expand(L1, tenant_id + user_id)`

**特性**:

- 每个用户独立密钥
- 缓存 5 分钟（TTL）
- 用于派生 L3 Credential Keys
- 过期后自动清理

**派生示例**:

```rust
use vault_service::tee::KeyManager;

let manager = KeyManager::new(&master_key);

// 派生用户密钥（自动缓存）
let user_key = manager.derive_user_vault_key(
    "tenant_123",
    "user_456"
)?;

// 缓存 5 分钟后自动过期
```

#### L3: Credential Encryption Key（凭证加密密钥）

**来源**: 从 L2 User Vault Key 通过 HKDF-Expand 派生

**派生路径**: `HKDF-Expand(L2, credential_id + purpose)`

**特性**:

- 每条凭证独立密钥
- 单次使用，用完即焚
- 用于 AES-256-GCM 加密凭证数据
- 不缓存，即时派生

**派生示例**:

```rust
use vault_service::crypto::keys::KeyPurpose;

let credential_key = manager.derive_credential_key(
    user_key,
    "cred_789",
    KeyPurpose::CredentialEncryption
)?;

// 使用后立即清理
let encrypted = aes_gcm_encrypt(&credential_key, plaintext)?;
zeroize(&credential_key); // 立即清零
```

### 密钥层次特性对比

| 层级   | 名称               | 生命周期         | 缓存 | 用途     | 派生方式                  |
| ------ | ------------------ | ---------------- | ---- | -------- | ------------------------- |
| **L0** | Hardware Root Key  | Enclave 生命周期 | 否   | 根密钥   | SGX Sealing               |
| **L1** | Enclave Master Key | Enclave 生命周期 | 是   | 派生 L2  | HKDF-Extract              |
| **L2** | User Vault Key     | 5 分钟 TTL       | 是   | 派生 L3  | HKDF-Expand(tenant+user)  |
| **L3** | Credential Key     | 单次使用         | 否   | 加密凭证 | HKDF-Expand(cred+purpose) |

---

## 密钥派生与 HKDF 算法

### HKDF（HMAC-based Extract-and-Expand Key Derivation Function）

ToaniVault 使用 HKDF-SHA-256 进行密钥派生，符合 RFC 5869 标准。

### HKDF 两个阶段

#### 1. Extract（提取）

从输入密钥材料中提取固定长度的伪随机密钥（PRK）：

```rust
use vault_service::crypto::hkdf::Hkdf;
use ring::hkdf::HKDF_SHA256;

// Extract 阶段
// salt: 可选的随机盐值（增加随机性）
// ikm: 输入密钥材料（Input Key Material）
let hkdf = Hkdf::new(Some(salt), &ikm)?;
```

**参数说明**:

- **Salt**: 可选的随机值，增加派生密钥的随机性
- **IKM**: 输入密钥材料（如 Sealing Key）
- **输出**: PRK（Pseudorandom Key）

#### 2. Expand（扩展）

从 PRK 扩展出所需长度的输出密钥材料（OKM）：

```rust
// Expand 阶段
// info: 上下文信息（用于派生不同用途的密钥）
// length: 输出密钥长度
let mut okm = vec![0u8; 32];
hkdf.expand(info, &mut okm)?;
```

**参数说明**:

- **Info**: 上下文信息，如 `tenant_id + user_id`
- **Length**: 输出密钥长度（通常 32 字节）
- **Output**: OKM（Output Key Material）

### 完整派生流程示例

```rust
use vault_service::crypto::hkdf::Hkdf;

// L0 → L1: 从 Sealing Key 派生 Master Key
let salt = b"ToaniVault L1 Salt";
let ikm = &sealing_key; // L0
let hkdf = Hkdf::new(Some(salt), ikm)?;

let mut master_key = vec![0u8; 32]; // L1
hkdf.expand(b"ToaniVault Master Key", &mut master_key)?;

// L1 → L2: 从 Master Key 派生 User Vault Key
let tenant_id = b"tenant_123";
let user_id = b"user_456";
let info = [tenant_id, user_id].concat(); // 上下文信息

let mut user_key = vec![0u8; 32]; // L2
hkdf.expand(&info, &mut user_key)?;

// L2 → L3: 从 User Key 派生 Credential Key
let credential_id = b"cred_789";
let purpose = b"encryption";
let info = [credential_id, purpose].concat();

let mut credential_key = vec![0u8; 32]; // L3
hkdf.expand(&info, &mut credential_key)?;
```

### 密钥派生安全性

**设计原则**:

1. **前向安全性**: 即使 L2 泄露，不影响其他用户的 L2
2. **后向安全性**: 即使 L3 泄露，不影响 L2 和其他 L3
3. **密钥隔离**: 不同用途的密钥使用不同的 info 参数
4. **随机盐值**: L1 派生使用随机 salt，防止彩虹表攻击

**Info 参数设计**:

```rust
// 用户密钥派生
info = tenant_id || user_id

// 凭证密钥派生
info = credential_id || purpose

// 不同用途分离
purpose = "encryption" | "authentication" | "signing"
```

---

## 密钥密封与存储

### SGX Sealing（密封）机制

Sealing 允许将敏感数据加密持久化到磁盘，且只能在相同 Enclave 中解封。

### 密封策略

#### 1. MRENCLAVE（严格模式）

仅当前版本的 Enclave 可解封：

```rust
use vault_service::tee::SealPolicy;

let policy = SealPolicy::Mrenclave;
let sealed = sealing_service.seal_data(plaintext, aad, policy)?;

// 仅相同 MRENCLAVE 的 Enclave 可解封
// 适用于：安全要求极高的场景
```

**特性**:

- 绑定到特定 Enclave 版本
- Enclave 更新后无法解封旧数据
- 最高安全性

#### 2. MRSIGNER（兼容模式，推荐）

同一签名者的不同版本 Enclave 可解封：

```rust
use vault_service::tee::SealPolicy;

let policy = SealPolicy::Mrsigner;
let sealed = sealing_service.seal_data(plaintext, aad, policy)?;

// 同一签名者的不同 Enclave 版本可解封
// 适用于：生产环境（允许版本升级）
```

**特性**:

- 绑定到签名者身份
- 允许 Enclave 版本升级
- 平衡安全性和可维护性

### 密封存储流程

```rust
use vault_service::tee::{SealingService, SealPolicy};

let service = SealingService::new();

// 密封数据
let plaintext = b"sensitive master key";
let aad = b"additional authenticated data"; // 可选的附加认证数据

let sealed = service.seal_data(
    plaintext,
    aad,
    SealPolicy::Mrsigner
)?;

// 持久化到磁盘
std::fs::write(".sealed_master_key", sealed)?;

// ... Enclave 重启后 ...

// 从磁盘读取
let sealed = std::fs::read(".sealed_master_key")?;

// 解封数据
let decrypted = service.unseal_data(&sealed)?;
assert_eq!(decrypted, plaintext);
```

### 密封数据格式

```
┌─────────────────────────────────────────────────────────┐
│              Sealed Data Structure                       │
├─────────────────────────────────────────────────────────┤
│  Header (32 bytes)                                      │
│  ├─ Magic Number (4 bytes): "CRED"                      │
│  ├─ Version (4 bytes): 1                                │
│  ├─ Policy (4 bytes): 0=Mrenclave, 1=Mrsigner          │
│  ├─ Reserved (20 bytes)                                 │
├─────────────────────────────────────────────────────────┤
│  SGX Sealed Data (variable)                             │
│  ├─ KeyID (32 bytes)                                    │
│  ├─ MAC (16 bytes)                                      │
│  ├─ IV (12 bytes)                                       │
│  ├─ Ciphertext (variable)                               │
│  └─ AAD (optional)                                      │
└─────────────────────────────────────────────────────────┘
```

### L1 Master Key 密封存储

Enclave 关闭前密封 L1 Master Key：

```rust
use vault_service::tee::KeyManager;

let manager = KeyManager::new(&sealing_key);

// Enclave 关闭前
manager.seal_master_key(&master_key_material, SealPolicy::Mrsigner)?;

// Enclave 重启后
let (recovered_key, metadata) = manager.restore_master_key()?;

// 自动验证 MRSIGNER 兼容性
```

---

## 密钥轮换

### 密钥轮换策略

| 密钥层级 | 轮换触发条件    | 轮换方式        |
| -------- | --------------- | --------------- |
| **L0**   | Enclave 重启    | 自动从 SGX 获取 |
| **L1**   | Enclave 重启    | 从 L0 重新派生  |
| **L2**   | 5 分钟 TTL 过期 | 自动重新派生    |
| **L3**   | 每次使用        | 即时派生        |

### L1 Master Key 轮换

Enclave 重启时自动轮换：

```rust
// Enclave 重启
let mut enclave = Enclave::new(config);
enclave.initialize()?;

// 自动从新的 Sealing Key 派生新的 Master Key
// 旧数据需要重新加密（如果使用旧的 MRENCLAVE 策略）
```

### L2 User Key 轮换

TTL 过期后自动轮换：

```rust
use vault_service::tee::UserKeyCache;

let mut cache = UserKeyCache::new(300); // 5 分钟 TTL

// 第一次访问：派生并缓存
let key1 = cache.get_or_derive(tenant_id, user_id, &master_key)?;

// 5 分钟后...
// 自动过期，下次访问重新派生
let key2 = cache.get_or_derive(tenant_id, user_id, &master_key)?;
// key2 != key1（不同的密钥材料）
```

### 凭证密钥轮换

凭证更新时自动轮换 L3 密钥：

```rust
// 更新凭证
let new_plaintext = b"new-password";
let new_encrypted = enclave.encrypt_credential(
    tenant_id,
    user_id,
    credential_id,
    new_plaintext
)?;

// 自动使用新的 L3 密钥加密
// 旧的 L3 密钥已销毁
```

### 密钥版本管理

```rust
use vault_service::crypto::keys::KeyMetadata;

let metadata = KeyMetadata {
    version: 1,
    created_at: timestamp,
    expires_at: timestamp + 300, // 5 分钟
    purpose: KeyPurpose::CredentialEncryption,
    parent_key_id: "user_vault_key_v1",
};
```

---

## 密钥清理

### 自动清理机制

#### ZeroizeOnDrop

所有密钥结构体实现 `ZeroizeOnDrop`：

```rust
use zeroize::ZeroizeOnDrop;
use vault_service::crypto::keys::CredentialKey;

{
    let key = CredentialKey::new(
        key_material,
        tenant_id.to_string(),
        user_hash.to_string(),
        credential_id.to_string(),
        KeyPurpose::CredentialEncryption,
        timestamp,
    );

    // 使用密钥...

} // 离开作用域时，key_material 自动被 zeroize
```

#### TTL 缓存清理

```rust
use vault_service::tee::UserKeyCache;

let mut cache = UserKeyCache::new(300); // 5 分钟 TTL

// 过期后自动清理
cache.cleanup_expired(); // 移除所有过期条目并 zeroize
```

#### 后台调度器

```rust
use vault_service::tee::{CleanupScheduler, CleanupConfig};
use std::time::Duration;

let config = CleanupConfig {
    cleanup_interval: Duration::from_secs(60), // 每分钟清理
    deep_cleanup_on_drop: true,
    verify_cleanup: false,
    ..Default::default()
};

let mut scheduler = CleanupScheduler::new(config);
scheduler.start(cache.clone());

// 定期清理过期密钥
```

### 手动清理

```rust
use vault_service::tee::KeyCleaner;

// 清理字节数组
let mut sensitive_data = vec![0x42u8; 32];
KeyCleaner::clear_bytes(&mut sensitive_data);

// 深度清理（多次覆写）
let mut key_material = [0x42u8; 32];
KeyCleaner::deep_clear(&mut key_material);
// 执行 3 次覆写：0x00, 0xFF, 0x00

// 清理并验证
let mut data = vec![0x42u8; 16];
assert!(KeyCleaner::clear_and_verify(&mut data));
```

---

## 相关文档

- [TEE 安全架构](tee-security-architecture.md) - Enclave 设计
- [加密算法实现](加密算法实现.md) - AES-256-GCM 加密
- [密钥派生](密钥派生.md) - HKDF 详细实现
- [密钥清理指南](密钥清理指南.md) - 密钥清理最佳实践
- [内存安全与密钥清理](../../README.md#内存安全与密钥清理-story-14) - 主文档

---

**文档版本**: v1.0  
**最后更新**: 2026-03-20
