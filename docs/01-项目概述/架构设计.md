# CredBridge 架构设计

## 整体架构

```text
Clients
  -> React Web Console
  -> TypeScript SDK
  -> Rust SDK
  -> CLI
      -> CredBridge HTTP Service
          -> auth / token / vault / audit / tenant / sandbox / attestation
              -> PostgreSQL / Redis / Vault / immudb / SGX runtime
```

## 关键原则

### 统一主服务边界

- 认证、授权、审计、租户和 TEE 相关业务规则由主服务统一承载。
- SDK 和 CLI 都是主服务的客户端适配层，不复制业务实现。

### TEE 优先

- `TEE_MODE=simulation` 用于本地开发和常规 CI。
- `TEE_MODE=hardware` 用于 SGX 环境，要求签名后的 enclave 制品和硬件依赖。
- 硬件路径在缺失必要依赖时应失败关闭。

### 凭证与密钥层次

```text
L0: SGX Sealing Key
  -> L1: Enclave Master Key
    -> L2: User Vault Key
      -> L3: Credential Encryption Key
```

## 主要模块

- `src/api/`: HTTP 路由、中间件、API 入口
- `src/tee/`: TEE 生命周期、证明、密封、沙箱、硬件运行时桥接
- `src/token/`: PASETO 与 Token 相关逻辑
- `src/vault/`: 凭证模型、存储与后端
- `src/services/`: 业务编排层
- `cli/`: 自动化和运维命令面
- `frontend/`: React 控制台

## 运行时依赖

- PostgreSQL：主数据存储
- Redis：会话与缓存
- Vault：可选密钥/机密后端
- immudb：审计存储
- SGX 运行时：硬件部署路径

## 流水线与运行模式

- 常规后端校验：`cargo fmt`、`cargo clippy --tests -- -D warnings`、`cargo test`
- 硬件编译和硬件测试：由 `.drone.yml` 中的 SGX 流水线承担
- 前端构建独立通过 `frontend` 的 Vite 流程完成

---

**最后更新**: 2026-03-26
