# CredBridge CLI

CredBridge 命令行管理工具

## 安装

```bash
cargo install --path cli
```

## 快速开始

```bash
# 登录到 CredBridge 服务
credbridge auth login --url https://api.credbridge.io --token <your-token>

# 查看登录状态
credbridge auth status

# 列出所有凭证
credbridge credentials list

# 创建新凭证
credbridge credentials create --name "api-key" --type api_key --value "secret123"
```

## 命令参考

### 认证 (auth)

| 命令 | 描述 |
|------|------|
| `credbridge auth login` | 登录到服务 |
| `credbridge auth status` | 查看登录状态 |
| `credbridge auth logout` | 注销 |

### 凭证 (credentials)

| 命令 | 描述 |
|------|------|
| `credbridge credentials list` | 列出凭证 |
| `credbridge credentials get <id>` | 获取单个凭证 |
| `credbridge credentials create` | 创建凭证 |
| `credbridge credentials update <id>` | 更新凭证 |
| `credbridge credentials delete <id>` | 删除凭证 |
| `credbridge credentials decrypt <id>` | 解密凭证 |
| `credbridge credentials versions <id>` | 查看版本历史 |
| `credbridge credentials rollback <id> <version>` | 回滚版本 |

**示例：**

```bash
# 列出凭证（表格格式）
credbridge credentials list

# 列出凭证（JSON 格式）
credbridge --output json credentials list

# 创建 API Key 凭证
credbridge credentials create \
  --name "production-api-key" \
  --type api_key \
  --value "sk-live-xxx"

# 创建用户名密码凭证
credbridge credentials create \
  --name "db-credentials" \
  --type username_password \
  --value "secret-password" \
  --metadata "username=admin"

# 解密凭证
credbridge credentials decrypt <credential-id>

# 删除凭证（带确认）
credbridge credentials delete <credential-id>

# 强制删除凭证
credbridge credentials delete <credential-id> --force
```

### Token (tokens)

| 命令 | 描述 |
|------|------|
| `credbridge tokens create` | 创建 Token |
| `credbridge tokens list` | 列出 Token |
| `credbridge tokens revoke <id>` | 撤销 Token |
| `credbridge tokens verify` | 验证 Token |

**示例：**

```bash
# 验证当前配置的 Token
credbridge tokens verify

# 撤销当前 Token
credbridge tokens revoke
```

### 审计 (audit)

| 命令 | 描述 |
|------|------|
| `credbridge audit logs` | 查询审计日志 |
| `credbridge audit export <file>` | 导出审计日志 |
| `credbridge audit verify` | 验证日志完整性 |

### 沙箱 (sandbox)

| 命令 | 描述 |
|------|------|
| `credbridge sandbox create-session` | 创建沙箱会话 |
| `credbridge sandbox list-sessions` | 列出沙箱会话 |
| `credbridge sandbox get-session <id>` | 获取会话详情 |
| `credbridge sandbox terminate <id>` | 终止会话 |
| `credbridge sandbox execute <session-id>` | 执行操作 |
| `credbridge sandbox get-operation <id>` | 获取操作结果 |
| `credbridge sandbox stats` | 查看沙箱统计 |

### 配置 (config)

| 命令 | 描述 |
|------|------|
| `credbridge config init` | 初始化配置 |
| `credbridge config show` | 查看配置 |
| `credbridge config set <key> <value>` | 设置配置项 |
| `credbridge config get <key>` | 获取配置项 |

**示例：**

```bash
# 交互式初始化配置
credbridge config init

# 使用参数初始化配置
credbridge config init \
  --url https://api.credbridge.io \
  --token "v4.local.xxx"

# 查看当前配置
credbridge config show

# 设置输出格式为 JSON
credbridge config set output_format json

# 设置超时时间
credbridge config set timeout 60
```

## 全局选项

| 选项 | 描述 |
|------|------|
| `-o, --output <format>` | 输出格式: table, json (默认: table) |
| `-c, --config <path>` | 配置文件路径 |
| `-v, --verbose` | 详细日志输出 |
| `-h, --help` | 显示帮助信息 |
| `-V, --version` | 显示版本信息 |

## 环境变量

| 变量 | 描述 |
|------|------|
| `CREDBRIDGE_URL` | 服务 URL |
| `CREDBRIDGE_TOKEN` | API Token |
| `HOME` | 配置目录 (默认: ~/.config/credbridge/) |

## 配置示例

```toml
# ~/.config/credbridge/config.toml
url = "https://api.credbridge.io"
token = "v4.local.xxx"
output_format = "table"
timeout = 30
```

配置文件权限自动设置为 0600（仅用户可读写）。

## 凭证类型

支持的凭证类型：

| 类型 | 说明 |
|------|------|
| `username_password` | 用户名密码 |
| `api_key` | API 密钥 |
| `oauth_refresh` | OAuth 刷新令牌 |
| `session_cookie` | 会话 Cookie |
| `kyc_document` | KYC 文档 |
| `certificate` | 证书 |
| `ssh_key` | SSH 密钥 |
| `database_connection` | 数据库连接 |

## 开发

```bash
# 编译
cargo build

# 运行测试
cargo test

# 发布构建
cargo build --release
```

## 许可证

MIT
