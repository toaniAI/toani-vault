# CredBridge CLI 工具试用报告

## 试用信息
- **试用日期**: 2026-03-11
- **试用人员**: claude_glm (后端开发 + CLI 工具负责人)
- **试用版本**: vault-service v0.1.0
- **试用结论**: ❌ **不通过** - CLI 功能未实现

---

## 1. 项目概述

CredBridge 是一个 TEE（可信执行环境）凭证保险库系统，专为 AI Agent 设计，采用四层密钥层次架构（L0-L3）实现零知识安全存储。

### 1.1 项目结构
```
src/
├── api/           # HTTP API 路由和处理函数
├── audit/         # 审计日志模块
├── crypto/        # 加密核心模块
├── models/        # 数据模型
├── services/      # 业务服务
├── tee/           # TEE 可信执行环境
├── tenant/        # 租户管理
├── token/         # Token 管理
├── vault/         # 凭证保险库
└── main.rs        # CLI 入口点
```

---

## 2. CLI 功能试用

### 2.1 编译测试

| 检查项 | 状态 | 结果 |
|--------|------|------|
| 编译成功 | ✅ | `cargo build` 成功，生成 vault-service 可执行文件 |
| 编译警告 | ⚠️ | 54 个警告（主要是未使用导入和废弃函数） |
| 测试通过 | ✅ | 325 个单元测试全部通过，2 个被忽略 |

### 2.2 CLI 命令试用

#### 2.2.1 帮助文档测试

**测试命令**: `cargo run -- --help`

**预期结果**: 显示 CLI 帮助文档，列出所有可用命令

**实际结果**: ❌ CLI 忽略所有参数，直接执行演示程序

```
╔══════════════════════════════════════════════════════════╗
║           CredBridge - TEE Credential Vault              ║
╚══════════════════════════════════════════════════════════╝
📋 演示: L0-L3 四层密钥层次架构 (HKDF-SHA256)
...
```

**问题**: CLI 没有子命令系统，不支持 `--help`、`-h` 或任何命令行参数

#### 2.2.2 凭证管理命令

| 命令 | 状态 | 说明 |
|------|------|------|
| `create` | ❌ 未实现 | 无 CLI 命令 |
| `update` | ❌ 未实现 | 无 CLI 命令 |
| `delete` | ❌ 未实现 | 无 CLI 命令 |
| `list` | ❌ 未实现 | 无 CLI 命令 |
| `get` | ❌ 未实现 | 无 CLI 命令 |
| `decrypt` | ❌ 未实现 | 无 CLI 命令 |

**API 端点可用**: `src/api/routes.rs` 中定义了完整的凭证管理 API 路由
```rust
pub const CREATE: &str = "/credentials";
pub const LIST: &str = "/credentials";
pub const GET: &str = "/credentials/:id";
pub const UPDATE: &str = "/credentials/:id";
pub const DELETE: &str = "/credentials/:id";
pub const DECRYPT: &str = "/credentials/:id/decrypt";
```

#### 2.2.3 Token 管理命令

| 命令 | 状态 | 说明 |
|------|------|------|
| `generate` | ❌ 未实现 | 无 CLI 命令 |
| `revoke` | ❌ 未实现 | 无 CLI 命令 |
| `validate` | ❌ 未实现 | 无 CLI 命令 |

**API 端点可用**: `src/api/routes.rs`
```rust
pub const CREATE: &str = "/tokens";
pub const VERIFY: &str = "/tokens/verify";
pub const REVOKE: &str = "/tokens/:id/revoke";
```

#### 2.2.4 审计日志命令

| 命令 | 状态 | 说明 |
|------|------|------|
| `query` | ❌ 未实现 | 无 CLI 命令 |
| `export` | ❌ 未实现 | 无 CLI 命令 |
| `filter` | ❌ 未实现 | 无 CLI 命令 |

**API 端点可用**: `src/api/routes.rs`
```rust
pub const LIST: &str = "/audit/logs";
pub const GET: &str = "/audit/logs/:id";
pub const EXPORT: &str = "/audit/export";
pub const VERIFY: &str = "/audit/verify";
```

#### 2.2.5 租户管理命令

| 命令 | 状态 | 说明 |
|------|------|------|
| `create` | ❌ 未实现 | 无 CLI 命令 |
| `configure` | ❌ 未实现 | 无 CLI 命令 |
| `switch` | ❌ 未实现 | 无 CLI 命令 |

**API 端点可用**: `src/api/tenant.rs`

#### 2.2.6 系统命令

| 命令 | 状态 | 说明 |
|------|------|------|
| `status` | ❌ 未实现 | 无 CLI 命令 |
| `health` | ❌ 未实现 | 无 CLI 命令 |
| `version` | ❌ 未实现 | 无 CLI 命令 |

**API 端点可用**: `src/api/routes.rs`
```rust
pub const CHECK: &str = "/health";
pub const CHECK_DETAIL: &str = "/health/detail";
pub const METRICS: &str = "/metrics";
```

---

## 3. 检查清单验证

