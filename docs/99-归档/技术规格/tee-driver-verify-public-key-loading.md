---
title: 'TEE 驱动验证：从安全存储加载公钥'
slug: 'tee-driver-verify-public-key-loading'
created: '2026-03-19'
status: 'implemented'
priority: 'P2'
---

# Tech-Spec: TEE 驱动验证——从安全存储加载公钥

## 概述

### 问题陈述

`src/tee/driver_verify.rs` 中的 `get_trusted_public_key()` 函数（行 354–361）是驱动完整性验证的核心信任锚点，但当前实现为空：

```rust
fn get_trusted_public_key(_fingerprint: &str) -> Option<Vec<u8>> {
    // TODO(#TEE-301): 从安全存储加载公钥
    // 需要: 集成 Vault 或 HSM 密钥存储服务
    None
}
```

该函数始终返回 `None`，意味着 `verify_driver_signature()` 每次都会因"未找到受信任的公钥"而失败，驱动签名验证完全无法工作。攻击者可通过任意驱动绕过完整性验证（因为验证路径永远报错，上层若捕获错误而放行，则构成安全漏洞）。

### 解决方案

实现 `get_trusted_public_key()` 从 Vault（或 HSM）动态查询受信任公钥：
1. 以 `fingerprint` 为 Vault 路径键，查询对应的 DER 格式 ECDSA P-256 公钥
2. 实现内存缓存（TTL 5 分钟），避免每次驱动加载都向 Vault 发请求
3. 缓存过期或 Vault 不可达时，拒绝验证（Fail-Closed 策略）

因 `get_trusted_public_key` 当前是同步函数但 Vault 查询需要异步，需重构为 `async fn` 或通过 `tokio::runtime::Handle::block_on` 桥接（推荐前者）。

### 范围

- **包含**：`get_trusted_public_key` 重构为异步 + Vault 集成 + 内存缓存
- **不包含**：`TeeDriverVerifier` 结构体的其他验证逻辑（哈希验证、版本白名单均已实现）、Vault 初始化配置

---

## 开发上下文

### 当前代码（关键片段）

**TEE-301：`get_trusted_public_key`（行 354–361）**

```rust
/// 获取受信任的公钥
///
/// 在实际实现中，这应该从安全的密钥存储中获取
fn get_trusted_public_key(_fingerprint: &str) -> Option<Vec<u8>> {
    // TODO(#TEE-301): 从安全存储加载公钥
    // 需要: 集成 Vault 或 HSM 密钥存储服务
    None
}
```

**调用处：`verify_driver_signature`（行 337–344）**

```rust
let public_key = get_trusted_public_key(signer_fingerprint).ok_or_else(|| {
    DriverVerifyError::SignatureVerificationFailed(format!(
        "未找到受信任的公钥：{}",
        signer_fingerprint
    ))
})?;

let unparsed_key = UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, &public_key);

match unparsed_key.verify(hash.as_ref(), &signature) {
    Ok(_) => Ok(true),
    Err(e) => Err(DriverVerifyError::SignatureVerificationFailed(
        e.to_string(),
    )),
}
```

**`TeeDriverVerifier` 结构体（行 ~130–180，含 `trusted_keys` 内存列表）**

```rust
pub struct TeeDriverVerifier {
    config: VerifierConfig,
    trusted_keys: Vec<Vec<u8>>,       // 内存静态信任公钥列表（当前替代方案）
    allowed_versions: Vec<String>,
    last_verification: Option<u64>,
    verification_count: u64,
}
```

`TeeDriverVerifier` 已有 `add_trusted_key()` 方法，接受静态内存公钥。TEE-301 的目标是让 `get_trusted_public_key()` 动态从 Vault 查询，两者可以并存（先查 Vault，Vault 不可达时降级为内存列表）。

### 完整驱动验证流程（上下文）

