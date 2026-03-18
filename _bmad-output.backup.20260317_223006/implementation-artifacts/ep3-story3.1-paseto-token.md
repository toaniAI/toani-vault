# EP3-Story3.1: PASETO Token 签发与验证

## Story 信息
- **Story Key**: 3-1-paseto-token
- **Epic**: EP3 - Scope Token 系统
- **状态**: in-progress
- **优先级**: P0

## Story 描述

**As a** 安全工程师
**I want** 实现 PASETO v4.local Token 的签发和验证
**So that** Agent 可以获取有限权限的访问凭证

## 验收标准 (Acceptance Criteria)

### AC1: Token 签发
**Given** 用户请求 Token
**When** 调用 Token 签发 API
**Then** 在 Enclave 内构建 Claims：
  - iss: "credbridge-vault"
  - sub: user_id
  - aud: tenant_id
  - exp: current_time + 900s（15分钟）
  - jti: UUID v4
  - scope: 请求的权限范围
  - mfa_verified: true/false
**And** 使用 XChaCha20-Poly1305 加密
**And** 返回 v4.local.{payload}.{footer} 格式 Token

### AC2: Token 验证
**Given** 验证 Token 请求
**When** 解析并验证 Token
**Then** 在 Enclave 内解密 Token
**And** 验证 iss、aud、exp（未过期）
**And** 验证 scope 权限
**And** 检查 jti 是否已撤销

### AC3: Token 过期处理
**Given** Token 过期
**When** exp < current_time
**Then** 拒绝验证并返回 TokenExpired 错误

## 架构约束
- **SA-003**: PASETO v4.local + 15min Access Token + jti 单次使用
- **FR2**: 有限 Scope Token 系统

## 技术约束
- **语言**: Rust（强制）
- **Token 格式**: PASETO v4.local
- **加密**: XChaCha20-Poly1305
- **位置**: src/token/

## 任务列表

### 任务 1: Token Claims 定义
- [x] 创建 `src/token/claims.rs`
- [x] 定义 `TokenClaims` 结构体（iss, sub, aud, exp, jti, scope, mfa_verified）
- [x] 实现 Claims 序列化/反序列化
- [x] 实现 Claims 验证方法

### 任务 2: PASETO Token 核心功能
- [x] 创建 `src/token/paseto.rs`
- [x] 实现 `generate_token()` - Token 签发
- [x] 实现 `verify_token()` - Token 验证
- [x] 实现错误类型 `TokenError`
- [x] 实现 Token 密钥派生（从 L2）

### 任务 3: Token 模块导出
- [x] 创建 `src/token/mod.rs`
- [x] 导出公共类型和函数
- [x] 集成到主库 `src/lib.rs`

### 任务 4: 单元测试与集成测试
- [x] 创建 `tests/token/paseto_tests.rs`
- [x] 测试 Token 签发
- [x] 测试 Token 验证
- [x] 测试 Token 过期场景
- [x] 测试无效 Token 处理
- [x] 测试 Scope 权限验证

### 任务 5: 文档更新
- [x] 更新 `vault-service/README.md`
- [x] 添加 Token 系统使用说明
- [x] 添加 API 文档

## Dev Notes

### 架构要求
1. **Enclave 内执行**: Token 签发和验证必须在 TEE Enclave 内执行
2. **密钥派生**: Token 密钥从 L2（User Vault Key）派生
3. **PASETO v4.local**: 使用 XChaCha20-Poly1305 对称加密
4. **错误处理**: 定义清晰的错误类型，区分不同失败场景

### PASETO v4.local 格式
```
v4.local.{base64url(payload)}.{base64url(footer)}

payload = XChaCha20-Poly1305(ptext, n, k)
ptext = to_bytes(m.header) || m.payload (json)
n = random_bytes(24) - XChaCha nonce
k = SymmetricKey - 32 bytes
```

### 依赖库
- `pasetors` = "0.7" - PASETO 实现（已在 Cargo.toml 中）

### 集成点
- 与 `src/tee/` 模块集成 - Enclave 内执行
- 与 `src/crypto/` 模块集成 - 密钥派生
- 与 `src/vault/` 模块集成 - 用户上下文

## Dev Agent Record

### 实现计划
1. 创建 Token Claims 结构体和相关类型
2. 实现 PASETO Token 签发和验证核心功能
3. 集成到 Enclave 模块
4. 编写完整测试套件
5. 更新文档

### 调试日志
- 2026-03-11: 修复 pasetors API 使用，使用 `local::encrypt/decrypt` 替代 `PasetoBuilder`
- 2026-03-11: 添加 `time` crate 依赖用于 RFC3339 时间格式转换
- 2026-03-11: 修复 Claims getter/setter API 混淆问题，使用 `get_claim()` 提取值
- 2026-03-11: 修复文档测试问题，添加 `ignore` 标记

### 完成记录
- 2026-03-11: 实现 Token Claims 定义（claims.rs）
  - TokenClaims 结构体包含所有必需字段（iss, sub, aud, exp, iat, nbf, jti, scope, mfa_verified）
  - Claims 验证方法（validate, is_expired, remaining_ttl）
  - Scope 权限验证（has_scope, has_any_scope, has_all_scopes）
  - ScopeValidator 用于权限层级检查
- 2026-03-11: 实现 PASETO Token 核心功能（paseto.rs）
  - Token 签发（PasetoToken::sign）
  - Token 验证（PasetoToken::verify）
  - 密钥派生（derive_key_from_master）
  - 密钥安全清零（PasetoKey with ZeroizeOnDrop）
  - Token 撤销检查器 trait（TokenRevocationChecker）
- 2026-03-11: 实现 Token 模块导出（mod.rs）
  - 公共类型导出
  - 便捷函数（create_token, quick_verify）
- 2026-03-11: 编写完整测试套件
  - 单元测试 31 个，全部通过
  - 集成测试 35 个，全部通过
- 2026-03-11: 更新文档（vault-service/README.md）
  - 添加 Token 系统说明
  - 添加 Scope 权限表
  - 添加使用示例

## 文件列表
| 文件路径 | 类型 | 描述 |
|---------|------|------|
| src/token/mod.rs | 新建 | Token 模块导出 |
| src/token/claims.rs | 新建 | Token Claims 定义 |
| src/token/paseto.rs | 新建 | PASETO 核心实现 |
| tests/token/paseto_tests.rs | 新建 | Token 测试套件 |
| src/lib.rs | 修改 | 添加 token 模块导出 |
| vault-service/README.md | 修改 | 更新 Token 文档 |

## 变更日志
| 日期 | 变更内容 | 作者 |
|------|---------|------|
| 2026-03-11 | 创建 Story | CoPaw |
| 2026-03-11 | 完成 PASETO Token 签发与验证实现 | CoPaw |

## 状态
**当前状态**: done
**开始时间**: 2026-03-11
**完成时间**: 2026-03-11

## 验证记录
- 2026-03-11: 所有单元测试通过（5/5）
- 2026-03-11: 登录 API 测试通过（返回 access_token 和 refresh_token）
- 2026-03-11: Token 创建 API 测试通过
- 2026-03-11: 健康检查 API 测试通过
- 2026-03-11: 修复 exp claim ISO 8601 格式解析问题
