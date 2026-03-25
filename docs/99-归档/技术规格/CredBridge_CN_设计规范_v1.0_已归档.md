> ⚠️ **过时文档警告**
> 
> 此文档已归档，不再反映当前实现。
> - **原始版本**: v1.0 草案 (2026 年 3 月)
> - **归档原因**: 技术栈已从 TypeScript 迁移至 Rust，架构已更新
> - **当前实现**: 请参考 `docs/01-项目概述/架构设计.md` 和源代码
> - **归档日期**: 2026-03-20

---

__CredBridge__

AI 原生身份与凭证桥接系统

*商用技术设计规范  ·  OpenClaw 深度集成版*

基于 TEE 可信执行环境 \+ 现代密码学 \+ 零知识架构

v1\.0 草案  ·  2026 年 3 月  ·  机密  ·  **已归档**

__属性__

__内容__

文档状态

**已归档** — 历史参考文档

目标读者

工程、产品、安全、法务

安全级别

机密 — 仅限内部流通

产品定位

商用级（对标 1Password 安全标准）

集成目标

OpenClaw Gateway Skill \+ mcporter MCP Server

核心技术

TEE \(Intel SGX / AMD SEV / ARM TrustZone\) \+ 现代密码学 \+ 零知识

实现语言

**TypeScript \(Node\.js 22\+\)** ⚠️ *已迁移至 Rust*

合规目标

SOC 2 Type II · ISO 27001 · GDPR/CCPA · FIPS 140\-3 Level 3

# 目录

1\. 执行摘要与产品定位

2\. 问题陈述与现状分析

3\. 安全威胁模型（Threat Model）

4\. 密码学设计

5\. 可信执行环境（TEE）架构

6\. 系统总体架构

7\. 数据模型与存储设计

8\. OpenClaw 集成方案

9\. MCP 工具规范

10\. 连接器系统（Connector Runtime）

11\. 人工审批（Human\-in\-the\-Loop）

12\. 审计与合规

13\. 安全攻击面分析

14\. 部署架构

15\. 实施路线图

16\. 附录

# 1\. 执行摘要与产品定位

## 1\.1  核心问题

OpenClaw 等 AI Agent 框架当前存在一个根本性的安全缺口：当 Agent 需要代替用户登录银行、政府门户、券商平台、SaaS 系统等任何需要身份验证的服务时，用户的真实凭证——密码、SSN、API Key、证件文件——没有安全存储和使用的机制。

更严重的是，OpenClaw 当前将所有凭证以明文形式存储在 ~/\.openclaw/ 目录下。安全研究人员已将此标记为高危问题（CVE\-2026\-25253，CVSS 8\.8），信息窃取类恶意软件已将该路径列为标准攻击目标。

__核心设计不变量  __AI Agent 进程中，任何时候都不持有用户真实凭证的明文。凭证的解密、使用和销毁全部发生在 TEE 安全边界内部，且仅在经过密码学验证的授权请求触发后执行。

## 1\.2  产品定位

CredBridge 是一个商用级、可自托管的 AI 身份与凭证桥接系统，达到与 1Password 同等的安全标准。其三大核心能力：

1. 安全凭证保险库（Secure Vault）：基于 TEE \+ HSM 的加密存储，保护用户所有身份数据
2. AI 行动代理（Action Proxy）：Agent 通过有限 Scope Token 发起请求，CredBridge 在 TEE 内解密凭证并执行真实操作，Agent 只获得操作结果
3. OpenClaw 原生集成：以 Skill \+ MCP Server 形式接入 OpenClaw，无需修改 OpenClaw 核心代码

## 1\.3  对标定位

__能力维度__

__OpenClaw 原生__

__Plaid__

__1Password CLI__

__CredBridge（目标）__

凭证加密存储

❌ 明文

❌ N/A

✅

✅ TEE\+HSM

AI Agent 调用接口

部分

❌

❌

✅ MCP Tools

写操作 / 代理执行

❌

❌

❌

✅ Action Proxy

TEE 硬件隔离

❌

❌

❌

✅ SGX/SEV/TZ

零知识架构

❌

❌

部分

✅

人工审批流程

❌

N/A

❌

✅ HITL

不可篡改审计日志

❌

❌

❌

✅ immudb

浏览器自动化

✅（无隔离）

❌

❌

✅（TEE隔离）

政府/传统服务

❌

❌

❌

✅

SOC 2 / FIPS 合规

❌

✅

✅

✅（目标）

# 2\. 问题陈述与现状分析

## 2\.1  OpenClaw 架构中的安全缺口

通过对 OpenClaw 源码的深度分析，凭证安全缺口存在于以下三个层面：

### 层面 1：存储层（高危）

- OpenClaw Gateway 将所有第三方服务凭证以明文 JSON 形式写入 ~/\.openclaw/config\.json
- 恶意 ClawHub Skill（已发现超过 20% 含恶意代码）可直接读取该文件
- CVE\-2026\-25253：WebSocket token 泄露可导致完整 Gateway 接管，进而读取所有明文凭证

### 层面 2：传输层（中危）

- 当 Agent 执行浏览器自动化时，凭证以环境变量形式注入 Playwright 进程
- 进程内存中的明文密码可被同主机其他进程通过 /proc/\[pid\]/mem 读取
- Browser CDP 调试端口（默认 9222）若暴露，可导致会话劫持

### 层面 3：执行层（中危）

- Agent LLM 上下文中如包含凭证片段，会被记录到 MEMORY\.md，造成持久化泄露
- OpenClaw 的 skills 沙箱不阻止子进程读取环境变量中的秘密
- 多 Agent 路由（Multi\-Agent Routing）中子 Agent 可继承父 Agent 的完整工具权限

## 2\.2  适用场景全景

__场景分类__

__典型服务__

__典型 Agent 操作__

__身份/凭证要求__

金融服务

SoFi、Schwab、Robinhood、Coinbase、Chase

开户、下单、查余额、申请贷款

账号密码 \+ KYC \+ 信用评分

政府服务

IRS、DMV、USCIS、SSA、护照机构

报税、换证、查询案件、提交表格

SSN \+ 政府ID \+ 人脸识别

医疗健康

MyChart、Zocdoc、保险门户

预约、获取报告、提交预授权

患者ID \+ 保险卡号

电商平台

Amazon、eBay、Shopify

跟踪订单、发起退货、管理订阅

账号密码 \+ 支付方式

SaaS/企业

GitHub、Stripe、Twilio、AWS

管理账单、轮换密钥、导出数据