```
verify_driver()（对外入口）
    │
    ├─ DriverMetadata::from_file()        # 加载元数据（name/version/hash/signer_fingerprint）
    ├─ metadata.validate()                # 基础字段校验
    ├─ check_driver_hash()                # SHA256 哈希比对（已实现）
    ├─ check_driver_version()             # 版本白名单（已实现）
    └─ verify_driver_signature()          # 签名验证（调用 get_trusted_public_key）
           │
           └─ get_trusted_public_key(fingerprint)  ← TEE-301（当前返回 None）
                  │
                  └─ UnparsedPublicKey::new(ECDSA_P256_SHA256_ASN1, &public_key)
                         └─ unparsed_key.verify(hash, signature)
```

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `src/tee/driver_verify.rs` | 主实现文件，TEE-301 在此 |
| `src/vault/` | Vault 客户端，查询公钥的底层接口 |
| `src/tee/upgrade.rs` | 同级 TEE 模块，参考错误处理和 Vault 调用模式 |
| `Cargo.toml` | 确认 `vaultrs` 或自定义 Vault client crate 版本 |

### 技术决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 存储后端 | HashiCorp Vault KV v2 | 项目已有 Vault 集成；审计日志完善 |
| Vault 路径约定 | `secret/tee/trusted-keys/<fingerprint>` | 与其他 TEE 密钥路径命名一致 |
| 存储格式 | DER 编码的 ECDSA P-256 公钥（`Vec<u8>`） | 与 `ring::signature::UnparsedPublicKey` 直接兼容 |
| 缓存策略 | `DashMap<String, (Vec<u8>, Instant)>`，TTL 5 分钟 | 读多写少，DashMap 无锁并发；5 分钟平衡安全性与性能 |
| 函数签名重构 | 改为 `async fn`，调用方链式 `.await` | 比 `block_on` 更 Rust 原生；调用栈已在 async 上下文 |
| Vault 不可达时策略 | Fail-Closed（返回 `None`，拒绝验证） | 安全优先；避免降级攻击 |

---

## 实现计划

### 任务

#### 任务 1：定义公钥缓存结构

在 `driver_verify.rs` 顶部（imports 区域之后）添加全局缓存：

```rust
use dashmap::DashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// 公钥缓存条目
struct CachedPublicKey {
    key_bytes: Vec<u8>,
    cached_at: Instant,
}

/// 全局公钥缓存（fingerprint → CachedPublicKey）
static PUBLIC_KEY_CACHE: LazyLock<DashMap<String, CachedPublicKey>> =
    LazyLock::new(DashMap::new);

const PUBLIC_KEY_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
```

#### 任务 2：重构 `get_trusted_public_key` 为 async

**当前签名**：
```rust
fn get_trusted_public_key(_fingerprint: &str) -> Option<Vec<u8>>
```

**新签名**：
```rust
async fn get_trusted_public_key(fingerprint: &str) -> Option<Vec<u8>>
```

**实现逻辑**：
1. 检查缓存：若缓存命中且未过期，直接返回 `key_bytes.clone()`
2. 缓存未命中或已过期：向 Vault 发起查询
3. Vault 查询成功：写入缓存，返回公钥字节
4. Vault 查询失败：记录警告日志，返回 `None`（Fail-Closed）

**伪代码结构**：
```rust
async fn get_trusted_public_key(fingerprint: &str) -> Option<Vec<u8>> {
    // 1. 缓存查询
    if let Some(entry) = PUBLIC_KEY_CACHE.get(fingerprint) {
        if entry.cached_at.elapsed() < PUBLIC_KEY_CACHE_TTL {
            return Some(entry.key_bytes.clone());
        }
    }

    // 2. Vault 查询
    let vault_path = format!("secret/tee/trusted-keys/{}", fingerprint);
    match vault_client::get_secret(&vault_path).await {
        Ok(key_bytes) => {
            PUBLIC_KEY_CACHE.insert(fingerprint.to_string(), CachedPublicKey {
                key_bytes: key_bytes.clone(),
                cached_at: Instant::now(),
            });
            Some(key_bytes)
        }
        Err(e) => {
            tracing::warn!("Vault 公钥查询失败 fingerprint={}: {}", fingerprint, e);
            None  // Fail-Closed
        }
    }
}
```

