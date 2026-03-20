# CredBridge MCP 集成指南

本文档介绍如何将 CredBridge 与 AI Agent 通过 MCP (Model Context Protocol) 集成。

## 概述

CredBridge MCP Server 为 AI Agent 提供安全的凭证管理能力，支持：

- 创建和管理多类型凭证（用户名密码、API 密钥、OAuth 令牌等）
- 细粒度的 Scope 权限控制
- TEE（可信执行环境）内的安全解密
- 完整的审计日志记录

## 架构

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   AI Agent      │────▶│  MCP Protocol    │────▶│  CredBridge     │
│  (Claude/Cursor)│     │  (stdio/SSE)     │     │  MCP Server     │
└─────────────────┘     └──────────────────┘     └────────┬────────┘
                                                          │
                           ┌──────────────────────────────┼────────┐
                           │                              │        │
                    ┌──────▼──────┐  ┌──────────▼────────┐  ┌─────▼─────┐
                    │   Vault     │  │   Audit Logger    │  │    TEE    │
                    │   Storage   │  │   (immudb)        │  │  (SGX)    │
                    └─────────────┘  └───────────────────┘  └───────────┘
```

## 配置

### 1. MCP Server 配置

在 MCP 客户端配置文件中添加 CredBridge MCP Server：

**Claude Desktop (`~/Library/Application Support/Claude/claude_desktop_config.json`):**

```json
{
  "mcpServers": {
    "credbridge": {
      "command": "/path/to/credbridge-mcp-server",
      "env": {
        "CREDBRIDGE_LOG_LEVEL": "info"
      }
    }
  }
}
```

**Cursor (`~/.cursor/mcp.json`):**

```json
{
  "servers": [
    {
      "name": "credbridge",
      "command": "/path/to/credbridge-mcp-server",
      "type": "stdio"
    }
  ]
}
```

### 2. SSE 模式配置

如需通过 SSE（Server-Sent Events）模式连接：

```json
{
  "mcpServers": {
    "credbridge": {
      "url": "http://localhost:3721/sse",
      "type": "sse"
    }
  }
}
```

启动 SSE 服务器：

```bash
CREDBRIDGE_MCP_TRANSPORT=sse \
CREDBRIDGE_MCP_SSE_PORT=3721 \
./credbridge-mcp-server
```

## 工具使用示例

### 创建凭证

**UsernamePassword 类型：**

```json
{
  "name": "create_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "service_id": "github",
    "credential_type": "username_password",
    "plaintext_data": {
      "username": "johndoe",
      "password": "secure_password_123",
      "2fa_enabled": true
    },
    "scope": "credential:write"
  }
}
```

**API Key 类型：**

```json
{
  "name": "create_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "service_id": "aws",
    "credential_type": "api_key",
    "plaintext_data": {
      "access_key_id": "AKIAIOSFODNN7EXAMPLE",
      "secret_access_key": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
      "region": "us-east-1"
    },
    "scope": "credential:write",
    "expires_at": 1735689600
  }
}
```

**OAuth Refresh Token 类型：**

```json
{
  "name": "create_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "service_id": "google_workspace",
    "credential_type": "oauth_refresh",
    "plaintext_data": {
      "refresh_token": "1//0dXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      "client_id": "123456789.apps.googleusercontent.com",
      "scopes": ["https://www.googleapis.com/auth/gmail.readonly"]
    },
    "scope": "credential:write"
  }
}
```

### 列出凭证

```json
{
  "name": "list_credentials",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe"
  }
}
```

**响应：**

```json
{
  "credentials": [
    {
      "id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
      "service_id": "github",
      "credential_type": "username_password",
      "created_at": "2026-03-11T10:30:00Z",
      "expires_at": null
    },
    {
      "id": "018f1b4f-8faf-8f4b-9c6d-3e5f7g9b1c3d",
      "service_id": "aws",
      "credential_type": "api_key",
      "created_at": "2026-03-11T11:00:00Z",
      "expires_at": "2026-12-31T23:59:59Z"
    }
  ],
  "total": 2
}
```

### 更新凭证

```json
{
  "name": "update_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "plaintext_data": {
      "username": "johndoe_v2",
      "password": "new_secure_password_456",
      "2fa_enabled": true
    },
    "scope": "credential:write"
  }
}
```

### 删除凭证

```json
{
  "name": "delete_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "scope": "credential:write"
  }
}
```

### 解密凭证（高风险操作）

```json
{
  "name": "decrypt_credential",
  "arguments": {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
    "scope": "credential:decrypt"
  }
}
```

**响应：**

```json
{
  "credential_id": "018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c",
  "decrypted_data": "{\"username\":\"johndoe\",\"password\":\"secure_password_123\",\"2fa_enabled\":true}",
  "decrypted_at": "2026-03-11T12:00:00Z",
  "tee_verified": true,
  "warning": "This data is sensitive and should be handled securely."
}
```

### 查询 TEE 状态

```json
{
  "name": "tee_status",
  "arguments": {}
}
```

**响应：**

```json
{
  "enabled": true,
  "mrenclave": "a1b2c3d4e5f6...",
  "mrsigner": "f6e5d4c3b2a1...",
  "isv_svn": 1,
  "quote_valid": true
}
```

## Scope 权限系统

### Scope 类型

| Scope | 权限说明 | 工具 |
|-------|---------|------|
| `credential:read` | 读取凭证元数据 | list_credentials, get_credential |
| `credential:write` | 创建/更新/删除凭证 | create_credential, update_credential, delete_credential |
| `credential:delete` | 删除凭证（可与 write 互换） | delete_credential |
| `credential:decrypt` | 解密凭证内容 | decrypt_credential |
| `admin` | 所有权限 | 所有工具 |

### 权限继承

- `admin` Scope 包含所有其他 Scope 的权限
- 某些操作需要特定 Scope（如解密需要 `credential:decrypt`）
- 权限不足时会返回 `PermissionDenied` 错误并记录审计日志

## 安全最佳实践

### 1. 最小权限原则

为每个 Agent 分配最小必要的 Scope：

```json
// 仅允许读取的 Agent
{
  "scope": "credential:read"
}

