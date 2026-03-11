# AI Agent 使用指南

本指南帮助 AI Agent 开发者快速集成 CredBridge 凭证管理服务。

## 快速开始

### 1. 确认 MCP Server 可用

首先检查 CredBridge MCP Server 是否已配置：

```json
{
  "name": "tee_status",
  "arguments": {}
}
```

如果返回 TEE 状态信息，说明服务正常运行。

### 2. 创建第一个凭证

```json
{
  "name": "create_credential",
  "arguments": {
    "tenant_id": "your_tenant",
    "user_id": "your_user",
    "service_id": "github",
    "credential_type": "username_password",
    "plaintext_data": {
      "username": "your_github_username",
      "password": "your_github_password"
    },
    "scope": "credential:write"
  }
}
```

保存返回的 `credential_id`，后续操作需要使用。

## 凭证类型速查表

### UsernamePassword

用于用户名/密码认证的服务。

```json
{
  "credential_type": "username_password",
  "plaintext_data": {
    "username": "user@example.com",
    "password": "secure_password",
    "totp_secret": "optional_2fa_secret"
  }
}
```

**适用服务：** GitHub, GitLab, Bitbucket, 企业内网, VPN, 数据库等

### ApiKey

用于 API 密钥认证的服务。

```json
{
  "credential_type": "api_key",
  "plaintext_data": {
    "api_key": "your_api_key",
    "api_secret": "your_api_secret",
    "endpoint": "https://api.example.com/v1"
  }
}
```

**适用服务：** AWS, Azure, GCP, Stripe, Twilio, SendGrid, OpenAI 等

### OAuthRefresh

用于 OAuth 2.0 刷新令牌。

```json
{
  "credential_type": "oauth_refresh",
  "plaintext_data": {
    "refresh_token": "your_refresh_token",
    "client_id": "your_client_id",
    "client_secret": "your_client_secret",
    "token_endpoint": "https://oauth.example.com/token"
  }
}
```

**适用服务：** Google Workspace, Microsoft 365, Salesforce, Slack, Notion 等

### SessionCookie

用于基于 Cookie 的会话管理。

```json
{
  "credential_type": "session_cookie",
  "plaintext_data": {
    "session_id": "session_identifier",
    "csrf_token": "csrf_protection_token",
    "domain": ".example.com"
  }
}
```

**适用服务：** 内部管理系统、 legacy 应用等

### KycDocument

用于 KYC（了解你的客户）文档。

```json
{
  "credential_type": "kyc_document",
  "plaintext_data": {
    "document_type": "passport",
    "document_number": "encrypted_document_reference",
    "issuing_country": "US"
  }
}
```

**适用服务：** 金融服务、合规审查等

## 常见工作流

### 工作流 1: 管理 GitHub 凭证

**步骤 1: 创建凭证**

```json
{
  "name": "create_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "developer_1",
    "service_id": "github",
    "credential_type": "username_password",
    "plaintext_data": {
      "username": "dev1@acme.com",
      "password": "github_personal_access_token",
      "2fa_enabled": true
    },
    "scope": "credential:write"
  }
}
```

**步骤 2: 验证凭证已创建**

```json
{
  "name": "list_credentials",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "developer_1",
    "service_id": "github"
  }
}
```

**步骤 3: 需要时使用凭证**

```json
{
  "name": "decrypt_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "developer_1",
    "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "scope": "credential:decrypt"
  }
}
```

### 工作流 2: 轮换 AWS 访问密钥

**步骤 1: 列出现有 AWS 凭证**

```json
{
  "name": "list_credentials",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "devops_team",
    "service_id": "aws"
  }
}
```

**步骤 2: 更新为新密钥**

```json
{
  "name": "update_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "devops_team",
    "credential_id": "existing_aws_cred_id",
    "plaintext_data": {
      "access_key_id": "NEW_AKIAIOSFODNN7EXAMPLE",
      "secret_access_key": "NEW_wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
      "region": "us-west-2"
    },
    "scope": "credential:write",
    "expires_at": 1743465600
  }
}
```

### 工作流 3: 清理过期凭证

**步骤 1: 列出所有凭证**

```json
{
  "name": "list_credentials",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe"
  }
}
```

**步骤 2: 删除不再需要的凭证**

```json
{
  "name": "delete_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "credential_id": "credential_to_delete",
    "scope": "credential:write"
  }
}
```

## 安全提醒

### ⚠️ 解密凭证前检查

在执行 `decrypt_credential` 前，Agent 应该：

1. **验证 TEE 状态**

```json
{
  "name": "tee_status",
  "arguments": {}
}
```

确认 `enabled: true` 和 `quote_valid: true`。

2. **确认操作必要性**

只在真正需要明文凭证时执行解密，如：
- 调用第三方 API
- 执行自动化任务
- 用户明确要求查看

3. **避免日志泄露**

