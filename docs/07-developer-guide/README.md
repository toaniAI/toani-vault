# 开发者指南

本目录汇总当前仓库面向开发者的后端测试、SGX 验证和系统验收相关文档。

## 当前入口

- [测试策略](test-strategy/README.md) - Rust 验证命令、验收材料与测试边界
- [SGX Runner 手册](test-strategy/SGX_RUNNER_RUNBOOK.md) - Linux SGX runner 准备、运行与排障
- [SGX 测试完整指南](test-strategy/SGX_TESTING_COMPLETE_GUIDE.md) - SGX 测试矩阵与执行建议
- [系统验收计划](test-strategy/acceptance-test-plan.md) - 当前验收范围、执行顺序与结论边界

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

- 私有 `master` 分支保留验收证据、需求补充和 QA 驱动的修复输入；public 镜像不发布这些目录。
- 如果本文档与实际命令不一致，以 `Cargo.toml` 和仓库脚本为准。
