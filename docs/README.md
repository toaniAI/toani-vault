# Toani Vault 文档导航

这是当前对外公开文档的主入口。主入口只保留与现有代码、构建和运行路径一致的内容；历史方案、规划稿、审阅报告和内部资料不再从这里暴露。

## 核心入口

- 项目概述：[01-project-overview/README.md](01-project-overview/README.md)
- 核心功能：[02-core-modules/README.md](02-core-modules/README.md)
- API 参考：[03-api-reference/README.md](03-api-reference/README.md)
- SDK 与工具：[04-sdk-and-tooling/README.md](04-sdk-and-tooling/README.md)
- 部署与运维：[05-deployment-and-operations/README.md](05-deployment-and-operations/README.md)
- 安全与合规：[06-security-and-compliance/README.md](06-security-and-compliance/README.md)
- 开发者指南：[07-developer-guide/README.md](07-developer-guide/README.md)
- 故障排除：[09-troubleshooting/README.md](09-troubleshooting/README.md)
- 用户手册：[08-user-guide/USER_MANUAL.md](08-user-guide/USER_MANUAL.md)

## 常用跳转

- Rust SDK: [../sdk-rust/README.md](../sdk-rust/README.md)
- TypeScript SDK: [../sdk-typescript/README.md](../sdk-typescript/README.md)
- CLI: [../cli/README.md](../cli/README.md)
- Docker 部署: [05-deployment-and-operations/DEPLOYMENT.md](05-deployment-and-operations/DEPLOYMENT.md)
- SGX Runner 手册: [07-developer-guide/test-strategy/SGX_RUNNER_RUNBOOK.md](07-developer-guide/test-strategy/SGX_RUNNER_RUNBOOK.md)

## 文档边界

- `docs/project-docs/` 与 `docs/plans/` 保留内部参考性质内容，不视为正式用户文档。
- 私有 `master` 分支保留需求输入、验收证据和内部修复闭环材料；public 镜像不发布这些目录。
- 如果文档内容与当前代码不一致，以仓库中的实际实现和构建入口为准。
