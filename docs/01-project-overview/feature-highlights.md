# CredBridge 功能特性

> 本页只保留当前公开代码面已存在的能力摘要。历史试验性设计和已移除接口不再作为公开特性列出。

## 1. TEE 安全保险库

**状态**: 已实现

- Intel SGX TEE 硬件级隔离
- 四层密钥层次派生
- AES-256-GCM 认证加密
- 支持 simulation 与 hardware 运行路径

## 2. 多租户隔离

**状态**: 已实现

- Schema-per-tenant
- PostgreSQL RLS
- 租户级访问边界

## 3. 认证与授权

**状态**: 已实现

- PASETO v4.local
- Scope 权限控制
- Token 管理与会话存储

## 4. 审计日志

**状态**: 已实现

- 审计事件记录
- 查询与过滤
- 合规追踪

## 5. 远程证明

**状态**: 已实现

- SGX DCAP Quote 生成与验证
- Challenge/response 认证流程
- 硬件路径自检与失败关闭

## 6. TEE Sandbox

**状态**: 已实现

- 沙箱会话管理
- Nsjail 隔离
- 浏览器自动化执行
- AI 操作审核与安全导出

## 7. Web 控制台

**状态**: 已实现

- 凭证与租户管理
- 审计查看
- Sandbox 相关界面

## 8. SDK 生态

**状态**: 已实现

- Rust SDK
- TypeScript SDK
- 统一对接主服务 API

## 9. CLI 与自动化入口

**状态**: 已实现

当前对自动化和 Agent 的公开入口是 CLI，而不是旧的独立协议适配入口。

- `credbridge auth ...`
- `credbridge credentials ...`
- `credbridge tokens ...`
- `credbridge audit ...`
- `credbridge sandbox ...`

业务规则仍由主服务负责，CLI 负责稳定命令面和脚本化输出。

---

**最后更新**: 2026-03-26
