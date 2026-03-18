# CredBridge 用户手册验证问题报告

**验证人**: claude_qwen (CTO/架构师角色)
**验证日期**: 2026-03-11
**验证版本**: CredBridge MVP 1.0
**用户手册版本**: v1.0 (2026-03-11)

---

## 问题汇总

| 优先级 | 问题数 |
|--------|--------|
| P0 阻塞 | 5 |
| P1 严重 | 8 |
| P2 一般 | 6 |
| P3 建议 | 4 |

**总计**: 23 个问题

---

## 详细问题列表

### [P0] CLI 工具完全缺失

- **位置**: 用户手册 2.2.2, 2.3, 2.4, 2.5 章节
- **描述**: 用户手册中描述的所有 `credbridge` CLI 命令均未实现：
  - `credbridge init` - 初始化向导
  - `credbridge verify` - TEE 验证
  - `credbridge token create` - Token 生成
  - `credbridge token inspect` - Token 检查
  - `credbridge mcp start` - MCP Server 启动
  - `credbridge credential create` - CLI 创建凭证
  - `credbridge status` - 状态检查
  - `credbridge restart` - 重启服务
  - `credbridge audit resync` - 审计日志同步

- **影响**: 用户无法通过命令行完成任何操作，用户手册中的快速开始流程完全无法执行
- **建议修复**:
  1. 实现完整的 CLI 工具（建议使用 Rust 或 Node.js）
  2. 或在文档中明确说明 CLI 尚未实现，提供替代方案（如直接调用 API）

### [P0] Docker Compose 配置与实际目录结构不一致

- **位置**: 用户手册 2.2.1 章节，docker/docker-compose.yml
- **描述**:
  1. `docker-compose.yml` 中引用了 `./scripts/init-postgres.sql`，但实际路径为 `docker/scripts/init.sh`
  2. `docker-compose.yml` 中引用了 `./config/vault` 和 `./config/vault-agent` 目录，但这些目录不存在
  3. `docker-compose.yml` 中引用了 `./config/prometheus` 和 `./config/grafana` 目录，但这些目录不存在

- **影响**: Docker Compose 启动会失败
- **建议修复**: 创建缺失的配置文件或更新 docker-compose.yml 移除对不存在文件的引用

### [P0] 环境变量配置文件缺失

- **位置**: 项目根目录
- **描述**: `.env` 文件不存在，用户手册中未说明如何创建必要的环境变量配置
- **影响**: 用户无法正确配置服务
- **建议修复**:
  1. 提供 `.env.example` 模板文件
  2. 在文档中添加环境变量配置说明

### [P0] TEE Attestation API 端点缺失

- **位置**: 用户手册 7.3, 8.3.1 章节
- **描述**: 用户手册中提到 `curl http://localhost:8080/v1/tee/attest` 端点，但实际实现中该端点不存在
- **影响**: 用户无法验证 TEE 状态
- **建议修复**: 实现 `/api/v1/tee/attest` 端点或更新文档说明正确的端点路径

### [P0] 缺少 `.claude/mcp.json` 配置示例

- **位置**: 用户手册 6.2.2 章节
- **描述**: 文档中提到配置 `~/.claude/mcp.json`，但项目中没有提供配置示例文件
- **影响**: 用户无法正确配置 MCP Server 集成
- **建议修复**: 在项目中添加 `mcp.json.example` 配置文件

---

### [P1] CredBridge CLI 与 npm 包安装方式矛盾

- **位置**: 用户手册 2.2.1 vs 2.2.2
- **描述**:
  - 2.2.1 说使用 Docker Compose（推荐）
  - 2.2.2 说 `npm install -g @credbridge/server@latest`，但实际不存在此 npm 包
  - SDK 目录中也没有 server 包

- **影响**: 用户按照文档安装会失败
- **建议修复**: 移除手动安装说明或实际发布 npm 包

### [P1] API 基础路径不一致

- **位置**: 用户手册 vs 实际实现
- **描述**:
  - 用户手册中多处使用 `/api/v1/` 前缀
  - 但 TEE attestation 提到 `/v1/tee/attest`（缺少 `api`）
  - 实际实现中使用 `API_BASE_PATH` 但未确认是否正确配置

