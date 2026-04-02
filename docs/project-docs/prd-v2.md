# Product Requirements Document - CredBridge 开源发布准备

**Author:** yvan
**Date:** 2026-03-24
**Status:** Draft for Internal Review
**Target:** Openclaw Clawhub 社区

---

## 1. 概述

### 1.1 项目背景

CredBridge 是一个面向 AI Agent 设计的零信任凭证保险库，基于 Intel SGX TEE 硬件隔离技术，让 AI Agent 能够安全地代表用户访问需要凭证的真实服务，同时确保明文凭证永远不离开硬件安全边界。

当前 CredBridge 已具备完整的后端服务（Rust/Axum）、前端控制台（React）、TypeScript/Rust 双 SDK、CLI 工具和 MCP Server，功能覆盖凭证加密存储、TEE Sandbox 浏览器自动化、不可篡改审计日志和多租户管理。

### 1.2 本次需求目标

本 PRD 定义的是 **CredBridge 开源发布准备项目** —— 将现有内部工程产品转化为可供全球 AI Agent 开发者社区使用的开源项目，目标平台为 Openclaw 的 Clawhub 社区。

这不是功能开发，而是一次**产品形态转型**：从面向内部团队的工程代码库，转变为面向外部开发者的、安全的、品牌统一的、文档完善的、开箱即用的开源基础设施。

### 1.3 核心工作范围

1. **安全脱敏**：清除硬编码凭证和内部基础设施信息
2. **国际化**：代码和文档英文化
3. **品牌统一**：前端 UI 对齐 zk.me 设计语言
4. **SDK 发布**：NPM / crates.io 包封装
5. **社区规范建设**：LICENSE、贡献指南、行为准则
6. **开发者体验优化**：一键启动开发环境、端到端使用指南

### 1.4 战略意义

CredBridge 是 AI Agent 领域第一个专用的零信任凭证保险库开源方案。市场上的密码管理器（1Password、Bitwarden）为人类设计，无法与 Agent 框架原生集成；传统 OAuth 仅覆盖支持标准协议的服务。CredBridge 通过 TEE 硬件隔离 + `{{CREDENTIAL.xxx}}` 零知识占位符协议，让 Agent 能安全执行凭证操作但永远接触不到真实密码，填补了 Agent Economy 规模化落地的关键基础设施空白。

开源这一产品的战略意义在于：通过社区采用建立 AI Agent 凭证安全的事实标准，同时强化 zkMe 在 Agent Trust Gateway 赛道的技术品牌影响力。

---

## 2. 需求列表