API Key \+ OAuth

出行旅游

航空公司、酒店、Airbnb

值机、改签、取回行程

会员账号 \+ 护照号

HR/薪酬

Workday、ADP、Gusto

提交考勤、查询薪资、福利注册

员工ID \+ SSO

教育

Canvas、Coursera、大学门户

提交作业、查看成绩、注册课程

学号 \+ 密码

# 3\. 安全威胁模型（Threat Model）

## 3\.1  攻击者画像

__攻击者类型__

__能力假设__

__攻击目标__

网络攻击者

控制网络路径，可执行 MitM 攻击

拦截凭证传输，重放 Action Token

恶意 Skill 作者

可发布恶意 ClawHub Skill，可执行任意代码

读取明文凭证、注入恶意指令

Prompt Injection 攻击者

可控制 Agent 处理的外部内容（网页、文件、邮件）

诱导 Agent 执行未授权操作

内部威胁（员工）

有权访问生产数据库和服务器文件系统

直接读取用户凭证数据库

物理攻击者

可获取服务器物理访问权，可 cold\-boot 内存

提取内存中的明文密钥材料

云服务商

作为 IaaS 提供商，有 Hypervisor 级访问权限

读取 VM 内存，访问存储卷

供应链攻击者

可污染依赖包，可向 ClawHub 注入恶意 Skill

持久化后门，凭证外泄

## 3\.2  安全目标（Security Properties）

- 机密性（Confidentiality）：用户凭证明文只能在 TEE enclave 内部短暂存在，任何 enclave 外部实体——包括 OS、Hypervisor、Cloud Provider——均无法读取
- 完整性（Integrity）：凭证存储和 Action Token 不能被篡改；TEE 远程认证保证 enclave 代码未被修改
- 授权绑定（Authorization Binding）：每个操作必须绑定特定用户 × 特定服务 × 特定 Scope，不可越权
- 不可否认性（Non\-repudiation）：所有操作有不可篡改的密码学证明记录
- 前向保密（Forward Secrecy）：历史操作的安全性不因当前密钥泄露而受影响
- 最小权限（Least Privilege）：Agent 进程运行时权限严格受限，即使 Agent 被完全控制，攻击者也无法超出 Scope

## 3\.3  不在保护范围内（Out of Scope）

- 用户终端设备的全盘加密和物理安全（TEE 无法保护用户自己的设备）
- 目标服务本身的安全性（如银行系统被入侵）
- 用户在授权时的决策错误（人为授权过多 Scope）
- 量子计算攻击（当前威胁模型不包含量子对手，待 PQC 标准化后更新）

# 4\. 密码学设计

__设计原则  __所有密码学原语使用经过 NIST/FIPS 认证的算法，不自造密码学。密钥材料永不落地（never at rest in plaintext），所有加密操作在 TEE enclave 内部或 HSM 内执行。

## 4\.1  密钥层次结构（Key Hierarchy）

CredBridge 采用四层密钥层次，每层密钥职责严格分离：

┌─────────────────────────────────────────────────────────────┐

│                    密钥层次结构                              │

│                                                             │

│  L0: Root of Trust（信任根）                                │

│      ├── 硬件绑定：Intel SGX MRENCLAVE / AMD SEV 固件密钥   │

│      └── 可选 HSM：CloudHSM / Azure Managed HSM            │

│                           │                                 │

│  L1: Master Key（主密钥）  ↓                                │

│      ├── 由 L0 派生，仅在 TEE enclave 内部存在              │

│      ├── 256\-bit，使用 HKDF\-SHA\-256 从硬件测量值派生        │

│      └── 用于加密/解密 L2 密钥包                            │

│                           │                                 │

│  L2: User Vault Key（用户保险库密钥）                        │

│      ├── 每用户独立，256\-bit AES\-GCM 密钥                   │

│      ├── 以 L1 加密后存储在数据库中                          │

│      └── 解密操作仅在 TEE 内发生                             │

│                           │                                 │

│  L3: Credential Encryption Key（凭证加密密钥）               │

│      ├── 每条凭证记录独立，256\-bit                           │

│      ├── 以 L2 加密后与密文一起存储（AEAD 封装）             │

│      └── 使用 AES\-256\-GCM，96\-bit nonce，128\-bit auth tag   │

└─────────────────────────────────────────────────────────────┘

## 4\.2  凭证加密方案

### 主加密算法：AES\-256\-GCM

// 凭证加密（在 TEE enclave 内执行）

interface EncryptedBlob \{

  version:    2;

  algorithm:  'AES\-256\-GCM';

  kdf:        'HKDF\-SHA\-256';

  // 96\-bit 随机 nonce（每次加密独立生成）

  nonce:      Uint8Array;   // 12 bytes

  // 128\-bit GCM Authentication Tag

  auth\_tag:   Uint8Array;   // 16 bytes

  // 关联数据（AAD）: user\_id \+ service\_id \+ record\_id（不加密但防篡改）

  aad:        Uint8Array;

  // 密文

  ciphertext: Uint8Array;

  // L3 密钥以 L2 加密后的包

  key\_envelope: Uint8Array;

\}

// 密钥派生（HKDF）

// IKM = L2 User Vault Key（来自 TEE）

// Salt = 随机 32 bytes（每条记录独立）

// Info = 'credbridge\-v2' || user\_id || service\_id

const l3\_key = HKDF\(SHA\-256, ikm=l2\_key, salt, info\);

### Action Token：PASETO v4\.local

CredBridge 使用 PASETO（Platform\-Agnostic Security Token）v4\.local 替代 JWT，彻底避免 JWT 的 alg:none 和 RS256 算法混淆攻击。

// PASETO v4\.local 使用 XChaCha20\-Poly1305 对称加密

// 密钥来自 TEE 内部的 Token Signing Key（L2 派生）

interface ActionTokenPayload \{

  jti:        string;    // UUID v4，单次使用强制（存 Redis，TTL=15min）

  iss:        'credbridge\-tee';

  sub:        string;    // user\_id

  iat:        string;    // 签发时间 ISO8601

  exp:        string;    // 到期时间（固定 15 分钟，不可延长）

  service:    string;    // 目标服务 ID，如 'schwab' 'irs' 'amazon'

  action:     string;    // 操作 ID，如 'get\_balance' 'open\_account'

  scope\_hash: string;    // SHA\-256\(granted\_scopes\)，防止 scope 降级攻击

  vault\_ref:  string;    // 指向 Vault 中凭证记录的不透明引用 ID

