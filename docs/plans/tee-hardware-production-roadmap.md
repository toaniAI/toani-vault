# CredBridge 真实 TEE（Intel SGX）运行实现路线图

> **文档目的**：将当前代码库中仍为模拟或 fail-closed 的部分，整理为可执行的工程清单，**唯一重点目标**：主服务在真实 Intel SGX 环境下能够**完整启动**并对外提供与 attestation / 密钥层次一致的行为（非「仅配置为 hardware 即报错退出」）。
>
> **范围**：以本仓库主二进制（`vault-service` crate 作为库 + 主程序）为主；`vault-service` 独立二进制中的 attestation 若纳入同一部署目标，在文末单独列出。
>
> **非目标**：TDX / SEV-SNP 全量支持、重写全部业务 API、一次性完成所有安全审计项（可在后续迭代追加）。

---

## 1. 成功标准（Definition of Done）

在同时满足以下条件下，视为「系统已成功运行在 TEE 上」：

1. **进程级**：在目标主机上设置 `TEE_MODE=hardware`，且 SGX/DCAP 前置（驱动、设备节点、AESM、PCCS 或可达 Intel PCS）就绪后，**主进程可完成初始化并持续监听端口**，不因 L0、Enclave、DCAP 初始化而必然退出。
2. **密钥根**：`HardwareRootKey`（L0）来自**真实硬件 sealing 路径**（或与真实 enclave 内 EGETKEY 结果一致的唯一来源），`root_key_source` 可明确标识为硬件来源，且 **hardware 模式下禁止**使用 simulation L0。
3. **测量值**：`MRENCLAVE` / `MRSIGNER` 来自**已加载的签名 enclave**，而非当前基于包名与版本字符串的哈希模拟。
4. **远程证明**：`DcapService` 在 hardware 下能生成**可被标准 DCAP 流程消费的 Quote**（或经文档说明的等价格式），并完成与 PCS/PCCS 的**真实交互**（注册/拉取抵押数据等按产品设计至少满足最小闭环）。
5. **Attestation API**：`init_attestation_api` 在 hardware 下成功完成 Enclave + DCAP + `AttestationService` 初始化；对外状态与文档一致，**不出现**「请求 hardware 却返回 simulated quote」的静默降级。
6. **回归**：在无硬件的 CI 上仍可 `TEE_MODE=simulation` 全量通过现有 simulation-safe 测试；硬件验收在专用 runner 上执行（与现有 `.drone.yml` 思路一致）。

---

## 2. 当前状态摘要

| 领域 | 现状 | 对 hardware 启动的影响 |
|------|------|-------------------------|
| `TEE_MODE` / 运行时探测 | 已统一；hardware 下设备/RA 前置缺失会 fail-closed | 环境就绪后可通过探测 |
| `HardwareRootKey::for_runtime_mode(Hardware)` | **直接 `Err`，未实现** | **主进程在密钥层次初始化处失败** |
| `SealingService::get_sealing_key` | `simulate_egetkey`，注释标明模拟 | 无真实 SGX sealing |
| `Enclave::initialize` / 测量值 | 用户态模拟生命周期；`generate_measurement` 为假 MRENCLAVE/MRSIGNER | 与真实 enclave 不一致 |
| `DcapService::generate_dcap_quote` | hardware 分支**显式拒绝** | **Attestation 初始化必然失败** |
| PCS / 证书链 | 部分结构存在；hardware 路径多处 `fail-closed` 错误返回 | 需接真实现 |
| 依赖 | Cargo 中**无** Intel SGX SDK / `sgx_urts` 等典型依赖 | 需新增构建与链接形态 |
| `vault-service` 独立 attestation | `TEE_MODE=hardware` 时**拒绝初始化** | 若部署包含该组件需单独实现 |

---

## 3. 启动链路上的硬阻塞顺序

主程序当前大致顺序为：

