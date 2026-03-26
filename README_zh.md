# CredBridge

[English README](README.md)

CredBridge 是一个围绕 Intel SGX TEE 构建的 AI 原生零信任凭证保险库。当前仓库的公开使用面已经收口到主服务、前端控制台、Rust/TypeScript SDK，以及面向运维和自动化的 CLI。

## 当前能力

- TEE 硬件模式与 simulation 模式双运行路径
- 四层密钥层次和 AES-256-GCM 凭证加密
- PASETO v4.local 认证、Redis 会话与 Token 流程
- Schema-per-tenant 与 PostgreSQL RLS 多租户隔离
- 审计日志、远程证明、TEE Sandbox 执行接口
- React 管理界面、Rust SDK、TypeScript SDK、CLI

## 架构摘要

```text
L0: SGX Sealing Key
  -> L1: Enclave Master Key
    -> L2: User Vault Key
      -> L3: Credential Encryption Key
        -> AES-256-GCM encrypted credential
```

主要目录：

- `src/api/`：HTTP 路由与中间件
- `src/tee/`：TEE 生命周期、证明、密封、沙箱、硬件运行时桥接
- `src/token/`：PASETO 与会话处理
- `src/vault/`：凭证存储与后端
- `cli/`：命令行入口
- `sdk-rust/`、`sdk-typescript/`：客户端 SDK
- `frontend/`：React 控制台

## 文档入口

- 文档总入口：[docs/README.md](docs/README.md)
- 部署与运维：[docs/05-部署与运维/README.md](docs/05-部署与运维/README.md)
- API 参考：[docs/03-API 参考/README.md](docs/03-API 参考/README.md)
- 开发者指南：[docs/07-开发者指南/README.md](docs/07-开发者指南/README.md)
- SGX Runner 手册：[docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md](docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md)

## 本地开发

核心校验门禁：

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

前端构建：

```bash
cd frontend
npm run build
```

Linux SGX 环境下的硬件编译检查：

```bash
cargo test --features tee-hardware --lib --tests --no-run
```

## 部署说明

本地完整栈：

```bash
docker compose -f docker/docker-compose.yml up
```

硬件模式至少需要：

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so
```

补充参考：

- [docker/sgx/README.md](docker/sgx/README.md)
- [sgx-enclave/README.md](sgx-enclave/README.md)
- [cli/README.md](cli/README.md)
- [sdk-rust/README.md](sdk-rust/README.md)
- [sdk-typescript/README.md](sdk-typescript/README.md)
