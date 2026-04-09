# Toani Vault CLI Skill

## 目标

为 AI 代理提供一套稳定、可复用的 `@toani/vault-cli`（`toani`）调用规范，用于 CredBridge 的认证、凭证、Token、服务账号、沙箱执行与审计操作。

## 安装手段

- Registry 安装（推荐）：
```bash
npm install -g @toani/vault-cli@0.0.1
```
- 本地源码安装（用于开发/调试）：
```bash
cd /Users/yvan/AIWorkspace/credbridge/cli
npm install
npm run build
npm pack
npm install -g ./toani-vault-cli-0.0.1.tgz
```
- 先决条件：
```bash
node -v   # >= 22
npm -v
```

## 工具说明

- 可执行命令：`toani`
- 包名：`@toani/vault-cli`
- 主要命令组：
- `auth`：登录状态、会话交换、成员信息、访问令牌管理
- `config`：初始化配置、查看/修改配置项
- `credentials`：凭证增删改查、解密、版本回滚
- `tokens`：Token 创建、校验、统计、吊销
- `service-accounts`：服务账号与子 token 管理
- `sandbox`：沙箱会话创建、执行操作、查询操作
- `audit`：审计日志检索、导出、校验
- 全局参数：
- `--output json|table`
- `--base-url <URL>`
- `--token <TOKEN>`

## 使用方式

- 地址解析优先级（高到低）：
- `--base-url`
- `TOANI_BASE_URL`
- `CREDBRIDGE_BASE_URL`
- `config.baseUrl`
- 默认值：`https://dev-credbridge.bitkinetic.com/`

- Token 解析优先级（高到低）：
- `--token`
- `config.token`
- `config.sessionToken`

- 初始化建议：
```bash
toani config init --url https://dev-credbridge.bitkinetic.com/ --token <API_ACCESS_TOKEN>
toani auth status
```

- 面向 AI 的输出建议：
- 自动化链路默认使用 `--output json`，便于结构化解析
- 交互排障使用 `table`，便于人工阅读

## 最佳使用案例

- 案例 1：CI/CD 中按环境切换地址，不落盘改配置
```bash
export TOANI_BASE_URL=https://dev-credbridge.bitkinetic.com/
toani --output json credentials list --service-id svc_xxx
```

- 案例 2：最小步骤创建并验证服务账号 token
```bash
toani service-accounts create --name bot-ci --scope credential:read,credential:write
toani service-accounts token create <service-account-id> --scope credential:read --ttl-seconds 3600
toani tokens verify --token <TOKEN>
```

- 案例 3：审计导出与校验闭环
```bash
toani audit logs --from 2026-04-01T00:00:00Z --to 2026-04-09T00:00:00Z --limit 200
toani audit export --format json --from 2026-04-01T00:00:00Z --to 2026-04-09T00:00:00Z
toani audit verify --payload '{"log_id":"..."}'
```

## 常见异常与建议解决办法

- 异常：`Unknown command group` 或参数报缺失
- 建议：先执行 `toani --help`，确认命令组与参数名；所有 `--xxx` 选项区分完整拼写

- 异常：401/403（鉴权失败）
- 建议：优先检查 `--token` 是否覆盖了配置；执行 `toani auth status` 与 `toani tokens verify`

- 异常：连接失败或超时（DNS/网络/TLS）
- 建议：确认 `TOANI_BASE_URL` 或 `--base-url` 是否正确；用 `curl <base-url>/health` 先测连通性

- 异常：返回非预期环境数据
- 建议：检查地址优先级是否被环境变量覆盖；必要时在命令中显式传 `--base-url`

- 异常：本地配置污染（历史 token/地址）
- 建议：执行 `toani config show` 审核；必要时 `toani config set baseUrl <url>` 和 `toani config set token <token>`

- 异常：发布 npm 包时报 2FA/Scope 权限错误
- 建议：使用具备 publish 权限的 token；确认组织 scope 成员身份与包访问级别（public/private）设置

## 安全注意事项

- 本地配置文件路径：`~/.toani/config.json`
- 文件中含 token 信息，禁止提交到仓库
- 自动化场景优先使用环境变量注入 token，不在脚本中明文硬编码
