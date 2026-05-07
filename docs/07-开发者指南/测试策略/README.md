# 测试策略

本目录包含 CredBridge 当前仍在维护的后端测试策略、SGX 运行手册和系统验收规范。

## 测试文档

### 测试计划

- [验收测试用例](ACCEPTANCE_TEST_CASES.md) - 完整的验收测试用例集
- [测试计划](acceptance-test-plan.md) - 测试策略和计划
- [系统验收与证据判定规范](系统验收与证据判定规范.md) - 系统验收原则、证据权重、操作规范与禁止事项

### SGX 测试

- [SGX 测试完整指南](SGX_TESTING_COMPLETE_GUIDE.md) - SGX 功能全面测试指南
- [SGX 测试执行指南](SGX_TEST_EXECUTION_GUIDE.md) - SGX 测试执行步骤

## 测试类型

### 单元测试

- Rust 单元测试：`cargo test`

### 集成测试

- API 集成测试
- 数据库集成测试
- TEE Enclave 集成测试

### 端到端测试

- 当前仓库没有统一的 `npm run test:e2e` 入口
- 浏览器验收与探索性测试证据统一沉淀到 `docs/qa_reports/`
- 关键用户流程验证以对应的 claim matrix / summary 为准

### 安全测试

- 渗透测试
- 漏洞扫描
- TEE 安全性验证
- 密钥管理审计

## 测试执行

### 本地测试

```bash
# Rust 测试
cargo test
```

### CI/CD 测试

- 后端硬门禁：`cargo fmt`、`cargo clippy --tests -- -D warnings`、`cargo test`
- SGX 相关验证以 runner / Drone 流程和 `SGX_RUNNER_RUNBOOK.md` 为准

## 测试报告

- 当前验收与回归报告保存在 `docs/qa_reports/`
- 需求缺口与补充说明保存在 `docs/requirements/`

---

**更新时间**: 2026-05-06
