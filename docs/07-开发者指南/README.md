# 开发者指南

本目录汇总当前仓库面向开发者的后端测试、SGX 验证和验收证据规范文档。

## 当前入口

- [测试策略](测试策略/README.md) - Rust 验证命令、验收材料与测试边界
- [SGX Runner 手册](测试策略/SGX_RUNNER_RUNBOOK.md) - Linux SGX runner 准备、运行与排障
- [SGX 测试完整指南](测试策略/SGX_TESTING_COMPLETE_GUIDE.md) - SGX 测试矩阵与执行建议
- [系统验收与证据判定规范](测试策略/系统验收与证据判定规范.md) - 证据分级、通过门槛与禁止事项

## 当前仓库的验证基线

### 后端

```bash
cargo fmt
cargo clippy --tests -- -D warnings
cargo test
```

### SGX / 硬件相关

```bash
cargo test --features tee-hardware --lib --tests --no-run
```

## 文档边界

- `docs/qa_reports/` 保存历史验收与回归证据。
- `docs/requirements/` 保存产品缺口、需求补充和 QA 驱动的修复输入。
- 如果本文档与实际命令不一致，以 `Cargo.toml` 和仓库脚本为准。
