# CredBridge ImmuDb 安装和配置指南

本文档介绍如何为 CredBridge 审计日志系统安装和配置 immudb 不可篡改数据库。

## 目录

- [概述](#概述)
- [安装 immudb](#安装-immudb)
- [配置 CredBridge](#配置-credbridge)
- [环境变量](#环境变量)
- [数据库初始化](#数据库初始化)
- [验证安装](#验证安装)
- [故障排除](#故障排除)

## 概述

CredBridge 使用 immudb 作为审计日志的持久化存储后端。immudb 是一个轻量级、高性能的不可篡改数据库，基于 Merkle Tree 提供数据完整性保证。

### 特性

- **不可篡改**: 所有数据写入后无法修改或删除
- **Merkle Tree**: 密码学验证数据完整性
- **高性能**: 支持每秒数万次写入
- **轻量级**: 单个二进制文件，无需外部依赖
- **SQL 支持**: 支持标准 SQL 查询

## 安装 immudb

### Docker 安装（推荐）

```bash
# 拉取 immudb 镜像
docker pull codenotary/immudb:latest

# 运行 immudb 容器
docker run -d --name immudb \
  -p 3322:3322 \
  -p 9497:9497 \
  -v immudb_data:/var/lib/immudb \
  codenotary/immudb:latest

# 查看日志
docker logs -f immudb
```

### 二进制安装

#### macOS

```bash
# 使用 Homebrew
brew tap codenotary/tap
brew install immudb

# 启动 immudb
immudb
```

#### Linux

```bash
# 下载最新版本
wget https://github.com/codenotary/immudb/releases/latest/download/immudb-linux-amd64

# 添加执行权限
chmod +x immudb-linux-amd64

# 移动到 PATH
sudo mv immudb-linux-amd64 /usr/local/bin/immudb

# 启动 immudb
immudb
```

### 系统服务配置（Linux）

创建 systemd 服务文件 `/etc/systemd/system/immudb.service`：

```ini
[Unit]
Description=immudb - immutable database
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/immudb
Restart=always
RestartSec=5
User=immudb
Group=immudb

[Install]
WantedBy=multi-user.target
```

启用并启动服务：

```bash
sudo systemctl daemon-reload
sudo systemctl enable immudb
sudo systemctl start immudb
```

## 配置 CredBridge

### 环境变量配置

CredBridge 通过环境变量配置 immudb 连接：

```bash
# immudb 服务器地址
export IMMUDB_HOST=localhost

# immudb 服务器端口（默认 3322）
export IMMUDB_PORT=3322

# 数据库名称
export IMMUDB_DATABASE=credbridge_audit

# 用户名（默认 immudb）
export IMMUDB_USERNAME=immudb

# 密码（默认 immudb）
export IMMUDB_PASSWORD=your_secure_password

# 连接超时（秒，默认 30）
export IMMUDB_TIMEOUT=30

# 是否使用 TLS（默认 false）
export IMMUDB_USE_TLS=false

# 集合名称（默认 audit_logs）
export IMMUDB_COLLECTION=audit_logs

# 开发环境下若暂时没有 immudb，可显式允许回退到内存审计
export CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK=false

# 可选：固定审计签名密钥路径，确保重启后 verify 仍使用同一把密钥
export CREDBRIDGE_AUDIT_SIGNING_KEY_PATH=/var/lib/credbridge/audit-signing-key.json
```

### 代码配置

```rust
use vault_service::audit::{ImmuDbAuditStore, ImmuDbStoreConfig, ImmuDbConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方法 1: 从环境变量加载配置
    let store = ImmuDbAuditStore::from_env(
        signer_fingerprint,
        public_key,
    ).await?;

    // 方法 2: 手动配置
    let config = ImmuDbStoreConfig {
        immudb: ImmuDbConfig {
            host: "localhost".to_string(),
            port: 3322,
            database: "credbridge_audit".to_string(),
            username: "immudb".to_string(),
            password: "your_password".to_string(),
            timeout_secs: 30,
            use_tls: false,
            collection: "audit_logs".to_string(),
        },
        max_cache_size: 10_000,
        auto_sync_interval_secs: 60,
    };

    let store = ImmuDbAuditStore::new(
        config,
        signer_fingerprint,
        public_key,
    ).await?;

    Ok(())
}
```

## 环境变量

| 变量名 | 默认值 | 说明 |
|--------|--------|------|
| `IMMUDB_HOST` | `localhost` | immudb 服务器地址 |
| `IMMUDB_PORT` | `3322` | immudb 服务器端口 |
| `IMMUDB_DATABASE` | `credbridge_audit` | 数据库名称 |
| `IMMUDB_USERNAME` | `immudb` | 连接用户名 |
| `IMMUDB_PASSWORD` | `immudb` | 连接密码 |
| `IMMUDB_TIMEOUT` | `30` | 连接超时（秒） |
| `IMMUDB_USE_TLS` | `false` | 是否使用 TLS |
| `IMMUDB_COLLECTION` | `audit_logs` | 审计日志集合名称 |
| `CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK` | `false` | 仅开发环境使用；显式允许无 immudb 时回退到内存审计 |
| `CREDBRIDGE_AUDIT_SIGNING_KEY_PATH` | 系统临时目录下的 `credbridge-immudb-sim/*.signing-key.json` | 审计签名密钥持久化路径，确保重启后校验公钥稳定 |

### 生产环境配置

```bash
# .env.production
IMMUDB_HOST=immudb.internal.company.com
IMMUDB_PORT=3322
IMMUDB_DATABASE=credbridge_audit_prod
IMMUDB_USERNAME=credbridge_app
IMMUDB_PASSWORD=<your-strong-password>
IMMUDB_TIMEOUT=60
IMMUDB_USE_TLS=true
IMMUDB_COLLECTION=audit_logs
CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK=false
CREDBRIDGE_AUDIT_SIGNING_KEY_PATH=/var/lib/credbridge/audit-signing-key.json
```

## 数据库初始化

### 自动初始化

CredBridge 启动时会自动执行以下操作：

1. 连接到 immudb 服务器
2. 创建数据库（如果不存在）
3. 创建审计日志集合
4. 创建必要的索引

### 手动初始化

如需手动创建数据库：

```bash
# 使用 immuclient 连接
immuclient

# 登录
login immudb

# 创建数据库
database create credbridge_audit

# 使用数据库
use credbridge_audit

# 创建集合（SQL）
CREATE TABLE audit_logs (
    id VARCHAR PRIMARY KEY,
    user_id_hash VARCHAR NOT NULL,
    timestamp BIGINT NOT NULL,
    session_id VARCHAR,
    service VARCHAR,
    action VARCHAR,
    risk_tier VARCHAR,
    outcome VARCHAR,
    tee_mrenclave VARCHAR,
    action_token_jti VARCHAR,
    params JSON,
    error_message VARCHAR,
    content_hash VARCHAR NOT NULL,
    prev_hash VARCHAR,
    signature VARCHAR NOT NULL,
    signer_fingerprint VARCHAR NOT NULL,
    log_index BIGINT NOT NULL,
    merkle_root VARCHAR NOT NULL
);

# 创建索引
CREATE INDEX idx_timestamp ON audit_logs(timestamp);
CREATE INDEX idx_user_id ON audit_logs(user_id_hash);
CREATE INDEX idx_action ON audit_logs(action);
CREATE INDEX idx_outcome ON audit_logs(outcome);
```

## 验证安装

### 1. 检查 immudb 服务状态

```bash
# 检查服务状态
systemctl status immudb

# 或检查端口
netstat -tlnp | grep 3322
```

### 2. 运行集成测试

```bash
# 运行所有 immudb 测试
cargo test --test immudb_tests

# 运行特定测试模块
cargo test --test immudb_tests immudb_client_tests
cargo test --test immudb_tests immudb_storage_tests
cargo test --test immudb_tests immudb_integration_tests

# 详细输出
cargo test --test immudb_tests -- --nocapture
```

### 3. 验证连接

```rust
use vault_service::audit::{ImmuDbClient, ImmuDbConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ImmuDbConfig::from_env();
    let mut client = ImmuDbClient::new(config);

    // 连接
    client.connect().await?;
    println!("Connected to immudb: {}", client.is_connected());

    // 获取状态
    let state = client.current_state().await?;
    println!("Database: {}", state.database);
    println!("Tree size: {}", state.tree_size);
    println!("State hash: {}", state.state_hash);

    client.disconnect().await?;
    Ok(())
}
```

## 故障排除

### 连接失败

**问题**: `Failed to connect to immudb`

**解决方案**:
1. 检查 immudb 服务是否运行：
   ```bash
   docker ps | grep immudb
   # 或
   systemctl status immudb
   ```

2. 检查防火墙设置：
   ```bash
   # 检查端口是否开放
   telnet localhost 3322
   ```

3. 验证配置：
   ```bash
   echo $IMMUDB_HOST
   echo $IMMUDB_PORT
   ```

### 认证失败

**问题**: `Authentication failed`

**解决方案**:
1. 验证用户名和密码：
   ```bash
   immuclient -a localhost -p 3322 login immudb
   ```

2. 重置密码（如需）：
   ```bash
   immuclient -a localhost -p 3322 login immudb --password
   ```

### 数据库不存在

**问题**: `Database not found`

**解决方案**:
1. 手动创建数据库：
   ```bash
   immuclient -a localhost -p 3322
   > login immudb
   > database create credbridge_audit
   ```

2. 或使用自动初始化（默认行为）

### 性能问题

**问题**: 写入速度慢

**解决方案**:
1. 增加批处理大小：
   ```rust
   // 使用批量存储
   store.store_batch(&entries).await?;
   ```

2. 调整缓存大小：
   ```rust
   let config = ImmuDbStoreConfig {
       max_cache_size: 50_000,  // 增加缓存
       ..Default::default()
   };
   ```

3. 启用连接池（生产环境）

### TLS 配置

**问题**: TLS 连接失败

**解决方案**:
1. 生成证书：
   ```bash
   openssl req -x509 -newkey rsa:4096 -keyout immudb.key -out immudb.crt -days 365 -nodes
   ```

2. 启动 immudb 时启用 TLS：
   ```bash
   immudb --tls --certificate immudb.crt --key immudb.key
   ```

3. 配置 CredBridge：
   ```bash
   export IMMUDB_USE_TLS=true
   ```

## 安全最佳实践

1. **强密码**: 生产环境使用强密码
2. **TLS**: 启用 TLS 加密通信
3. **网络隔离**: 将 immudb 部署在私有网络
4. **访问控制**: 使用专用数据库用户
5. **备份**: 定期备份 immudb 数据

## 备份与恢复

### 备份

```bash
# 使用 immuadmin 备份
immuadmin backup

# 或使用 Docker 卷备份
docker run --rm -v immudb_data:/data -v $(pwd):/backup alpine tar czf /backup/immudb_backup.tar.gz -C /data .
```

### 恢复

```bash
# 使用 immuadmin 恢复
immuadmin restore backup_file

# 或使用 Docker 卷恢复
docker run --rm -v immudb_data:/data -v $(pwd):/backup alpine tar xzf /backup/immudb_backup.tar.gz -C /data
```

## 监控

### 指标

- **写入吞吐量**: entries/second
- **查询延迟**: ms
- **存储使用**: bytes
- **连接数**: count

### 健康检查

```rust
use vault_service::audit::ImmuDbAuditStore;

async fn health_check(store: &ImmuDbAuditStore) -> bool {
    match store.verify().await {
        Ok(valid) => valid,
        Err(_) => false,
    }
}
```

## 参考资料

- [immudb 官方文档](https://docs.immudb.io/)
- [immudb GitHub](https://github.com/codenotary/immudb)
- [CredBridge 架构文档](docs/CredBridge_CN_设计规范_v1.0.md)
