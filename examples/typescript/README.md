# Toani Vault SDK TypeScript 示例

本目录包含 Toani Vault SDK 的使用示例。

## 安装依赖

```bash
npm install
```

## 配置环境变量

```bash
export TOANI_VAULT_BASE_URL="https://api.toani.io"
export TOANI_VAULT_TOKEN="your-api-token"
```

## 运行示例

### 快速启动

使用提供的脚本自动获取 Token 并运行示例：

```bash
# 运行基础示例（默认）
./run-examples.sh

# 运行指定示例
./run-examples.sh basic   # 基础使用示例
./run-examples.sh batch   # 批量操作示例
./run-examples.sh token   # Token 管理示例
./run-examples.sh error   # 错误处理示例
./run-examples.sh express # Express 集成示例
```

### 手动运行

1. 获取 Token：
```bash
curl -X POST http://localhost:8082/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}'
```

2. 设置环境变量：
```bash
export TOANI_VAULT_BASE_URL="http://localhost:8082"
export TOANI_VAULT_TOKEN="v4.local.xxx..."
```

3. 运行示例：
```bash
npm run basic
```

### 1. 基础使用示例

展示凭证创建、获取、解密和删除的基本操作：

```bash
npm run basic
```

### 2. 批量操作示例

展示批量创建、获取和删除凭证的操作：

```bash
npm run batch
```

### 3. Token 管理示例

展示 Token 验证、权限检查和刷新操作：

```bash
npm run token
```

### 4. 错误处理示例

展示各种错误场景的处理方式：

```bash
npm run error
```

### 5. Express 集成示例

展示如何在 Express 应用中集成 Toani Vault SDK：

```bash
npm run express
```

然后访问 http://localhost:3000

## API 端点

Express 示例提供以下 API 端点：

| 方法 | 路径 | 描述 | 所需权限 |
|------|------|------|----------|
| GET | `/health` | 健康检查 | 无 |
| GET | `/api/me` | 获取当前用户信息 | 无 |
| GET | `/api/credentials` | 获取凭证列表 | `credential:read` |
| GET | `/api/credentials/:id` | 获取凭证详情 | `credential:read` |
| POST | `/api/credentials/:id/decrypt` | 解密凭证 | `credential:decrypt` |
| POST | `/api/credentials` | 创建凭证 | `credential:write` |
| DELETE | `/api/credentials/:id` | 删除凭证 | `credential:write` |

## 示例代码结构

```
examples/typescript/
├── basic-usage.ts        # 基础使用示例
├── batch-operations.ts   # 批量操作示例
├── token-management.ts   # Token 管理示例
├── error-handling.ts     # 错误处理示例
├── express-integration.ts # Express 集成示例
├── package.json
├── tsconfig.json
└── README.md
```

## 依赖

- `@toani/vault-sdk`: Toani Vault SDK
- `express`: Web 框架示例
- `tsx`: TypeScript 执行器