  risk\_tier:  0 | 1 | 2; // 0=自动批准 1=软审批 2=硬审批

  audit\_id:   string;    // 预分配的审计日志条目 ID

  tee\_mrenclave: string; // TEE enclave 测量值，供 Connector 验证

\}

### 凭证分片存储：Shamir 秘密共享

KYC 身份文件（护照、SSN、驾照）采用 Shamir's Secret Sharing \(SSS\) 进行门限分片，即使数据库被完整导出，没有足够分片数量也无法重建原始数据。

// Shamir 秘密共享参数

// n = 总分片数 = 7

// k = 重建阈值 = 4（任意 4 片可重建）

// 分片分布：

//   Shard 1\-3: CredBridge Vault \(3 个独立加密数据库节点\)

//   Shard 4\-5: 用户持有（加密导出到用户设备）

//   Shard 6\-7: 托管 HSM（仅在监管机构要求时可用）

// 重建仅在 TEE enclave 内部进行

// 重建后的明文生命周期：操作完成后立即 memset\(0\) 清零

### 传输层加密

// 所有服务间通信使用 mTLS 1\.3

// 证书策略：

//   \- Gateway ↔ TEE Enclave: 基于 ECDSA P\-256 的短期证书（24h TTL）

//   \- TEE ↔ Connector Runtime: 基于 Enclave Quote 的证书绑定

//   \- 客户端 → Gateway: TLS 1\.3，HSTS，Certificate Transparency

// 密码套件白名单（仅允许以下）：

TLS\_AES\_256\_GCM\_SHA384

TLS\_CHACHA20\_POLY1305\_SHA256

// 禁用：TLS 1\.0/1\.1/1\.2（允许降级攻击的版本）

// 禁用：RSA 密钥交换（无前向保密）

# 5\. 可信执行环境（TEE）架构

## 5\.1  TEE 选型分析

__TEE 方案__

__隔离粒度__

__远程认证__

__云支持__

__CredBridge 适用性__

__推荐用途__

Intel SGX

进程级 Enclave
（EPC 内存加密）

✅ DCAP/ECDSA

Azure DCsv3
AWS Nitro
Alibaba g7t

★★★★★

主力方案
密钥操作/凭证解密

AMD SEV\-SNP

VM 级隔离
（内存加密\+完整性）

✅ SNP Attestation

Azure CVM
AWS EC2 C7a
GCP C3

★★★★☆

Connector Runtime 隔离

ARM TrustZone

系统级 Secure World

✅（有限）

移动端原生
Raspberry Pi

★★★☆☆

移动端 HITL 审批

AWS Nitro Enclaves

EC2 隔离分区
（内存\+CPU）

✅ PCR 测量

AWS 专有

★★★★☆

AWS 部署替代方案

软件 TEE
（作为 Fallback）

进程隔离
（无硬件保证）

❌

通用

★★☆☆☆

开发环境/
无 TEE 硬件降级

## 5\.2  TEE Enclave 设计

CredBridge 将以下操作放入 SGX Enclave（Trusted Computing Base，TCB）：

┌─────────────────────────────────────────────────────────────┐

│                  SGX Enclave（TCB 边界）                     │

│                                                             │

│  ┌─────────────────────┐  ┌──────────────────────────┐    │

│  │   密钥管理模块       │  │    凭证加解密模块          │    │

│  │  \- 主密钥派生\(HKDF\)  │  │  \- AES\-256\-GCM 加密       │    │

│  │  \- 密钥树管理        │  │  \- Shamir 分片重建         │    │

│  │  \- Sealing/Unsealing │  │  \- 明文生命周期管理        │    │

│  └─────────────────────┘  └──────────────────────────┘    │

│                                                             │

│  ┌─────────────────────┐  ┌──────────────────────────┐    │

│  │   Token 签发模块     │  │    远程认证模块            │    │

│  │  \- PASETO v4 签发    │  │  \- Quote 生成\(DCAP\)       │    │

│  │  \- Token 验证        │  │  \- 认证报告生成            │    │

│  │  \- 单次使用强制      │  │  \- Enclave 度量值对比      │    │

│  └─────────────────────┘  └──────────────────────────┘    │

│                                                             │

│  ─────── EPC 加密内存边界（Intel Memory Encryption Engine） ─│

└─────────────────────────────────────────────────────────────┘

            ↑ 唯一出口：通过 ECALL/OCALL 接口的结构化 API

Untrusted Region（不可信区域，OS/Gateway 运行在此）:

  \- 加密的 Vault 数据库（密文）

  \- Action Token（PASETO 密文，TEE 内签发）

  \- 审计日志（明文操作记录，无凭证数据）

## 5\.3  远程认证（Remote Attestation）流程

远程认证确保用户和审计方可以密码学验证：运行中的 CredBridge 代码未被篡改，且确实在真实的 TEE 硬件中运行。

__步骤__

__操作__

__技术实现__

1\. 用户请求认证

用户客户端向 Gateway 发起认证挑战

HTTP GET /v1/tee/attest，返回 nonce

2\. Enclave 生成 Quote

SGX enclave 将 nonce \+ MRENCLAVE 测量值签名

Intel DCAP: QE 生成 ECDSA\-P256 签名 Quote

3\. Quote 上传

Gateway 将 Quote 发送给用户客户端

Quote 包含: MRENCLAVE, MRSIGNER, 产品ID, 安全版本

4\. 本地或云端验证

用户侧或第三方验证 Quote 有效性

Intel Trust Authority / Azure Attestation / 本地 PCCS

5\. 策略评估

验证 MRENCLAVE 是否匹配已发布的代码哈希

对比 CredBridge 发布的官方 MRENCLAVE 注册表

6\. 建立可信信道

验证通过后，用户与 enclave 建立 TLS over RA\-TLS

RA\-TLS：证书由 enclave 签发，绑定 enclave 测量值

## 5\.4  Sealing（密封存储）

TEE Sealing 允许 enclave 将数据加密持久化到磁盘，且解密只能发生在具有相同测量值的 enclave 实例中。CredBridge 使用两种 Sealing 策略：

- MRENCLAVE Sealing（严格模式）：仅当前版本 enclave 可解封。用于短期操作状态，升级时自动失效。
- MRSIGNER Sealing（兼容模式）：同一签名者的不同版本 enclave 可解封。用于跨版本持久化的 L1 主密钥包，支持安全升级。

// Sealing 密钥派生（SGX SDK）

// SEAL\_KEY = PRF\(SGX\_SEAL\_KEY, MRENCLAVE/MRSIGNER, ISVSVN, CPUSVN\)

