# Toani Vault

[English README](README.md)

Toani Vault 是一个围绕 Intel SGX TEE 构建的 AI 原生零信任凭证保险库。当前仓库的公开使用面已经收口到主服务、前端控制台、Rust/TypeScript SDK，以及面向运维和自动化的 CLI。

## 当前能力

- TEE 硬件模式与 simulation 模式双运行路径
- 四层密钥层次和 AES-256-GCM 凭证加密
- PASETO v4.local 认证、Redis 会话与 Token 流程
- Schema-per-tenant 与 PostgreSQL RLS 多租户隔离
- 审计日志、远程证明、TEE Sandbox 执行接口
- React 管理界面、Rust SDK、TypeScript SDK、CLI

## 运行时存储策略（Phase 5）

服务启动时会按域强制校验存储后端：

- `vault`：PostgreSQL 或 HashiCorp Vault（`CREDBRIDGE_STORAGE_BACKEND`）
- `auth`：PostgreSQL（`DATABASE_URL`）
- `tenant config`：PostgreSQL（`DATABASE_URL`）
- `sandbox records`：PostgreSQL（`DATABASE_URL`）
- `audit`：默认 immudb（`IMMUDB_*`）
- `token state`：Redis（`REDIS_URL`）

在生产环境，缺失必需持久化后端会直接启动失败。

仅在开发/测试环境，且显式开启开关时允许内存回退：

- `CREDBRIDGE_AUTH_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_TENANT_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_SANDBOX_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_TOKEN_ALLOW_MEMORY_FALLBACK=true`

## 架构摘要

```text
L0: SGX Sealing Key
  -> L1: Enclave Master Key
    -> L2: User Vault Key
      -> L3: Credential Encryption Key
        -> AES-256-GCM encrypted credential
```

主要目录：

- `src/api/`：HTTP 路由与中间件
- `src/tee/`：TEE 生命周期、证明、密封、沙箱、硬件运行时桥接
- `src/token/`：PASETO 与会话处理
- `src/vault/`：凭证存储与后端
- `cli/`：命令行入口
- `sdk-rust/`、`sdk-typescript/`：客户端 SDK
- `frontend/`：React 控制台

## 文档入口

- 文档总入口：[docs/README.md](docs/README.md)
- 部署与运维：[docs/05-部署与运维/README.md](docs/05-部署与运维/README.md)
- API 参考：[docs/03-API 参考/README.md](docs/03-API 参考/README.md)
- 开发者指南：[docs/07-开发者指南/README.md](docs/07-开发者指南/README.md)
- SGX Runner 手册：[docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md](docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md)

## CLI 使用指南

Toani Vault 提供了功能强大的命令行工具，适用于运维人员和自动化工作流。

### 安装

```bash
# 从源码安装
cargo install --path cli

# 验证安装
toani --version
```

### 快速开始

```bash
# 1. 配置 CLI 连接到 Toani Vault 服务
toani config init --url https://api.toani.ai --token <your-token>

# 或使用交互式模式
toani config init

# 2. 验证连接
toani auth status

# 3. 开始管理凭证
toani credentials list
```

### CLI 命令概览

#### 认证命令 (`auth`)

| 命令 | 描述 |
|------|------|
| `toani auth login --url <url> --token <token>` | 登录到 Toani Vault 服务 |
| `toani auth status` | 检查登录状态 |
| `toani auth logout` | 注销并移除本地配置 |

#### 凭证管理 (`credentials`)

| 命令 | 描述 |
|------|------|
| `toani credentials list` | 列出所有凭证 |
| `toani credentials get <id>` | 获取凭证详情 |
| `toani credentials create` | 创建新凭证 |
| `toani credentials update <id>` | 更新现有凭证 |
| `toani credentials delete <id>` | 删除凭证 |
| `toani credentials decrypt <id>` | 解密并查看凭证值 |
| `toani credentials versions <id>` | 查看版本历史 |
| `toani credentials rollback <id> <version>` | 回滚到指定版本 |

**支持的凭证类型：**
- `api_key` - API 密钥
- `username_password` - 用户名/密码对
- `oauth_refresh` - OAuth 刷新令牌
- `session_cookie` - 会话 Cookie
- `ssh_key` - SSH 密钥
- `certificate` - TLS/SSL 证书
- `database_connection` - 数据库连接字符串
- `kyc_document` - KYC 文档

**示例：**

```bash
# 创建 API Key 凭证
toani credentials create \
  --name "production-api-key" \
  --type api_key \
  --value "sk-live-xxx"

# 创建带元数据的凭证
toani credentials create \
  --name "db-credentials" \
  --type username_password \
  --value "secret-password" \
  --metadata "username=admin" \
  --metadata "host=db.example.com"

# 按类型过滤列表
toani credentials list --type api_key

# 解密查看凭证
toani credentials decrypt <credential-id>

# 删除（带确认）
toani credentials delete <credential-id>

# 强制删除（不确认）
toani credentials delete <credential-id> --force

# 查看版本历史
toani credentials versions <credential-id>

# 回滚到上一版本
toani credentials rollback <credential-id> 3
```

