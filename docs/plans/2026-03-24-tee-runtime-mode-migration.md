# TEE 真实环境 / 模拟环境统一切换 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将 CredBridge 从“开发环境自动进入 simulation”调整为“通过统一开关显式选择 `hardware` 或 `simulation`”，并完成真实 TEE 硬件路径的接入与文档/测试分层。

**Architecture:** 以 `TEE_MODE` 作为唯一运行时来源，定义统一的 `TeeRuntimeMode`/等价配置对象，禁止 `Environment::Development`、Docker 脚本或 API 初始化逻辑隐式改写 TEE 行为。`hardware` 模式下系统必须 fail-closed：硬件探测失败、Quote 生成失败、验证材料缺失时直接报错，不允许自动降级到 simulation。`simulation` 模式仍保留，但只能在显式开启时启用，主要服务于开发机、单元测试与无 SGX runner 的 CI。

**Tech Stack:** Rust, Axum, Intel SGX/DCAP, Docker Compose, `.env` 配置, Markdown 文档, cargo test/clippy/fmt

---

## 设计约束

1. **单一开关**
   所有 TEE 运行模式统一由 `TEE_MODE` 控制，只允许两个明确值：`hardware`、`simulation`。

2. **显式优于隐式**
   `Environment::Development`、`debug_mode`、测试 PCS URL、默认 Docker 配置都不能再隐式决定 TEE 模式。

3. **硬件模式 fail-closed**
   在 `TEE_MODE=hardware` 下，以下任何条件不满足都必须初始化失败，而不是自动改成 simulation：
   - 未探测到受支持的硬件 TEE
   - Enclave 初始化失败
   - DCAP/attestation 依赖不可用
   - Quote 无法生成
   - 验证公钥、证书链、PCK 材料不完整

4. **simulation 仅作显式兼容路径**
   保留 simulation 以支撑本地开发、单元测试、无硬件 CI，但要从生产默认路径中剥离。

5. **测试与运行时分层**
   允许保留 simulation 测试，但必须与 hardware 测试、staging 验证、部署文档明确分层，避免“测试通过 = 真硬件可用”的误判。

---

## 目标状态

### 配置层

- `TEE_MODE=hardware` 或 `TEE_MODE=simulation` 成为唯一模式来源。
- `.env.example`、Docker 初始化脚本、Compose 配置不再默认写死 `simulation`。
- 开发环境是否使用 simulation，由调用方显式配置，不再由 `Environment::Development` 推导。

### 代码层

- 存在统一的 TEE 运行时模式类型，例如：

```rust
pub enum TeeRuntimeMode {
    Hardware,
    Simulation,
}
```

- `src/main.rs`、`src/api/attestation.rs`、`vault-service/src/api/attestation.rs`、`src/tee/mod.rs`、`src/tee/dcap.rs`、`src/tee/attestation.rs` 都通过同一配置对象获取模式。
- `detect_tee()` 进行真实硬件探测，不再固定返回 `Simulation`。
- `hardware` 模式下不允许任何 `allow_simulation(true)` 或 `simulation_mode=true` 的旁路生效。

### 文档层

- README、API、部署文档明确说明双模式开关与使用场景。
- 文档中把“开发环境默认 simulation”的表述改为“显式设置 `TEE_MODE=simulation` 时进入模拟模式”。

### 测试层

- 单元测试可继续使用 simulation。
- 集成测试和 SGX 硬件测试明确使用 `TEE_MODE=hardware`。
- CI 至少拆为：普通 CI（simulation）、专用 runner/staging（hardware）。

---

## 范围界定

### 本计划包含

- 统一 TEE 运行模式配置入口
- 移除“开发环境自动 simulation”的隐式行为
- 修正 attestation / DCAP / TEE 探测链路
- 保留 simulation 开关，但改成显式控制
- 更新测试与文档

### 本计划不包含

- 新增对非 SGX 平台的完整硬件实现（如 TDX/SEV-SNP 全量支持）
- 重新设计全部加密层级
- 完整重写历史归档文档

---

## 任务拆解

### Task 1: 建立统一的 TEE 运行模式配置源

**Files:**