// 外部存储格式（磁盘）：

interface SealedKeyPackage \{

  seal\_policy: 'MRENCLAVE' | 'MRSIGNER';

  cpusvn:      Buffer;    // CPU 安全版本号

  isvsvn:      number;    // ISV 安全版本号

  keyrequest:  Buffer;    // SGX 密钥请求结构

  ciphertext:  Buffer;    // 密封后的 L1 主密钥

  mac:         Buffer;    // GCM 认证标签

\}

# 6\. 系统总体架构

## 6\.1  组件总览

┌────────────────────────────────────────────────────────────────────┐

│  用户交互层                                                         │

│  WhatsApp / Telegram / Slack / iMessage / WebChat \(via OpenClaw\)   │

└──────────────────────────┬─────────────────────────────────────────┘

                           │ 消息

┌──────────────────────────▼─────────────────────────────────────────┐

│  OpenClaw Gateway \(:18789\)                                          │

│  Channel Adapters → Session Manager → Pi Agent Runtime             │

│                              │                                      │

│                    ┌─────────▼──────────┐                          │

│                    │  System Prompt 组装 │                          │

│                    │  SOUL\.md \+ Skills   │ ← credbridge/SKILL\.md   │

│                    └─────────┬──────────┘                          │

└──────────────────────────────┼─────────────────────────────────────┘

                               │ MCP Tool Call via mcporter

┌──────────────────────────────▼─────────────────────────────────────┐

│  CredBridge Gateway Layer                                           │

│  ┌─────────────────┐  ┌────────────────┐  ┌──────────────────┐    │

│  │ OAuth2/PKCE     │  │ Policy Engine  │  │  MCP Server      │    │

│  │ Consent Engine  │  │ Rate Limit     │  │  \(SSE Transport\) │    │

│  └────────┬────────┘  └───────┬────────┘  └────────┬─────────┘    │

└───────────┼───────────────────┼───────────────────\-┼───────────────┘

            │ 授权请求           │ 策略检查            │ 工具调用

┌───────────▼───────────────────▼────────────────────▼───────────────┐

│  TEE Enclave 层（Intel SGX / AMD SEV）                              │

│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │

│  │ 密钥管理     │  │ Token 签发   │  │ 凭证解密（按需，即用即销）│  │

│  │ \(HSM可选\)    │  │ PASETO v4    │  │ AES\-256\-GCM / Shamir     │  │

│  └──────────────┘  └──────────────┘  └──────────────────────────┘  │

└──────────────────────────────┬──────────────────────────────────────┘

                               │ 临时凭证材料（仅在内存中，操作后清零）

┌──────────────────────────────▼──────────────────────────────────────┐

│  Connector Runtime（隔离容器，AMD SEV\-SNP 可选）                    │

│  Direct API / OAuth Exchange / Playwright Browser / Form Submit     │

└──────────────────────────────┬──────────────────────────────────────┘

                               │ HTTPS / API 调用

┌──────────────────────────────▼──────────────────────────────────────┐

│  目标服务（银行/政府/电商/SaaS/\.\.\.）                                 │

└─────────────────────────────────────────────────────────────────────┘

                               ↕ 异步

┌─────────────────────────────────────────────────────────────────────┐

│  安全存储层                                                          │

│  HashiCorp Vault（凭证密文）\+ immudb（不可篡改审计日志）             │

│  \+ PostgreSQL with RLS（元数据）\+ Redis（Token 单次使用状态）        │

└─────────────────────────────────────────────────────────────────────┘

## 6\.2  端到端操作生命周期（8步流程）

__步骤__

__执行位置__

__操作描述__

__安全机制__

1\. Agent 发起调用

OpenClaw Pi Runtime

Agent 调用 MCP Tool: credbridge\_execute\(\{service, action, params\}\)

MCP Bearer Token 验证 Agent 会话

2\. 权限预检

CredBridge Gateway

查询用户已授权的 Scope 列表，对比请求的 action 是否在范围内

Scope 白名单，数据库行级安全

3\. 风险分级

Policy Engine

根据操作类型、金额、时间、历史模式评估风险等级（0/1/2）

机器学习风险评分 \+ 规则引擎

4\. HITL 审批（如需）

推送通知 → 用户设备

Tier 1/2 操作推送手机审批，用户确认后继续；超时自动拒绝

TOTP 二次验证，设备绑定

5\. TEE 内 Token 签发

SGX Enclave

验证授权后，在 enclave 内签发 PASETO v4\.local Action Token，写入 Redis 单次使用记录

enclave 内部密钥，Redis jti 单次强制

6\. TEE 内凭证解密

SGX Enclave

使用 Action Token 的 vault\_ref 定位密文，在 enclave 内解密得到临时明文材料

AES\-256\-GCM，明文仅在 enclave EPC 内存中存在

7\. Connector 执行

隔离容器

从 enclave 安全信道接收临时凭证（不落磁盘），执行 API 调用或浏览器操作

容器完成后立即销毁，凭证 memset 清零

8\. 结果返回与审计

Gateway \+ immudb

清洗结果（移除 PII），写入不可篡改审计日志，返回 Agent，通知用户

immudb 加密哈希链，Agent 只收到业务结果

# 7\. 数据模型与存储设计

## 7\.1  凭证记录结构

// Vault 凭证记录（TypeScript 类型定义）

interface VaultEntry \{

  // ─── 标识符（明文存储，无敏感信息）────────────────────

  id:           string;          // UUID v7，外部唯一引用

  user\_id:      string;          // 用户 ID（哈希后存储）

  service\_id:   string;          // 服务标识，如 'schwab' 'irs'

  created\_at:   Date;

  updated\_at:   Date;

  expires\_at:   Date | null;     // 凭证有效期（如 OAuth token）

  // ─── 加密载荷（仅 TEE 可解密）──────────────────────────

  credential\_type: CredentialType;

  encrypted\_payload: EncryptedBlob;  // 见 §4\.2

  // ─── Scope 与权限（明文，用于 Gateway 快速检查）─────────

  granted\_scopes: ActionScope\[\];     // 已授权的操作列表

  scope\_hash:     string;            // SHA\-256\(granted\_scopes\) 防篡改

  // ─── MFA 配置（仅存元数据，TOTP 种子加密存储）──────────

  mfa\_type:    'totp' | 'sms' | 'push' | null;

  mfa\_hint:    string | null;        // 如：手机号末四位

  // ─── Shamir 分片元数据（KYC文件专用）───────────────────

