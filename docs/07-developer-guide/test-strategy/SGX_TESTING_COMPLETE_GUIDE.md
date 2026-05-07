# SGX 硬件测试完整指南

本文档是 CredBridge SGX 硬件测试的完整入口，整合了所有测试相关的文档、脚本和用例。

---

## 📋 文档导航

| 文档                                                         | 描述           | 用途                   |
| ------------------------------------------------------------ | -------------- | ---------------------- |
| [SGX_HARDWARE_TEST_PLAN.md](./SGX_HARDWARE_TEST_PLAN.md)     | 测试计划与设计 | 了解测试策略和用例设计 |
| [SGX_TEST_EXECUTION_GUIDE.md](./SGX_TEST_EXECUTION_GUIDE.md) | 测试执行指南   | 按步骤执行测试         |
| [DCAP_SETUP.md](./DCAP_SETUP.md)                             | DCAP 配置指南  | 安装和配置 DCAP        |
| [SANDBOX_ANALYSIS.md](./SANDBOX_ANALYSIS.md)                 | 沙箱功能分析   | 了解沙箱架构           |

---

## 🚀 快速开始

### 30 秒环境检查

```bash
cd /path/to/credbridge
./scripts/check_sgx_environment.sh
```

### 1 分钟运行测试

```bash
# 确保环境正常
./scripts/check_sgx_environment.sh

# 运行所有硬件测试
cargo test --test sgx_hardware_tests --release -- --test-threads=1
```

### 5 分钟完整测试流程

```bash
# 1. 环境检查
./scripts/check_sgx_environment.sh

# 2. 编译测试
cargo test --test sgx_hardware_tests --release --no-run

# 3. 执行测试
cargo test --test sgx_hardware_tests --release -- --test-threads=1

# 4. 生成覆盖率报告
cargo tarpaulin --test sgx_hardware_tests --output-dir ./coverage --out Html

# 5. 查看报告
xdg-open ./coverage/tarpaulin-report.html
```

---

## 📁 文件结构

```
credbridge/
├── docs/
│   ├── SGX_HARDWARE_TEST_PLAN.md      # 测试计划文档
│   ├── SGX_TEST_EXECUTION_GUIDE.md    # 测试执行指南
│   ├── SGX_TESTING_COMPLETE_GUIDE.md  # 本文档
│   ├── DCAP_SETUP.md                  # DCAP 配置指南
│   └── SANDBOX_ANALYSIS.md            # 沙箱分析报告
│
├── tests/
│   ├── sgx_hardware_tests.rs          # SGX 硬件测试套件
│   └── tee_attestation_tests.rs       # TEE 认证测试
│
├── scripts/
│   └── check_sgx_environment.sh       # 环境检查脚本
│
└── src/
    └── tee/
        ├── enclave.rs                 # Enclave 核心实现
        ├── dcap.rs                    # DCAP 服务实现
        ├── attestation.rs             # 认证协议实现
        ├── challenge.rs               # 挑战 - 响应协议
        └── sealing.rs                 # Sealing 服务
```

---

## 🧪 测试套件概览

### 测试用例列表

| 编号   | 测试名称             | 测试类型 | 执行时间 |
| ------ | -------------------- | -------- | -------- |
| HW-001 | SGX 硬件基础验证     | 冒烟测试 | <1s      |
| HW-002 | Enclave 测量值一致性 | 功能测试 | <2s      |
| HW-003 | DCAP Quote 生成      | 功能测试 | <3s      |
| HW-004 | DCAP Quote 验证      | 功能测试 | <2s      |
| HW-005 | 挑战 - 响应协议      | 集成测试 | <5s      |
| HW-006 | SGX Sealing 密钥     | 安全测试 | <2s      |
| HW-007 | 密钥层次结构         | 安全测试 | <3s      |
| HW-008 | 安全通道建立         | 集成测试 | <2s      |
| HW-009 | 测量值白名单         | 安全测试 | <3s      |
| HW-010 | 重放攻击防护         | 安全测试 | <2s      |

### 测试覆盖的功能

✅ **Quote 生成与验证**

- DCAP Quote 在真实硬件上生成
- Quote 签名验证
- 测量值比对

✅ **远程认证协议**

- 挑战 - 响应完整流程
- Nonce 绑定机制
- 重放攻击防护

✅ **密钥管理**

- SGX Sealing Key 派生
- 密钥层次结构（L0→L1→L2→L3）
- 租户隔离

✅ **安全通道**

- 基于认证的密钥协商
- 加密通信

✅ **访问控制**

- 测量值白名单
- 证书链验证

---

## 🔧 环境要求

### 硬件要求

| 组件 | 最低要求          | 推荐配置             |
| ---- | ----------------- | -------------------- |
| CPU  | Intel 第 6 代酷睿 | Intel 第 10 代或更新 |
| SGX  | FLC 支持          | SGX2 支持            |
| 内存 | 4GB               | 8GB+                 |
| BIOS | SGX Enabled       | SGX + VT-x Enabled   |

