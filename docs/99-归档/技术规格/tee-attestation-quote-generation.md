---
title: 'TEE Attestation Quote 生成实现'
slug: 'tee-attestation-quote-generation'
created: '2026-03-19'
status: 'implemented'
priority: 'P1'
---

# Tech-Spec: TEE Attestation Quote 生成实现

## 概述

### 问题陈述

`vault-service/src/api/attestation.rs` 中的 `generate_real_quote()` 函数对 SGX、TDX、SEV-SNP 三种 TEE 平台均返回 `Err("... not implemented")`，导致在真实 TEE 硬件环境中无法完成远程认证，vault-service 无法向客户端证明自身运行在受信任的执行环境中。

### 解决方案

为三种 TEE 平台分别引入对应的 native SDK crate，在 `generate_real_quote()` 的每个分支中调用平台专属 API 生成 Quote 或 Attestation Report，并以 Base64 编码格式返回。

### 范围

- `vault-service/src/api/attestation.rs`：实现 `generate_real_quote()` 的三个 `match` 分支
- `vault-service/Cargo.toml`：按 feature flag 引入平台 SDK 依赖
- 模拟模式路径（`generate_simulated_quote()`）**不在**本规格范围内，保持不变

---

## 开发上下文

### 当前代码（关键片段）

```rust
// vault-service/src/api/attestation.rs:317-344

/// 生成真实 Quote（TEE 环境）
fn generate_real_quote(state: &AttestationState) -> Result<String, String> {
    // 在实际实现中，这里应该调用 TEE SDK 生成 Quote
    // 例如：
    // - SGX: 调用 sgx_ql_get_quote()
    // - TDX: 调用 tdx_attest_get_quote()
    // - SEV-SNP: 调用 SNP firmware 获取 attestation report

    if !state.initialized {
        return Err("TEE not initialized".to_string());
    }

    match state.tee_type {
        TeeType::Sgx => {
            // TODO: 调用 SGX DCAP 库生成 Quote          ← 第 331 行
            Err("SGX quote generation not implemented".to_string())
        }
        TeeType::Tdx => {
            // TODO: 调用 TDX attestation 库生成 Quote   ← 第 335 行
            Err("TDX quote generation not implemented".to_string())
        }
        TeeType::SevSnp => {
            // TODO: 调用 SEV-SNP 库生成 attestation report ← 第 339 行
            Err("SEV-SNP attestation not implemented".to_string())
        }
        _ => Err(format!("Unsupported TEE type: {:?}", state.tee_type)),
    }
}
```

### TEE 平台说明

| 平台 | 厂商 | Quote 格式 | 主要 API 入口 | 依赖运行环境 |
|------|------|-----------|--------------|------------|
| **SGX** | Intel | ECDSA Quote v3/v4（二进制） | `sgx_ql_get_quote()` via DCAP | AESM 服务 + DCAP 驱动 |
| **TDX** | Intel | TD Quote（扩展 SGX Quote 格式） | `tdx_attest_get_quote()` | Linux TDX 模块 + DCAP |
| **SEV-SNP** | AMD | Attestation Report（结构体 binary） | `/dev/sev-guest` ioctl `SNP_GET_REPORT` | AMD SP firmware |

**Report Data（用户数据绑定）：**
- SGX/TDX：64 字节 `report_data` 字段，通常填入 `SHA-256(challenge ‖ nonce)` 并用 0 填充至 64 字节
- SEV-SNP：`SNP_REPORT_REQ.user_data`，64 字节，同上

**调用方当前传入参数：**
`AttestationState` 包含 `mrenclave`（`Option<String>`）和 `mrsigner`（`Option<String>`），但不包含 `report_data`/`nonce`——实现时需确认挑战值的传递路径（建议在函数签名中增加 `report_data: &[u8; 64]` 参数，或从 `state` 扩展）。

### 需要参考的文件

| 文件 | 用途 |
|------|------|
| `vault-service/src/api/attestation.rs` | 目标实现文件，`generate_real_quote()` 函数 |
| `vault-service/Cargo.toml` | 需要添加平台 SDK feature 依赖 |
| `src/tee/attestation.rs` | 顶层认证服务，了解 `ReportData` 构造方式 |
| `src/tee/dcap.rs` | DCAP 验证逻辑，理解 Quote 格式与数据结构 |

### 技术决策

#### SGX DCAP（第 331 行）

推荐 crate：`dcap-qbg`（或 `sgx-dcap-ql-sys`，需 Intel DCAP 动态库）

```toml
# vault-service/Cargo.toml
[features]
sgx = ["dcap-qbg"]

[dependencies]
dcap-qbg = { version = "0.1", optional = true }
```

核心调用流程：
1. 构造 `sgx_report_data_t`（64 字节，填入挑战哈希）
2. 调用 `dcap_qbg::quote::get_quote(&report_data)` 或 FFI `sgx_ql_get_quote()`
3. 返回值为原始 Quote 字节，Base64 编码后返回

#### TDX（第 335 行）

推荐 crate：`tdx-attest`（Intel 官方 Rust wrapper）或直接 FFI `libtdx_attest`

```toml
[features]
tdx = ["tdx-attest"]

[dependencies]
tdx-attest = { version = "0.1", optional = true }
```