| 检查项 | 验证内容 | 状态 | 备注 |
|--------|----------|------|------|
| 帮助文档 | `--help` 输出完整准确 | ❌ 未通过 | CLI 不支持参数 |
| 参数验证 | 必填参数、类型、范围验证 | ❌ 未通过 | 无参数解析 |
| 输出格式 | JSON/表格/文本格式正确 | ⚠️ 部分通过 | 演示程序输出格式化文本 |
| 错误处理 | 错误信息清晰，退出码正确 | ❌ 未通过 | 无错误处理 |
| 配置文件 | 配置加载和优先级正确 | ❌ 未验证 | CLI 未实现 |
| 环境变量 | 环境变量覆盖正常工作 | ❌ 未验证 | CLI 未实现 |
| 管道支持 | 支持输入输出管道 | ❌ 未通过 | 无 stdin/stdout 处理 |
| 自动补全 | shell 自动补全正常 | ❌ 未实现 | 无自动补全 |

---

## 4. 当前 CLI 功能分析

### 4.1 已实现功能

当前 `main.rs` 实现了一个演示程序，展示以下内容：

1. **L0-L3 密钥层次架构演示**
   - Step 1: 初始化 SGX Enclave 和 L0 硬件根密钥
   - Step 2: 从 L0 派生 L1 Enclave Master Key
   - Step 3: 从 L1 派生 L2 User Vault Key
   - Step 4: 从 L2 派生 L3 Credential Encryption Key
   - Step 5: 使用 L3 密钥加密凭证 (AES-256-GCM)
   - Step 6: 使用 L3 密钥解密凭证
   - Step 7: 安全属性验证
   - 附录: 加密格式规范

2. **输出格式**
   - 使用 Unicode 框线美化输出
   - 中文界面
   - 彩色文本（通过终端控制字符）

### 4.2 代码质量

| 指标 | 状态 | 说明 |
|------|------|------|
| 单元测试 | ✅ 325/325 通过 | 覆盖 crypto、token、vault、audit、tee 等模块 |
| 编译警告 | ⚠️ 54 个 | 主要是未使用导入，需清理 |
| 代码结构 | ✅ 良好 | 模块化设计，职责分离清晰 |
| 文档 | ✅ 完善 | Rust doc 注释完整 |

---

## 5. 与 API 功能对比

CLI 仅实现了演示功能，而项目实际上已实现完整的 HTTP API：

| 功能模块 | CLI 状态 | API 状态 | 单元测试 |
|----------|----------|----------|----------|
| 凭证管理 | ❌ | ✅ | ✅ 35 个测试 |
| Token 管理 | ❌ | ✅ | ✅ 42 个测试 |
| 审计日志 | ❌ | ✅ | ✅ 15 个测试 |
| 租户管理 | ❌ | ✅ | ✅ 18 个测试 |
| TEE/Enclave | ✅ 演示 | ✅ | ✅ 25 个测试 |
| 加密核心 | ✅ 演示 | ✅ | ✅ 30 个测试 |

---

## 6. 发现的问题

### 6.1 严重问题

| 问题编号 | 问题描述 | 影响 | 建议 |
|----------|----------|------|------|
| CLI-001 | CLI 无子命令系统，所有参数被忽略 | 🔴 高 | 使用 `clap` crate 实现完整的 CLI 框架 |
| CLI-002 | 无 `--help`、`-h`、`-v`、`-V` 等标准参数 | 🔴 高 | 添加标准 CLI 参数支持 |
| CLI-003 | CLI 与 API 功能不匹配 | 🔴 高 | 为所有 API 端点提供对应的 CLI 命令 |

### 6.2 一般问题

| 问题编号 | 问题描述 | 影响 | 建议 |
|----------|----------|------|------|
| CLI-004 | 54 个编译警告 | 🟡 中 | 运行 `cargo fix` 修复大部分问题 |
| CLI-005 | 使用废弃的 `base64::encode/decode` | 🟡 中 | 迁移到 `Engine::encode/decode` |

---

## 7. 试用结论

### 7.1 总体评价

**试用结论**: ❌ **不通过**

当前 CLI 仅是一个演示程序，而非完整的命令行工具。项目主要作为 HTTP API 服务实现，所有核心业务功能（凭证管理、Token 管理、审计日志、租户管理）都通过 API 端点提供，但缺少对应的 CLI 接口。

### 7.2 建议改进项

1. **引入 CLI 框架**: 使用 `clap` crate 构建完整的命令行界面
2. **命令映射**: 为每个 API 端点创建对应的 CLI 子命令
3. **配置管理**: 支持配置文件和环境变量
4. **输出格式**: 支持 JSON、表格、文本多种输出格式
5. **错误处理**: 统一的错误处理和退出码
6. **文档**: 完善 CLI 使用文档和示例

### 7.3 后续工作

如需完整 CLI 功能，建议创建 Story 进行开发：
- 实现 `credential` 子命令组
- 实现 `token` 子命令组
- 实现 `audit` 子命令组
- 实现 `tenant` 子命令组
- 实现 `system` 子命令组

---

## 8. 附录

### 8.1 测试环境

- OS: Darwin 25.2.0 (macOS)
- Rust: 1.85.0
- Cargo: 1.85.0

### 8.2 依赖版本

| 依赖 | 版本 | 用途 |
|------|------|------|
| axum | 0.7 | HTTP API 框架 |
| tokio | 1.x | 异步运行时 |
| pasetors | 0.7 | PASETO Token |
| redis | 0.24 | Redis 客户端 |
| ring | 0.17 | 加密库 |
| aes-gcm | 0.10 | AES-GCM 加密 |
| uuid | 1.7 | UUID 生成 |

### 8.3 相关文件

- `src/main.rs` - CLI 入口点
- `src/api/routes.rs` - API 路由定义
- `Cargo.toml` - 依赖配置