  shard\_config: ShardConfig | null;  // n, k, 分片位置

\}

type CredentialType =

  | 'username\_password'  // 经典账号密码

  | 'oauth\_refresh'      // OAuth2 refresh token

  | 'api\_key'            // API 密钥

  | 'session\_cookie'     // 持久化会话 Cookie

  | 'kyc\_document'       // 身份证明文件（Shamir 分片）

  | 'saml\_assertion'     // SAML SSO

  | 'cert\_p12';          // 客户端证书

## 7\.2  存储分层策略

__数据类型__

__存储引擎__

__加密方式__

__访问控制__

凭证密文（EncryptedBlob）

HashiCorp Vault（KV v2 引擎）

Vault 自身 AES\-256 \+ L3 密钥双重加密

仅 TEE enclave 有 Vault Token

KYC 文件分片

加密 S3 / MinIO（自托管）

SSE\-C \+ AES\-256\-GCM，Shamir 分片

分片分布在不同物理节点

用户/服务元数据

PostgreSQL \+ Row Level Security

列级加密（敏感字段）

每用户独立 DB 角色

Action Token 状态

Redis（jti 单次使用记录）

Redis TLS \+ 内存加密

TTL=15min 自动过期

审计日志

immudb（不可篡改）

哈希链 \+ 数字签名

Append\-only，无 DELETE 权限

操作会话状态

Redis（短期）

TLS 传输加密

TTL=5min，操作完成立即清除

# 8\. OpenClaw 集成方案

## 8\.1  集成架构概述

CredBridge 通过两条路径与 OpenClaw 集成，无需修改 OpenClaw 核心代码（packages/core 或 packages/gateway）：

- 路径一 — SKILL\.md：注册 CredBridge 为 OpenClaw Skill，使 Pi Agent Runtime 能感知其能力，在 system prompt 中包含工具描述
- 路径二 — mcporter：CredBridge 实现标准 MCP Server（SSE Transport），通过 mcporter 无缝接入 OpenClaw 的 MCP 工具生态

## 8\.2  Skill 安装与配置

\# 通过 ClawHub 安装（正式上线后）

/skill install credbridge

\# 手动安装

npm install \-g @credbridge/openclaw\-skill@latest

credbridge init          \# 引导式初始化：TEE 检测、Vault 后端、加密配置

credbridge connect        \# 连接服务（引导式配置具体服务凭证）

credbridge verify\-tee    \# 验证本地 TEE 环境可用性

credbridge start          \# 启动 MCP Server（默认端口 3721）

\# 服务启动后自动配置 mcporter：

\# ~/\.openclaw/mcporter\.json 将被追加以下配置

// ~/\.openclaw/mcporter\.json（credbridge init 自动追加）

\{

  "servers": \{

    "credbridge": \{

      "url":        "http://127\.0\.0\.1:3721/sse",

      "transport":  "sse",

      "auth": \{

        "type":     "bearer",

        "token":    "$\{CREDBRIDGE\_AGENT\_TOKEN\}"

      \},

      "autoStart":  true,

      "healthCheck":"http://127\.0\.0\.1:3721/health",

      "tee\_attestation": \{

        "require":  true,

        "verify\_url":"https://credbridge\.ai/registry/mrenclave"

      \}

    \}

  \}

\}

## 8\.3  SKILL\.md 设计

SKILL\.md 是 OpenClaw Skill 系统的核心文件，决定了 Pi Agent 何时将 CredBridge 注入 system prompt：

\-\-\-

name: credbridge

description: |

  当 Agent 需要以用户身份登录或操作任何需要认证的服务时使用。

  涵盖：银行/券商/政府门户/电商/SaaS/医疗/HR 等所有需要凭证的场景。

  Agent 自身不持有任何凭证，通过 CredBridge 安全代理执行操作。

triggers:

  \- 用户要求查询/操作需要登录的账户

  \- 需要提交包含个人身份信息的表单

  \- 需要以用户身份调用第三方 API

security: tee\-verified

version: 1\.0\.0

\-\-\-

\# CredBridge — AI 行动授权与凭证桥接

通过 CredBridge，你可以安全地代表用户与外部服务交互。

\*\*关键约束：永远不要请求用户直接提供明文密码或身份证号。\*\*

调用 credbridge\_list\_services 了解用户已授权的服务，

调用 credbridge\_execute 执行具体操作。

## 8\.4  与 OpenClaw 安全边界的协同

CredBridge 与 OpenClaw 现有安全机制协同工作，而非替代：

__OpenClaw 安全机制__

__CredBridge 补充__

DM 配对（Pairing）：控制谁能向 Gateway 发消息

CredBridge 进一步控制已配对用户能执行哪些敏感操作

Skill 沙箱：限制 Skill 的 API 访问

CredBridge 在沙箱外另设 TEE 边界，比 Skill 沙箱更强

Prompt Injection 防御：上下文隔离

Action Token Scope 绑定确保即使 Agent 被注入，也无法超权

openclaw security audit：扫描配置安全

CredBridge 替换 ~/\.openclaw/ 明文凭证，从根本解决审计发现的高危问题

# 9\. MCP 工具规范

## 9\.1  工具清单

__工具名称__

__参数__

__风险等级__

__说明__

credbridge\_list\_services

—

Tier 0

列出用户已配置的服务及其 Scope，供 Agent 了解可用能力

credbridge\_execute

service, action, params

Tier 0\-2
（按操作类型）

核心执行工具。验证 Scope → 签发 Token → 在 TEE 内解密凭证 → Connector 执行 → 返回结果

credbridge\_poll\_status

execution\_id

Tier 0

轮询异步操作状态（如等待 HITL 审批的操作）

credbridge\_get\_audit\_log

service?, limit?, since?

Tier 0

获取操作审计记录（仅业务结果，无凭证数据）

credbridge\_request\_scope

service, requested\_scopes, reason

Tier 1

请求用户授权新 Scope，触发用户设备审批流程

credbridge\_revoke\_service

service

Tier 2

吊销指定服务的所有授权和凭证，触发强制用户确认

credbridge\_tee\_status

—

Tier 0

返回当前 TEE enclave 状态和认证报告，供用户/审计验证

## 9\.2  核心工具 Schema（credbridge\_execute）

// MCP Tool Schema（JSON Schema）

