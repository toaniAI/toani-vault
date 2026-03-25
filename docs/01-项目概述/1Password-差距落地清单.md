# CredBridge 距离 1Password 体验的收敛提案（Sprint Change Proposal）

> 版本: v2.0  
> 日期: 2026-03-23  
> 适用范围: CredBridge 当前迭代（安全闭环与产品体验并行收敛）  
> 提案目的: 将“差距清单”收敛为可审批、可排期、可交付的变更提案。

---

## 1. 问题摘要（Issue Summary）

### 1.1 触发问题

当前 CredBridge 已具备较完整安全底座与控制台能力，但仍存在“实现级闭环缺失”和“产品级体验缺失”两类关键断点，导致：

- 无法对外稳定证明“真实 TEE + 真实审计不可篡改 + 真实 token 验证 + MCP 代理执行”已闭环。
- 无法达到 1Password 级别的高频使用体验（共享、自动填充、迁移恢复、身份健康治理）。

### 1.2 发现背景

在对“归档设计规范目标”与“1Password 体验差距”进行并行评估后，确认现状仍混有 simulation/placeholder/简化路径，且产品能力未形成完整用户闭环。

### 1.3 证据摘要

- 审计链路存在 `software_mode`、占位 `mrenclave`/`jti` 使用痕迹。
- MCP 工具仍以凭证 CRUD/解密为中心，尚未收敛到 `credbridge_execute` 主模型。
- immudb、attestation、token 验证、SSE 校验存在模拟或简化逻辑。
- 前端未覆盖 vault/collection/shared vault、TOTP/passkey、自动填充、迁移恢复等核心体验。

---

## 2. 影响分析（Impact Analysis）

### 2.1 Epic 影响

- 安全可信主线 Epic：需要从“模块可用”升级为“端到端可验证”。
- 代理执行主线 Epic：需要将 MCP 从“取密文/明文”改为“请求服务动作并跟踪执行状态”。
- 产品体验主线 Epic：需要新增“共享协作 + 身份健康 + 自动填充 + 迁移恢复”能力带。

### 2.2 Story 影响

- 当前及后续涉及 attestation、audit、token、MCP、connector、sandbox 的故事需补“真实化验收项”。
- 控制台相关故事需补“执行追踪、证据展示、版本追溯、高级检索”验收项。
- 新增体验类故事（vault 模型、共享、passkey/TOTP、导入恢复）将进入后续迭代池。

### 2.3 Artifact 冲突

- PRD/路线图当前偏“清单式分散定义”，需收敛为“里程碑驱动 + 验收门槛”。
- 架构文档需显式标注 simulation 与 hardware 双路径及切换边界。
- UX 文档需补审批流、执行状态、审计证据、版本详情、共享协作等流程图与页面规范。

### 2.4 技术影响

- 涉及后端安全主链路、MCP server 协议层、connector/sandbox 执行层、前端信息架构与交互层同步改造。
- 需要同步补齐部署与运维文档（DCAP、IMMUDB）及可验证证据导出流程。

---

## 3. 推荐路径（Recommended Approach）

### 3.1 选择路径

采用 **Direct Adjustment + MVP Review** 组合策略：

- Direct Adjustment：直接调整现有迭代内容，优先完成真实安全闭环（P0）与代理执行闭环（P1核心）。
- MVP Review：对“1Password 体验能力”做阶段化裁剪，先做共享与身份健康基础能力，再推进自动填充与迁移恢复。

### 3.2 推荐理由

- 当前最大风险不是“缺功能”，而是“可信闭环尚未成立”。
- 在可信闭环未完成前继续扩展体验功能，会放大合规与品牌风险。
- 先做闭环再做体验，可确保每个新增体验能力都可审计、可验证、可回溯。

### 3.3 估算与风险

- 预计周期：A/B/C 三阶段约 14-20 周（不含后续持续产品化优化）。
- 主要风险：硬件环境与外部依赖（SGX/DCAP、immudb）、跨模块联调复杂度、前后端一致性演进成本。
- 风险缓释：设置阶段性强验收门槛（M1/M2/M3），每阶段只接受“可演示且可验证”交付。