// 允许读写但不允许解密的 Agent
{
  "scope": "credential:read credential:write"
}

// 完整权限的 Agent（仅管理员）
{
  "scope": "admin"
}
```

### 2. 凭证过期策略

为高敏感度凭证设置合理的过期时间：

```json
{
  "credential_type": "api_key",
  "expires_at": 1735689600  // 90 天后过期
}
```

### 3. 审计监控

定期检查审计日志：

- 失败的权限尝试
- 异常的高频解密操作
- 非工作时间的凭证访问

### 4. TEE 验证

在使用解密功能前，验证 TEE 状态：

```json
{
  "name": "tee_status",
  "arguments": {}
}
```

确保 `enabled: true` 且 `quote_valid: true` 后再进行敏感操作。

## 故障排除

### 常见问题

**1. MCP Server 无法启动**

```bash
# 检查日志
RUST_LOG=debug cargo run
```

**2. 权限被拒绝**

- 确认提供的 Scope 包含所需权限
- 检查 `admin` Scope 是否正确配置

**3. 凭证未找到**

- 确认 `credential_id` 格式正确（UUID v7）
- 确认 `tenant_id` 和 `user_id` 与创建时一致
- 检查凭证是否已被删除（软删除）

**4. TEE 状态异常**

- 确认运行在支持 SGX 的硬件上
- 检查 SGX 驱动是否正确安装
- 查看 `/var/log/credbridge/tee.log` 获取详细错误

### 调试模式

启用详细日志：

```bash
CREDBRIDGE_LOG_LEVEL=debug ./credbridge-mcp-server
```

## 集成示例

### Python 客户端

```python
import json
import subprocess

class CredBridgeMCPClient:
    def __init__(self, server_path):
        self.proc = subprocess.Popen(
            [server_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True
        )

    def call_tool(self, name, arguments):
        request = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()
        response = json.loads(self.proc.stdout.readline())
        return response["result"]

# 使用示例
client = CredBridgeMCPClient("./credbridge-mcp-server")

# 创建凭证
result = client.call_tool("create_credential", {
    "tenant_id": "acme_corp",
    "user_id": "john_doe",
    "service_id": "github",
    "credential_type": "username_password",
    "plaintext_data": {"username": "user", "password": "pass"},
    "scope": "credential:write"
})
print(f"Created credential: {result['credential_id']}")
```

### Node.js 客户端

```javascript
const { spawn } = require('child_process');

class CredBridgeMCPClient {
  constructor(serverPath) {
    this.proc = spawn(serverPath);
    this.buffer = '';
    this.proc.stdout.on('data', (data) => {
      this.buffer += data.toString();
      const lines = this.buffer.split('\n');
      this.buffer = lines.pop();
      lines.forEach(line => this.handleResponse(line));
    });
  }

  callTool(name, arguments) {
    return new Promise((resolve) => {
      const request = {
        jsonrpc: '2.0',
        id: Date.now(),
        method: 'tools/call',
        params: { name, arguments }
      };
      this.proc.stdin.write(JSON.stringify(request) + '\n');
      this.pending = resolve;
    });
  }

  handleResponse(line) {
    if (this.pending && line) {
      const response = JSON.parse(line);
      this.pending(response.result);
      this.pending = null;
    }
  }
}

// 使用示例
const client = new CredBridgeMCPClient('./credbridge-mcp-server');

client.callTool('create_credential', {
  tenant_id: 'acme_corp',
  user_id: 'john_doe',
  service_id: 'github',
  credential_type: 'username_password',
  plaintext_data: { username: 'user', password: 'pass' },
  scope: 'credential:write'
}).then(result => {
  console.log('Created credential:', result.credential_id);
});
```

## 参考资料

- [MCP 协议规范](https://spec.modelcontextprotocol.io/)
- [CredBridge 架构文档](./architecture.md)
- [审计日志 API](./audit-api.md)
- [TEE 远程认证指南](./remote-attestation.md)

## 支持

如有问题，请提交 Issue 或联系支持团队：

- GitHub Issues: https://github.com/credbridge/credbridge/issues
- Email: support@credbridge.ai