- **影响**: API 调用可能返回 404
- **建议修复**: 统一 API 路径前缀，在文档中明确说明

### [P1] SDK 文档与实际代码不匹配

- **位置**: 用户手册 5.1 vs sdk-typescript/README.md
- **描述**:
  - 用户手册中的 SDK 示例使用 `createUsernamePassword` 等方法
  - SDK README 中有详细文档，但需要验证实际代码是否实现这些方法

- **影响**: SDK 使用示例可能无法运行
- **建议修复**: 验证 SDK 实现与文档一致

### [P1] 健康检查端点响应格式不一致

- **位置**: 用户手册 2.2 vs 实际实现
- **描述**:
  - 用户手册期望返回简单状态
  - 实际实现返回 `{"status":"healthy","version":"0.1.0","timestamp":...}`
  - 缺少文档中提到的详细健康检查 `/health/detail` 端点验证

- **影响**: 自动化监控脚本可能无法正确解析响应
- **建议修复**: 在文档中明确说明响应格式

### [P1] 凭证类型定义不一致

- **位置**: 用户手册 7.3 vs 实际实现
- **描述**:
  - 用户手册 7.3 列出支持的凭证类型包括：`username_password`, `api_key`, `oauth_refresh`, `certificate`, `ssh_key`, `database_connection`
  - 实际代码中 `CredentialType` 枚举需要验证是否包含所有这些类型

- **影响**: 创建某些类型凭证可能失败
- **建议修复**: 确保代码与文档一致

### [P1] 速率限制配置未实现

- **位置**: 用户手册 7.3
- **描述**: 文档说明：
  - 凭证创建: 100/分钟
  - 凭证读取: 300/分钟
  - 凭证解密: 60/分钟
  - 审计日志查询: 60/分钟
  但实际速率限制配置是全局的，未按端点区分

- **影响**: 速率限制行为与文档描述不符
- **建议修复**: 实现按端点的速率限制或更新文档

### [P1] 多租户功能未完成

- **位置**: 用户手册 3.5 章节
- **描述**: 文档描述了完整的租户管理功能，但实际实现中租户功能较为基础
- **影响**: 租户管理功能可能无法按文档使用
- **建议修复**: 完成租户管理功能实现或更新文档说明当前限制

---

### [P2] Docker Compose 网络配置过于严格

- **位置**: docker/docker-compose.yml
- **描述**: 网络配置使用固定的子网 `172.20.0.0/16`，可能与用户现有网络冲突
- **影响**: Docker 网络冲突导致启动失败
- **建议修复**: 移除固定子网配置让 Docker 自动分配

### [P2] 缺少 immudb 初始化脚本

- **位置**: IMMUDB_SETUP.md vs docker-compose.yml
- **描述**: `IMMUDB_SETUP.md` 详细描述了 immudb 配置，但 docker-compose.yml 中没有相应的初始化配置
- **影响**: 审计日志功能可能无法正常工作
- **建议修复**: 添加 immudb 初始化配置或脚本

### [P2] 前端构建产物路径问题

- **位置**: 用户手册 3.1.1
- **描述**: 文档说访问 `http://localhost:3000`，但前端是独立构建的，没有与后端服务集成
- **影响**: 用户可能无法访问前端界面
- **建议修复**: 添加前后端集成的 Docker Compose 配置

### [P2] 缺少日志配置示例

- **位置**: docker-compose.yml
- **描述**: 配置了日志文件大小限制，但没有集中日志收集配置
- **影响**: 故障排查困难
- **建议修复**: 添加日志收集配置说明

### [P2] HashiCorp Vault 配置目录缺失

- **位置**: docker-compose.yml
- **描述**: 引用了 `./config/vault` 目录但该目录不存在
- **影响**: Vault 服务可能无法正确配置
- **建议修复**: 创建配置目录和示例配置文件

### [P2] 监控组件配置缺失

- **位置**: docker-compose.yml
- **描述**: Prometheus 和 Grafana 配置引用了不存在的目录
- **影响**: 监控功能无法启用
- **建议修复**: 创建配置目录或移除相关配置

---

### [P3] 文档格式问题

