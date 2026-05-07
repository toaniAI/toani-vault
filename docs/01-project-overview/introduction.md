# ToaniVault 项目介绍

## 项目定位

ToaniVault 是一个围绕 Intel SGX TEE 构建的零信任凭证保险库。它把凭证处理、密钥派生、远程证明和部分沙箱执行能力统一到同一个主服务运行时中，并通过 SDK、CLI 和前端控制台对外暴露。

## 核心能力

### TEE 安全保险库

- Intel SGX TEE 硬件级隔离
- 四层密钥派生架构
- AES-256-GCM 凭证加密

### 多租户隔离

- Schema-per-tenant 架构
- PostgreSQL 行级安全
- 租户级访问边界

### 认证与审计

- PASETO v4.local Token
- Redis 会话与 Token 流程
- 不可篡改审计日志

### TEE Sandbox 执行

- 浏览器自动化沙箱
- Nsjail 隔离
- AI 操作审核与安全导出

### 接入方式

- React 管理界面
- Rust SDK
- TypeScript SDK
- CLI 自动化入口

## 技术栈

### 后端

- Rust (Edition 2024)
- Axum
- Tokio
- PostgreSQL + SQLx
- Redis
- HashiCorp Vault

### 前端

- React 19
- TypeScript
- Zustand + TanStack Query
- Tailwind CSS + shadcn/ui

### TEE 与安全

- Intel SGX
- SGX DCAP
- HKDF-SHA-256
- AES-256-GCM
- zeroize

## 项目目标

1. 让敏感凭证处理尽可能停留在硬件保护边界内。
2. 为 AI Agent 和自动化系统提供可审计的凭证能力。
3. 用统一主服务承载认证、审计、租户和 TEE 相关业务规则。
4. 同时支持本地开发、simulation CI 和硬件 SGX 部署。

---

**最后更新**: 2026-03-26