| 需求编号 | 需求名称 | 优先级 | 一句话说明 |
|---------|---------|--------|-----------|
| REQ-001 | 硬编码 Token 清理 | P0 | 移除仓库中所有硬编码的 PASETO Token 和测试凭证 |
| REQ-002 | 内部基础设施 URL 清理 | P0 | 替换或移除所有内部域名和镜像仓库地址 |
| REQ-003 | 开发者路径修复 | P0 | 修复 docker-compose 中硬编码的绝对路径为相对路径 |
| REQ-004 | 敏感文件 Git 管理 | P0 | 将 .env.bak 等敏感文件加入 .gitignore |
| REQ-005 | Git 历史安全评估 | P0 | 评估并清理 Git 历史中的敏感信息残留 |
| REQ-006 | 安全扫描验证 | P0 | 运行 trufflehog/git-secrets 验证零敏感信息泄露 |
| REQ-007 | MIT LICENSE 文件 | P0 | 在仓库根目录创建 MIT 许可证文件 |
| REQ-008 | README 国际化 | P0 | 将 README 翻译为英文，原中文版保留为 README_CN.md |
| REQ-009 | SDK 代码英文化 | P0 | 将 TypeScript/Rust SDK 的中文注释和字符串翻译为英文 |
| REQ-010 | 后端代码英文化 | P0 | 将后端 Rust 代码中的中文错误消息和注释翻译为英文 |
| REQ-011 | CLI 输出英文化 | P0 | 确保 CLI 工具的所有用户可见文本为英文 |
| REQ-012 | 前端 UI 品牌化 | P1 | 将控制台 UI 配色方案更新为 zk.me 品牌色系 |
| REQ-013 | 品牌资源替换 | P1 | 替换 favicon 和 Logo 为 CredBridge/zk.me 品牌资源 |
| REQ-014 | npm 包发布配置 | P1 | 完善 package.json 配置并发布 @toani/vault-sdk 到 npm (旧名 @credbridge/sdk 已弃用) |
| REQ-015 | 贡献指南 | P1 | 创建 CONTRIBUTING.md 贡献流程文档 |
| REQ-016 | 行为准则 | P1 | 创建 CODE_OF_CONDUCT.md 社区行为准则 |
| REQ-017 | Issue/PR 模板 | P1 | 创建 GitHub/Clawhub Issue 和 PR 模板 |
| REQ-018 | docker-compose 一键启动 | P1 | 修复 docker-compose 配置，支持一键启动完整开发环境 |
| REQ-019 | 环境变量示例 | P1 | 提供 .env.example 文件，包含所有必要环境变量说明 |
| REQ-020 | 文档目录清理 | P1 | 分离内部文档和公开文档，移除敏感内部报告 |

---

## 3. 需求详情

### REQ-001: 硬编码 Token 清理

**优先级**: P0
**验收标准**: 安全扫描工具报告零硬编码凭证

**详细描述**:

需要识别并移除以下位置的硬编码 Token：

1. **docs/test-token.txt**
   - 包含 admin/admin123 账号密码
   - 包含完整 PASETO Token 明文
   - **处理方式**: 删除文件，并在 .gitignore 中添加规则防止重新提交

2. **sdk-typescript/test-fetch.ts**
   - 硬编码完整 `v4.local...` Token
   - **处理方式**: 将 Token 移至环境变量，示例代码使用占位符

3. **其他潜在位置**
   - 扫描整个代码库查找 `v4.local`、`v4.public` 等 PASETO Token 模式
   - 扫描 `password`、`token`、`secret` 等关键词的硬编码赋值

**验证方式**:
```bash
# 使用 trufflehog 扫描
trufflehog filesystem .

# 使用 git-secrets 扫描
git-secrets --scan-history
```

---

### REQ-002: 内部基础设施 URL 清理

**优先级**: P0
**验收标准**: 公开仓库中不包含任何内部域名

**详细描述**:

需要处理以下文件中的内部基础设施信息：

1. **.drone.yml**
   - 移除 `hub.bitkinetic.com` 镜像仓库地址
   - 移除 `charts.bitkinetic.com` Helm 仓库地址
   - 移除 `zkme-dev`、`zkme-test` 等内部 Kubernetes 命名空间
   - **处理方式**: 替换为占位符或注释说明

2. **.values.yaml / .test.values.yaml**
   - 移除 `hub.bitkinetic.com/zkme/credbridge` 镜像路径
   - **处理方式**: 使用变量引用或占位符

3. **config/chart/values/values-prod.yaml**
   - 移除 `vault.bitkinetic.com` 内部主机名
   - **处理方式**: 使用环境变量或配置占位符

---

### REQ-003: 开发者路径修复

**优先级**: P0
**验收标准**: docker-compose 可在任意机器上正常运行

**详细描述**:

**docker/docker-compose.yml** 中 `build.context` 硬编码了开发者本机绝对路径 `/Users/yvan/AIWorkspace/credbridge`。

**处理方式**:
- 将绝对路径改为相对路径（如 `.` 或 `./..`）
- 验证修复后的配置在 macOS 和 Linux 上均可正常启动

---

### REQ-004: 敏感文件 Git 管理