\{

  "name": "credbridge\_execute",

  "description": "以用户身份安全执行对外部服务的操作。凭证从加密保险库获取，操作在 TEE 内执行，Agent 仅获得操作结果。",

  "inputSchema": \{

    "type": "object",

    "required": \["service", "action"\],

    "properties": \{

      "service": \{

        "type": "string",

        "description": "服务 ID，如 schwab/irs/amazon/sofi，见 credbridge\_list\_services 返回值"

      \},

      "action": \{

        "type": "string",

        "description": "操作 ID，见该服务的 Connector Manifest"

      \},

      "params": \{

        "type": "object",

        "description": "操作参数（仅非敏感参数，如 symbol/amount/date）"

      \},

      "reason": \{

        "type": "string",

        "description": "操作原因说明，将展示给用户审批（Tier 1/2 必填）"

      \}

    \}

  \}

\}

## 9\.3  调用示例

// 场景：用户问 "帮我查一下 Schwab 的持仓情况"

// Step 1: Agent 调用

await mcp\.call\("credbridge\_list\_services"\);

// 返回: \[\{ service: "schwab", scopes: \["read\_portfolio", "read\_balance"\], tier: 0 \}\]

// Step 2: Agent 执行操作

const result = await mcp\.call\("credbridge\_execute", \{

  service: "schwab",

  action:  "get\_portfolio\_summary"

\}\);

// CredBridge 内部流程（Agent 不可见）：

//  1\. Gateway 验证 scope：read\_portfolio ✅

//  2\. 风险评级：Tier 0（读操作）→ 自动审批

//  3\. TEE 签发 PASETO Token（15min TTL）

//  4\. TEE 解密 Schwab 凭证（enclave 内部）

//  5\. Connector 使用 OAuth 调用 Schwab API

//  6\. 结果清洗，写入审计日志

//  7\. 返回结果

// 返回给 Agent（无任何凭证信息）：

// \{

//   total\_value:      84320\.11,

//   day\_change:       \-1204\.33,

//   top\_holdings:     \[\{ symbol: "AAPL", value: 12400, pct: 14\.7 \}, \.\.\.\],

//   execution\_id:     "exec\_01HXY\.\.\.",

//   tee\_verified:     true

// \}

# 10\. 连接器系统（Connector Runtime）

## 10\.1  连接器类型

__连接器类型__

__适用场景__

__隔离方式__

__凭证注入方式__

Direct API

服务有官方 REST/GraphQL API（Schwab、Alpaca、Stripe）

进程隔离

TEE 通过安全信道注入内存

OAuth Exchange

服务支持 OAuth2，需要 refresh token 兑换

Docker 容器

refresh\_token 从 TEE 注入，access\_token 在内存中

Browser Automation

无 API，仅有 Web 门户（SoFi、DMV）

gVisor 沙箱容器 \+ AMD SEV\-SNP（可选）

密码从 TEE 注入，Playwright 执行后立即清零

Form Submission

政府/传统服务，需要上传 KYC 文件

gVisor 容器

Shamir 重建后的明文在 enclave 内注入，文件不落磁盘

Screen Understanding

UI 不稳定，无固定 CSS Selector

gVisor 容器 \+ Vision LLM

截图 → Vision LLM 识别元素 → Playwright 操作

Session Replay

服务支持持久化 Cookie/Session

进程隔离

Cookie 从 TEE 注入，会话到期自动降级为 Browser Auto

## 10\.2  容器隔离规范

每次操作在全新容器中执行，操作完成立即销毁。容器安全配置：

- 网络：出口仅允许目标服务域名（allowlist），无法访问其他地址
- 文件系统：只读根文件系统，/tmp 为 tmpfs（内存），操作完成后自动清除
- 系统调用：seccomp 白名单，仅允许必要 syscall（block ptrace / perf\_event 等监控类调用）
- 用户：以非 root 用户运行（uid=1000），无 CAP\_SYS\_PTRACE
- 内存：在 AMD SEV\-SNP 部署中，容器内存由 SEV 固件加密
- 凭证清零：操作完成后调用 sodium\_memzero\(\) 主动清除内存中的凭证副本

## 10\.3  Connector Manifest 格式

// 示例：irs\.connector\.json（美国国税局）

\{

  "service\_id":     "irs",

  "display\_name":   "美国国税局 \(IRS\)",

  "category":       "government/tax",

  "connector\_type": "form\_submission",

  "auth\_types":     \["username\_password"\],

  "base\_url":       "https://www\.irs\.gov",

  "kyc\_required":   \["ssn", "dob", "address"\],

  "jurisdiction":   "US",

  "legal\_note":     "自动化提交需符合 IRS e\-file 规定",

  "actions": \[

    \{

      "id":          "check\_refund\_status",

      "description": "查询退税状态",

      "risk\_tier":   0,

      "params":      \{\},

      "returns":     \{ "status": "string", "amount": "number|null", "date": "string|null" \}

    \},

    \{

      "id":          "submit\_extension",

      "description": "申请报税延期（Form 4868）",

      "risk\_tier":   2,

      "params":      \{ "tax\_year": "number" \},

      "kyc\_needed":  \["ssn", "dob", "address", "estimated\_tax"\],

      "returns":     \{ "confirmation\_number": "string", "deadline": "string" \}

    \}

  \]

\}

# 11\. 人工审批（Human\-in\-the\-Loop）

## 11\.1  风险分级策略

__风险等级__

__判定条件__

__审批机制__

__超时处理__

Tier 0
自动批准

纯读操作 / 写操作金额 < 用户设定阈值（默认 $0）/ 常规服务日常操作

有效 Action Token 即自动执行

N/A

Tier 1
软审批

首次使用新服务 / 写操作在阈值内 / 非常规时间（用户历史习惯模型判断）/ 新设备触发

推送通知到绑定设备，30分钟内需确认

超时自动拒绝，写入审计日志

Tier 2
强制审批

金额超过阈值 / 不可逆操作（开户/大额转账/文件提交）/ 政府服务 / 高风险操作

推送通知 \+ 需要 PIN 或生物识别确认

超时自动拒绝，需用户重新发起

## 11\.2  审批推送渠道

CredBridge 利用 OpenClaw 现有的消息渠道发送审批通知，无需额外安装 App：

- 主要渠道：通过 OpenClaw Gateway 向用户已配置的 Telegram/WhatsApp/Signal 发送带按钮的确认消息
- 备用渠道：独立移动 App（后期路线图），基于 ARM TrustZone 的设备绑定审批
- 降级方案：短信 OTP（需要服务器端 SMS 提供商）

审批消息格式示例（Telegram）：

╔══════════════════════════════════════╗

║  🔐 CredBridge 操作审批请求           ║

