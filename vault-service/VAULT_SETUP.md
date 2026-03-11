# HashiCorp Vault 安装与配置指南

本文档介绍如何安装、配置和运行 HashiCorp Vault 作为 CredBridge 的凭证存储后端。

## 目录

1. [安装 Vault](#安装-vault)
2. [开发环境配置](#开发环境配置)
3. [生产环境配置](#生产环境配置)
4. [CredBridge 集成](#credbridge-集成)
5. [安全最佳实践](#安全最佳实践)
6. [故障排除](#故障排除)

## 安装 Vault

### macOS

```bash
# 使用 Homebrew
brew tap hashicorp/tap
brew install hashicorp/tap/vault

# 验证安装
vault version
```

### Linux (Ubuntu/Debian)

```bash
# 添加 HashiCorp GPG 密钥
wget -O- https://apt.releases.hashicorp.com/gpg | sudo gpg --dearmor -o /usr/share/keyrings/hashicorp-archive-keyring.gpg

# 添加仓库
echo "deb [signed-by=/usr/share/keyrings/hashicorp-archive-keyring.gpg] https://apt.releases.hashicorp.com $(lsb_release -cs) main" | sudo tee /etc/apt/sources.list.d/hashicorp.list

# 安装
sudo apt update && sudo apt install vault

# 验证安装
vault version
```

### Docker

```bash
# 运行开发服务器
docker run -d --name vault-dev \
  -p 8200:8200 \
  -e 'VAULT_DEV_ROOT_TOKEN_ID=root' \
  -e 'VAULT_DEV_LISTEN_ADDRESS=0.0.0.0:8200' \
  hashicorp/vault:latest

# 查看日志
docker logs vault-dev
```

## 开发环境配置

### 1. 启动开发服务器

```bash
# 开发模式（内存存储，数据不会持久化）
vault server -dev -dev-root-token-id="root" -dev-listen-address="127.0.0.1:8200"
```

### 2. 配置环境变量

```bash
export VAULT_ADDR="http://127.0.0.1:8200"
export VAULT_TOKEN="root"
export VAULT_MOUNT_PATH="secret"
```

### 3. 启用 KV v2 引擎

```bash
# 登录
vault login root

# 启用 KV v2 引擎（默认路径为 secret）
vault secrets enable -version=2 -path=secret kv-v2

# 验证
vault secrets list
```

### 4. 测试写入/读取

```bash
# 写入测试数据
vault kv put secret/credbridge/test-tenant/test-cred \
  credential_id="test-cred" \
  tenant_id="test-tenant" \
  encrypted_payload="base64-encrypted-data"

# 读取数据
vault kv get secret/credbridge/test-tenant/test-cred

# 列出所有凭证
vault kv list secret/credbridge/test-tenant
```

## 生产环境配置

### 1. 配置 Vault 服务器

创建配置文件 `/etc/vault/config.hcl`：

```hcl
storage "raft" {
  path    = "/opt/vault/data"
  node_id = "node1"
}

listener "tcp" {
  address       = "0.0.0.0:8200"
  tls_cert_file = "/opt/vault/tls/vault.crt"
  tls_key_file  = "/opt/vault/tls/vault.key"
}

default_lease_ttl = "768h"
max_lease_ttl     = "8760h"

api_addr = "https://vault.credbridge.internal:8200"
cluster_addr = "https://vault.credbridge.internal:8201"

ui = true
```

### 2. 启动 Vault 服务

```bash
# 创建数据目录
sudo mkdir -p /opt/vault/data
sudo chown -R vault:vault /opt/vault/data

# 启动服务
vault server -config=/etc/vault/config.hcl
```

### 3. 初始化 Vault

```bash
# 初始化（仅在首次启动时执行）
vault operator init -key-shares=5 -key-threshold=3

# 保存输出的 Unseal Key 和 Root Token！
# 输出示例：
# Unseal Key 1: ...
# Unseal Key 2: ...
# ...
# Initial Root Token: hvs.xxx...
```

### 4. 解封 Vault

```bash
# 使用 3 个 Unseal Key 解封
vault operator unseal <Unseal Key 1>
vault operator unseal <Unseal Key 2>
vault operator unseal <Unseal Key 3>
```

### 5. 配置访问策略

创建 CredBridge 专用策略文件 `credbridge-policy.hcl`：

```hcl
# 允许读取/写入 credbridge 路径下的所有数据
path "secret/data/credbridge/*" {
  capabilities = ["create", "read", "update", "delete", "list"]
}

# 允许列出 credbridge 目录
path "secret/metadata/credbridge/*" {
  capabilities = ["list", "read", "delete"]
}
```

应用策略：

```bash
# 登录为 root
vault login <Root Token>

# 创建策略
vault policy write credbridge credbridge-policy.hcl

# 创建专用 Token（仅 TEE Enclave 持有）
vault token create -policy=credbridge -ttl=8760h
```

## CredBridge 集成

### 环境变量配置

```bash
# Vault 连接配置
export VAULT_ADDR="https://vault.credbridge.internal:8200"
export VAULT_TOKEN="hvs.xxx..."  # 仅 TEE Enclave 持有
export VAULT_MOUNT_PATH="secret"
export VAULT_NAMESPACE=""  # 企业版命名空间（可选）

# TLS 配置（生产环境）
export VAULT_CA_CERT="/path/to/ca.crt"
export VAULT_CLIENT_CERT="/path/to/client.crt"
export VAULT_CLIENT_KEY="/path/to/client.key"

# 超时配置
export VAULT_TIMEOUT_SECONDS="30"
export VAULT_MAX_RETRIES="3"
```

### 代码集成示例

```rust
use vault_service::vault::{
    VaultStorageBackend, VaultConfig, CredentialVault,
    backend::check_vault_health,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 从环境变量加载配置
    let config = VaultConfig::from_env()?;

    // 检查 Vault 健康状态
    match check_vault_health(&config).await? {
        VaultHealthStatus::Healthy => println!("Vault is healthy"),
        status => println!("Vault status: {:?}", status),
    }

    // 创建 Vault 存储后端
    let backend = VaultStorageBackend::new(config).await?;

    // 创建 CredentialVault
    let vault = CredentialVault::with_backend(Box::new(backend));

    // 现在可以使用 vault 进行凭证操作
    // ...

    Ok(())
}
```

### 使用代码方式配置

```rust
use vault_service::vault::{VaultStorageBackend, VaultConfig};

async fn setup_vault() -> Result<CredentialVault, Box<dyn std::error::Error>> {
    // 手动配置
    let config = VaultConfig {
        addr: "https://vault.credbridge.internal:8200".to_string(),
        token: std::env::var("VAULT_TOKEN")?,  // 仅 TEE Enclave 持有
        mount_path: "secret".to_string(),
        namespace: None,
        ca_cert_path: Some("/etc/vault/ca.crt".to_string()),
        client_cert_path: Some("/etc/vault/client.crt".to_string()),
        client_key_path: Some("/etc/vault/client.key".to_string()),
        timeout_seconds: 30,
        max_retries: 3,
    };

    let backend = VaultStorageBackend::new(config).await?;
    Ok(CredentialVault::with_backend(Box::new(backend)))
}
```

## 安全最佳实践

### 1. Token 管理

- **仅 TEE Enclave 持有 Vault Token**：Token 永远不要离开 TEE 安全边界
- **使用短期 Token**：设置合理的 TTL，定期轮换
- **专用 Token**：为 CredBridge 创建专用策略和 Token，不要复用 root token

### 2. 网络隔离

- **内部网络**：Vault 服务器应部署在内部网络，不直接暴露到公网
- **mTLS**：生产环境启用双向 TLS 认证
- **防火墙**：限制只有 TEE Enclave 可以访问 Vault 端口

### 3. 数据加密

CredBridge 实现**双重加密**：

1. **第一层（TEE 内）**：AES-256-GCM 加密凭证内容
2. **第二层（Vault）**：Vault 自带 AES-256 加密存储

即使 Vault 被攻破，攻击者也只能获得 TEE 加密后的密文，无法解密原始凭证。

### 4. 访问审计

启用 Vault 审计日志：

```bash
# 启用文件审计日志
vault audit enable file file_path=/var/log/vault/audit.log

# 启用 syslog 审计
vault audit enable syslog
```

### 5. 备份策略

```bash
# 创建快照
vault operator raft snapshot save backup.snap

# 恢复快照
vault operator raft snapshot restore backup.snap
```

## 故障排除

### 常见问题

#### 1. 连接被拒绝

```
Error: Vault connection failed: Connection refused
```

**解决方案**：
- 检查 Vault 服务器是否运行：`vault status`
- 检查 VAULT_ADDR 配置是否正确
- 检查防火墙规则

#### 2. 认证失败

```
Error: Authentication failed: permission denied
```

**解决方案**：
- 验证 VAULT_TOKEN 是否正确
- 检查 Token 是否过期：`vault token lookup`
- 检查策略权限：`vault token capabilities secret/credbridge/`

#### 3. 路径不存在

```
Error: Secret not found
```

**解决方案**：
- 确认 KV v2 引擎已启用：`vault secrets list`
- 检查路径是否正确
- 确认有读取权限

#### 4. TLS 证书错误

```
Error: certificate verify failed
```

**解决方案**：
- 验证 CA 证书路径
- 检查证书有效期：`openssl x509 -in vault.crt -text -noout`
- 开发环境可临时禁用 TLS 验证（不推荐生产环境）

### 调试命令

```bash
# 检查 Vault 状态
vault status

# 查看当前 Token 信息
vault token lookup

# 测试 KV 操作
vault kv get secret/credbridge/test/test

# 查看审计日志
sudo tail -f /var/log/vault/audit.log

# 启用调试日志
export VAULT_LOG_LEVEL=debug
vault server -dev
```

## 参考链接

- [HashiCorp Vault 官方文档](https://developer.hashicorp.com/vault/docs)
- [Vault KV v2 引擎](https://developer.hashicorp.com/vault/docs/secrets/kv/kv-v2)
- [Vault 安全配置](https://developer.hashicorp.com/vault/docs/configuration)
- [Vault API 文档](https://developer.hashicorp.com/vault/api-docs)
