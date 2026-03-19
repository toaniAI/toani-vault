# CredBridge 项目文档索引

**项目名称**: CredBridge
**项目类型**: 企业级凭证管理服务平台
**技术栈**: Rust (Axum) 后端 + React 前端
**文档生成日期**: 2026-03-18
**文档版本**: 1.0.0

---

## 项目概述

CredBridge 是一个企业级凭证管理服务平台，提供安全的凭证存储、访问控制、审计日志和多租户支持。平台采用 Rust (Axum) 后端 + React 前端的全栈架构，集成了 TEE（可信执行环境）用于安全代码执行。

### 核心特性

- **安全凭证管理**: AES-GCM 加密，Vault 后端存储
- **多租户架构**: 基于 PostgreSQL RLS 的行级安全隔离
- **TEE 安全执行**: 可信执行环境中的代码沙箱
- **完整审计日志**: 基于 immudb 的不可篡改日志
- **双 Token 认证**: PASETO 访问令牌 + 刷新令牌
- **MCP 集成**: Model Context Protocol 服务器支持
- **多语言 SDK**: Rust + TypeScript SDK

---

## 项目结构

```
credbridge/
├── src/                    # Rust 后端源码
│   ├── api/               # API 路由和处理器
│   ├── crypto/            # 加密功能 (AES-GCM, PASETO)
│   ├── models/            # 数据模型
│   ├── services/          # 业务服务层
│   ├── tee/               # TEE 可信执行环境
│   ├── vault/             # Vault 集成
│   ├── token/             # Token 管理
│   ├── audit/             # 审计日志
│   ├── tenant/            # 多租户支持
│   └── connector/         # 外部连接器
├── frontend/              # React 前端
│   ├── src/components/    # UI 组件
│   ├── src/pages/         # 页面组件
│   ├── src/hooks/         # 自定义 Hooks
│   ├── src/stores/        # Zustand 状态管理
│   └── src/lib/           # 工具函数
├── sdk-rust/              # Rust SDK
├── sdk-typescript/        # TypeScript SDK
├── mcp-server/            # MCP 服务器实现
├── cli/                   # CredBridge CLI 工具
├── tests/                 # 集成测试
├── migrations/            # 数据库迁移
├── docker/                # Docker 配置
└── docs/                  # 项目文档
```

---

## 技术架构

### 后端架构 (Rust)

| 组件 | 技术 | 用途 |
|------|------|------|
| Web 框架 | Axum | HTTP API 和 WebSocket |
| 数据库 | PostgreSQL + SQLx | 数据持久化 |
| 缓存 | Redis | Token 存储和会话缓存 |
| 加密存储 | HashiCorp Vault | 凭证安全存储 |
| 审计日志 | immudb | 不可篡改日志 |
| 异步运行时 | Tokio | 异步任务处理 |
| 加密 | ring, aes-gcm, pasetors | 密码学操作 |

### 前端架构 (React)

| 组件 | 技术 | 用途 |
|------|------|------|
| 框架 | React 19 | UI 框架 |
| 构建工具 | Vite | 开发和构建 |
| 样式 | Tailwind CSS | 原子化 CSS |
| UI 组件 | shadcn/ui + Radix UI | 组件库 |
| 状态管理 | Zustand | 全局状态 |
| 数据获取 | TanStack Query | 服务器状态管理 |
| 路由 | React Router v7 | 客户端路由 |

---

## 模块文档

### 核心模块

- [API 路由](./api-routes.md) - REST API 端点和路由结构 ✅
- [数据模型](./data-models.md) - 数据库模型和关系 ✅
- [服务层](./services.md) - 业务逻辑服务
- [加密系统](./crypto.md) - 加密实现和安全机制
- [认证授权](./auth.md) - PASETO Token 和权限控制

### 安全模块

- [TEE 沙箱](./tee-sandbox.md) - 可信执行环境代码执行 ✅
- [审计日志](./audit.md) - 审计事件和 immudb 集成
- [多租户](./multi-tenancy.md) - 租户隔离和 RLS
- [Vault 集成](./vault.md) - HashiCorp Vault 凭证存储

### 集成模块

- [MCP 服务器](./mcp-server.md) - Model Context Protocol 实现
- [SDK 文档](./sdk.md) - Rust/TypeScript SDK 使用指南
- [CLI 工具](./cli.md) - 命令行工具使用

### 前端模块

- [前端架构](./frontend-architecture.md) - React 应用结构 ✅
- [组件库](./components.md) - UI 组件和使用
- [API 集成](./frontend-api.md) - 前端与后端 API 集成

---

## 开发和部署

### 开发环境

```bash
# 后端开发
cargo build
cargo test

# 前端开发
cd frontend
npm install
npm run dev

# 数据库迁移
cargo sqlx migrate run
```

### Docker 部署

```bash
# 完整环境
docker-compose -f docker/docker-compose.yml up -d

# 生产环境
docker-compose -f docker/docker-compose.prod.yml up -d
```

### 测试

- **单元测试**: `cargo test`
- **集成测试**: `cargo test --test '*'`
- **E2E 测试**: `cd e2e-test && npm test`

---

## 文档导航

### 快速链接

- [API 文档](../API.md) - 完整 API 参考
- [部署指南](../DEPLOYMENT.md) - 部署和配置
- [用户手册](../USER_MANUAL.md) - 最终用户指南
- [SDK 指南](../SDK_GUIDE.md) - SDK 使用教程
- [AI Agent 指南](../AI_AGENT_GUIDE.md) - AI Agent 集成

### 设计文档

- [TEE 安全执行沙箱设计](../TEE_SECURE_EXECUTION_SANDBOX_DESIGN.md)
- [UI 设计规范](../UI_DESIGN_SPEC.md)
- [多租户设计](../MULTI_TENANCY.md)
- [MCP 集成](../MCP_INTEGRATION.md)

### 测试和验收

- [测试计划](../../test-cases/TEST-PLAN.md)
- [测试报告](../TEST_REPORT_v1.1.md)
- [TEE 沙箱验收计划](../TEE_SECURE_EXECUTION_SANDBOX_ACCEPTANCE_PLAN.md)

---

## 贡献指南

### 代码规范

- **Rust**: 遵循 `rust-coding-standards.md`
- **React**: 遵循 `react-coding-standards.md`
- **提交信息**: 使用 Conventional Commits

### Agent 团队

| Agent | 职责 | 触发条件 |
|-------|------|---------|
| frontend-specialist | UI 组件、页面实现 | React/TypeScript/Tailwind 修改 |
| backend-specialist | API、数据库、业务逻辑 | Rust/Axum/SQLx 修改 |
| product-manager | 需求分析、功能规划 | 新功能规划 |
| qa-engineer | 测试策略、E2E 测试 | 测试计划 |
| code-reviewer | 代码审查、架构评审 | 所有代码变更 |

---

## 许可证

[LICENSE](../../LICENSE)

---

*本文档由 BMAD document-project 工作流自动生成*
*生成时间: 2026-03-18*