╠══════════════════════════════════════╣

║  服务：Charles Schwab               ║

║  操作：提交市价单                     ║

║  标的：AAPL × 10 股                  ║

║  预估金额：~$2,340                   ║

║  发起时间：2026\-03\-09 14:23 PST      ║

║  发起来源：你的 OpenClaw Agent        ║

╠══════════════════════════════════════╣

║  \[✅ 确认执行\]    \[❌ 拒绝\]           ║

║  \[🔒 查看详情\]   \[⚠️ 举报异常\]        ║

╚══════════════════════════════════════╝

  有效期：30 分钟 | 超时自动拒绝

# 12\. 审计与合规

## 12\.1  不可篡改审计日志

CredBridge 使用 immudb 构建密码学证明的不可篡改日志。immudb 基于哈希树（Merkle Tree）和数字签名，任何历史记录的修改都会破坏哈希链并被立即发现。

// 审计日志条目结构

interface AuditEntry \{

  // ─── 标识（明文）──────────────────────────────────────

  id:           string;    // UUID v7

  user\_id\_hash: string;    // SHA\-256\(user\_id\)，保护用户隐私

  timestamp:    Date;      // UTC

  session\_id:   string;    // OpenClaw 会话 ID

  // ─── 操作信息（明文）────────────────────────────────

  service:      string;    // 如 'schwab'

  action:       string;    // 如 'place\_order'

  risk\_tier:    0 | 1 | 2;

  outcome:      'success' | 'failure' | 'denied' | 'timeout';

  error\_code:   string | null;

  // ─── HITL 记录（如适用）─────────────────────────────

  hitl\_required:  boolean;

  hitl\_decision:  'approved' | 'rejected' | 'timeout' | null;

  hitl\_timestamp: Date | null;

  // ─── 技术取证信息────────────────────────────────────

  tee\_mrenclave:    string;   // enclave 度量值

  connector\_image:  string;   // Docker 镜像哈希

  action\_token\_jti: string;   // Token ID（无法反推凭证）

  // ─── 参数摘要（PII 已脱敏）──────────────────────────

  sanitized\_params: Record<string, string>; // PII 替换为类型 token

  // 例：\{ amount: '$2340', symbol: 'AAPL', ssn: '\[SSN\_REDACTED\]' \}

\}

## 12\.2  合规认证路线

__认证标准__

__目标时间__

__核心要求__

__当前差距__

SOC 2 Type I

MVP \+ 3个月

安全、可用性、保密性控制文档化

需要完成安全控制文档

SOC 2 Type II

MVP \+ 12个月

上述控制持续有效运行（6个月观察期）

需要 6 个月运营数据

ISO 27001

MVP \+ 18个月

信息安全管理体系（ISMS）建立

需要完整 ISMS 建设

FIPS 140\-3 Level 3

MVP \+ 24个月

密码模块物理安全 \+ 身份验证

需要第三方实验室认证

GDPR 合规

上线前

数据主体权利、DPA、数据驻留

需要法律评审和 DPA 协议

CCPA 合规

上线前

加州居民数据权利、隐私通知

需要隐私政策更新

# 13\. 安全攻击面分析

## 13\.1  攻击向量与缓解措施

__攻击向量__

__风险等级__

__技术缓解措施__

__残余风险__

Vault 数据库全量泄露

高

数据库内只有 AES\-256\-GCM 密文；L1 主密钥在 SGX enclave 内 Sealing，密文离开 enclave 后不可用

极低（只有密文）

Agent Prompt Injection

高

Action Token Scope 硬编码绑定；Gateway 独立于 Agent 进行二次 Scope 校验；操作结果不含凭证信息

低（操作有限且审计）

TEE Side\-Channel（如 Spectre）

中高

禁用超线程（SGX 推荐配置）；启用 SGX SDK 的 ASLR；定期更新 Microcode；选用 TDX/SEV\-SNP 作为替代

中（存在于所有 TEE 实现）

Connector 容器逃逸

高

gVisor sandbox（用户态内核）；seccomp 严格白名单；无 CAP\_SYS\_PTRACE；AMD SEV\-SNP 隔离（可选）

低

MitM / TLS 降级

中

mTLS 1\.3 强制；Certificate Pinning；HSTS；RA\-TLS（证书绑定 enclave 测量值）

极低

Token 重放攻击

中

PASETO jti 存 Redis，单次使用强制；15min TTL；Token 绑定 user\_id \+ service \+ action

极低

Supply Chain（恶意 Skill）

高

CredBridge 在 OpenClaw Skill 沙箱外的 TEE 中运行；恶意 Skill 无法访问 enclave 内部；凭证不落明文

低

物理内存提取（Cold Boot）

中高

Intel TME\-MK（内存加密）；AMD SME；关键密钥材料仅在 TEE EPC 内；EPC 内存为硬件加密

低（EPC 内存加密）

内部人员越权

中

零知识架构：员工无法访问 enclave 内部；数据库仅有密文；强制双人签名策略（操作规程）

低

BGP 劫持 / DNS 投毒

低

Certificate Transparency 监控；DANE / DNSSEC；关键操作使用 IP Pinning

极低

## 13\.2  已知 TEE 局限性

__重要声明  __TEE 不是银弹。以下已知攻击类别需要额外的缓解措施，且仍存在残余风险。我们在设计文档中明确列出，以避免对安全性产生错误的过度自信。

- __侧信道攻击（Side\-channel）：Spectre、Meltdown 类攻击已有针对 SGX 的 PoC。缓解：禁用超线程、启用 Microcode 更新、使用 SGX 2\.0 的改进隔离__
- 电压故障注入（VoltJockey 类）：通过 DVFS 操控电压可能破坏 TrustZone AES 密钥提取。缓解：服务端部署 SGX（无直接物理访问），不依赖 TrustZone 作为服务端主力
- Rowhammer：DRAM 物理特性导致比特翻转。缓解：ECC 内存（服务器标准配置）、内存隔离
- TEE 供应商可信度：Intel/AMD 作为硬件厂商理论上可后门。缓解：开源 SDK、可审计的 MRENCLAVE、多厂商 TEE 交叉验证（可选）

# 14\. 部署架构

## 14\.1  部署模式

__部署模式__

__目标用户__

__TEE 支持__

__说明__

自托管（本地）

技术用户 / 开发者

软件 TEE
（无硬件 TEE）

与 OpenClaw 运行在同一主机，最低摩擦，安全级别较低，适合开发测试

自托管（SGX 服务器）