解密后的凭证**不应该**：
- 记录到日志文件
- 显示在用户界面（除非用户明确要求）
- 传输到外部服务

### ✅ 安全最佳实践

1. **使用最小权限 Scope**

不要请求 `admin` Scope 除非绝对必要：

```json
// 好的做法
"scope": "credential:read"

// 仅在需要时
"scope": "credential:read credential:decrypt"
```

2. **设置合理过期时间**

```json
// API 密钥 90 天后过期
{
  "expires_at": 1743465600
}
```

3. **定期轮换凭证**

建议每 90 天更新敏感凭证：
- AWS/GCP/Azure 访问密钥
- OAuth 刷新令牌
- API 密钥

## 错误处理

### 常见错误代码

| 错误 | 说明 | 解决方案 |
|------|------|----------|
| `PermissionDenied` | Scope 权限不足 | 检查提供的 Scope 是否包含所需权限 |
| `NotFound` | 凭证不存在 | 检查 credential_id 是否正确 |
| `InvalidInput` | 参数格式错误 | 检查 credential_type 是否为有效值 |
| `VaultError` | 存储操作失败 | 联系管理员检查 Vault 服务状态 |

### 错误响应示例

```json
{
  "error": "Permission denied",
  "message": "Missing 'credential:write' scope",
  "required_scope": "credential:write"
}
```

## 批量操作模式

### 批量创建凭证

```javascript
const credentials = [
  { service: 'github', type: 'username_password', data: {...} },
  { service: 'aws', type: 'api_key', data: {...} },
  { service: 'slack', type: 'oauth_refresh', data: {...} }
];

for (const cred of credentials) {
  await client.callTool('create_credential', {
    tenant_id: 'acme_corp',
    user_id: 'team_lead',
    service_id: cred.service,
    credential_type: cred.type,
    plaintext_data: cred.data,
    scope: 'credential:write'
  });
}
```

### 批量删除过期凭证

```javascript
// 1. 列出所有凭证
const list = await client.callTool('list_credentials', {
  tenant_id: 'acme_corp',
  user_id: 'john_doe'
});

// 2. 过滤过期凭证
const now = Date.now() / 1000;
const expiredCreds = list.credentials.filter(c =>
  c.expires_at && new Date(c.expires_at).getTime() / 1000 < now
);

// 3. 删除过期凭证
for (const cred of expiredCreds) {
  await client.callTool('delete_credential', {
    tenant_id: 'acme_corp',
    user_id: 'john_doe',
    credential_id: cred.id,
    scope: 'credential:write'
  });
}
```

## 与 CI/CD 集成

### GitHub Actions 示例

```yaml
name: Deploy with CredBridge

on: [push]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Get AWS Credentials from CredBridge
        id: creds
        run: |
          # 调用 MCP Server 获取凭证
          echo "::set-output name=aws_key::$(mcp-call decrypt_credential ...)"

      - name: Deploy to AWS
        env:
          AWS_ACCESS_KEY_ID: ${{ steps.creds.outputs.aws_key }}
        run: |
          aws s3 sync ./build s3://my-bucket
```

### 预提交钩子示例

```bash
#!/bin/bash
# .git/hooks/pre-commit

# 检查是否使用了硬编码凭证
if git diff --cached | grep -E '(password|secret|key).*=.*["\'][^"\']+["\']'; then
  echo "Error: Hardcoded credentials detected!"
  echo "Please use CredBridge to manage credentials."
  exit 1
fi
```

## 故障排除

### 凭证创建成功但无法解密

1. 检查 TEE 状态：`tee_status`
2. 确认 credential_id 正确
3. 验证 `credential:decrypt` Scope

### 更新凭证后数据未生效

1. 确认使用了正确的 `credential_id`
2. 检查 `plaintext_data` 格式是否正确
3. 验证 `credential:write` Scope

### 删除凭证后仍能列出

这是正常行为（软删除）：
- 凭证被标记为删除但保留元数据
- 使用 `include_deleted: false` 过滤已删除凭证
- 审计日志中保留删除记录

## 高级用法

### 自定义凭证字段

`plaintext_data` 支持任意 JSON 结构：

```json
{
  "credential_type": "api_key",
  "plaintext_data": {
    "api_key": "key123",
    "rate_limit": 1000,
    "allowed_ips": ["192.168.1.0/24"],
    "metadata": {
      "created_by": "admin",
      "purpose": "production"
    }
  }
}
```

### 多租户隔离

确保每个 `tenant_id` 只能访问自己的凭证：

```json
// Tenant A 创建凭证
{
  "tenant_id": "company_a",
  "user_id": "admin",
  ...
}

// Tenant B 无法访问
{
  "tenant_id": "company_b",  // 不同租户
  "user_id": "admin",
  "credential_id": "company_a_credential_id"  // 返回 NotFound
}
```

---

**需要帮助？** 查看 [MCP 集成完整文档](./MCP_INTEGRATION.md) 或联系支持团队。