- Modify: `src/config.rs` 或当前环境配置定义文件（若 TEE 配置已分散，新增集中配置模块）
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/main.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/.env.example`
- Modify: `/Users/yvan/AIWorkspace/credbridge/docker/docker-compose.yml`
- Modify: `/Users/yvan/AIWorkspace/credbridge/docker/scripts/init.sh`
- Optional: `/Users/yvan/AIWorkspace/credbridge/.env.bak`

**Step 1: 定义统一模式枚举与解析规则**

建议新增：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeeRuntimeMode {
    Hardware,
    Simulation,
}

impl TeeRuntimeMode {
    pub fn from_env(value: &str) -> Result<Self, ConfigError> {
        match value {
            "hardware" => Ok(Self::Hardware),
            "simulation" => Ok(Self::Simulation),
            other => Err(ConfigError::InvalidTeeMode(other.to_string())),
        }
    }
}
```

**Step 2: 将 `TEE_MODE` 变成唯一真值来源**

要求：

- 程序启动时统一读取 `TEE_MODE`
- 缺失时给出明确默认策略
- 推荐默认值：生产部署模板写 `hardware`，本地开发模板可保留 `simulation` 注释示例，但不要再通过 `Environment::Development` 推导

**Step 3: 删除隐式推导逻辑**

替换当前逻辑：

- `/Users/yvan/AIWorkspace/credbridge/src/main.rs:437` 不再使用 `config.environment == Environment::Development`

**Step 4: 修正初始化模板**

将以下位置从“写死 simulation”改为“显式可配置”：

- `/Users/yvan/AIWorkspace/credbridge/.env.example:51`
- `/Users/yvan/AIWorkspace/credbridge/docker/docker-compose.yml:43`
- `/Users/yvan/AIWorkspace/credbridge/docker/scripts/init.sh:166`

**Acceptance:**

- `TEE_MODE` 是唯一入口
- 无任何运行时路径再根据 `Development` 自动启用 simulation
- 配置非法时进程启动失败并输出清晰错误

---

### Task 2: 重构 TEE 探测逻辑为真实硬件探测

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/mod.rs`
- Optional: 新增 `src/tee/detection.rs`
- Optional: 新增对应测试文件

**Step 1: 移除固定返回 `Simulation` 的实现**

当前问题位置：

- `/Users/yvan/AIWorkspace/credbridge/src/tee/mod.rs:138`

目标：

- `detect_tee()` 基于平台能力、设备文件、驱动或 SGX 依赖结果返回真实值

**Step 2: 明确探测语义**

建议区分：

- “配置请求的模式”
- “底层探测到的能力”

例如：

```rust
pub struct TeeCapabilities {
    pub requested_mode: TeeRuntimeMode,
    pub detected_type: TeeType,
    pub hardware_available: bool,
    pub remote_attestation_available: bool,
}
```

**Step 3: hardware 模式下强制校验**

如果 `requested_mode == Hardware` 但探测结果非硬件可用，则返回初始化错误。

**Acceptance:**

- `detect_tee()` 不再固定返回 `Simulation`
- hardware 模式在无 SGX/无驱动/无设备时直接失败
- simulation 模式仍可在无硬件环境正常启动

---

### Task 3: 统一 Attestation API 初始化链路

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/src/api/attestation.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/main.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/vault-service/src/api/attestation.rs`

**Step 1: 将 `simulation_mode: bool` 改为模式枚举或集中配置对象**

避免多个 bool 横向扩散。建议：

```rust
pub struct TeeRuntimeConfig {
    pub mode: TeeRuntimeMode,
    pub debug_mode: bool,
    pub pcs_base_url: String,
}
```

**Step 2: 调整初始化映射规则**

当前链路问题：

- `/Users/yvan/AIWorkspace/credbridge/src/api/attestation.rs:967`
- `/Users/yvan/AIWorkspace/credbridge/src/api/attestation.rs:981`
- `/Users/yvan/AIWorkspace/credbridge/src/api/attestation.rs:999`

目标：

- `debug_mode` 与 `TEE_MODE` 解耦
- PCS URL 由环境或模式显式配置，而非简单 `simulation => TEST / hardware => PROD`
- `allow_simulation(...)` 只在 `mode == Simulation` 时开启

**Step 3: 修正外部 API 响应语义**

`vault-service` 中：