核心调用流程：
1. 构造 `tdx_report_data_t`（64 字节）
2. 调用 `tdx_attest::get_quote(&report_data, None)` → `Vec<u8>`
3. Base64 编码后返回

#### SEV-SNP（第 339 行）

推荐 crate：`sev`（virtee/sev，AMD 官方社区维护）

```toml
[features]
sev-snp = ["sev"]

[dependencies]
sev = { version = "4", features = ["snp"], optional = true }
```

核心调用流程：
1. 打开 `/dev/sev-guest`（需要 Linux 内核 5.19+）
2. 构造 `SnpReportReq { user_data: [u8; 64], vmpl: 0 }`
3. 调用 `sev::firmware::guest::Firmware::get_report(None, req)?`
4. 将 `AttestationReport` 序列化为字节，Base64 编码后返回

---

## 实现计划

### 任务

1. **SGX DCAP Quote 生成** — `vault-service/src/api/attestation.rs:331`
   - 引入 `dcap-qbg` 或 `sgx-dcap-ql-sys` feature
   - 实现 `TeeType::Sgx` 分支：构造 report_data → 调用 SDK → Base64 编码
   - 在 `AttestationState` 或函数参数中传入 nonce/report_data

2. **TDX Quote 生成** — `vault-service/src/api/attestation.rs:335`
   - 引入 `tdx-attest` feature
   - 实现 `TeeType::Tdx` 分支：构造 report_data → 调用 `tdx_attest::get_quote` → Base64 编码

3. **SEV-SNP Attestation Report 生成** — `vault-service/src/api/attestation.rs:339`
   - 引入 `sev` crate feature
   - 实现 `TeeType::SevSnp` 分支：打开 `/dev/sev-guest` → 调用 ioctl → 序列化 → Base64 编码

4. **函数签名扩展（前置任务）**
   - 在 `generate_real_quote()` 中增加 `report_data: Option<&[u8; 64]>` 参数
   - 或在 `AttestationState` 中添加 `challenge_data: Option<[u8; 64]>` 字段
   - 同步更新所有调用点

5. **错误类型规范化**
   - 将返回类型从 `Result<String, String>` 改为 `Result<String, AttestationApiError>`（新建或复用 `AttestationError`）

### 验收标准

- [ ] 在真实 SGX 硬件（或 SGX SDK 软件模拟器）上，`generate_real_quote()` 返回有效的 Base64 编码 Quote
- [ ] 在真实 TDX 环境上，`generate_real_quote()` 返回有效的 Base64 编码 TD Quote
- [ ] 在真实 SEV-SNP 环境上，`generate_real_quote()` 返回有效的 Base64 编码 Attestation Report
- [ ] `simulation_mode = true` 时，仍走 `generate_simulated_quote()` 路径，行为不变
- [ ] `state.initialized = false` 时，仍返回 `Err("TEE not initialized")`
- [ ] 各平台 feature flag 相互独立，编译时不引入未使用平台的依赖
- [ ] 单元测试覆盖：各平台 feature 关闭时编译通过；模拟模式返回预期格式

---

## 附加上下文

### 依赖（各平台所需 crate）

```toml
# vault-service/Cargo.toml 建议结构

[features]
default = []
sgx = ["dep:dcap-qbg"]
tdx = ["dep:tdx-attest"]
sev-snp = ["dep:sev"]

[dependencies]
dcap-qbg    = { version = "0.1",  optional = true }
tdx-attest  = { version = "0.1",  optional = true }
sev         = { version = "4",    features = ["snp"], optional = true }
base64      = "0.22"
```

### 测试策略（模拟环境 vs 真实 TEE 硬件）

| 环境 | 测试方式 | CI 可行性 |
|------|---------|---------|
| 开发机（无 TEE） | `simulation_mode = true`，测试模拟路径 | 可行 |
| SGX SDK 软件模拟 | 使用 Intel SGX SDK 的 `SIM` 模式 | 需安装 SDK |
| 真实 SGX 硬件 | 集成测试，需专用 CI runner | 仅 nightly/staging |
| 真实 TDX VM | 需 TDX 支持的宿主机 | 仅 nightly/staging |
| 真实 SEV-SNP VM | 需 AMD EPYC + SNP 固件 | 仅 nightly/staging |

**建议：** 在 CI 中只运行模拟模式测试；真实平台测试在专用 staging 环境手动或定期触发。

### 注意事项

1. **权限要求**：SEV-SNP 的 `/dev/sev-guest` 需要 root 或特定 cgroup 权限；TDX 的设备节点同理。容器部署时需 `--device` 挂载。
2. **AESM 服务依赖**：SGX DCAP 在 Linux 下依赖 `aesmd` 服务，Docker 镜像需包含并启动该服务，或通过 Unix socket 挂载宿主机的 AESM。
3. **Quote 大小限制**：SGX DCAP Quote v4 可达 ~4KB，SEV-SNP Report 固定 1184 字节；`MAX_QUOTE_LEN = 8192`（定义于 `src/tee/attestation.rs:50`）已足够。
4. **Cargo feature 互斥**：三个平台 feature 在单次编译时可同时启用，但运行时 `state.tee_type` 决定实际路径，互不干扰。
5. **安全清零**：从 SDK 获取的原始 Quote 字节在 Base64 编码完成后，若存在中间 `Vec<u8>` 缓冲区，应使用 `zeroize` 清除，与项目安全规范保持一致。