1. `validate_runtime_requirements`（设备/RA 前置）
2. `Enclave::initialize`（内部仍用模拟 sealing + 模拟测量）
3. **`HardwareRootKey::for_runtime_mode`** → **此处 hardware 必败（当前代码）**
4. 后续存储、审计等
5. `init_attestation_api` → **`DcapService::initialize` → `generate_dcap_quote`** → **hardware 下必败（当前代码）**

因此待实现工作需同时解决：

- **L0 与 Enclave 单一真源**：避免「Enclave 内一套 key、main 里再要一套 L0」长期分叉；推荐由**真实 enclave 导出或统一从 EGETKEY 派生**的路径提供 `HardwareRootKey` 所需材料。
- **DCAP Quote 真路径**：否则 attestation 模块无法初始化完成。

---

## 4. 待实现项清单（按工程模块）

### 4.1 构建与依赖体系

- [ ] 选定 Intel SGX SDK / DCAP 与 Rust 的集成方式（例如：`sgx_urts` + 已签名 `.so` enclave，或 `gramine` / 其他 loader 方案——需与团队架构决策一致）。
- [ ] 在 `Cargo.toml` / 特性（features）中增加 **可选的 hardware 构建**，使默认开发机仍可无 SGX 编译；hardware 构建在 SGX 镜像或本机 SDK 环境中打开。
- [ ] Enclave **签名流程**：签名密钥管理、MRSIGNER、发布与升级策略文档化。
- [ ] CI：保留 simulation 流水线；hardware 流水线在 SGX runner 上执行 **集成测试**（可基于现有 `sgx_hardware_tests` 扩展）。

### 4.2 真实 Enclave 与 Sealing

- [ ] 实现（或引入）**可加载的签名 enclave**，提供至少：sealing key 材料导出到受控接口、参与 report/quote 的数据结构。
- [ ] 将 `SealingService::get_sealing_key` 从 `simulate_egetkey` 替换为 **EGETKEY 或 SDK 等价调用**（在 enclave 内或 urts 约定边界内完成，符合 SGX 安全模型）。
- [ ] 将 `Enclave::generate_measurement` 替换为 **加载后真实 MRENCLAVE/MRSIGNER**（从 SDK/loader 查询）。
- [ ] 明确 **密封存储** 在硬件模式下与磁盘布局、备份、迁移的语义（当前 `.sealed` 路径与逻辑需与安全模型对齐）。

### 4.3 L0 / `HardwareRootKey`（`src/crypto/keys.rs`）

- [ ] 实现 `HardwareRootKey::for_runtime_mode(TeeRuntimeMode::Hardware)`：从 **真实 sealing** 构造（例如 `from_sgx_sealing_key` 已有，需接入真实 32 字节材料），并设置 `RootKeySource` 为硬件枚举值（若当前仅有 `Simulation`，需扩展类型与日志/指标）。
- [ ] 与 `main.rs` 中「先 `Enclave::initialize` 再 `for_runtime_mode`」的顺序对齐：若 L0 仅应由 enclave 内生成，则考虑 **合并初始化顺序** 或 **从 Enclave 初始化结果注入 L0**，避免双重来源。

### 4.4 DCAP Quote 与 PCS（`src/tee/dcap.rs` 及相关）

- [ ] 实现 `generate_dcap_quote` 的 **hardware 分支**：调用 Intel DCAP / QE / PCE（或经 FFI 的官方库），输入真实 REPORT，输出标准 Quote。
- [ ] 实现 `register_with_pcs`（及验证路径）与 **PCCS 或 Intel PCS** 的真实网络交互、错误处理与重试策略。
- [ ] 完成 **证书链与抵押数据（collateral）** 校验逻辑中与硬件模式相关的分支，确保与「simulation 跳过」严格区分。
- [ ] 校核 `validate_dcap_config` 在 hardware 下的约束（如 `verify_certificate_chain`、`use_test_environment`）与生产配置一致。

### 4.5 Attestation 服务与 API（`src/tee/attestation.rs`、`src/api/attestation.rs`）