- `/Users/yvan/AIWorkspace/credbridge/vault-service/src/api/attestation.rs:230`
- `/Users/yvan/AIWorkspace/credbridge/vault-service/src/api/attestation.rs:275`

要求：

- simulation 模式才返回 `QuoteStatus::Simulated`
- hardware 模式必须返回真实硬件状态或显式错误
- 不允许“请求的是 hardware，但悄悄给 simulated quote”

**Acceptance:**

- API 初始化全链路共享同一 TEE 运行模式
- hardware 模式不会返回 simulated 状态
- debug 配置不会隐式触发 simulation

---

### Task 4: 收紧 DCAP 和 Quote 生成/验证逻辑

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/quote.rs`
- Optional: 增补硬件 runner 所需配置说明

**Step 1: 区分 simulation 快路径与 hardware 真路径**

当前 simulation 旁路点：

- `/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs:499`
- `/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs:900`
- `/Users/yvan/AIWorkspace/credbridge/src/tee/dcap.rs:929`

要求：

- simulation：允许模拟 Quote 与跳过验证
- hardware：必须进入真实 Quote 生成、真实签名验证、真实证书链验证

**Step 2: 清晰定义未实现项的行为**

若当前仓库尚未具备完整硬件签名验证能力，则在 `hardware` 模式下应返回明确错误并阻止启动，不允许自动 fallback。

**Step 3: 统一日志语义**

日志必须能区分：

- `requested_mode=hardware, effective_mode=hardware`
- `requested_mode=simulation, effective_mode=simulation`
- 禁止出现 `requested_mode=hardware, effective_mode=simulation` 这种无声降级

**Acceptance:**

- hardware 模式下不存在验证旁路
- 未完成的硬件实现通过显式错误暴露
- simulation 与 hardware 的日志、指标、状态字段可清晰区分

---

### Task 5: 收紧 AttestationService 的 simulation 旁路

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/attestation.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/src/tee/challenge.rs`
- Modify: 相关调用点测试

**Step 1: 将 `allow_simulation` 语义限定到显式 simulation 模式**

当前风险位置：

- `/Users/yvan/AIWorkspace/credbridge/src/tee/attestation.rs:736`
- `/Users/yvan/AIWorkspace/credbridge/src/tee/attestation.rs:866`
- `/Users/yvan/AIWorkspace/credbridge/src/tee/attestation.rs:909`
- `/Users/yvan/AIWorkspace/credbridge/src/tee/attestation.rs:938`

要求：

- `allow_simulation` 不再是散落的业务开关
- 由统一 `TeeRuntimeMode` 决定是否允许模拟校验旁路

**Step 2: 让硬件模式严格要求白名单和公钥材料**

在 `hardware` 模式下：

- 白名单为空不能放行
- 验证者公钥缺失不能放行
- 非零签名未经真实验证不能放行

**Acceptance:**

- `allow_simulation(true)` 只能出现在 simulation 测试或 simulation 初始化分支中
- hardware 模式下所有测量值/签名校验都 fail-closed

---

### Task 6: 处理密钥层级中的 simulation 根密钥

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/src/crypto/keys.rs`
- Optional: `/Users/yvan/AIWorkspace/credbridge/src/crypto/mod.rs`
- Optional: 相关单元测试

**Step 1: 明确保留策略**

当前实现：

- `/Users/yvan/AIWorkspace/credbridge/src/crypto/keys.rs:35`
- `/Users/yvan/AIWorkspace/credbridge/src/crypto/keys.rs:62`

建议：

- 保留 `Simulation` 根密钥来源，但限制为显式 simulation 模式或测试专用
- 在 hardware 模式下，任何试图创建 `for_simulation()` 的路径都应报错

**Step 2: 增加审计信息**

建议所有关键日志/指标暴露 `root_key_source`，便于识别是否仍在使用 simulation L0。

**Acceptance:**

- simulation L0 不会在 hardware 模式下被使用
- 运行日志/状态接口可识别当前密钥根来源

---

### Task 7: 测试分层与命名整理

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/tests/api/attestation_tests.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/tests/tee_attestation_tests.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/tests/tee/dcap_tests.rs`
- Modify: `/Users/yvan/AIWorkspace/credbridge/tests/sgx_hardware_tests.rs`
- Modify: 各模块内联测试（如 `src/tee/dcap.rs`, `src/tee/attestation.rs`, `src/tee/challenge.rs`, `src/tee/quote.rs`）

