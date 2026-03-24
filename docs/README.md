# CredBridge 文档导航

本文档提供 CredBridge 项目完整文档的导航和索引。

## 📚 文档分类

### 01-项目概述
- [项目介绍](01-项目概述/README.md)
- [功能特性](01-项目概述/功能特性.md)
- [架构设计](01-项目概述/架构设计.md)
- [快速开始](01-项目概述/快速开始.md)
- [CredBridge 设计规范](01-项目概述/CredBridge_CN_设计规范_v1.0.md)
- [技术债务](01-项目概述/技术债务.md)
- [1Password 差距落地清单](01-项目概述/1Password-差距落地清单.md)
- [项目上下文](01-项目概述/project-context.md)
- [变更日志](01-项目概述/CHANGELOG.md)

### 02-核心功能模块
- [多租户隔离](02-核心功能模块/多租户隔离.md)
- TEE 安全模块:
  - [TEE 安全沙箱设计](02-核心功能模块/TEE 安全模块/TEE_SECURE_EXECUTION_SANDBOX_DESIGN.md)
  - [验收计划](02-核心功能模块/TEE 安全模块/TEE_SECURE_EXECUTION_SANDBOX_ACCEPTANCE_PLAN.md)
- 设计文档:
  - [凭证版本控制](02-核心功能模块/credential-versioning.md)
  - [MCP SSE 设计](02-核心功能模块/mcp-sse-design.md)
  - [RLS 策略](02-核心功能模块/rls-policy.md)

### 03-API 参考
- [REST API](03-API 参考/REST-API.md)
- [Tenant API](03-API 参考/TENANT-API.md)
- [Attestation API](03-API 参考/ATTESTATION-API.md)

### 04-SDK 与工具
- [SDK 指南](04-SDK 与工具/SDK-GUIDE.md)
- [SDK 文档摘要](04-SDK 与工具/SDK-SUMMARY.md)
- [SDK Sandbox 指南](04-SDK 与工具/SDK-SANDBOX-GUIDE.md)
- [MCP 集成](04-SDK 与工具/MCP-INTEGRATION.md)
- SDK 详细文档:
  - [TypeScript SDK](../sdk-typescript/docs/)
  - [Rust SDK](../sdk-rust/docs/)
  - [CLI 工具](../cli/README.md)
  - [MCP Server](../mcp-server/README.md)

### 05-部署与运维
- [部署指南](05-部署与运维/DEPLOYMENT.md)
- [DCAP 设置](05-部署与运维/DCAP-SETUP.md)
- [监控告警](05-部署与运维/MONITORING.md)
- [ImmuDB 设置](05-部署与运维/IMMUDB_SETUP.md)
- [Intel SGX 部署要求](05-部署与运维/INTEL_SGX_DEPLOYMENT_REQUIREMENTS.md)

### 06-安全与合规
- TEE 安全架构
- 远程认证协议
- 加密算法
- 审计系统

### 07-开发者指南
- [开发环境设置](07-开发者指南/开发环境设置.md)
- 测试策略:
  - [验收测试用例](07-开发者指南/测试策略/ACCEPTANCE_TEST_CASES.md)
  - [测试计划](07-开发者指南/测试策略/acceptance-test-plan.md)
  - [SGX 测试完整指南](07-开发者指南/测试策略/SGX_TESTING_COMPLETE_GUIDE.md)
  - [SGX 测试执行指南](07-开发者指南/测试策略/SGX_TEST_EXECUTION_GUIDE.md)
- [代码贡献](07-开发者指南/代码贡献.md)

### 08-用户指南
- [用户手册](08-用户指南/USER_MANUAL.md)
- [AI Agent 指南](AI_AGENT_GUIDE.md)

### 09-故障排除
- [常见问题](09-故障排除/常见问题.md)

### 99-归档
历史文档和技术规格，供参考使用：
- [历史报告](99-归档/历史报告/)
- [技术规格](99-归档/技术规格/)
- [代码评审](99-归档/代码评审/)

---

## 🔗 外部文档资源

### SDK 文档
- **TypeScript SDK**: `/sdk-typescript/docs/`
  - [API 参考](../sdk-typescript/docs/API_REFERENCE.md)
  - [快速开始](../sdk-typescript/docs/QUICKSTART.md)
  - [示例](../sdk-typescript/docs/EXAMPLES.md)
- **Rust SDK**: `/sdk-rust/docs/`
  - [API 参考](../sdk-rust/docs/API_REFERENCE.md)
  - [快速开始](../sdk-rust/docs/QUICKSTART.md)
  - [示例](../sdk-rust/docs/EXAMPLES.md)

### 示例代码
- [TypeScript 示例](../examples/typescript/README.md)
- [Rust 示例](../examples/rust/README.md)

### 项目文档
- [前端项目文档](../frontend/README.md)
- [Vault Service](../vault-service/README.md)

---

## 📖 文档维护

### 文档分类规则
- **核心文档**: 项目介绍、架构设计、API 参考、用户指南等
- **开发文档**: 测试策略、代码贡献、开发环境设置等
- **部署文档**: 部署指南、监控告警、运维手册等
- **归档文档**: 临时报告、历史文档、技术规格等

### 文档更新流程
1. 新增文档时，选择合适的分类目录
2. 更新本文档索引，添加新文档链接
3. 如替换旧文档，将旧文档移至 `99-归档/` 对应目录
4. 在文档顶部添加版本信息和更新日期

### 文档命名规范
- 使用中文文件名（除 API 和技术术语外）
- 文件名使用短横线分隔：`my-doc.md`
- 避免使用空格和特殊字符
- 英文缩写保持大写：`API`, `SDK`, `TEE`

---

## 📊 文档统计

- **核心文档**: ~30 个
- **API 文档**: 3 个
- **SDK 文档**: 8 个
- **部署文档**: 5 个
- **测试文档**: 4 个
- **归档文档**: ~30 个

**总计**: ~80 个核心文档（不含 Skills 系统）

---

## 🆘 帮助

如需查找特定文档但无法定位，请：
1. 使用项目搜索功能（`Cmd/Ctrl + Shift + F`）
2. 查看根目录 [README.md](../README.md) 的相关章节
3. 联系项目维护人员

---

**最后更新**: 2026-03-20
**文档维护者**: CredBridge 团队