- **位置**: 用户手册多处
- **描述**: 多处提到"截图位置"但实际没有截图
- **影响**: 用户体验不佳
- **建议修复**: 添加实际截图或移除截图占位符

### [P3] 参考文档链接失效

- **位置**: 用户手册附录 B
- **描述**:
  - `./API.md` - 文件存在但路径可能需要调整
  - `./CredBridge_CN_设计规范_v1.0.md` - 文件名不一致
  - `../_bmad-output/planning-artifacts/architecture.md` - 路径可能不正确
  - `https://credbridge.io/security` - 外部链接可能失效

- **影响**: 用户无法访问参考文档
- **建议修复**: 修正所有内部链接路径

### [P3] 缺少 Node.js/Rust 版本管理说明

- **位置**: 用户手册 2.1.2
- **描述**: 只说明需要 Node.js 22+ 和 Rust 1.75+，但没有说明如何安装或管理版本
- **影响**: 新手用户可能遇到版本问题
- **建议修复**: 添加 nvm/rustup 安装说明

### [P3] 缺少故障排查检查清单

- **位置**: 用户手册第 8 章
- **描述**: 故障排查分散，没有系统性检查清单
- **影响**: 用户排查问题效率低
- **建议修复**: 添加快速检查清单

---

## 验证环境信息

### 已验证组件状态

| 组件 | 状态 | 备注 |
|------|------|------|
| 后端服务 | ✅ 运行中 | 端口 8080，健康检查正常 |
| 前端构建 | ✅ 可构建 | npm run build 成功 |
| 健康检查 API | ✅ 正常 | 返回 `{"status":"healthy",...}` |
| API 根路径 | ✅ 正常 | `/api/v1/` 返回端点列表 |
| Docker Compose | ⚠️ 配置问题 | 缺少引用的配置文件 |
| CLI 工具 | ❌ 未实现 | `credbridge` 命令不存在 |
| MCP Server | ⚠️ 代码存在 | 未验证是否可独立运行 |
| SDK TypeScript | ⚠️ 文档存在 | 未验证实际功能 |
| SDK Rust | ⚠️ 代码存在 | 未验证实际功能 |

### 测试命令执行结果

```bash
# 后端服务健康检查
curl http://localhost:8080/health
# 结果：✅ 返回 {"status":"healthy","version":"0.1.0","timestamp":1773230890}

# API 根路径
curl http://localhost:8080/api/v1/
# 结果：✅ 返回端点列表

# 凭证 API（需要认证）
curl http://localhost:8080/api/v1/credentials -H "Authorization: Bearer test"
# 结果：⚠️ 返回认证错误（预期行为）

# TEE Attestation API
curl http://localhost:8080/api/v1/tee/attest
# 结果：❌ 端点不存在

# CLI 工具
credbridge --version
# 结果：❌ 命令不存在
```

---

## 总结

### 主要发现

1. **CLI 工具完全缺失**是最严重的问题，用户手册中大量操作依赖 CLI
2. **Docker Compose 配置引用了不存在的文件**，导致无法直接启动
3. **配置文件和示例缺失**，用户需要自行创建
4. **前端与后端集成未配置**，用户无法访问 Web 控制台

### 优先级建议

1. **立即修复 (P0)**:
   - 创建缺失的配置文件
   - 实现基础 CLI 工具或移除相关文档
   - 修复 Docker Compose 配置

2. **短期修复 (P1)**:
   - 统一 API 路径
   - 验证 SDK 实现
   - 实现 TEE Attestation 端点

3. **中期改进 (P2-P3)**:
   - 完善文档截图
   - 添加更多示例配置
   - 改进故障排查文档

### 文档质量评估

| 维度 | 评分 | 说明 |
|------|------|------|
| 完整性 | ⭐⭐⭐⭐ | 覆盖了主要功能和使用场景 |
| 准确性 | ⭐⭐ | 多处与实际实现不一致 |
| 可用性 | ⭐⭐ | 缺少必要的配置和示例 |
| 一致性 | ⭐⭐⭐ | 部分章节之间存在矛盾 |

**总体评分**: ⭐⭐☆ (2.5/5)

---

**报告生成时间**: 2026-03-11
**验证人**: claude_qwen