---

## 4. 详细变更提案（Detailed Change Proposals）

### 4.1 PRD/路线图变更（旧 -> 新）

**OLD**

- 以 P0/P1/P2 清单为主，任务项多、决策门槛弱、跨团队交付边界不清晰。

**NEW**

- 采用里程碑驱动提案：
1. M1 真实安全可验。
2. M2 设计规范主目标闭环。
3. M3 1Password 体验关键能力落地。

**Rationale**

- 用里程碑替代散点任务，降低排期噪声，确保每阶段都有业务可验证价值。

### 4.2 架构与安全主链路变更（按优先级）

1. **P0-1 TEE/Attestation 真实化**  
将 attestation 从演示路径切到真实 quote/report/验证链，明确 simulation/hardware 行为边界。

2. **P0-2 审计身份真实化**  
移除 `software_mode`、占位 `mrenclave/jti`，全链路注入真实执行上下文。

3. **P0-3 immudb 真实不可篡改**  
完成真实事务写入、证明导出与 `/api/v1/audit/verify` 实证校验。

4. **P0-5 token 真实验证**  
MCP token 加密存储、SSE 统一 validator、生命周期审计关联。

### 4.3 MCP 与执行模型变更

**OLD**

- 工具集合以 `list/get/create/update/delete/decrypt_credential` 为核心。

**NEW**

- 收敛为 Action Proxy：
  - `credbridge_execute`
  - `credbridge_poll_status`
  - `credbridge_request_scope`
  - `credbridge_revoke_service`
  - `credbridge_tee_status`

**Rationale**

- 从“拿明文”转向“请求动作 + 审批 + 状态机 + 审计证据”，符合设计规范与企业可控要求。

### 4.4 前端与体验闭环变更

1. P1：补执行状态、审批中心、版本追溯、审计证据展示、高级检索。  
2. P2：落地 vault/collection/shared vault、共享协作、TOTP/passkey、迁移恢复。  
3. 后续：自动填充与浏览器扩展、跨端一致性与性能优化。

### 4.5 阶段排期与里程碑

1. **阶段 A（4 周） -> M1**  
attestation、audit、token、immudb 真实闭环。

2. **阶段 B（4-6 周） -> M2**  
Action Proxy + 审批/执行/审计产品闭环，前端补执行与证据视图。

3. **阶段 C（6-10 周） -> M3**  
vault/共享模型 + 身份健康能力（TOTP/passkey/健康检查）。

4. **阶段 D（持续）**  
自动填充、导入迁移、恢复紧急访问、跨端优化。

---

## 5. 实施交接（Implementation Handoff）

### 5.1 变更范围分级

本提案评估为 **Major**（重大变更）：

- 涉及 PRD、架构、后端安全链路、MCP 协议、前端 IA/交互、运维文档多工件联动。

### 5.2 交接对象与职责

1. **PM/PO**
- 锁定 M1/M2/M3 验收门槛与范围边界。
- 将清单任务重排为里程碑 backlog，并设置 DoD。

2. **Architect/Security**
- 审核 TEE、attestation、audit、token、immudb 的真实化方案与验证方法。
- 评估 simulation/hardware 切换策略与失败兜底。

3. **Backend**
- 完成 P0/P1 核心链路改造，确保 API 与审计证据可追溯。

4. **Frontend**
- 实现执行状态、审批中心、证据展示、版本追溯与后续体验模块入口。

5. **QA**
- 建立 M1/M2/M3 验收测试矩阵，覆盖功能、负面路径、证据一致性检查。

### 5.3 成功标准

- M1：不再依赖占位安全上下文，核心安全能力可被外部验证。  
- M2：Action Proxy 与审批执行闭环落地，前端可观测且可追溯。  
- M3：共享组织与身份健康能力可用，达到“接近 1Password 体验”的最低门槛。

---

## 6. 执行决策建议

建议本提案作为当前迭代的正式变更基线执行，先以 M1 为硬门槛推进；M1 未达标前，不再新增“仅体验增强但无可信闭环”的需求项。