**Step 1: 给 simulation 测试显式标注模式**

例如：

- 测试名包含 `simulation`
- 测试初始化明确写 `TEE_MODE=simulation` 或等价配置

**Step 2: 给 hardware 测试显式标注前置条件**

例如：

- 需要 SGX 设备
- 需要 AESM/DCAP 服务
- 需要专用 runner

**Step 3: CI 分层**

建议拆为：

- `cargo test` 默认跑 simulation-safe 单元/集成测试
- SGX runner / staging 跑 hardware 测试套件

**Acceptance:**

- 测试名称与初始化方式能直接看出运行模式
- 普通 CI 不会误把 simulation 测试当成硬件验收

---

### Task 8: 更新运维、API、开发文档

**Files:**

- Modify: `/Users/yvan/AIWorkspace/credbridge/README.md`
- Modify: `/Users/yvan/AIWorkspace/credbridge/API.md`
- Modify: `/Users/yvan/AIWorkspace/credbridge/docs/03-API 参考/REST-API.md`
- Modify: `/Users/yvan/AIWorkspace/credbridge/docs/project-docs/tee-sandbox.md`
- Modify: `/Users/yvan/AIWorkspace/credbridge/docs/05-部署与运维/DCAP-SETUP.md`
- Optional: `/Users/yvan/AIWorkspace/credbridge/AGENTS.md`
- Optional: `/Users/yvan/AIWorkspace/credbridge/CLAUDE.md`

**Step 1: 统一表述**

把以下旧表述替换掉：

- “开发环境默认 simulation”
- “Development 环境自动走模拟模式”

改为：

- “通过 `TEE_MODE` 显式选择 `hardware` 或 `simulation`”
- “推荐生产/staging 使用 `hardware`，本地开发视需要显式使用 `simulation`”

**Step 2: 增加切换示例**

```bash
# 真实硬件环境
TEE_MODE=hardware
TEE_DEBUG=false

# 模拟环境
TEE_MODE=simulation
TEE_DEBUG=true
```

**Step 3: 更新 API 状态字段说明**

如果状态接口继续输出 `simulation_mode` 或 `Simulated`，需要同步文档并说明仅在显式 simulation 下出现。

**Acceptance:**

- 所有对外文档都反映“统一开关 + 双模式”设计
- 不再存在“默认 simulation”误导性描述

---

## 建议实施顺序

1. 先做配置统一与 `TEE_MODE` 单一真值源
2. 再改 `detect_tee()` 和 attestation 初始化主链
3. 然后收紧 DCAP/attestation 的 simulation 旁路
4. 再处理密钥源与状态接口
5. 最后做测试分层和文档收口

---

## 风险与注意事项

1. **不要把 debug_mode 等同于 simulation**
   debug enclave 与 simulation 是两个不同维度，不能继续复用一个 bool。

2. **不要 silent fallback**
   用户显式要求 `hardware` 时，任何 fallback 到 simulation 的行为都会制造严重的安全错觉。

3. **不要一次性删除 simulation 测试**
   simulation 仍然有价值，用于本地快速回归和无 SGX CI；要做的是显式隔离，不是粗暴删除。

4. **PCS/DCAP URL 需要独立配置项**
   不建议只靠 `mode` 推导测试或生产 PCS URL，最好允许环境变量单独配置。

5. **状态接口要输出“请求模式”和“实际状态”**
   便于排查“为什么没有进入硬件模式”。

---

## 完成定义（Definition of Done）

- 启动时统一从 `TEE_MODE` 决定运行模式
- `Environment::Development` 不再自动触发 simulation
- `detect_tee()` 不再固定返回 `Simulation`
- hardware 模式下，Quote/证书链/签名校验全部 fail-closed
- simulation 模式只在显式开启时生效
- Docker、`.env.example`、README、API、部署文档全部更新
- 测试分层清晰，simulation 与 hardware 不再混淆

---

## 交付建议

第一轮实现建议只完成“模式统一 + fail-closed + 文档更新”，不要在同一批里同时扩展多平台 TEE 支持。先把 SGX 硬件路径跑通，再考虑 TDX / SEV-SNP 的统一抽象，否则范围会失控。