**优先级**: P0
**验收标准**: .env.bak 等敏感文件不会被提交到仓库

**详细描述**:

1. **当前问题**: `.env.bak` 未被 `.gitignore` 忽略，可能包含敏感配置

2. **处理方式**:
   - 将 `.env.bak` 加入 `.gitignore`
   - 如果已提交，从 Git 历史中移除
   - 同时检查其他潜在的敏感文件模式：
     - `*.bak`
     - `.env.*`
     - `*.key`
     - `*.pem`

---

### REQ-005: Git 历史安全评估

**优先级**: P0
**验收标准**: Git 历史不包含敏感信息，或已进行清理

**详细描述**:

1. **评估内容**:
   - 扫描 Git 提交历史中的敏感信息（Token、密码、内部 URL）
   - 特别关注 `_bmad-output.backup.*` 文件历史
   - 检查是否包含 `git@git.bitkinetic.com:ai/credbridge.git` 等内部 Git 地址

2. **处理方式**:
   - 如果发现敏感信息，使用 `git filter-repo` 清理历史
   - 或考虑以 squash 方式首次提交推送到 Clawhub（新建仓库）

---

### REQ-006: 安全扫描验证

**优先级**: P0
**验收标准**: 安全扫描工具报告零敏感信息泄露

**详细描述**:

**扫描工具**:
- **trufflehog**: 检测 Git 历史中的敏感信息
- **git-secrets**: 防止提交敏感信息
- **detect-secrets**: 静态代码扫描

**验证流程**:
```bash
# 1. 安装扫描工具
pip install trufflehog git-secrets detect-secrets

# 2. 运行全量扫描
trufflehog filesystem .
git-secrets --scan-history
detect-secrets scan > .secrets.baseline

# 3. 确认扫描结果为零
```

---

### REQ-007: MIT LICENSE 文件

**优先级**: P0
**验收标准**: 仓库根目录存在有效的 LICENSE 文件

**详细描述**:

1. **文件位置**: `/LICENSE`

2. **内容要求**:
   - 使用标准 MIT 许可证文本
   - 版权年份：2026
   - 版权持有者：zkMe Technologies Ltd. 或 CredBridge Contributors

3. **验证方式**:
   - GitHub/Clawhub 能够自动识别许可证类型
   - 许可证与项目依赖的许可证兼容

---

### REQ-008: README 国际化

**优先级**: P0
**验收标准**: 根目录 README.md 为英文版本

**详细描述**:

1. **文件处理**:
   - 将现有 README.md 翻译为英文
   - 原中文版重命名为 README_CN.md
   - 在 README.md 顶部添加语言切换链接

2. **英文 README 内容结构**:
   - Project Overview
   - Features
   - Quick Start
   - Installation
   - Usage
   - API Documentation
   - Contributing
   - License

---

### REQ-009: SDK 代码英文化

**优先级**: P0
**验收标准**: SDK 代码中无中文注释和字符串

**详细描述**:

