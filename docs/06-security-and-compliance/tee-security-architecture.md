# TEE 安全架构

CredBridge 采用 Intel SGX（Software Guard Extensions）可信执行环境（TEE）技术，为敏感操作提供硬件级别的安全隔离。

## 目录

- [Enclave 设计原理](#enclave-设计原理)
- [安全边界和隔离](#安全边界和隔离)
- [Enclave 生命周期管理](#enclave-生命周期管理)
- [内存安全](#内存安全)
- [相关文档](#相关文档)

---

## Enclave 设计原理

### 什么是 Enclave

Enclave 是 Intel SGX 提供的可信执行环境，是一个受保护的内存区域。在 Enclave 内部执行的代码和数据受到硬件保护，即使操作系统内核或虚拟机监控器也无法访问。

### Enclave 内存布局

```
┌─────────────────────────────────────────────────────────┐
│                    系统内存（不可信）                     │
│                                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │              Enclave（可信区域）                   │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │         EPC（Enclave Page Cache）            │  │  │
│  │  │  ┌──────────────┐  ┌──────────────┐        │  │  │
│  │  │  │ 代码段       │  │ 数据段       │        │  │  │
│  │  │  │ (Code)       │  │ (Data)       │        │  │  │
│  │  │  └──────────────┘  └──────────────┘        │  │  │
│  │  │  ┌──────────────┐  ┌──────────────┐        │  │  │
│  │  │  │ 堆栈         │  │ 堆           │        │  │  │
│  │  │  │ (Stack)      │  │ (Heap)       │        │  │  │
│  │  │  └──────────────┘  └──────────────┘        │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  │                                                   │  │
│  │  • 内存加密：所有 Enclave 页面经过硬件加密         │  │
│  │  • 访问控制：只有 Enclave 内部代码可访问           │  │
│  │  • 完整性保护：MRENCLAVE 测量值验证               │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
│  • 操作系统无法访问 Enclave 内存                         │
│  • 物理攻击无法读取加密内存                             │
└─────────────────────────────────────────────────────────┘
```

### 可信执行环境边界

CredBridge 的 TEE 边界定义如下：

| 区域             | 可信度    | 说明                           |
| ---------------- | --------- | ------------------------------ |
| **Enclave 内部** | ✅ 可信   | 密钥派生、加密/解密、敏感操作  |
| **Enclave 外部** | ❌ 不可信 | API 网关、业务逻辑、数据库访问 |

**关键原则**：

- 密钥材料永不出 Enclave 边界
- 所有加密操作在 Enclave 内完成
- 外部只能访问加密后的数据

### 内存隔离机制

Intel SGX 提供以下内存隔离保护：

1. **内存加密**
   - 使用 128 位 AES 加密引擎
   - 每个内存页面独立加密
   - 加密密钥存储在 CPU 内部

2. **访问控制**
   - 只有 Enclave 内部代码可访问 Enclave 内存
   - 外部访问会触发 CPU 异常
   - 页表修改不影响 Enclave 访问

3. **完整性保护**
   - 每个 Enclave 有唯一的 MRENCLAVE 测量值
   - 启动时验证代码完整性
   - 运行时检测篡改行为

---

## 安全边界和隔离

### 多层隔离架构

CredBridge 实现多层安全隔离：

```
┌─────────────────────────────────────────────────────────────────┐
│                    CredBridge 安全隔离层次                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  第 1 层：硬件隔离（Intel SGX）                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Enclave 边界 - 硬件级别的内存加密和隔离                   │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                  │
│  第 2 层：沙箱隔离（nsjail）                                     │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  • PID Namespace    • Network Namespace                  │  │
│  │  • Mount Namespace  • IPC Namespace                       │  │
│  │  • UTS Namespace    • cgroups v2 资源限制                │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                  │
│  第 3 层：系统调用过滤（seccomp-bpf）                            │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  白名单机制：仅允许 ~50 个安全系统调用                      │  │
│  │  阻止：ptrace, mount, reboot 等危险调用                   │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                  │
│  第 4 层：凭证命名空间隔离                                        │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  每个会话独立的凭证访问空间                                │  │
│  │  会话间凭证完全隔离                                        │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 通信控制

Enclave 与外部的通信经过严格控制：

```rust
// Enclave 入口点（ECALL）
pub enum EnclaveCall {
    // 密钥操作
    DeriveKey { tenant_id, user_id },
    EncryptData { plaintext, key_id },
    DecryptData { ciphertext, key_id },

    // 认证操作
    GenerateQuote { challenge },
    VerifySignature { data, signature },

    // 管理操作
    Initialize { config },
    Shutdown { },
}

// Enclave 出口点（OCALL）
pub enum EnclaveExit {
    // 有限的系统调用
    WriteLog { message },
    GetTime { },
    RandomBytes { length },
}
```

**安全原则**：

- ECALL 参数经过严格验证
- OCALL 仅限于必要的系统调用
- 所有调用记录审计日志

### 沙箱安全隔离

TEE Sandbox 提供额外的安全隔离层：

| 隔离机制       | 实现                  | 保护目标                 |
| -------------- | --------------------- | ------------------------ |
| **Namespace**  | PID/Net/Mount/IPC/UTS | 进程、网络、文件系统隔离 |
| **cgroups v2** | CPU/内存/IO 限制      | 资源滥用防护             |
| **seccomp**    | 系统调用白名单        | 内核攻击面最小化         |
| **凭证隔离**   | 每会话独立命名空间    | 凭证泄露隔离             |

### 密钥清理和内存安全

所有密钥材料遵循严格的生命周期管理：

```rust
use zeroize::ZeroizeOnDrop;

// 密钥结构体实现自动清理
#[derive(ZeroizeOnDrop)]
struct MasterKey {
    key_material: Vec<u8>,  // 离开作用域时自动清零
}

// TTL 缓存自动过期
let mut cache = UserKeyCache::new(300); // 5 分钟 TTL
// 过期后自动 zeroize 并移除

// 后台清理调度器
let scheduler = CleanupScheduler::new(config);
scheduler.start(cache); // 定期清理过期密钥
```

**清理策略**：

- **L1 Master Key**: Enclave 生命周期内持久化，关闭时清理
- **L2 User Key**: 5 分钟 TTL，过期自动清理
- **L3 Credential Key**: 单次使用，用完即焚

---

## Enclave 生命周期管理

### 状态机

Enclave 遵循严格的状态转换：

```
┌─────────────────────────────────────────────────────────┐
│              Enclave 状态机                              │
├─────────────────────────────────────────────────────────┤
│                                                         │
│   ┌──────────────┐                                     │
│   │Uninitialized │                                     │
│   └──────┬───────┘                                     │
│          │ initialize()                                 │
│          ▼                                              │
│   ┌──────────────┐                                     │
│   │Initializing  │                                     │
│   └──────┬───────┘                                     │
│          │ 初始化完成                                   │
│          ▼                                              │
│   ┌──────────────┐                                     │
│   │   Running    │ ◄──────┐                            │
│   └──────┬───────┘        │                            │
│          │                │ pause()/resume()            │
│          │ shutdown()     │                            │
│          ▼                │                            │
│   ┌──────────────┐  ┌──────────────┐                  │
│   │ShuttingDown  │─►│  Shutdown    │                  │
│   └──────────────┘  └──────────────┘                  │
│                                                         │
│   任何状态都可能因错误进入 Error 状态                      │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Enclave 创建

```rust
use vault_service::tee::{Enclave, EnclaveConfig};

// 配置 Enclave
let config = EnclaveConfig {
    // 用户密钥缓存 TTL（秒）
    user_key_ttl: 300,  // 5 分钟

    // 密封策略
    seal_policy: SealPolicy::Mrsigner,  // 或 Mrenclave

    // 密封数据存储路径
    sealed_storage_path: ".sealed".to_string(),

    // Enclave 名称
    name: "credbridge-enclave".to_string(),

    // 调试模式（仅开发使用）
    debug_mode: false,
};

// 创建 Enclave 实例
let mut enclave = Enclave::new(config);
```

### Enclave 初始化

```rust
// 初始化 Enclave
enclave.initialize()
    .expect("Failed to initialize enclave");

// 初始化流程：
// 1. 从 SGX 硬件获取 Sealing Key（L0）
// 2. 使用 HKDF 派生 Master Key（L1）
// 3. 验证 MRSIGNER/MRENCLAVE
// 4. 从密封存储恢复（如果有）
// 5. 进入 Running 状态
```

### Enclave 运行

在 Running 状态下，Enclave 提供以下服务：

```rust
// 加密凭证
let encrypted = enclave.encrypt_credential(
    "tenant_123",      // 租户 ID
    "user_456",        // 用户 ID
    "cred_789",        // 凭证 ID
    plaintext
)?;

// 解密凭证
let decrypted = enclave.decrypt_credential(
    "tenant_123",
    "user_456",
    "cred_789",
    &encrypted
)?;

// 派生用户密钥
let user_key = enclave.derive_user_vault_key(
    "tenant_123",
    "user_456"
)?;

// 生成 Quote（用于远程认证）
let quote = enclave.generate_quote(&challenge)?;
```

### Enclave 关闭

```rust
// 正常关闭
enclave.shutdown()
    .expect("Failed to shutdown enclave");

// 关闭流程：
// 1. 停止接受新请求
// 2. 等待进行中的操作完成
// 3. 清理所有缓存的密钥（L2/L3）
// 4. 密封 L1 Master Key（可选）
// 5. 覆写敏感内存
// 6. 进入 Shutdown 状态
```

### Enclave 重启恢复

Enclave 重启后可以从密封存储恢复：

```rust
// 重启后重新初始化
let mut enclave = Enclave::new(config);
enclave.initialize()?;

// 自动从密封存储恢复 L1 Master Key
// 验证 MRSIGNER 兼容性
// 恢复后缓存状态清空，需要重新派生
```

---

## 内存安全

### ZeroizeOnDrop 自动清理

所有包含密钥的结构体都实现了 `ZeroizeOnDrop` trait：

```rust
use zeroize::ZeroizeOnDrop;
use vault_service::crypto::keys::{MasterKey, UserVaultKey, CredentialKey};

// 密钥离开作用域时自动清零
{
    let key = CredentialKey::new(
        key_material,
        tenant_id,
        user_id,
        credential_id,
        purpose,
        timestamp,
    );

    // 使用密钥加密/解密...

} // key 自动被 zeroize，key_material 被覆写为零
```

### TTL 缓存自动过期

用户密钥缓存（L2）具有 5 分钟 TTL：

```rust
use vault_service::tee::{UserKeyCache, CachedKeyEntry, KeyType};

let mut cache = UserKeyCache::new(300); // 5 分钟 TTL

// 添加条目
let entry = CachedKeyEntry::new(
    handle,
    "tenant_1".to_string(),
    "user_hash".to_string(),
    key_material,
    KeyType::UserVault,
);
cache.insert("tenant_1", "user_hash", entry);

// 过期后自动清理
// 调用 get() 时检测过期并触发 zeroize
cache.get("tenant_1", "user_hash"); // 返回 None 如果已过期
```

### 密钥清理调度器

后台线程定期清理过期密钥：

```rust
use vault_service::tee::{CleanupScheduler, CleanupConfig};
use std::time::Duration;

let config = CleanupConfig {
    cleanup_interval: Duration::from_secs(60), // 每分钟清理一次
    deep_cleanup_on_drop: true,
    verify_cleanup: false,
    ..Default::default()
};

let mut scheduler = CleanupScheduler::new(config);
scheduler.start(cache.clone());

// 停止调度器
scheduler.stop();
```

### 受保护内存区域

敏感数据存储在受保护的内存区域：

```rust
use vault_service::tee::ProtectedMemory;

// 创建受保护内存
let mut memory = ProtectedMemory::new(64, "master_key_buffer");

// 使用内存
memory.as_mut_slice().copy_from_slice(&key_data);

// 自动清理（drop 时）
drop(memory);

// 或手动清理
memory.secure_clear();
```

### 手动密钥清理

提供手动清理工具：

```rust
use vault_service::tee::KeyCleaner;

// 清理字节数组
let mut sensitive_data = vec![0x42u8; 32];
KeyCleaner::clear_bytes(&mut sensitive_data);

// 深度清理（多次覆写）
let mut key_material = [0x42u8; 32];
KeyCleaner::deep_clear(&mut key_material);

// 清理并验证
let mut data = vec![0x42u8; 16];
assert!(KeyCleaner::clear_and_verify(&mut data));
```

---

## 相关文档

- [密钥层次架构](key-hierarchy-architecture.md) - 四层密钥派生系统
- [远程认证协议](remote-attestation-protocol.md) - SGX DCAP 认证
- [沙箱安全隔离](沙箱安全隔离.md) - TEE Sandbox 设计
- [架构设计](../01-project-overview/architecture.md) - 整体架构
- [内存安全与密钥清理](../../README.md#内存安全与密钥清理-story-14) - 详细实现

---

**文档版本**: v1.0  
**最后更新**: 2026-03-20
