# SGX 硬件测试执行指南

本文档详细介绍如何在 Intel SGX 硬件环境下执行 ToaniVault 测试套件。

---

## 目录

1. [环境准备](#1-环境准备)
2. [执行测试](#2-执行测试)
3. [结果分析](#3-结果分析)
4. [故障排查](#4-故障排查)
5. [性能基准](#5-性能基准)

---

## 1. 环境准备

### 1.1 硬件要求

在开始测试前，确认系统满足以下要求：

```bash
# 检查 CPU SGX 支持
cat /proc/cpuinfo | grep -E "sgx|sgx_lc|sgx_dcap"

# 应该看到类似输出：
# flags: ... sgx sgx_lc ...
```

### 1.2 BIOS 设置

重启进入 BIOS，确保以下设置已启用：

- **Intel SGX**: Enabled 或 Software Controlled
- **Virtualization Technology (VT-x)**: Enabled
- **FLC (Flexible Launch Control)**: Enabled（如果可用）

### 1.3 安装依赖

#### Ubuntu/Debian

```bash
# 更新包列表
sudo apt update

# 安装 SGX 驱动（内核 < 5.11）
sudo apt install libsgx-enclave-common libsgx-enclave-common-dev

# 安装 DCAP 库
sudo apt install \
    libsgx-dcap-ql \
    libsgx-dcap-ql-dev \
    libsgx-quote-ex \
    libsgx-dcap-default-qpl \
    sgx-aesm-service

# 安装 PCCS（可选，用于生产环境）
sudo apt install sgx-dcap-pccs
```

#### CentOS/RHEL

```bash
# 添加 EPEL 仓库
sudo yum install epel-release

# 安装依赖
sudo yum install \
    libsgx-enclave-common \
    libsgx-dcap-ql \
    libsgx-dcap-default-qpl
```

### 1.4 启动服务

```bash
# 启动 AESM 服务
sudo systemctl start aesmd
sudo systemctl enable aesmd

# 检查状态
systemctl status aesmd

# 启动 PCCS（如果安装）
sudo systemctl start pccs
sudo systemctl enable pccs
```

### 1.5 配置环境变量

```bash
# 添加到 ~/.bashrc
export SGX_MODE=hardware
export RUST_LOG=info

# 如果安装了 SGX SDK
source /opt/intel/sgxsdk/environment

# 使配置生效
source ~/.bashrc
```

### 1.6 运行环境检查脚本

```bash
# 切换到项目目录
cd /path/to/credbridge

# 运行检查脚本
chmod +x scripts/check_sgx_environment.sh
./scripts/check_sgx_environment.sh
```

**预期输出**:

```
========================================
SGX 硬件环境检查
========================================

1. 操作系统检查
----------------------------------------
✓ 操作系统：Linux 5.15.0-generic

2. CPU SGX 支持检查
----------------------------------------
✓ CPU 支持 SGX
✓ 支持 FLC (Flexible Launch Control)
✓ 支持 DCAP

3. SGX 驱动检查
----------------------------------------
✓ SGX 驱动已加载 (intel_sgx)

4. SGX 设备节点检查
----------------------------------------
✓ /dev/sgx_enclave 存在
✓ /dev/sgx_provision 存在

5. AESM 服务检查
----------------------------------------
✓ AESM 服务运行中

6. DCAP 库检查
----------------------------------------
✓ libsgx_dcap_ql 已安装
✓ libsgx_quote_ex 已安装

7. SGX SDK 检查
----------------------------------------
✓ Intel SGX SDK 已安装

8. EPC 内存检查
----------------------------------------
✓ EPC 内存：93 MB

========================================
检查总结
========================================
✓ 所有检查通过，SGX 环境就绪
```

---

## 2. 执行测试

### 2.1 编译测试

```bash
# 切换到项目目录
cd /path/to/credbridge

# 编译测试（释放模式）
cargo test --test sgx_hardware_tests --release --no-run
```

### 2.2 运行完整测试套件

```bash
# 运行所有硬件测试（单线程，避免并发问题）
cargo test --test sgx_hardware_tests -- --test-threads=1
```

**预期输出**:

```
running 11 tests
test test_suite_info ... ok
test test_sgx_hardware_availability ... ok
test test_enclave_measurement_consistency ... ok
test test_dcap_quote_generation_hardware ... ok
test test_dcap_quote_verification_hardware ... ok
test test_challenge_response_protocol_hardware ... ok
test test_sgx_sealing_key_derivation ... ok
test test_key_hierarchy_hardware ... ok
test test_secure_channel_establishment_hardware ... ok
test test_measurement_whitelist_hardware ... ok
test test_replay_attack_protection_hardware ... ok

test result: ok. 11 passed; 0 failed; 0 ignored
```

### 2.3 运行特定测试

```bash
# 运行单个测试
cargo test test_sgx_hardware_availability -- --exact --nocapture

# 运行特定前缀的测试
cargo test test_dcap -- --nocapture

# 跳过某些测试
cargo test -- --skip test_replay_attack_protection
```

### 2.4 详细日志输出

```bash
# 启用详细日志
RUST_LOG=debug cargo test --test sgx_hardware_tests -- --nocapture

# 仅查看特定模块日志
RUST_LOG=vault_service::tee=debug cargo test --test sgx_hardware_tests
```

### 2.5 生成测试报告

```bash
# JSON 格式报告
cargo test --test sgx_hardware_tests -- --format=json > test_results.json

# JUnit XML 格式（用于 CI/CD）
cargo test --test sgx_hardware_tests -- --format=junit > test_results.xml

# 使用 cargo2junit 转换
cargo install cargo2junit
cargo test --test sgx_hardware_tests -- --format=json | cargo2junit > results.xml
```

### 2.6 代码覆盖率

```bash
# 安装 tarpaulin
cargo install cargo-tarpaulin

# 运行覆盖率测试
cargo tarpaulin \
    --test sgx_hardware_tests \
    --output-dir ./coverage \
    --out Html

# 查看报告
xdg-open ./coverage/tarpaulin-report.html
```

---

## 3. 结果分析

### 3.1 解读测试结果

**通过 (ok)**:

```
test test_sgx_hardware_availability ... ok
```

表示测试断言全部通过，SGX 功能正常。

**失败 (FAILED)**:

```
test test_dcap_quote_generation_hardware ... FAILED
```

表示测试失败，需要查看错误信息。

**失败 (ignored)**:

```
test test_some_feature ... ignored
```

表示测试被跳过（通常因为配置或条件不满足）。

### 3.2 常见错误分析

#### 错误 1: SGX not available

```
thread 'test_sgx_hardware_availability' panicked at 'SGX 硬件初始化失败'
```

**原因**:

- SGX 驱动未加载
- AESM 服务未运行
- BIOS 中 SGX 未启用

**解决方案**:

```bash
# 检查驱动
lsmod | grep sgx

# 加载驱动
sudo modprobe intel_sgx

# 启动 AESM
sudo systemctl start aesmd
```

#### 错误 2: Quote generation failed

```
thread 'test_dcap_quote_generation_hardware' panicked at 'Quote 生成失败'
```

**原因**:

- DCAP 库未安装
- AESM 服务异常
- EPC 内存不足

**解决方案**:

```bash
# 检查 DCAP 库
ldconfig -p | grep sgx

# 重启 AESM
sudo systemctl restart aesmd

# 检查 EPC 内存
cat /sys/devices/system/cpu/sgx/epc_size
```

#### 错误 3: Measurement mismatch

```
thread 'test_measurement_whitelist_hardware' panicked at 'MeasurementMismatch'
```

**原因**:

- 白名单中的 MRENCLAVE 与实际不匹配

**解决方案**:

```rust
// 使用实际的 MRENCLAVE
let config = DcapConfig {
    allowed_mrenclaves: vec![enclave.mrenclave()],
    ..Default::default()
};
```

### 3.3 性能指标

记录以下性能指标用于基准对比：

| 测试项 | 预期时间 | 实际时间 | 状态 |
| ------ | -------- | -------- | ---- |
| HW-001 | <100ms   | \_\_\_ms | ☐    |
| HW-003 | <500ms   | \_\_\_ms | ☐    |
| HW-004 | <200ms   | \_\_\_ms | ☐    |
| HW-005 | <1000ms  | \_\_\_ms | ☐    |

---

## 4. 故障排查

### 4.1 收集诊断信息

```bash
# 创建诊断目录
mkdir -p ~/sgx_diagnosis

# 收集系统信息
lscpu | grep -i sgx > ~/sgx_diagnosis/cpu_info.txt
dmesg | grep -i sgx > ~/sgx_diagnosis/dmesg.txt

# 收集服务日志
journalctl -u aesmd > ~/sgx_diagnosis/aesmd.log
journalctl -u pccs > ~/sgx_diagnosis/pccs.log

# 收集驱动信息
lsmod | grep sgx > ~/sgx_diagnosis/modules.txt
ls -la /dev/sgx_* > ~/sgx_diagnosis/devices.txt

# 打包
tar czf sgx_diagnosis.tar.gz ~/sgx_diagnosis
```

### 4.2 常见问题速查表

| 问题         | 检查项                                     | 解决方案                          |
| ------------ | ------------------------------------------ | --------------------------------- |
| SGX 不可用   | BIOS 设置                                  | 重启进入 BIOS，启用 SGX           |
| 驱动未加载   | `lsmod \| grep sgx`                        | `sudo modprobe intel_sgx`         |
| AESM 未运行  | `systemctl status aesmd`                   | `sudo systemctl start aesmd`      |
| DCAP 库缺失  | `ldconfig -p \| grep sgx`                  | `sudo apt install libsgx-dcap-ql` |
| 设备节点缺失 | `ls -la /dev/sgx_*`                        | 重新加载驱动或重启                |
| EPC 内存不足 | `cat /sys/devices/system/cpu/sgx/epc_size` | 关闭其他 SGX 应用                 |

### 4.3 调试模式

```bash
# 启用 SGX 调试日志
export SGX_DEBUG=1

# 启用 DCAP 调试
export DCAP_DEBUG=1

# 完整调试模式
RUST_LOG=trace SGX_DEBUG=1 DCAP_DEBUG=1 \
    cargo test --test sgx_hardware_tests -- --nocapture 2>&1 | tee debug.log
```

---

## 5. 性能基准

### 5.1 建立基准

在标准硬件上运行测试并记录结果：

```bash
# 运行基准测试
cargo test --test sgx_hardware_tests --release -- --test-threads=1 \
    | tee benchmark_$(date +%Y%m%d).log
```

### 5.2 性能优化建议

1. **使用释放模式编译**:

   ```bash
   cargo test --release --test sgx_hardware_tests
   ```

2. **减少日志输出**:

   ```bash
   RUST_LOG=error cargo test --test sgx_hardware_tests
   ```

3. **确保独占访问**:
   关闭其他 SGX 应用，避免 EPC 内存竞争

### 5.3 性能对比表

| 硬件平台           | Quote 生成 | Quote 验证 | 总测试时间 |
| ------------------ | ---------- | ---------- | ---------- |
| Intel i7-10700     | \_\_\_ms   | \_\_\_ms   | \_\_\_s    |
| Intel i5-9400      | \_\_\_ms   | \_\_\_ms   | \_\_\_s    |
| Intel Xeon E-2278G | \_\_\_ms   | \_\_\_ms   | \_\_\_s    |

---

## 附录 A: 快速参考命令

```bash
# 环境检查
./scripts/check_sgx_environment.sh

# 运行所有测试
cargo test --test sgx_hardware_tests -- --test-threads=1

# 运行单个测试
cargo test test_sgx_hardware_availability -- --exact --nocapture

# 生成覆盖率报告
cargo tarpaulin --test sgx_hardware_tests --output-dir ./coverage --out Html

# 收集诊断信息
mkdir -p ~/sgx_diagnosis && \
    lscpu | grep -i sgx > ~/sgx_diagnosis/cpu_info.txt && \
    dmesg | grep -i sgx > ~/sgx_diagnosis/dmesg.txt && \
    journalctl -u aesmd > ~/sgx_diagnosis/aesmd.log && \
    tar czf sgx_diagnosis.tar.gz ~/sgx_diagnosis
```

---

## 附录 B: 相关文档

- [SGX 硬件测试计划](./SGX_HARDWARE_TEST_PLAN.md)
- [DCAP 设置指南](./DCAP_SETUP.md)
- [TEE 沙箱分析](./SANDBOX_ANALYSIS.md)
- [ToaniVault API 文档](./API.md)