1. **sdk-typescript/src/**
   - 翻译所有中文注释为英文
   - 翻译 JSDoc 文档
   - 翻译用户可见的字符串（如 `已过期`、`分钟`）

2. **sdk-rust/**
   - 翻译所有中文注释为英文
   - 翻译 Rustdoc 文档

3. **重点文件**:
   - `sdk-typescript/src/token.ts` - 包含用户可见的中文字符串
   - `sdk-typescript/src/index.ts` - 主要接口文档
   - `sdk-rust/src/lib.rs` - 库入口文档

---

### REQ-010: 后端代码英文化

**优先级**: P0
**验收标准**: 后端代码中无中文错误消息和注释

**详细描述**:

1. **src/api/**
   - 翻译 API 错误响应消息
   - 翻译路由注释

2. **src/services/**
   - 翻译业务逻辑注释
   - 翻译错误消息

3. **src/models/**
   - 翻译数据模型注释

4. **错误消息规范**:
   - 使用英文描述性错误消息
   - 保持错误代码不变（如 `AUTH_001`、`VAL_002`）

---

### REQ-011: CLI 输出英文化

**优先级**: P0
**验收标准**: CLI 工具的所有输出为英文

**详细描述**:

1. **CLI 命令输出**:
   - 帮助信息（--help）
   - 操作成功/失败提示
   - 进度指示
   - 错误消息

2. **处理方式**:
   - 扫描 CLI 源码中的中文字符串
   - 统一替换为英文

---

### REQ-012: 前端 UI 品牌化

**优先级**: P1
**验收标准**: UI 配色与 zk.me 品牌色板一致

**详细描述**:

1. **品牌色板**（参照 `docs/UI_DESIGN_SPEC.md`）:
   - 主色：`#002E33`（深青）
   - 次色：`#005563`（中青）
   - 强调色：Teal 渐变

2. **需要更新的组件**:
   - 主题配置（tailwind.config.ts）
   - 全局 CSS 变量
   - 按钮、卡片、导航等核心组件
   - 图表和数据可视化配色

---

### REQ-013: 品牌资源替换

**优先级**: P1
**验收标准**: 所有品牌资源为 CredBridge/zk.me 官方资源

**详细描述**:

1. **favicon**:
   - 替换为 CredBridge 品牌 favicon
   - 支持多尺寸（16x16, 32x32, 180x180）

2. **Logo**:
   - 替换登录页 Logo
   - 替换导航栏 Logo
   - 提供 SVG 和 PNG 格式

3. **品牌名称**:
   - 统一使用 "CredBridge"
   - 移除内部项目名称

---

### REQ-014: npm 包发布配置

**优先级**: P1
**验收标准**: @toani/vault-sdk 可在 npm 公有 registry 安装 (旧包名 @credbridge/sdk 已弃用)

**详细描述**:

1. **package.json 完善**:
   ```json
   {
     "name": "@toani/vault-sdk",
     "version": "1.0.0",
     "repository": {
       "type": "git",
       "url": "https://github.com/openclaw/credbridge.git"
     },
     "homepage": "https://github.com/openclaw/credbridge#readme",
     "bugs": {
       "url": "https://github.com/openclaw/credbridge/issues"
     },
     "publishConfig": {
       "access": "public"
     },
     "scripts": {
       "prepublishOnly": "npm run build"
     }
   }
   ```

2. **版本同步**:
   - `ToaniVaultSDK.get version` 中的硬编码版本号与 `package.json` 版本同步
   - 考虑从 package.json 动态读取版本

3. **发布流程**:
   ```bash
   npm run build
   npm test
   npm publish --access public
   ```

---

### REQ-015: 贡献指南

**优先级**: P1
**验收标准**: 仓库根目录存在 CONTRIBUTING.md

**详细描述**:

**文件位置**: `/CONTRIBUTING.md`

**内容要求**:
1. 贡献流程：Fork → Branch → PR → Review
2. 开发环境搭建步骤
3. 代码规范（Rust/React 编码标准）
4. Commit 格式规范（Conventional Commits）
5. 测试要求
6. PR 检查清单

---

### REQ-016: 行为准则

**优先级**: P1
**验收标准**: 仓库根目录存在 CODE_OF_CONDUCT.md

**详细描述**:

**文件位置**: `/CODE_OF_CONDUCT.md`

**内容要求**:
- 采用 Contributor Covenant 标准文本
- 包含报告不当行为的联系方式
- 明确不可接受行为的定义

---

### REQ-017: Issue/PR 模板

**优先级**: P1
**验收标准**: 存在规范的 Issue 和 PR 模板

**详细描述**:

1. **Issue 模板**（.github/ISSUE_TEMPLATE/）:
   - Bug Report（bug_report.md）
   - Feature Request（feature_request.md）
   - Question（question.md）

2. **PR 模板**（.github/pull_request_template.md）:
   - 变更描述
   - 相关 Issue
   - 测试说明
   - 检查清单（Checklist）

---

### REQ-018: docker-compose 一键启动

**优先级**: P1
**验收标准**: `docker-compose up` 可在 10 分钟内启动完整环境

**详细描述**:

1. **服务组成**:
   - CredBridge 后端服务
   - PostgreSQL 数据库
   - Redis 缓存

2. **配置要求**:
   - 无需手动配置即可启动
   - 使用默认环境变量
   - 自动执行数据库迁移

3. **验证方式**:
   - 在干净环境（macOS 和 Linux）测试
   - 从 clone 到服务响应 < 10 分钟

---

### REQ-019: 环境变量示例

**优先级**: P1
**验收标准**: 存在完整的 .env.example 文件

**详细描述**:

**文件位置**: `/.env.example`

**内容要求**:
- 包含所有必要环境变量
- 使用 `your_xxx_here` 占位符（非真实值）
- 每个变量附带说明注释
- 标注哪些变量是必需的，哪些是可选的

**示例变量**:
```
# Database
DATABASE_URL=postgresql://user:password@localhost/credbridge

# Redis
REDIS_URL=redis://localhost:6379

# JWT/PASETO Secret
PASETO_SECRET=your_secret_here

# TEE Configuration
TEE_ENABLED=false
```

---

### REQ-020: 文档目录清理

**优先级**: P1
**验收标准**: docs/ 目录仅包含面向开发者的公开文档

**详细描述**:

1. **需要移除的内部文档**:
   - 代码评审报告
   - 测试验收报告
   - 商业就绪差距分析
   - 内部架构决策记录

2. **保留的公开文档**:
   - API 参考文档
   - SDK 使用指南
   - Quick Start 教程
   - 部署指南

3. **处理方式**:
   - 将内部文档移至私有仓库或内部 Wiki
   - 或在 `.gitignore` 中排除敏感文档

---

## 附录 A：核心需求汇总（MVP）

以下 6 项是**最小可发布版本（MVP）**的底线要求：

1. 所有硬编码 Token 和凭证清除（REQ-001/002/003/004/005/006）
2. 添加 MIT LICENSE（REQ-007）
3. README 改为英文（REQ-008）
4. SDK 核心代码英文化（REQ-009）
5. docker-compose 可正常运行（REQ-018）
6. 提供 .env.example（REQ-019）

完成这 6 项即可安全推送到 Clawhub 社区。

---

## 附录 B：成功指标

### 用户成功指标

| 指标 | 目标值 | 验证方式 |
|------|--------|----------|
| 首次体验成功率 | 开发者 clone 后 10 分钟内启动环境 | 内部测试 |
| SDK 集成体验 | 5 行代码内完成首次 API 调用 | 文档验证 |
| 文档自助率 | 无需联系团队支持完成集成 | 用户测试 |
| 零困惑感 | 所有文档和代码为英文 | 人工审查 |

### 业务成功指标

| 指标 | 目标值 | 时间范围 |
|------|--------|----------|
| 社区发布 | 成功推送到 Clawhub 社区 | 发布时 |
| npm 包可用 | @toani/vault-sdk 可安装 (旧名 @credbridge/sdk) | 发布后 |
| GitHub/Clawhub Stars | > 500 | 发布后 3 个月 |
| 外部贡献 | 至少 10 个 Issue 或 PR | 发布后 3 个月 |

### 技术成功指标

| 指标 | 目标值 | 验证方式 |
|------|--------|----------|
| 安全零泄露 | 0 敏感信息 | trufflehog 扫描 |
| CI 通过 | 所有测试通过 | CI 验证 |
| SDK 功能对齐 | TS/Rust API 一致 | 代码审查 |
| 合规就绪 | MIT LICENSE 存在 | 文件检查 |

---

*PRD Version 2.0 — Ready for Internal Review*