- [ ] 确认 `AttestationService` 在 `TeeRuntimeMode::Hardware` 下 **所有** 校验路径均要求真实签名/白名单/验证者公钥，无遗留 `allows_simulation` 误用。
- [ ] `ChallengeProtocol` / `ProverProtocol` 与真实 Quote 字段、时间戳、重放防护对齐。
- [ ] 对外 JSON 中 `requested_mode` / `effective_mode` / `root_key_source` 与实现一致；更新 `API.md` 与 `docs/03-API 参考/REST-API.md` 中的硬件示例。

### 4.6 vault-service 独立二进制（可选但建议写明）

若生产部署包含 `vault-service` 独立进程的 attestation 路由：

- [ ] 移除或替换「hardware 直接 `Err`」的占位实现，改为与主库 **共享** Enclave/DCAP 初始化，或明确文档声明「硬件 attestation 仅由主二进制提供」。

### 4.7 运维与配置

- [ ] 在 `docs/05-部署与运维/` 中增加「从 0 到 hardware 启动」的检查表：BIOS、驱动、`aesmd`、PCCS、`TEE_PCS_BASE_URL`、`TEE_DEBUG` 与生产策略等。
- [ ] `docker-compose.prod.yml` 与环境变量与真实主机部署差异说明（容器内 SGX 通常需设备映射与特权，需单独论证）。

---

## 5. 建议实施阶段

| 阶段 | 内容 | 产出 |
|------|------|------|
| **P0** | 构建链 + 最小 enclave 加载 + 真实 MRENCLAVE/MRSIGNER | 可在机器上确认 enclave 已加载且测量值非模拟 |
| **P1** | EGETKEY / 真实 Sealing + `HardwareRootKey::for_runtime_mode(Hardware)` + 与 `main` 初始化顺序收敛 | **主进程在 hardware 下越过 L0 初始化** |
| **P2** | DCAP Quote 生成 + PCS/PCCS 最小闭环 | **`init_attestation_api` 成功** |
| **P3** | 验证链硬化、白名单与运维文档、扩展硬件集成测试 | 达到第 1 节 DoD 全条 |

阶段间可并行文档与 CI，但**代码依赖顺序**建议严格按 P0→P1→P2，否则会出现「Quote 有了但 L0 仍假」等不一致状态。

---

## 6. 风险与依赖

1. **SGX 机型与模式**：FLC、EPID vs DCAP、云厂商 enclave 形态不同，需锁定目标环境并写进验收环境说明。
2. **密钥与合规**：签名私钥、密封数据备份策略需与安全/合规评审同步。
3. **模拟与硬件双轨**：所有新代码路径必须用 `TeeRuntimeMode` 严格分支，避免 CI 用 simulation 时链接 SGX 库失败（推荐 feature gating）。

---

## 7. 参考代码锚点（便于拆任务）

| 文件 | 与硬件相关的关键点 |
|------|-------------------|
| `src/main.rs` | `HardwareRootKey::for_runtime_mode`、`validate_runtime_requirements`、`init_attestation_api` 调用链 |
| `src/crypto/keys.rs` | `for_runtime_mode`、`from_sgx_sealing_key` |
| `src/tee/sealing.rs` | `get_sealing_key` / `simulate_egetkey` |
| `src/tee/enclave.rs` | `initialize`、`generate_measurement` |
| `src/tee/dcap.rs` | `generate_dcap_quote`、`register_with_pcs`、`validate_dcap_config` |
| `src/api/attestation.rs` | `init_attestation_api` |
| `vault-service/src/api/attestation.rs` | `init_attestation_api` hardware 分支 |
| `.drone.yml` | `backend-hardware-attestation` 流水线扩展 |

---

## 8. 文档维护

- 本文档随实现推进勾选清单并更新「当前状态摘要」表。
- 与 [TEE runtime 模式迁移计划](./2026-03-24-tee-runtime-mode-migration.md) 的关系：**迁移计划**完成的是「开关统一 + fail-closed + 文档分层」；**本文档**完成的是「fail-closed 之后的真实能力填充」，二者前后衔接，不重复定义 `TEE_MODE` 语义。

---

**版本**：1.0  
**日期**：2026-03-24