#### Token 管理 (`tokens`)

| 命令 | 描述 |
|------|------|
| `toani tokens create --name <name>` | 创建新的访问令牌 |
| `toani tokens list` | 列出所有令牌 |
| `toani tokens revoke <id>` | 撤销令牌 |
| `toani tokens verify [token]` | 验证令牌有效性 |

**示例：**

```bash
# 创建指定权限和过期时间的令牌
toani tokens create \
  --name "ci-token" \
  --expires-in 7200 \
  --scopes "credential:read,credential:write"

# 验证当前令牌
toani tokens verify

# 验证指定令牌
toani tokens verify "v4.local.xxx"
```

#### 沙箱操作 (`sandbox`)

在隔离的 TEE 沙箱会话中执行凭证消费操作。

| 命令 | 描述 |
|------|------|
| `toani sandbox create-session --credential-id <id> --original-intent <desc>` | 创建沙箱会话 |
| `toani sandbox list-sessions` | 列出活动会话 |
| `toani sandbox get-session <id>` | 获取会话详情 |
| `toani sandbox terminate <id>` | 终止会话 |
| `toani sandbox execute <session-id> --operation-type <type>` | 执行操作 |
| `toani sandbox get-operation <operation-id>` | 获取操作结果 |
| `toani sandbox stats` | 查看沙箱统计 |

**示例：**

```bash
# 为凭证创建沙箱会话
toani sandbox create-session \
  --credential-id <cred-id> \
  --original-intent "数据库备份操作"

# 在沙箱中执行操作
toani sandbox execute <session-id> \
  --operation-type "database_query" \
  --params '{"query": "SELECT * FROM users"}'

# 查看沙箱统计
toani sandbox stats
```

#### 审计日志 (`audit`)

| 命令 | 描述 |
|------|------|
| `toani audit logs` | 查询审计日志 |
| `toani audit export <file>` | 导出审计日志 |
| `toani audit verify` | 验证审计日志完整性 |

**示例：**

```bash
# 查询最近的审计日志
toani audit logs --limit 100

# 按时间范围和操作类型查询
toani audit logs \
  --from "2024-01-01T00:00:00Z" \
  --to "2024-01-31T23:59:59Z" \
  --action "credential_access"

# 导出为 JSON
toani audit export audit-export.json

# 导出为 CSV
toani audit export audit-export.csv --format csv

# 验证审计完整性
toani audit verify
```

#### 配置管理 (`config`)

| 命令 | 描述 |
|------|------|
| `toani config init` | 初始化配置 |
| `toani config show` | 显示当前配置 |
| `toani config set <key> <value>` | 设置配置项 |
| `toani config get <key>` | 获取配置项 |

**配置键：**
- `url` - Toani Vault 服务 URL
- `token` - API 认证令牌
- `output_format` - 输出格式：`table` 或 `json`
- `timeout` - 请求超时（秒）

**示例：**

```bash
# 交互式初始化
toani config init

# 使用参数初始化
toani config init \
  --url https://api.toani.ai \
  --token "v4.local.xxx"

# 设置输出格式为 JSON
toani config set output_format json

# 设置超时时间为 60 秒
toani config set timeout 60

# 查看当前配置
toani config show
```

### 全局选项

| 选项 | 描述 |
|------|------|
| `-o, --output <format>` | 输出格式：`table` 或 `json`（默认：table） |
| `-c, --config <path>` | 自定义配置文件路径 |
| `-v, --verbose` | 启用详细日志 |
| `-h, --help` | 显示帮助信息 |
| `-V, --version` | 显示版本信息 |

### 环境变量

| 变量 | 描述 |
|------|------|
| `CREDBRIDGE_URL` | 服务 URL 覆盖 |
| `CREDBRIDGE_TOKEN` | API 令牌覆盖 |
| `HOME` | 配置目录（默认：`~/.config/credbridge/`） |

### 配置文件

CLI 将配置存储在 `~/.config/credbridge/config.toml`：

```toml
url = "https://api.toani.ai"
token = "v4.local.xxx"
output_format = "table"
timeout = 30
```

配置文件权限自动设置为 `0600`（仅用户可读写）。

---

## 本地开发

核心校验门禁：

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

前端构建：

```bash
cd frontend
npm run build
```

Linux SGX 环境下的硬件编译检查：

```bash
cargo test --features tee-hardware --lib --tests --no-run
```

## 部署说明

本地完整栈：

```bash
docker compose -f docker/docker-compose.yml up
```

硬件模式至少需要：

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so
```

补充参考：

- [docker/sgx/README.md](docker/sgx/README.md)
- [sgx-enclave/README.md](sgx-enclave/README.md)
- [cli/README.md](cli/README.md)
- [sdk-rust/README.md](sdk-rust/README.md)
- [sdk-typescript/README.md](sdk-typescript/README.md)
