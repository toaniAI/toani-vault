# CredBridge 用户使用手册

**版本**: v1.0
**最后更新**: 2026-03-11
**适用版本**: CredBridge MVP 1.0+

---

## 目录

1. [产品简介](#1-产品简介)
2. [快速开始](#2-快速开始)
3. [Web 控制台使用指南](#3-web-控制台使用指南)
4. [API 使用指南](#4-api-使用指南)
5. [SDK 使用指南](#5-sdk-使用指南)
6. [MCP Server 集成](#6-mcp-server-集成)
7. [常见问题 FAQ](#7-常见问题-faq)
8. [故障排查](#8-故障排查)

---

## 1. 产品简介

### 1.1 什么是 CredBridge

**CredBridge** 是专为 AI Agent 设计的商用级零信任凭证保险库系统，达到与 1Password 同等的安全标准。它基于 **TEE（可信执行环境）**技术，让 AI 能够安全地代表用户行动，而无需暴露用户的真实凭证。

### 1.2 核心功能

| 功能 | 描述 |
|------|------|
| **安全凭证保险库** | 基于 TEE + HSM 的加密存储，保护用户所有身份数据 |
| **AI 行动代理** | Agent 通过有限 Scope Token 发起请求，CredBridge 在 TEE 内解密凭证并执行真实操作 |
| **四层密钥架构** | L0（硬件根密钥）→ L1（Enclave 主密钥）→ L2（用户保险库密钥）→ L3（凭证加密密钥） |
| **不可篡改审计日志** | 基于 immudb 的密码学证明日志，任何历史记录修改都会被立即发现 |
| **人工审批流程** | Tier 0/1/2 三级风险分级，高风险操作需要用户实时确认 |
| **多租户隔离** | Schema-per-Tenant + RLS 行级安全，满足 B2B SaaS 需求 |

### 1.3 适用场景

- **金融服务**: 银行、券商、加密货币平台自动化操作
- **政府服务**: 报税、证件办理、案件查询
- **医疗健康**: 预约、报告获取、保险理赔
- **电商平台**: 订单跟踪、退货、订阅管理
- **SaaS/企业**: GitHub、AWS、Stripe 等平台管理
- **HR/薪酬**: 考勤、薪资查询、福利注册

### 1.4 安全架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                    用户交互层 (OpenClaw Gateway)              │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                  CredBridge Gateway 层                       │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐    │
│  │ OAuth2/PKCE │  │ Policy Eng. │  │ MCP Server       │    │
│  └──────┬──────┘  └──────┬──────┘  └────────┬─────────┘    │
└─────────┼────────────────┼──────────────────┼──────────────┘
          │                │                  │
┌─────────▼────────────────▼──────────────────▼──────────────┐
│              TEE Enclave 层 (Intel SGX)                     │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐   │
│  │ 密钥管理     │  │ Token 签发   │  │ 凭证解密(按需)  │   │
│  │ (L0-L3)      │  │ PASETO v4    │  │ AES-256-GCM     │   │
│  └──────────────┘  └──────────────┘  └─────────────────┘   │
└────────────────────────────────────────────────────────────┘
```

---

## 2. 快速开始

### 2.1 系统要求

#### 2.1.1 硬件要求

| 部署模式 | 硬件要求 | TEE 支持 |
|----------|----------|----------|
| 开发/测试 | 标准 x86_64 服务器 | 软件 TEE（模拟） |
| 生产（自托管） | Intel Core 6代+ 或 Xeon E3/E-系列 | Intel SGX |
| 云托管（推荐） | Azure DCsv3 / AWS EC2 C7a | AMD SEV-SNP |

#### 2.1.2 软件依赖

| 依赖 | 版本要求 | 用途 |
|------|----------|------|
| **Docker** | 24.0+ | 容器化部署 |
| **Docker Compose** | 2.20+ | 服务编排 |
| **Node.js** | 22+ | SDK 开发 |
| **Rust** | 1.75+ | 服务端开发 |

**版本管理工具安装（推荐）**

为了方便管理 Node.js 和 Rust 版本，建议使用以下版本管理工具：

**nvm（Node.js 版本管理）:**
```bash
# macOS/Linux
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash

# 安装后重新加载 shell
source ~/.bashrc  # 或 ~/.zshrc

# 安装 Node.js 22
nvm install 22
nvm use 22
nvm alias default 22

# 验证安装
node --version  # v22.x.x
npm --version
```

**rustup（Rust 版本管理）:**
```bash
# 安装 rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 选择默认工具链
source $HOME/.cargo/env

# 安装指定版本
rustup install 1.75.0
rustup default 1.75.0

# 验证安装
rustc --version  # rustc 1.75.0
cargo --version
```

### 2.2 安装

#### 2.2.1 使用 Docker Compose（推荐）

```bash
# 1. 克隆仓库
git clone https://github.com/credbridge/credbridge.git
cd credbridge

# 2. 启动所有服务
docker-compose up -d

# 3. 验证服务状态
docker-compose ps
curl http://localhost:8080/health
```

#### 2.2.2 手动安装

```bash
# 1. 安装 Vault Service
npm install -g @credbridge/server@latest

# 2. 初始化配置
credbridge init

# 3. 启动服务
credbridge start
```

### 2.3 初始化配置

```bash
# 运行初始化向导
credbridge init

# 向导步骤：
# 1. 检测 TEE 环境（SGX / SEV-SNP / 软件降级）
# 2. 配置 Vault 后端（HashiCorp Vault / AWS Secrets Manager）
# 3. 生成 TEE enclave 并进行远程认证
# 4. 配置 Redis（单次 Token 使用强制）
# 5. 配置 immudb（不可篡改审计日志）
# 6. 生成 OpenClaw mcporter 配置
```

### 2.4 验证安装

```bash
# 验证 TEE 认证
credbridge verify

# 预期输出：
# TEE Status: Enabled
# MRENCLAVE: 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
# Quote Valid: true
# Remote Attestation: PASSED
```

### 2.5 第一次使用

#### 2.5.1 创建第一个凭证

```bash
# 使用 CLI 创建凭证
credbridge credential create \
  --service schwab \
  --type username_password \
  --data '{"username":"user@example.com","password":"secret123"}'
```

#### 2.5.2 生成 API Token

```bash
# 生成 API Token
credbridge token create \
  --name "development-token" \
  --scope "credential:read,credential:write" \
  --expires 30d

# 输出（仅显示一次）：
# Token: v4.local.xxxxxx
# 请妥善保存，此 Token 不会再次显示
```

---

## 3. Web 控制台使用指南

### 3.1 登录

#### 3.1.1 访问控制台

打开浏览器访问 `http://localhost:3000`（或您的部署地址）。

> 💡 **界面说明**: 登录页面包含用户名/密码输入框、登录按钮、忘记密码链接。首次访问时请使用管理员账号登录。

#### 3.1.2 用户名/密码登录

1. 输入用户名和密码
2. 点击「登录」按钮
3. 如启用 MFA，输入验证码

#### 3.1.3 多因素认证 (MFA)

**启用 MFA**:
1. 进入「个人设置」→「安全设置」
2. 点击「启用 MFA」
3. 使用 Authenticator App 扫描二维码
4. 输入验证码确认
5. **保存恢复码**（用于 MFA 设备丢失时恢复）

> 💡 **界面说明**: MFA 设置页面显示二维码（用于 Authenticator App 扫描）和 10 个恢复码，请务必保存恢复码到安全位置。

### 3.2 凭证管理

#### 3.2.1 凭证列表

**导航**: 侧边栏 → Credentials

**功能**:
- 表格/卡片双视图切换
- 搜索（名称、标签、描述）
- 按类型、标签、日期过滤
- 分页（每页 10/25/50 条）

> 💡 **界面说明**: 凭证列表页面以表格形式展示所有凭证，支持按类型、标签、日期过滤，可切换为卡片视图。

#### 3.2.2 创建凭证

1. 点击「新建凭证」按钮
2. 选择凭证类型：
   - **Password**: 用户名/密码（网站登录）
   - **API Key**: API 密钥（服务集成）
   - **Certificate**: 客户端证书
   - **Secure Note**: 安全文本

3. 填写凭证信息：

| 字段 | 说明 | 示例 |
|------|------|------|
| 名称 | 凭证标识 | "Schwab 主账户" |
| 服务 | 目标服务 | schwab / github / aws |
| 标签 | 分类标签 | finance, trading |
| 过期时间 | 自动失效时间 | 2026-12-31 |

4. 点击「保存」

> 💡 **界面说明**: 创建凭证表单根据所选类型动态变化，包含名称、服务、标签、过期时间等通用字段，以及类型特定的字段（如用户名/密码、API Key 等）。

#### 3.2.3 查看和编辑凭证

**查看详情**:
1. 点击凭证行或「查看」按钮
2. 敏感字段默认显示为 `***`
3. 点击眼睛图标显示明文
4. 点击复制按钮复制到剪贴板

**编辑凭证**:
1. 点击「编辑」按钮
2. 修改字段值
3. 点击「保存」

#### 3.2.4 删除凭证

1. 点击「删除」按钮
2. 确认删除操作
3. 凭证将被软删除（可恢复）

### 3.3 Token 管理

#### 3.3.1 Token 列表

**导航**: 侧边栏 → Tokens

显示信息：
- Token 名称
- 权限范围（Scopes）
- 创建时间
- 过期时间
- 最后使用时间
- 状态（活跃/已撤销/已过期）

> 💡 **界面说明**: Token 列表页面展示所有 API Token 的名称、权限范围、创建/过期时间、最后使用时间和状态。

#### 3.3.2 生成 Token

1. 点击「生成 Token」
2. 填写信息：
   - **名称**: Token 标识（如 "Production API"）
   - **权限范围**:
     - `credential:read` - 读取凭证
     - `credential:write` - 创建/更新凭证
     - `credential:decrypt` - 解密凭证内容
     - `audit:read` - 读取审计日志
     - `admin` - 所有权限
   - **过期时间**: 7天 / 30天 / 90天 / 永不过期

3. 点击「生成」
4. **复制 Token**（仅显示一次！）

**⚠️ 重要提示**: Token 生成后仅显示一次，请务必立即复制保存。

> 💡 **界面说明**: 生成 Token 弹窗显示新生成的 PASETO Token 字符串（仅显示一次）和复制按钮，请立即复制保存。

#### 3.3.3 撤销 Token

1. 在 Token 列表中找到目标 Token
2. 点击「撤销」按钮
3. 确认撤销操作
4. 该 Token 将立即失效

### 3.4 审计日志

#### 3.4.1 日志列表

**导航**: 侧边栏 → Audit Logs

显示字段：
- 时间戳
- 用户
- 操作类型（CREATE/READ/UPDATE/DELETE）
- 资源类型
- 结果（成功/失败/拒绝）
- IP 地址

> 💡 **界面说明**: 审计日志列表以时间倒序展示所有操作记录，包括时间戳、用户、操作类型、资源、结果和 IP 地址。

#### 3.4.2 日志过滤

**过滤条件**:
- 时间范围（今天/本周/本月/自定义）
- 用户
- 操作类型
- 资源类型
- 结果状态

#### 3.4.3 日志导出

1. 设置过滤条件
2. 点击「导出」按钮
3. 选择格式：JSON 或 CSV
4. 选择范围：当前筛选结果 或 全部
5. 下载导出文件

> 💡 **界面说明**: 日志导出弹窗允许选择导出格式（JSON/CSV）和范围（当前筛选结果或全部）。

### 3.5 租户管理（管理员）

#### 3.5.1 租户设置

**导航**: 侧边栏 → Settings → Tenant

可配置项：
- 租户显示名称
- 会话超时时间
- 密码策略（最小长度、复杂度要求）
- 功能开关

#### 3.5.2 用户管理

**导航**: 侧边栏 → Users

功能：
- 查看用户列表
- 邀请新用户
- 修改用户角色（Admin / User / Viewer）
- 启用/禁用用户
- 重置用户密码

#### 3.5.3 配额管理

显示当前配额使用情况：
- 凭证数量
- API Token 数量
- 存储空间
- API 调用次数

---

## 4. API 使用指南

### 4.1 认证

#### 4.1.1 Bearer Token

所有 API 请求必须在 `Authorization` 头中包含 Bearer Token：

```http
Authorization: Bearer <paseto_v4_local_token>
```

#### 4.1.2 Token Scope 权限

| Scope | 权限说明 |
|-------|----------|
| `credential:read` | 读取凭证元数据 |
| `credential:decrypt` | 解密凭证获取明文 |
| `credential:write` | 创建/更新凭证 |
| `audit:read` | 读取审计日志 |
| `admin` | 所有管理权限 |

### 4.2 主要 API 端点

#### 4.2.1 凭证管理 API

**创建凭证**

```http
POST /api/v1/credentials
Authorization: Bearer <token>
Content-Type: application/json

{
  "service_id": "schwab",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "user@example.com",
    "password": "secret_password"
  },
  "expires_at": 1893456000
}
```

**响应**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "created_at": "1709990400",
  "expires_at": "1893456000"
}
```

**获取凭证列表**

```http
GET /api/v1/credentials?service_id=schwab&only_valid=true
Authorization: Bearer <token>
```

**响应**:
```json
{
  "credentials": [
    {
      "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "credential_type": "username_password",
      "service_id": "schwab",
      "created_at": "1709990400Z",
      "expires_at": "1893456000Z"
    }
  ],
  "total": 1
}
```

**获取凭证详情**

```http
GET /api/v1/credentials/:id
Authorization: Bearer <token>
```

**解密凭证**

```http
POST /api/v1/credentials/:id/decrypt
Authorization: Bearer <token>
Content-Type: application/json

{
  "reason": "用户登录操作"
}
```

**响应**:
```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "service_id": "schwab",
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "user@example.com",
    "password": "secret_password"
  }
}
```

**删除凭证**

```http
DELETE /api/v1/credentials/:id
Authorization: Bearer <token>
```

#### 4.2.2 审计日志 API

**查询审计日志**

```http
GET /api/v1/audit/logs?start_time=1704067200&end_time=1706745600&limit=20
Authorization: Bearer <token>
```

**响应**:
```json
{
  "entries": [
    {
      "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "user_id_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "timestamp": 1709990400000,
      "action": "CredentialDecrypt",
      "risk_tier": "High",
      "outcome": "Success",
      "tee_mrenclave": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
    }
  ],
  "total": 150,
  "has_more": true
}
```

**导出审计日志**

```http
POST /api/v1/audit/export
Authorization: Bearer <token>
Content-Type: application/json

{
  "start_time": 1704067200,
  "end_time": 1706745600,
  "format": "json",
  "include_verification": true
}
```

### 4.3 代码示例

#### 4.3.1 cURL 示例

```bash
# 获取凭证列表
curl -X GET "https://api.credbridge.io/api/v1/credentials" \
  -H "Authorization: Bearer v4.local.xxxxxx" \
  -H "Content-Type: application/json"

# 创建凭证
curl -X POST "https://api.credbridge.io/api/v1/credentials" \
  -H "Authorization: Bearer v4.local.xxxxxx" \
  -H "Content-Type: application/json" \
  -d '{
    "service_id": "github",
    "credential_type": "api_key",
    "plaintext_data": {
      "key": "ghp_xxxxxxxxxxxx"
    }
  }'
```

#### 4.3.2 Python 示例

```python
import requests

BASE_URL = "https://api.credbridge.io"
TOKEN = "v4.local.xxxxxx"

headers = {
    "Authorization": f"Bearer {TOKEN}",
    "Content-Type": "application/json"
}

# 创建凭证
response = requests.post(
    f"{BASE_URL}/api/v1/credentials",
    headers=headers,
    json={
        "service_id": "stripe",
        "credential_type": "api_key",
        "plaintext_data": {
            "publishable_key": "pk_live_...",
            "secret_key": "sk_live_..."
        }
    }
)
credential_id = response.json()["credential_id"]

# 解密凭证
response = requests.post(
    f"{BASE_URL}/api/v1/credentials/{credential_id}/decrypt",
    headers=headers,
    json={"reason": "Payment processing"}
)
data = response.json()["plaintext_data"]
```

---

## 5. SDK 使用指南

### 5.1 TypeScript SDK

#### 5.1.1 安装

```bash
npm install @credbridge/sdk
# 或
yarn add @credbridge/sdk
```

#### 5.1.2 初始化

```typescript
import { CredBridgeClient, CredentialType } from '@credbridge/sdk';

const client = new CredBridgeClient({
  baseUrl: 'https://api.credbridge.io',
  token: 'v4.local.xxxxxx',
  timeout: 30000,
  maxRetries: 3,
});
```

#### 5.1.3 凭证操作

```typescript
// 创建用户名密码凭证
const userCredential = await client.credentials.createUsernamePassword(
  'schwab',
  'user@example.com',
  'SecurePassword123!',
  { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30 }
);

// 创建 API Key 凭证
const apiCredential = await client.credentials.createApiKey(
  'stripe',
  'sk_live_51H...',
  'sk_secret_...',
  { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 90 }
);

// 获取凭证列表
const { credentials, total } = await client.credentials.list();

// 获取凭证详情
const credential = await client.credentials.get(credentialId);

// 解密凭证
const decrypted = await client.credentials.decrypt(
  credentialId,
  '演示解密操作'
);

// 删除凭证
await client.credentials.delete(credentialId);
```

#### 5.1.4 错误处理

```typescript
import { CredBridgeError, TokenExpiredError, PermissionDeniedError } from '@credbridge/sdk';

try {
  const credential = await client.credentials.get(credentialId);
} catch (error) {
  if (error instanceof TokenExpiredError) {
    console.log('Token 已过期，请重新登录');
  } else if (error instanceof PermissionDeniedError) {
    console.log('权限不足，需要 credential:decrypt Scope');
  } else if (error instanceof CredBridgeError) {
    console.log('CredBridge 错误:', error.code, error.message);
  } else {
    console.log('未知错误:', error);
  }
}
```

#### 5.1.5 Token 管理

```typescript
// 获取 Token 信息
const tokenInfo = client.getTokenInfo();
console.log('租户ID:', tokenInfo.tenantId);
console.log('权限:', tokenInfo.scopes);

// 获取剩余时间
const remainingTime = client.token.getRemainingTimeFormatted();
console.log('Token 剩余时间:', remainingTime);

// 自动刷新（如配置）
client.on('token:expiring', async () => {
  console.log('Token 即将过期，准备刷新...');
});
```

### 5.2 Rust SDK

#### 5.2.1 添加依赖

```toml
[dependencies]
credbridge-sdk = "1.0"
tokio = { version = "1", features = ["full"] }
```

#### 5.2.2 基本使用

```rust
use credbridge_sdk::{CredBridgeClient, CredentialType};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化客户端
    let client = CredBridgeClient::new(
        "https://api.credbridge.io",
        "v4.local.xxxxxx",
    )?;

    // 创建凭证
    let credential = client
        .credentials()
        .create_username_password(
            "schwab",
            "user@example.com",
            "SecurePassword123!",
        )
        .await?;

    println!("创建成功! ID: {}", credential.credential_id);

    // 解密凭证
    let decrypted = client
        .credentials()
        .decrypt(&credential.credential_id, "演示操作")
        .await?;

    println!("用户名: {}", decrypted.plaintext_data.username);

    Ok(())
}
```

### 5.3 批量操作示例

```typescript
import { CredBridgeClient } from '@credbridge/sdk';

const client = new CredBridgeClient({
  baseUrl: 'https://api.credbridge.io',
  token: process.env.CREDBRIDGE_TOKEN!,
});

async function batchImportCredentials(credentials: Array<{
  service: string;
  type: string;
  data: object;
}>) {
  const results = [];

  for (const cred of credentials) {
    try {
      const result = await client.credentials.create({
        service_id: cred.service,
        credential_type: cred.type,
        plaintext_data: cred.data,
      });
      results.push({ success: true, id: result.credentialId });
    } catch (error) {
      results.push({ success: false, error: error.message });
    }
  }

  return results;
}

// 使用示例
const credentialsToImport = [
  {
    service: 'github',
    type: 'api_key',
    data: { key: 'ghp_xxxxxxxxxxxx' }
  },
  {
    service: 'stripe',
    type: 'api_key',
    data: { secret_key: 'sk_live_...' }
  },
];

batchImportCredentials(credentialsToImport)
  .then(results => console.log(results));
```

---

## 6. MCP Server 集成

### 6.1 什么是 MCP

MCP（Model Context Protocol）是 Anthropic 推出的开放协议，用于标准化 AI 助手与外部工具的集成。CredBridge MCP Server 允许 Claude 等 AI 助手安全地访问用户凭证。

### 6.2 配置 MCP Server

#### 6.2.1 启动 MCP Server

```bash
# 启动 MCP Server（默认端口 3721）
credbridge mcp start

# 或指定端口
credbridge mcp start --port 3721
```

#### 6.2.2 配置 Claude Code

在 Claude Code 配置中添加 MCP Server：

```json
// ~/.claude/mcp.json
{
  "servers": {
    "credbridge": {
      "url": "http://127.0.0.1:3721/sse",
      "transport": "sse",
      "auth": {
        "type": "bearer",
        "token": "${CREDBRIDGE_AGENT_TOKEN}"
      }
    }
  }
}
```

#### 6.2.3 环境变量配置

```bash
export CREDBRIDGE_AGENT_TOKEN="v4.local.xxxxxx"
export CREDBRIDGE_MCP_URL="http://127.0.0.1:3721"
```

### 6.3 MCP 工具列表

| 工具名称 | 描述 | 风险等级 |
|----------|------|----------|
| `list_credentials` | 列出用户凭证列表 | Tier 0 |
| `get_credential` | 获取单个凭证元数据 | Tier 0 |
| `create_credential` | 创建新凭证 | Tier 1 |
| `update_credential` | 更新现有凭证 | Tier 1 |
| `delete_credential` | 删除凭证 | Tier 2 |
| `decrypt_credential` | 在 TEE 内解密凭证 | Tier 2 |
| `tee_status` | 获取 TEE 状态信息 | Tier 0 |

### 6.4 使用示例

#### 6.4.1 在 Claude Code 中使用

```
用户: 帮我查看 Schwab 账户的持仓情况

Claude: 我需要使用 CredBridge 来获取您的 Schwab 凭证。

[Claude 调用 list_credentials 工具]

找到了您的 Schwab 凭证。现在我将查询持仓信息...

[Claude 调用 decrypt_credential 获取凭证并在 TEE 内执行操作]

您的 Schwab 持仓情况：
- 总资产: $84,320.11
- 今日变动: -$1,204.33
- 主要持仓:
  - AAPL: $12,400 (14.7%)
  - MSFT: $10,200 (12.1%)
  - GOOGL: $8,500 (10.1%)
```

#### 6.4.2 创建凭证

```json
// create_credential 请求示例
{
  "tenant_id": "tenant_123",
  "user_id": "user_456",
  "service_id": "github",
  "credential_type": "api_key",
  "plaintext_data": {
    "key": "ghp_xxxxxxxxxxxx"
  },
  "scope": "credential:write"
}
```

### 6.5 安全注意事项

1. **Scope 限制**: MCP Server 仅拥有配置的 Scope 权限，无法越权操作
2. **审计记录**: 所有 MCP 调用都会被记录到审计日志
3. **Token 过期**: MCP Token 有过期时间，过期后需要重新授权
4. **本地运行**: 建议 MCP Server 在本地运行，避免网络传输风险

---

## 7. 常见问题 FAQ

### 7.1 一般问题

**Q: CredBridge 与 1Password 有什么区别？**

A: CredBridge 专为 AI Agent 设计：
- 提供程序化访问接口（API/MCP）
- 支持 TEE 硬件隔离
- 内置 AI 操作审计和人工审批流程
- 可与 OpenClaw 等 AI 框架无缝集成

**Q: 我的凭证数据安全吗？**

A: CredBridge 采用多层安全保护：
- TEE（可信执行环境）硬件隔离
- 四层密钥架构（L0-L3）
- AES-256-GCM 加密
- 零知识架构（服务器不接触明文）
- 不可篡改审计日志

**Q: 支持哪些 TEE 硬件？**

A: 目前支持：
- Intel SGX（推荐，主力方案）
- AMD SEV-SNP
- AWS Nitro Enclaves
- 软件 TEE（开发测试用）

### 7.2 使用问题

**Q: 如何重置密码？**

A:
1. 在登录页面点击「忘记密码」
2. 输入注册邮箱
3. 查收重置邮件并点击链接
4. 设置新密码

**Q: Token 丢失了怎么办？**

A: Token 丢失后无法找回，需要：
1. 在 Web 控制台撤销旧 Token
2. 生成新的 Token
3. 更新应用配置中的 Token

**Q: 凭证过期后会发生什么？**

A: 凭证过期后：
- 无法用于解密操作
- 在列表中标记为「已过期」
- 可选择删除或更新过期时间

### 7.3 技术问题

**Q: 如何验证 TEE 状态？**

A: 使用以下命令：
```bash
credbridge verify
# 或调用 API
curl http://localhost:8080/v1/tee/attest
```

**Q: 支持哪些凭证类型？**

A: 支持：
- username_password（用户名/密码）
- api_key（API 密钥）
- oauth_refresh（OAuth Refresh Token）
- certificate（客户端证书）
- ssh_key（SSH 密钥）
- database_connection（数据库连接串）

**Q: API 有速率限制吗？**

A: 是的，默认限制：
- 凭证创建: 100/分钟
- 凭证读取: 300/分钟
- 凭证解密: 60/分钟
- 审计日志查询: 60/分钟

### 7.4 部署问题

**Q: 如何备份凭证数据？**

A:
1. 凭证数据存储在 HashiCorp Vault 中，使用 Vault 的备份机制
2. 定期导出审计日志
3. 密钥材料通过 SGX Sealing 保护，无法直接备份

**Q: 可以自托管吗？**

A: 是的，支持多种部署模式：
- 本地 SGX 服务器（最高安全级别）
- 云托管（Azure/AWS）
- Docker Compose（快速部署）

**Q: 如何实现高可用？**

A: 建议架构：
- 多个 Vault 节点（Shamir 分片）
- PostgreSQL 主从复制
- Redis Cluster
- 负载均衡器

---

## 8. 故障排查

### 8.1 安装问题

#### 8.1.1 TEE 检测失败

**症状**: `credbridge init` 显示 "TEE not detected"

**排查步骤**:
```bash
# 1. 检查 CPU 支持
cat /proc/cpuinfo | grep sgx

# 2. 检查 SGX 驱动
ls -la /dev/sgx*

# 3. 检查 BIOS 设置
# 进入 BIOS 启用 Intel SGX

# 4. 使用软件 TEE 降级模式
credbridge init --tee-mode software
```

#### 8.1.2 Docker 启动失败

**症状**: `docker-compose up` 报错

**排查步骤**:
```bash
# 1. 检查 Docker 版本
docker --version  # 需要 24.0+

# 2. 检查端口占用
lsof -i :8080
lsof -i :3000

# 3. 查看详细日志
docker-compose logs vault-service
docker-compose logs frontend
```

### 8.2 认证问题

#### 8.2.1 Token 无效

**症状**: API 返回 `401 invalid_token`

**排查步骤**:
1. 检查 Token 是否过期
2. 检查 Token 是否被撤销
3. 检查 Authorization 头格式：
   ```
   Authorization: Bearer v4.local.xxxxxx
   ```
4. 重新生成 Token

#### 8.2.2 权限不足

**症状**: API 返回 `403 insufficient_scope`

**排查步骤**:
1. 检查 Token 拥有的 Scope：
   ```bash
   credbridge token inspect <token>
   ```
2. 确认所需权限：
   - 读取凭证 → `credential:read`
   - 解密凭证 → `credential:decrypt`
   - 创建凭证 → `credential:write`
3. 生成新 Token 时添加所需 Scope

### 8.3 凭证操作问题

#### 8.3.1 凭证解密失败

**症状**: `decrypt_credential` 返回错误

**可能原因**:
1. TEE Enclave 未启动
   ```bash
   credbridge status
   ```
2. 密钥层次派生失败
3. 凭证数据损坏

**解决方案**:
```bash
# 重启 TEE 服务
credbridge restart

# 检查 TEE 状态
credbridge verify
```

#### 8.3.2 凭证找不到

**症状**: API 返回 `404 credential_not_found`

**排查步骤**:
1. 确认凭证 ID 正确
2. 检查凭证是否被删除（软删除）
3. 确认租户 ID 和用户 ID 匹配
4. 使用列表 API 确认凭证存在：
   ```bash
   curl http://localhost:8080/api/v1/credentials
   ```

### 8.4 审计日志问题

#### 8.4.1 日志查询无结果

**症状**: 审计日志查询返回空列表

**排查步骤**:
1. 检查时间范围是否正确
2. 检查过滤条件是否过于严格
3. 确认 immudb 服务正常运行：
   ```bash
   docker-compose ps immudb
   ```
4. 检查审计日志权限：
   - 需要 `audit:read` 或 `admin` Scope

#### 8.4.2 日志验证失败

**症状**: 日志验证 API 返回 `verification_failed`

**可能原因**:
1. 日志数据被篡改
2. immudb 状态不一致
3. 网络问题导致验证失败

**解决方案**:
```bash
# 检查 immudb 完整性
docker-compose exec immudb immuadmin status

# 重新同步审计日志
credbridge audit resync
```

### 8.5 性能问题

#### 8.5.1 API 响应慢

**症状**: API 响应时间 > 1秒

**排查步骤**:
1. 检查 TEE EPC 内存使用率：
   ```bash
   cat /sys/kernel/debug/sgx/epc_pages
   ```
2. 检查数据库连接池状态
3. 检查 Redis 连接状态
4. 启用性能监控：
   ```bash
   curl http://localhost:8080/metrics
   ```

#### 8.5.2 内存使用过高

**症状**: 服务内存使用持续增长

**解决方案**:
1. 检查密钥缓存 TTL 设置
2. 检查审计日志缓冲区
3. 重启服务释放内存：
   ```bash
   docker-compose restart vault-service
   ```

### 8.6 获取帮助

如果以上排查步骤无法解决问题，请通过以下方式获取帮助：

1. **查看日志**:
   ```bash
   docker-compose logs -f
   ```

2. **检查系统状态**:
   ```bash
   credbridge status --verbose
   ```

3. **提交 Issue**:
   - GitHub: https://github.com/credbridge/credbridge/issues
   - 提供：错误信息、日志片段、复现步骤

4. **联系支持**:
   - 邮箱: support@credbridge.io
   - 社区: https://discord.gg/credbridge

---

### 8.7 故障排查快速检查清单 ✅

使用以下检查清单进行系统性故障排查：

#### 服务启动检查
- [ ] Docker 版本 >= 24.0 (`docker --version`)
- [ ] Docker Compose 版本 >= 2.20 (`docker-compose --version`)
- [ ] 必要端口未被占用 (8080, 3000, 5432, 3322, 8200)
- [ ] `.env` 文件已正确配置
- [ ] 配置文件存在于 `docker/config/` 目录

#### 依赖服务检查
- [ ] PostgreSQL 服务正常运行 (`docker-compose ps postgres`)
- [ ] Redis 服务正常运行 (`docker-compose ps redis`)
- [ ] immudb 服务正常运行 (`docker-compose ps immudb`)
- [ ] Vault 服务正常运行 (`docker-compose ps vault`)
- [ ] 数据库连接正常 (`docker-compose exec postgres pg_isready -U credbridge`)

#### TEE 环境检查
- [ ] TEE 驱动已加载 (`ls /dev/sgx*`)
- [ ] CPU 支持 SGX (`grep sgx /proc/cpuinfo`)
- [ ] 软件 TEE 降级模式可用 (`credbridge init --tee-mode software`)

#### 认证检查
- [ ] Token 未过期
- [ ] Token 未被撤销
- [ ] Authorization 头格式正确 (`Bearer v4.local.xxx`)
- [ ] Token 拥有所需 Scope

#### API 连接检查
- [ ] 健康检查端点返回 200 (`curl http://localhost:8080/health`)
- [ ] 网络连通性正常 (`ping localhost`)
- [ ] 防火墙未阻断端口

#### 性能检查
- [ ] 磁盘空间充足 (`df -h`)
- [ ] 内存使用率正常 (`docker stats`)
- [ ] CPU 负载正常 (`uptime`)

#### 日志检查
- [ ] 查看 vault-service 日志 (`docker-compose logs vault-service`)
- [ ] 查看 PostgreSQL 日志 (`docker-compose logs postgres`)
- [ ] 日志中无 ERROR 级别错误
- [ ] 审计日志正常写入

#### 恢复操作
如果以上检查均正常但问题仍存在：
1. [ ] 重启服务: `docker-compose restart`
2. [ ] 清理重建: `docker-compose down -v && docker-compose up -d`
3. [ ] 检查最新版本更新
4. [ ] 联系技术支持并提供完整日志

---

## 附录

### A. 术语表

| 术语 | 说明 |
|------|------|
| TEE | Trusted Execution Environment，可信执行环境 |
| SGX | Intel Software Guard Extensions |
| Enclave | TEE 中的安全执行区域 |
| PASETO | Platform-Agnostic Security Tokens，平台无关的安全令牌 |
| HKDF | HMAC-based Extract-and-Expand Key Derivation Function |
| MFA | Multi-Factor Authentication，多因素认证 |
| MCP | Model Context Protocol，模型上下文协议 |

### B. 参考文档

- [API 详细文档](../API.md) - RESTful API 完整参考
- [设计规范](./CredBridge_CN_设计规范_v1.0.md) - 系统设计文档
- [架构设计](../_bmad-output/planning-artifacts/architecture.md) - 技术架构决策
- [安全白皮书](https://credbridge.io/security) 🔗 - 外部链接（需网络访问）
- [部署指南](../docker/README.md) - Docker 部署配置
- [SDK 文档](./SDK_GUIDE.md) - TypeScript/Rust SDK 使用指南
- [MCP 集成](./MCP_INTEGRATION.md) - Model Context Protocol 配置

### C. 更新日志

| 版本 | 日期 | 变更内容 |
|------|------|----------|
| 1.0 | 2026-03-11 | 初始版本 |

---

**© 2026 CredBridge. All rights reserved.**

**安全声明**: 本文档包含 CredBridge 系统的敏感配置信息，请妥善保管，不要泄露给未授权人员。
