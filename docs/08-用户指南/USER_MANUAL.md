# CredBridge 用户手册

本手册面向当前仓库的公开使用方式，覆盖 Web 控制台、SDK、CLI、部署和常见操作。旧的独立协议适配入口已不再作为公开使用面。

## 1. 系统组成

- 主服务：负责认证、凭证、审计、TEE、租户和 Sandbox 相关业务能力
- 前端控制台：提供管理界面
- Rust SDK / TypeScript SDK：提供程序化接入
- CLI：提供脚本化和运维入口

## 2. 快速开始

### 2.1 本地运行

```bash
docker compose -f docker/docker-compose.yml up
```

### 2.2 后端校验

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

### 2.3 前端构建

```bash
cd frontend
npm run build
```

## 3. 常见使用面

### 3.1 Web 控制台

- 管理凭证
- 查看审计信息
- 管理租户与 Token
- 使用 Sandbox 相关界面

### 3.2 CLI

CLI 是当前推荐的自动化入口，适用于脚本、运维和 Agent 工作流。

常见命令族：

- `credbridge auth ...`
- `credbridge credentials ...`
- `credbridge tokens ...`
- `credbridge audit ...`
- `credbridge sandbox ...`

### 3.3 SDK

- Rust SDK: `sdk-rust/`
- TypeScript SDK: `sdk-typescript/`

## 4. 运行模式

### 4.1 Simulation 模式

- 适用于本地开发和常规 CI
- 不依赖真实 SGX 硬件

### 4.2 Hardware 模式

- 适用于 SGX 环境
- 需要签名后的 enclave 制品和硬件依赖

最小环境变量：

```bash
export TEE_MODE=hardware
export TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so
```

## 5. 常见文档入口

- 文档导航：`docs/README.md`
- API 参考：`docs/03-API 参考/`
- 部署运维：`docs/05-部署与运维/`
- 开发者指南：`docs/07-开发者指南/`

## 6. 说明

- 历史设计稿、归档报告和内部计划仍可能保留在仓库中，但不属于当前公开使用面。
- 若文档描述与代码不一致，以当前代码、构建入口和 CI 配置为准。
