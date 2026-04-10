# 测试策略

本目录包含 CredBridge 的测试文档、测试用例和测试报告。

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
- TypeScript 单元测试：`npm test`
- 覆盖率要求：>80%

### 集成测试

- API 集成测试
- 数据库集成测试
- TEE Enclave 集成测试

### 端到端测试

- Playwright E2E 测试
- 关键用户流程验证
- 跨浏览器测试

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

# TypeScript 测试
npm test

# E2E 测试
npm run test:e2e
```

### CI/CD测试

- GitHub Actions 自动运行
- 代码提交触发
- 合并请求强制检查

## 测试报告

历史测试报告已归档至：`99-归档/历史报告/`

---

**更新时间**: 2026-04-02
