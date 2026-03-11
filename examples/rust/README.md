# CredBridge Rust SDK 示例

本目录包含 CredBridge Rust SDK 的使用示例。

## 配置环境变量

```bash
export CREDBRIDGE_BASE_URL="https://api.credbridge.io"
export CREDBRIDGE_TOKEN="your-api-token"
```

## 运行示例

### 运行所有示例

```bash
cargo run
```

### 运行单个示例

```bash
# 基础使用示例
cargo run --example basic_usage

# 批量操作示例
cargo run --example batch_operations

# Token 管理示例
cargo run --example token_management

# 错误处理示例
cargo run --example error_handling

# Axum 集成示例
cargo run --example axum_integration
```

## 示例说明

### 1. 基础使用示例 (basic_usage)

展示凭证创建、获取、解密和删除的基本操作：

- 创建用户名密码凭证
- 创建 API Key 凭证
- 获取凭证列表
- 获取凭证详情
- 解密凭证
- 检查 Token 信息
- 删除凭证

### 2. 批量操作示例 (batch_operations)

展示批量创建、获取和删除凭证的操作：

- 批量创建凭证
- 并发获取凭证详情
- 按服务过滤凭证
- 并发解密多个凭证
- 批量删除凭证
- 凭证轮换示例

### 3. Token 管理示例 (token_management)

展示 Token 验证、权限检查和刷新操作：

- 获取 Token 信息
- 检查 Token 有效性
- 检查权限
- 权限组合检查
- 验证 Token（向服务器确认）
- Token 刷新监控

### 4. 错误处理示例 (error_handling)

展示各种错误场景的处理方式：

- 凭证不存在错误
- 权限不足错误
- 网络错误重试
- Token 过期处理
- 通用错误处理函数
- 带重试的操作
- 批量错误处理

### 5. Axum 集成示例 (axum_integration)

展示如何在 Axum Web 应用中集成 CredBridge SDK：

- 权限检查中间件
- RESTful API 端点
- 错误响应处理
- 状态管理

## API 端点

Axum 示例提供以下 API 端点：

| 方法 | 路径 | 描述 | 所需权限 |
|------|------|------|----------|
| GET | `/api/me` | 获取当前用户信息 | 无 |
| GET | `/api/credentials` | 获取凭证列表 | `credential:read` |
| GET | `/api/credentials/:id` | 获取凭证详情 | `credential:read` |
| POST | `/api/credentials/:id/decrypt` | 解密凭证 | `credential:decrypt` |
| POST | `/api/credentials` | 创建凭证 | `credential:write` |
| DELETE | `/api/credentials/:id` | 删除凭证 | `credential:write` |

## 示例代码结构

```
examples/rust/
├── Cargo.toml
├── src/
│   ├── main.rs               # 主入口
│   ├── basic_usage.rs        # 基础使用示例
│   ├── batch_operations.rs   # 批量操作示例
│   ├── token_management.rs   # Token 管理示例
│   ├── error_handling.rs     # 错误处理示例
│   └── axum_integration.rs   # Axum 集成示例
└── README.md
```

## 依赖

- `credbridge-sdk`: CredBridge Rust SDK
- `tokio`: 异步运行时
- `serde_json`: JSON 序列化
- `chrono`: 日期时间处理
- `anyhow`: 错误处理
- `futures`: 异步工具

## 开发

添加新示例：

1. 在 `src/` 目录下创建新的示例文件
2. 在 `main.rs` 中添加模块引用
3. 在 `main()` 函数中调用示例函数
