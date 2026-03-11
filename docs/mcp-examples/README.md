# CredBridge MCP 配置示例

## Claude Desktop 配置

将以下内容添加到 `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "credbridge": {
      "command": "/path/to/credbridge-mcp-server",
      "env": {
        "CREDBRIDGE_API_URL": "http://localhost:8080",
        "CREDBRIDGE_LOG_LEVEL": "info"
      }
    }
  }
}
```

## Cursor IDE 配置

在 Cursor 设置中添加 MCP Server:

```json
{
  "mcp": {
    "servers": {
      "credbridge": {
        "command": "/path/to/credbridge-mcp-server",
        "args": ["--config", "/path/to/config.json"]
      }
    }
  }
}
```

## 支持的工具

### credential_create
创建新凭证

```json
{
  "name": "credential_create",
  "arguments": {
    "credential_type": "username_password",
    "metadata": {
      "name": "Production Database",
      "description": "Main production database credentials"
    },
    "scope": "production:db:read"
  }
}
```

### credential_get
获取凭证（在 TEE 内解密）

```json
{
  "name": "credential_get",
  "arguments": {
    "credential_id": "cred_01ARZ3N...",
    "scope": "production:db:read"
  }
}
```

### credential_list
列出凭证元数据

```json
{
  "name": "credential_list",
  "arguments": {
    "filter": {
      "type": "username_password",
      "scope_prefix": "production"
    }
  }
}
```

### credential_delete
删除凭证（软删除）

```json
{
  "name": "credential_delete",
  "arguments": {
    "credential_id": "cred_01ARZ3N..."
  }
}
```

## 环境变量

| 变量 | 描述 | 默认值 |
|------|------|--------|
| CREDBRIDGE_API_URL | CredBridge API 地址 | http://localhost:8080 |
| CREDBRIDGE_LOG_LEVEL | 日志级别 | info |
| CREDBRIDGE_TIMEOUT | 请求超时（秒） | 30 |
| CREDBRIDGE_TEE_MODE | TEE 模式 | simulation |

## 安全最佳实践

1. **Scope 权限**: 始终使用最小权限 scope
2. **凭证轮换**: 定期轮换敏感凭证
3. **审计日志**: 监控所有凭证访问
4. **TEE 验证**: 生产环境启用 TEE 远程认证