#### 任务 3：更新调用链为 async

`verify_driver_signature` 需改为 `async fn`，同时其调用链向上更新：
- `verify_driver_signature()` → `async fn`
- `verify()` / `verify_driver()` → 检查是否需要 `.await`

#### 任务 4：Vault 数据写入（初始化脚本）

提供一次性管理脚本（非代码变更），将信任公钥写入 Vault：

```bash
# 将 DER 格式公钥写入 Vault
vault kv put secret/tee/trusted-keys/<fingerprint> \
  key_der="$(base64 < trusted_key.der)"
```

Vault 存储格式：
```json
{
  "key_der": "<base64-encoded DER>",
  "algorithm": "ECDSA_P256",
  "created_at": "2026-03-19T00:00:00Z",
  "description": "TEE 驱动签名验证信任公钥"
}
```

`get_trusted_public_key` 从响应中读取 `key_der` 字段，base64 解码后返回。

---

### 验收标准

**功能验收**：
- [ ] Vault 中存在对应 `fingerprint` 的公钥时，`verify_driver_signature()` 完成签名验证并返回 `Ok(true)`
- [ ] 缓存生效：同一 `fingerprint` 在 TTL 内第二次调用不触发 Vault 请求（可通过计数器或 mock 验证）
- [ ] TTL 过期后，自动重新向 Vault 查询

**安全验收**：
- [ ] Vault 不可达时，`get_trusted_public_key()` 返回 `None`，`verify_driver_signature()` 返回 `Err(SignatureVerificationFailed)`，不静默放行
- [ ] 不存在的 `fingerprint` 查询，Vault 返回 404，函数返回 `None`，拒绝验证
- [ ] 缓存中的公钥字节不在日志中打印（防密钥泄露）

**回归验收**：
- [ ] 现有测试 `test_secure_hash_compare`、`test_verifier_config_default`、`test_verifier_status` 全部通过
- [ ] `TeeDriverVerifier::add_trusted_key()` 的内存信任列表路径仍可正常工作（两路径并存，用于测试环境）

---

## 附加上下文

### 依赖

| Crate | 版本 | 用途 |
|-------|------|------|
| `dashmap` | 已有或添加 | 无锁并发缓存 |
| `std::sync::LazyLock` | Rust 1.80+ | 全局静态缓存初始化 |
| 现有 Vault client | — | `get_secret(path)` 接口 |
| `tracing` | 已有 | 缓存命中/未命中、Vault 错误日志 |
| `base64` | 已有 | Vault 返回值 base64 解码 |

### 测试策略

- **单元测试**：mock Vault client，注入预设公钥，验证签名验证通过
- **单元测试**：mock Vault client 返回错误，验证 Fail-Closed 行为
- **缓存测试**：设置极短 TTL（1ms），验证过期后触发重新查询
- **负面测试**：篡改驱动文件内容，验证哈希/签名验证失败返回正确错误类型

### 注意事项

1. **函数签名传染性**：将 `get_trusted_public_key` 改为 `async fn` 会向上传染调用链。需确认 `verify_driver_signature` 的所有调用方已在 async 上下文中，否则需要 `tokio::task::block_in_place` 桥接。
2. **缓存失效边界**：TTL 5 分钟的选择基于"公钥轮换频率极低"的假设。若业务需要更快的公钥撤销（如应急场景），可提供 `invalidate_key_cache(fingerprint)` 接口强制清除。
3. **Vault 路径权限**：TEE 服务的 Vault Policy 需要对 `secret/tee/trusted-keys/*` 有 `read` 权限，确保与运维团队对齐。
4. **DER vs PEM**：`ring::signature::UnparsedPublicKey` 接受 DER 格式（SubjectPublicKeyInfo），Vault 存储 base64(DER)，避免 PEM 解析复杂度。