高安全需求用户

Intel SGX
Full TEE

用户自行购买 SGX 服务器（如 Intel NUC with SGX），最高安全级别

云托管（Azure CVM）

企业/SaaS 用户

AMD SEV\-SNP
机密 VM

部署在 Azure DCsv3 / Cobalt CVM，结合 Azure Attestation，最易部署

云托管（AWS Nitro）

AWS 用户

AWS Nitro Enclaves

EC2 \+ Nitro Enclave，AWS 用户首选，支持 PCR 测量值验证

多方部署（企业级）

大型企业

多 TEE 实例

密钥 Shamir 分片跨多个 TEE 实例，需要多方签名执行高风险操作

## 14\.2  自托管 \+ SGX 快速部署

\# 前提：Intel SGX 支持的 CPU（Intel Core 6代\+ 或 Xeon E3/E\-系列）

\# 检测 SGX 支持：

sgx\-detect  \# 来自 fortanix/rust\-sgx\-sdk

\# 安装 CredBridge

npm install \-g @credbridge/server@latest

\# 初始化（引导向导）

credbridge init

\#   → 检测 TEE 环境（SGX / SEV\-SNP / 软件降级）

\#   → 生成 TEE enclave 并进行 Remote Attestation

\#   → 初始化 Vault 后端（HashiCorp Vault 或 AWS Secrets Manager）

\#   → 生成 L0 Root Key（存入 SGX Sealing）

\#   → 配置 Redis（单次 Token 使用强制）

\#   → 配置 immudb（不可篡改审计日志）

\#   → 生成 OpenClaw mcporter 配置

\# 以 systemd 服务启动

credbridge service install

systemctl \-\-user start credbridge

\# 验证 TEE 认证

credbridge verify  \# 打印 MRENCLAVE 和认证报告

# 15\. 实施路线图

__阶段__

__时间__

__核心交付物__

__技术里程碑__

Phase 0
基础加密层

0 → 4 周

密钥层次设计 \+ AES\-256\-GCM 凭证加密 \+ PASETO v4 Token \+ 基础 Vault 存储 \+ immudb 审计日志

通过 NIST 密码算法合规检查
完成密钥层次 Code Review

Phase 1
MVP

4 → 10 周

软件 TEE（无硬件依赖）\+ MCP Server \+ OpenClaw Skill \+ 5 个种子连接器（Alpaca/GitHub/Amazon）\+ HITL 推送审批（Telegram）

与 OpenClaw 端到端集成联调
首个真实服务操作成功

Phase 2
TEE 硬化

10 → 20 周

Intel SGX Enclave 实现 \+ AMD SEV\-SNP Connector 隔离 \+ 远程认证流程 \+ Shamir KYC 文件存储 \+ 政府服务连接器（IRS/DMV）

TEE 认证验证流程上线
SGX enclave 通过安全审计

Phase 3
商用化

20 → 36 周

50\+ 连接器 \+ 云部署（Azure/AWS Nitro）\+ SOC 2 Type I 认证 \+ 开放 Connector SDK \+ ClawHub Skill 发布 \+ 企业多方部署

SOC 2 Type I 报告获取
生产环境稳定运行 30 天

Phase 4
认证与生态

36 → 60 周

SOC 2 Type II \+ FIPS 140\-3 Level 3 认证 \+ 100\+ 连接器 \+ 连接器市场 \+ PQC 预研（后量子密码迁移路径）

FIPS 认证获取
连接器生态 100\+ 上线

# 16\. 附录

## 附录 A — 技术选型理由

__技术组件__

__选型理由__

__替代方案__

Intel SGX \(主力 TEE\)

应用级 enclave 粒度，适合凭证解密场景；Azure/AWS 均支持；DCAP 远程认证成熟

AMD SEV（VM 级，粒度较粗）/ AWS Nitro（AWS 锁定）

PASETO v4\.local

消除 JWT 的 alg:none / RS256 混淆漏洞；XChaCha20\-Poly1305 比 AES\-GCM 更适合软件实现（恒定时间）

JWT \+ RS256（有已知漏洞）

AES\-256\-GCM

NIST FIPS 197 标准；硬件加速（AES\-NI）；AEAD 同时提供加密和完整性

ChaCha20\-Poly1305（软件更快，但无 FIPS 认证）

Shamir's Secret Sharing

数学上证明安全；门限方案防止单点泄露；无需单一可信方

多方计算（MPC）—— 更复杂，当前成熟度不足

immudb

专为不可篡改日志设计；Merkle Tree \+ 数字签名；开源自托管；API 简单

Amazon QLDB（AWS 锁定）/ Hyperledger（复杂度高）

HashiCorp Vault

行业标准秘密管理；动态秘密；审计日志；KV v2 版本控制；自托管友好

AWS Secrets Manager（AWS 锁定）/ Azure Key Vault（Azure 锁定）

gVisor \(runsc\)

用户态内核，Connector 容器的系统调用被拦截，大幅减少容器逃逸面

标准 runc（无额外系统调用隔离）/ Kata Containers（更重）

HKDF\-SHA\-256

NIST SP 800\-56C 推荐的密钥派生函数；域分离防止密钥复用攻击

PBKDF2（较慢，适合口令，不适合密钥派生）

## 附录 B — 开放问题

__编号__

__问题__

__当前倾向__

B\-1

SGX EPC 内存限制（默认 128MB\-256MB）是否会在处理大量并发凭证解密时成为瓶颈？

采用 SGX Paging（性能有损）或分批处理；监控 EPC 使用率

B\-2

用户自托管时无 SGX 硬件，软件 TEE 降级方案的安全声明如何措辞，避免误导用户？

明确标记为「降级模式」，UI 显示安全等级，拒绝 KYC 文件存储

B\-3

浏览器自动化是否违反目标服务 ToS（如银行的「不得使用自动化工具」条款）？

优先对接官方 API；浏览器自动化仅用于无 API 服务；用户法律责任自担，需要 ToS 同意

B\-4

TEE 厂商后门风险如何向用户披露？

在认证报告中明确列出信任链；提供多厂商交叉验证选项；纳入安全白皮书

B\-5

后量子密码（PQC）迁移时间表？

跟踪 NIST PQC 标准化（CRYSTALS\-Kyber/Dilithium）；2028 年前完成迁移规划

B\-6

企业多租户 vs 单用户部署的密钥隔离方案？

多租户：每用户独立 L2 密钥，物理隔离 Vault namespace；单用户：全量 L1 绑定用户设备

*— 文档结束 —*