### 软件要求

| 组件         | 版本  | 说明                                  |
| ------------ | ----- | ------------------------------------- |
| Linux Kernel | 5.11+ | 内置 SGX 驱动                         |
| SGX Driver   | 2.11+ | 内核<5.11 时需要                      |
| SGX SDK      | 2.24+ | Intel 官方 SDK                        |
| DCAP Library | 1.15+ | Data Center Attestation Primitives    |
| AESM Service | 最新  | Architectural Enclave Service Manager |

### 快速检查清单

```bash
# ✓ CPU 支持 SGX
cat /proc/cpuinfo | grep sgx

# ✓ 驱动已加载
lsmod | grep sgx

# ✓ AESM 服务运行中
systemctl is-active aesmd

# ✓ DCAP 库已安装
ldconfig -p | grep sgx

# ✓ 设备节点存在
ls -la /dev/sgx_*
```

---

## 📊 测试结果解读

### 通过标准

所有测试应通过：

```
test result: ok. 11 passed; 0 failed; 0 ignored
```

### 失败处理

如果看到失败：

```
test test_dcap_quote_generation_hardware ... FAILED
```

1. **查看详细错误**: 使用 `--nocapture` 参数
2. **检查日志**: 查看 AESM 和 DCAP 日志
3. **参考故障排查**: 见 [SGX_TEST_EXECUTION_GUIDE.md](./SGX_TEST_EXECUTION_GUIDE.md#4-故障排查)

### 性能基准

在推荐配置上的预期性能：

| 指标         | 预期值 |
| ------------ | ------ |
| Quote 生成   | <500ms |
| Quote 验证   | <200ms |
| 完整测试套件 | <30s   |
| 代码覆盖率   | >80%   |

---

## 🛠️ 常见问题

### Q1: 测试必须在 SGX 硬件上运行吗？

**A**: 是的。测试套件设计为在真实 SGX 硬件上运行。如果需要在无硬件环境开发，请使用模拟模式：

```rust
let config = EnclaveConfig {
    debug_mode: true,  // 模拟模式
    ..Default::default()
};
```

### Q2: 如何在 CI/CD 中集成测试？

**A**: 使用 JUnit 格式输出：

```bash
cargo test --test sgx_hardware_tests \
    -- --format=junit > test_results.xml
```

然后在 Jenkins/GitLab CI 中解析 XML 报告。

### Q3: 测试失败如何调试？

**A**: 启用详细日志：

```bash
RUST_LOG=debug cargo test --test sgx_hardware_tests -- --nocapture
```

### Q4: 如何在 macOS/Windows 上测试？

**A**: SGX 硬件测试仅支持 Linux。在其他平台可以：

1. 使用虚拟机（启用 SGX 穿透）
2. 使用模拟模式运行单元测试
3. 使用远程 Linux 测试服务器

---

## 📈 持续改进

### 添加新测试

在 `tests/sgx_hardware_tests.rs` 中添加：

```rust
#[test]
#[cfg(target_os = "linux")]
fn test_new_feature_hardware() {
    // 新测试实现
}
```

### 更新基准

定期在不同硬件平台上运行测试并记录性能：

```bash
# 记录基准
cargo test --test sgx_hardware_tests --release \
    | tee benchmark_$(date +%Y%m%d).log
```

### 贡献测试用例

欢迎提交新的测试用例，特别是：

- 边界条件测试
- 压力测试
- 安全场景测试

---

## 📚 相关资源

### 官方文档

- [Intel SGX 官方文档](https://www.intel.com/content/www/us/en/developer/tools/software-guard-extensions/overview.html)
- [DCAP GitHub](https://github.com/intel/SGXDataCenterAttestationPrimitives)
- [Linux SGX 驱动](https://github.com/intel/linux-sgx-driver)

### 项目文档

- [CredBridge 架构文档](./ARCHITECTURE.md)
- [TEE 安全模型](./TEE_SECURITY_MODEL.md)
- [API 参考](./API.md)

### 社区资源

- [Intel SGX 开发者论坛](https://community.intel.com/t5/Intel-Software-Guard-Extensions/bd-p/software-guard-extensions)
- [Rust SGX 生态系统](https://github.com/apache/incubator-teaclave-sgx-sdk)

---

## 📞 获取帮助

### 故障排查

1. 查看 [SGX_TEST_EXECUTION_GUIDE.md](./SGX_TEST_EXECUTION_GUIDE.md#4-故障排查)
2. 运行诊断脚本收集日志
3. 查看项目 Issues 是否有类似问题

### 联系支持

- 创建 GitHub Issue
- 发送邮件至安全团队
- 参与 Intel SGX 社区讨论

---

## 📝 更新日志

| 日期       | 版本  | 变更                             |
| ---------- | ----- | -------------------------------- |
| 2026-03-20 | 1.0.0 | 初始版本，包含 10 个核心测试用例 |

---

**最后更新**: 2026-03-20
**维护者**: CredBridge 安全团队
