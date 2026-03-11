# CredBridge MCP Server

CredBridge MCP Server 是为 AI Agent 提供安全凭证管理的 Model Context Protocol (MCP) 服务。

## 功能特性

- **凭证管理**: 创建、读取、更新、删除凭证（CRUD）
- **类型支持**: UsernamePassword、ApiKey、OAuthRefresh、SessionCookie、KycDocument
- **TEE 集成**: 基于 Intel SGX 的硬件级安全保障
- **审计日志**: 完整的操作审计追踪
- **Scope 权限**: 细粒度的访问控制
- **多租户**: 数据隔离支持

## 安装

```bash
cd mcp-server
cargo build --release
```

## 使用方法

### 启动服务器

```bash
# stdio 模式（默认）
cargo run

# SSE 模式
CREDBRIDGE_MCP_TRANSPORT=sse cargo run

# 自定义端口
CREDBRIDGE_MCP_SSE_PORT=8080 cargo run
```

### 环境变量

| 变量名 | 说明 | 默认值 |
|--------|------|--------|
| `CREDBRIDGE_MCP_TRANSPORT` | 传输模式 (stdio/sse) | `stdio` |
| `CREDBRIDGE_MCP_SSE_BIND` | SSE 绑定地址 | `127.0.0.1` |
| `CREDBRIDGE_MCP_SSE_PORT` | SSE 端口 | `3721` |
| `CREDBRIDGE_LOG_LEVEL` | 日志级别 | `info` |

## MCP 工具列表

### 凭证管理工具

#### `create_credential`

创建新凭证。

**参数:**
- `tenant_id`: 租户 ID (string, required)
- `user_id`: 用户 ID (string, required)
- `service_id`: 服务标识，如 "github", "aws" (string, required)
- `credential_type`: 凭证类型 (enum, required)
  - `username_password`: 用户名密码
  - `api_key`: API 密钥
  - `oauth_refresh`: OAuth 刷新令牌
  - `session_cookie`: 会话 Cookie
  - `kyc_document`: KYC 文档
- `plaintext_data`: 凭证明文数据 (object, required)
- `scope`: 授权 Scope (string, required, 需包含 `credential:write`)
- `expires_at`: 过期时间 Unix 时间戳 (integer, optional)

**示例:**
```json
{
  "tenant_id": "tenant_123",
  "user_id": "user_456",
  "service_id": "github",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "myuser",
    "password": "mypassword"
  },
  "scope": "credential:write"
}
```

#### `update_credential`

更新现有凭证。

**参数:**
- `tenant_id`: 租户 ID (string, required)
- `user_id`: 用户 ID (string, required)
- `credential_id`: 凭证 ID (string, required)
- `plaintext_data`: 新的凭证明文数据 (object, optional)
- `scope`: 授权 Scope (string, required, 需包含 `credential:write`)
- `expires_at`: 新的过期时间 (integer/null, optional)

**示例:**
```json
{
  "tenant_id": "tenant_123",
  "user_id": "user_456",
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "plaintext_data": {
    "username": "newuser",
    "password": "newpassword"
  },
  "scope": "credential:write"
}
```

#### `delete_credential`

删除凭证（软删除）。

**参数:**
- `tenant_id`: 租户 ID (string, required)
- `user_id`: 用户 ID (string, required)
- `credential_id`: 凭证 ID (string, required)
- `scope`: 授权 Scope (string, required, 需包含 `credential:write` 或 `credential:delete`)

#### `list_credentials`

列出用户凭证列表。

**参数:**
- `tenant_id`: 租户 ID (string, required)
- `user_id`: 用户 ID (string, required)
- `service_id`: 服务筛选 (string, optional)
- `credential_type`: 凭证类型筛选 (string, optional)

#### `get_credential`

获取单个凭证元数据。

**参数:**
- `tenant_id`: 租户 ID (string, required)
- `user_id`: 用户 ID (string, required)
- `credential_id`: 凭证 ID (string, required)

#### `decrypt_credential`

在 TEE 内解密凭证内容。

**参数:**
- `tenant_id`: 租户 ID (string, required)
- `user_id`: 用户 ID (string, required)
- `credential_id`: 凭证 ID (string, required)
- `scope`: 授权 Scope (string, required, 需包含 `credential:decrypt`)

### 系统工具

#### `tee_status`

获取 TEE 状态信息。

**参数:** 无

## Scope 权限说明

| Scope | 说明 |
|-------|------|
| `credential:read` | 读取凭证元数据 |
| `credential:write` | 创建/更新/删除凭证 |
| `credential:delete` | 删除凭证（可与 write 互换） |
| `credential:decrypt` | 解密凭证内容 |
| `admin` | 所有权限 |

## 测试

```bash
# 运行所有测试
cargo test

# 运行 MCP 凭证工具测试
cargo test --test mcp_credential_tests
```

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                    AI Agent (MCP Client)                    │
└───────────────────────┬─────────────────────────────────────┘
                        │ MCP Protocol (stdio/SSE)
┌───────────────────────▼─────────────────────────────────────┐
│                 CredBridge MCP Server                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │   Tools     │  │  Handlers   │  │   Audit Logger      │ │
│  │  - create   │  │  - validate │  │  - record create    │ │
│  │  - update   │  │  - dispatch │  │  - record update    │ │
│  │  - delete   │  │  - response │  │  - record delete    │ │
│  │  - decrypt  │  └─────────────┘  └─────────────────────┘ │
│  └──────┬──────┘                                           │
└─────────┼───────────────────────────────────────────────────┘
          │
┌─────────▼───────────────────────────────────────────────────┐
│                  CredBridge Vault Service                   │
│        (Credential Storage, TEE, Encryption)                │
└─────────────────────────────────────────────────────────────┘
```

## 版本历史

### v0.1.0
- 初始版本
- 实现凭证 CRUD 工具
- TEE 状态查询
- Scope 权限验证

## 许可证

MIT OR Apache-2.0
