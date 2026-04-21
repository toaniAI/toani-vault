# Toani Vault

[English README](README.md)

Toani Vault 是一个围绕 Intel SGX TEE 构建的 AI 原生零信任凭证保险库。当前仓库的公开使用面已经收口到主服务、前端控制台、Rust/TypeScript SDK，以及一个覆盖 onboarding、只读凭证元数据查询和 sandbox 操作的 CLI。

## 当前能力

- TEE 硬件模式与 simulation 模式双运行路径
- 四层密钥层次和 AES-256-GCM 凭证加密
- PASETO v4.local 认证、Redis 会话与 Token 流程
- Schema-per-tenant 与 PostgreSQL RLS 多租户隔离
- 审计日志、远程证明、TEE Sandbox 执行接口
- React 管理界面、Rust SDK、TypeScript SDK、CLI

## 运行时存储策略（Phase 5）

服务启动时会按域强制校验存储后端：

- `vault`：PostgreSQL 或 HashiCorp Vault（`CREDBRIDGE_STORAGE_BACKEND`）
- `auth`：PostgreSQL（`DATABASE_URL`）
- `tenant config`：PostgreSQL（`DATABASE_URL`）
- `sandbox records`：PostgreSQL（`DATABASE_URL`）
- `audit`：默认 immudb（`IMMUDB_*`）
- `token state`：Redis（`REDIS_URL`）

在生产环境，缺失必需持久化后端会直接启动失败。

仅在开发/测试环境，且显式开启开关时允许内存回退：

- `CREDBRIDGE_AUTH_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_TENANT_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_SANDBOX_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK=true`
- `CREDBRIDGE_TOKEN_ALLOW_MEMORY_FALLBACK=true`

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
- `cli/`：负责 onboarding、只读凭证元数据查询和 sandbox 操作的 CLI
- `sdk-rust/`、`sdk-typescript/`：客户端 SDK
- `frontend/`：React 控制台

## 文档入口

- 文档总入口：[docs/README.md](docs/README.md)
- 部署与运维：[docs/05-部署与运维/README.md](docs/05-部署与运维/README.md)
- API 参考：[docs/03-API 参考/README.md](docs/03-API 参考/README.md)
- 开发者指南：[docs/07-开发者指南/README.md](docs/07-开发者指南/README.md)
- SGX Runner 手册：[docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md](docs/07-开发者指南/测试策略/SGX_RUNNER_RUNBOOK.md)

## CLI 使用指南

Toani Vault 提供了一个命令行工具，覆盖引导式接入、本地 `config` 配置、只读凭证元数据查询，以及携带 bearer token 发起受限 sandbox 请求。

CLI 最新安装与使用说明请优先参考：

- [CLI 安装与使用（npm）](cli/README.md)
- [面向 AI 代理的 CLI Skill](cli/SKILL.md)

### 安装

```bash
# 从 npm 安装（推荐）
npm install -g @toani/vault-cli@latest

# 从源码安装
cd cli
npm install
npm run build
npm pack
npm install -g ./toani-vault-cli-*.tgz

# 验证安装
toani --version
```

### 快速开始

```bash
# 推荐首跑流程
toani login
toani doctor

# 只读查询凭证元数据
toani --output json credentials list

# 查看 sandbox 连通性 / 会话状态
toani sandbox stats
toani sandbox list-sessions
```

### CLI 命令概览

#### 当前公开命令面

当前公开 CLI 命令组只有：

- `login`
- `doctor`
- `config`（`init`、`show`）
- `credentials`（`list`、`get`）
- `sandbox`

不要默认认为公开 CLI 已经提供 `auth`、可变更的 `credentials`、`tokens`、`service-accounts` 或 `audit` 命令，除非你已经验证过更高版本。

`login` 是推荐的接入路径。它会打开 Dashboard，引导完成凭证与 token 获取，校验 token，并在可用时写入 OS Keychain。`doctor` 会检查 CLI 版本、Node.js、token 存储、token 格式、Base URL 连通性和 token 有效性。

`credentials` 当前只提供只读元数据查询，不负责创建、更新、解密或删除凭证。

`sandbox` 当前支持：

- `create-session`
- `list-sessions`
- `get-session`
- `terminate`
- `pause`
- `resume`
- `bootstrap-page`
- `execute`
- `export-dom`
- `export-data`
- `get-operation`
- `stats`

详细命令矩阵、参数和示例请以 [cli/README.md](cli/README.md) 为准。

### 全局选项

| 选项                    | 描述                                       |
| ----------------------- | ------------------------------------------ |
| `--output <format>`     | 输出格式：`table` 或 `json`（默认：table） |
| `--base-url <url>`      | 覆盖服务 URL                               |
| `--token <token>`       | 覆盖 bearer token                          |
| `-h, --help`            | 显示帮助信息                               |
| `-v, --version`         | 显示版本信息                               |

### 环境变量

| 变量               | 描述                                      |
| ------------------ | ----------------------------------------- |
| `TOANI_BASE_URL`      | 主服务 URL 覆盖                      |
| `CREDBRIDGE_BASE_URL` | 次级服务 URL 覆盖                    |
| `TOANI_VAULT_TOKEN`   | 主 bearer token 覆盖                 |
| `CREDBRIDGE_TOKEN`    | 次级 bearer token 覆盖               |
| `HOME`                | `~/.toani/config.json` 所在基目录    |

### 配置文件

CLI 将配置存储在 `~/.toani/config.json`。`baseUrl`、`output` 和 `timeout` 会写入这里；通过 `toani login` 或 `toani config init --token` 配置的 token 会优先写入 OS Keychain，明文 token 只保留兼容读取路径。

---

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
