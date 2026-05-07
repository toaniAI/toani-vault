# DCAP 远程认证设置指南

本文档介绍如何在 ToaniVault 中配置和使用 Intel SGX DCAP (Data Center Attestation Primitives) 远程认证功能。

## 目录

- [概述](#概述)
- [环境要求](#环境要求)
- [安装 DCAP 驱动和库](#安装-dcap-驱动和库)
- [配置 Intel PCS](#配置-intel-pcs)
- [ToaniVault DCAP 配置](#credbridge-dcap-配置)
- [调试 Demo](#调试-demo)
- [API 使用](#api-使用)
- [故障排查](#故障排查)

## 概述

DCAP 远程认证允许远程验证方密码学验证 ToaniVault Enclave 的真实性。主要功能包括：

- **Quote 生成**: Enclave 启动时自动生成 DCAP Quote
- **Intel PCS 注册**: 向 Intel 配置服务注册 Enclave
- **远程验证**: 客户端可验证 Quote 的签名和测量值
- **证书链验证**: 验证 PCK 证书链到 Intel 根证书

### DCAP 架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        ToaniVault Enclave                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   Enclave   │  │   DCAP      │  │   Quote Generation      │  │
│  │   Core      │  │   Service   │  │   (MRENCLAVE/MRSIGNER)  │  │
│  └──────┬──────┘  └──────┬──────┘  └─────────────────────────┘  │
│         │                │                                      │
│         └────────────────┘                                      │
│                   │                                             │
│              Quote with Signature                               │
└───────────────────┼─────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Intel PCS (Cloud)                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │   PCK       │  │   TCB       │  │   Certificate           │  │
│  │   Certs     │  │   Info      │  │   Chain Validation      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Remote Verifier                           │
│  - Verify Quote Signature                                        │
│  - Check MRENCLAVE White list                                    │
│  - Validate Certificate Chain                                    │
└─────────────────────────────────────────────────────────────────┘
```

## 环境要求

### 硬件要求

- Intel CPU with SGX support (第6代酷睿或更新的处理器)
- SGX Enabled in BIOS
- 支持 FLC (Flexible Launch Control) 的处理器（用于 DCAP）

### 软件要求

- Linux Kernel 5.11+ (内置 SGX 驱动)
- 或者 Intel SGX Driver (对于旧内核)
- Intel SGX SDK
- Intel DCAP 库

### 检查 SGX 支持

```bash
# 检查 CPU 是否支持 SGX
cat /proc/cpuinfo | grep sgx

# 应该输出包含以下内容的行：
# - sgx: Software Guard Extensions
# - sgx_lc: SGX launch configuration
# - sgx_dcap: SGX DCAP
```

## 安装 DCAP 驱动和库

### 1. 安装 SGX 驱动（如果内核 < 5.11）

```bash
# 下载并安装驱动
git clone https://github.com/intel/linux-sgx-driver.git
cd linux-sgx-driver
make
sudo mkdir -p "/lib/modules/$(uname -r)/kernel/drivers/intel/sgx"
sudo cp isgx.ko "/lib/modules/$(uname -r)/kernel/drivers/intel/sgx"
sh -c "echo '/lib/modules/$(uname -r)/kernel/drivers/intel/sgx/isgx.ko' >> /etc/modules"
sudo depmod
sudo modprobe isgx
```

### 2. 安装 Intel SGX SDK

```bash
# 下载 SGX SDK
wget https://download.01.org/intel-sgx/latest/linux-latest/distro/ubuntu22.04-server/sgx_linux_x64_sdk_2.24.100.3.bin

# 安装
chmod +x sgx_linux_x64_sdk_2.24.100.3.bin
sudo ./sgx_linux_x64_sdk_2.24.100.3.bin --prefix=/opt/intel

# 设置环境变量
echo 'source /opt/intel/sgxsdk/environment' >> ~/.bashrc
source /opt/intel/sgxsdk/environment
```

### 3. 安装 DCAP 库

```bash
# 添加 Intel SGX 仓库
echo 'deb [arch=amd64 signed-by=/usr/share/keyrings/intel-sgx-keyring.gpg] https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main' | sudo tee /etc/apt/sources.list.d/intel-sgx.list

# 添加密钥
wget https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key
cat intel-sgx-deb.key | sudo tee /usr/share/keyrings/intel-sgx-keyring.gpg > /dev/null

# 更新并安装
sudo apt update
sudo apt install -y libsgx-dcap-ql libsgx-dcap-ql-dev libsgx-quote-ex libsgx-dcap-default-qpl
```

### 4. 配置默认 Quote Provider Library (QPL)

```bash
# 编辑配置文件
sudo nano /etc/sgx_default_qcnl.conf

# 配置内容：
{
  "pccs_url": "https://pccs.example.com/sgx/certification/v4/",
  "use_secure_cert": true,
  "collateral_service": "https://api.trustedservices.intel.com/sgx/certification/v4/",
  "pccs_api_version": "3.1"
}
```

## 配置 Intel PCS

### 选项 1: 使用 Intel 公有 PCS 服务

当你显式选择 `TEE_MODE=hardware` 且需要直接访问 Intel PCS 时，可以使用 Intel 的公有 PCS 服务：

```
https://api.trustedservices.intel.com/sgx/certification/v4/
```

注意：需要 API Key 才能访问。

### 选项 2: 部署本地 PCCS (Provisioning Certificate Caching Service)

当你显式选择 `TEE_MODE=hardware` 并准备长期运行硬件 attestation 时，建议部署本地 PCCS：

```bash
# 安装 PCCS
sudo apt install sgx-dcap-pccs

# 配置 PCCS
sudo nano /opt/intel/sgx-dcap-pccs/config/default.json

# 启动 PCCS
sudo systemctl start pccs
sudo systemctl enable pccs
```

### 获取 Intel PCS API Key

1. 访问 [Intel Trusted Services Portal](https://api.portal.trustedservices.intel.com/)
2. 注册/登录账户
3. 创建新的 API Key
4. 保存 API Key（仅显示一次）

## ToaniVault DCAP 配置

### 环境变量配置

```bash
# TEE 运行模式（必须显式设置）
export TEE_MODE=hardware

# 显式 simulation 示例
# export TEE_MODE=simulation

# Intel PCS URL
export INTEL_PCS_URL=https://api.trustedservices.intel.com/sgx/certification/v4/

# Intel PCS API Key
export INTEL_PCS_API_KEY=your_api_key_here

# Quote 最大有效期（秒）
export DCAP_QUOTE_MAX_AGE=3600

# 是否验证证书链
export DCAP_VERIFY_CERT_CHAIN=true
```

- `TEE_MODE=hardware` 会走真实 SGX/DCAP 路径；若 SGX/DCAP/AESM/PCCS（或 Intel PCS）未就绪，将 fail-closed。
- `TEE_MODE=simulation` 只用于显式模拟路径；相关模拟状态字段只会在该模式下出现。

### 代码配置示例

```rust
use vault_service::config::TeeRuntimeMode;
use vault_service::tee::{
    dcap::{DcapConfig, DcapService, INTEL_PCS_BASE_URL_PROD},
    enclave::{Enclave, EnclaveConfig},
};

// 创建 DCAP 配置
let dcap_config = DcapConfig {
    runtime_mode: TeeRuntimeMode::Hardware,
    pcs_base_url: INTEL_PCS_BASE_URL_PROD.to_string(),
    use_test_environment: false,
    api_key: Some("your_api_key".to_string()),
    quote_max_age_seconds: 3600,
    verify_certificate_chain: true,
    allowed_mrenclaves: vec![
        // 允许的 MRENCLAVE 白名单
        hex::decode("0123456789abcdef...").unwrap().try_into().unwrap(),
    ],
    allowed_mrsigners: vec![
        // 允许的 MRSIGNER 白名单
        hex::decode("fedcba9876543210...").unwrap().try_into().unwrap(),
    ],
    ..Default::default()
};

// 创建 DCAP 服务
let dcap_service = DcapService::new(dcap_config)?;

// 创建并初始化 Enclave
let mut enclave = Enclave::new(EnclaveConfig::default());
enclave.initialize()?;

// 初始化 DCAP（自动生成 Quote）
let quote = dcap_service.initialize(&enclave)?;
println!("MRENCLAVE: {}", hex::encode(quote.report_body.mrenclave));
println!("MRSIGNER: {}", hex::encode(quote.report_body.mrsigner));
```

### 显式 simulation 模式配置

在没有 SGX 硬件的环境中，如需运行 simulation-safe 测试或文档示例，请显式设置 `TEE_MODE=simulation`：

````rust
use vault_service::config::TeeRuntimeMode;

let dcap_config = DcapConfig {
    runtime_mode: TeeRuntimeMode::Simulation,
    ..Default::default()
};

let dcap_service = DcapService::new(dcap_config)?;

## 调试 Demo

仓库提供两个可独立运行的 demo，用于区分“旧错误复现”与“标准流程验证”：

```bash
# 反例：故意跳过 sgx_qe_get_target_info()
cargo run --features tee-hardware --bin sgx_dcap_quote_legacy_demo

# 正例：按 Intel 推荐顺序执行
TEE_ENCLAVE_PATH=/path/to/credbridge_enclave.signed.so \
cargo run --features tee-hardware --bin sgx_dcap_quote_standard_demo
````

- `sgx_dcap_quote_legacy_demo` 用于稳定复现旧链路，输出已加载 `.so` 路径、`get_quote_size` 返回码和失败阶段。
- `sgx_dcap_quote_standard_demo` 用于验证修复后的执行顺序，输出 `.so` 路径、`get_target_info rc`、`get_quote_size rc`、`quote_size` 和最终失败阶段。

````

## API 使用

### 远程认证 API 端点

启动 ToaniVault 服务后，可以使用以下 API 进行远程认证：

#### 1. 获取 Quote

```bash
curl http://localhost:3000/api/v1/attestation/quote
````

响应：

```json
{
  "success": true,
  "data": {
    "version": 3,
    "sign_type": 2,
    "mrenclave": "0123456789abcdef...",
    "mrsigner": "fedcba9876543210...",
    "timestamp": 1640000000,
    "quote_b64": "BASE64_ENCODED_QUOTE"
  },
  "error": null
}
```

#### 2. 验证 Quote

```bash
curl -X POST http://localhost:3000/api/v1/attestation/verify \
  -H "Content-Type: application/json" \
  -d '{
    "quote_b64": "BASE64_ENCODED_QUOTE",
    "nonce": "OPTIONAL_NONCE_BASE64"
  }'
```

响应：

```json
{
  "success": true,
  "valid": true,
  "mrenclave": "0123456789abcdef...",
  "mrsigner": "fedcba9876543210...",
  "timestamp": 1640000000,
  "error": null
}
```

#### 3. 获取认证报告

```bash
curl http://localhost:3000/api/v1/attestation/report
```

响应：

```json
{
  "success": true,
  "data": {
    "version": "1.0.0",
    "mrenclave": "0123456789abcdef...",
    "mrsigner": "fedcba9876543210...",
    "security_version": 1,
    "product_id": 1,
    "attributes": "0500000000000000...",
    "timestamp": 1640000000,
    "quote_b64": "BASE64_ENCODED_QUOTE",
    "certificate_info": {
      "subject": "CN=Intel SGX PCK Certificate",
      "issuer": "CN=Intel SGX PCK Platform CA",
      "not_before": "2024-01-01T00:00:00Z",
      "not_after": "2025-01-01T00:00:00Z",
      "fingerprint": "abcd1234..."
    }
  },
  "error": null
}
```

#### 4. 刷新 Quote

```bash
curl -X POST http://localhost:3000/api/v1/attestation/refresh
```

#### 5. 健康检查

```bash
curl http://localhost:3000/api/v1/attestation/health
```

响应：

```json
{
  "status": "healthy",
  "enclave_state": "running",
  "dcap_version": "1.0.0",
  "quote_valid": true
}
```

显式 `TEE_MODE=simulation` 时，相关状态接口才会出现模拟标记；若设置 `TEE_MODE=hardware` 但真实能力未接通，请预期初始化失败，而不是得到模拟健康状态。

### 客户端验证示例

```rust
use vault_service::tee::{
    dcap::{DcapConfig, DcapService},
    quote::{QuoteParser, QuoteValidator},
};

// 从服务器获取 Quote
let quote_b64 = fetch_quote_from_server().await?;
let quote_bytes = base64::decode(&quote_b64)?;

// 解析 Quote
let quote = QuoteParser::parse(&quote_bytes)?;

// 验证 Quote
let validator = QuoteValidator::new();
validator.validate(&quote)?;

// 使用 DCAP 服务验证
let dcap_config = DcapConfig::default();
let dcap_service = DcapService::new(dcap_config)?;
let report = dcap_service.verify_attestation(&quote_bytes, None)?;

if report.result.success {
    println!("Enclave verification successful!");
    println!("MRENCLAVE: {}", report.mrenclave_hex);
}
```

## 故障排查

### 常见问题

#### 1. "SGX not available" 错误

**原因**: SGX 驱动未加载或 BIOS 中未启用 SGX

**解决方案**:

```bash
# 检查 SGX 驱动状态
lsmod | grep sgx

# 如果没有输出，加载驱动
sudo modprobe intel_sgx

# 检查 BIOS 设置
# 确保 SGX 设置为 "Enabled" 或 "Software Controlled"
```

#### 2. "Failed to generate quote" 错误

**原因**: AESM (Architectural Enclave Service Manager) 未运行

**解决方案**:

```bash
# 启动 AESM
sudo systemctl start aesmd
sudo systemctl enable aesmd

# 检查状态
sudo systemctl status aesmd
```

#### 3. "Certificate verification failed" 错误

**原因**: PCCS 配置错误或无法连接到 Intel PCS

**解决方案**:

```bash
# 检查 PCCS 状态
sudo systemctl status pccs

# 检查网络连接
curl -v https://api.trustedservices.intel.com/sgx/certification/v4/crl

# 检查 API Key 是否有效
```

#### 4. "Measurement mismatch" 错误

**原因**: Quote 的测量值不在白名单中

**解决方案**:

```rust
// 添加正确的 MRENCLAVE 到白名单
let mut service = DcapService::new(config)?;
service.allow_mrenclave(actual_mrenclave);
```

### 调试模式

启用详细日志：

```bash
export RUST_LOG=debug
export DCAP_DEBUG=1

# 运行 ToaniVault
cargo run
```

### 验证 DCAP 安装

```bash
# 运行 Intel 提供的测试工具
/opt/intel/sgxsdk/bin/x64/sgx_sign dump -enclave your_enclave.signed.so -dumpfile enclave.info

# 检查 Quote 生成
/opt/intel/sgxsdk/SampleCode/SampleAttestedTLS/build/sample_attested_tls_app
```

## CI / 验收语义

- 默认 CI 只运行 simulation-safe 测试，并显式设置 `TEE_MODE=simulation`。
- hardware-only 测试应在带 SGX/DCAP/AESM 的专用 runner 或 staging 主机执行，并显式设置 `TEE_MODE=hardware`。

### 启动自检与探针约定

- `GET /health` 只表示进程存活（liveness），不代表 SGX/DCAP 已就绪。
- `GET /ready` 与 `GET /health/detail` 表示服务就绪（readiness）；当 SGX 设备、AESM、PCCS/PCS、Quote 初始化或关键启动自检失败时应返回 `503`。
- 在 `TEE_MODE=hardware` 下，若 DCAP 依赖缺失，服务应 fail-closed，而不是静默降级到 simulation。

### 建议验收矩阵

| 类别                | 运行环境                           | 目标                                     | 建议命令                                                                               |
| ------------------- | ---------------------------------- | ---------------------------------------- | -------------------------------------------------------------------------------------- |
| `simulation-safe`   | 普通 CI / 开发机                   | 验证默认构建、格式、lint、单测不依赖 SGX | `cargo fmt --check` / `cargo clippy --tests -- -D warnings` / `cargo test`             |
| `service-dependent` | 可访问数据库、Redis、immudb 的环境 | 验证服务集成行为，但不要求 SGX           | `TEE_MODE=simulation cargo test --test attestation_api_tests`                          |
| `hardware-only`     | SGX 专用 runner / 预发机           | 验证真实 SGX/DCAP/AESM/PCCS 闭环         | `TEE_MODE=hardware cargo test --test sgx_hardware_tests -- --ignored --test-threads=1` |

### 无法执行硬件验证时的记录模板

当当前环境没有 SGX runner、`aesmd`、PCCS 或 Intel PCS 凭证时，请在验收记录中明确写出：

```text
未执行验证：
- hardware-only: `TEE_MODE=hardware cargo test --test sgx_hardware_tests -- --ignored --test-threads=1`

未执行原因：
- 当前环境缺少 /dev/sgx_enclave 与 /dev/sgx_provision
- aesmd / PCCS 未部署，无法完成真实 Quote 闭环

影响范围：
- 无法证明真实 SGX sealing、真实 Quote 生成、PCCS collateral 拉取在本次变更中可用
```

### 联系支持

如果遇到无法解决的问题：

1. 收集日志: `journalctl -u pccs -n 100`
2. 收集系统信息: `lscpu | grep sgx`
3. 创建 Issue 并提供上述信息

## 参考资源

- [Intel SGX 官方文档](https://www.intel.com/content/www/us/en/developer/tools/software-guard-extensions/overview.html)
- [Intel DCAP 仓库](https://github.com/intel/SGXDataCenterAttestationPrimitives)
- [SGX SDK 仓库](https://github.com/intel/linux-sgx)
- [ToaniVault API 文档](API.md)
