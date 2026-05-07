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

当前公开 Web 控制台入口为：

- `/credentials`：管理凭证列表、创建凭证、查看基础元数据、删除凭证
- `/tokens`：签发和查看受限 token
- `/developer`：查看 API / SDK / CLI 示例，并使用 API tester
- `/onboarding`：新用户引导流程

说明：

- `api_key` 凭证创建弹窗支持 `provider`、`allowed_domains`、OKX `passphrase`、以及
  `provider=custom` 时的 `custom_functions`
- 审计、租户、设置、用户、Profile 代码模块仍保留在仓库中，但当前前端路由不会直接开放
  这些页面；访问旧路径会重定向到 `/credentials`

### 3.2 CLI

CLI 是当前推荐的自动化入口，适用于脚本、运维和 Agent 工作流。

常见命令族：

- `toani login`
- `toani doctor`
- `toani config init|show`
- `toani credentials list|get`
- `toani sandbox ...`

说明：

- 当前公开 CLI 不提供 `auth`、`tokens`、`service-accounts`、`audit` 等命令族。
- `credentials` 当前只读，不负责创建、更新、解密或删除凭证。
- 推荐先执行 `toani login`，再执行 `toani doctor`。

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
- API 参考：`docs/03-api-reference/`
- 部署运维：`docs/05-deployment-and-operations/`
- 开发者指南：`docs/07-developer-guide/`

## 6. 说明

- 历史设计稿、归档报告和内部计划仍可能保留在仓库中，但不属于当前公开使用面。
- 若文档描述与代码不一致，以当前代码、构建入口和 CI 配置为准。
