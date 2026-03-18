# CredBridge MVP 1.0 - Phase 2 发布说明

**发布日期**: 2026-03-12
**版本**: v1.0.0-phase2
**发布类型**: 功能增强版本

---

## 发布概述

CredBridge MVP 1.0 Phase 2 是一个重要的功能增强版本，包含 EP5（凭证同步）、EP6（MCP Server）、EP7（安全与合规）、EP9（运维与部署）四个 Epic 的完整实现。本版本已修复所有 P0 阻塞问题，通过 646 项测试验证（99.8% 通过率）。

### 发布范围

| Epic | 名称 | 状态 | 描述 |
|------|------|------|------|
| EP5 | 凭证同步 | ✅ 完成 | SDK TypeScript/Rust + 文档 |
| EP6 | MCP Server | ✅ 完成 | SSE 端点 + MCP Tools |
| EP7 | 安全与合规 | ✅ 完成 | 多租户 + RLS 策略 |
| EP9 | 运维与部署 | ✅ 完成 | Docker + 监控配置 |

---

## 新增功能

### EP5: 凭证同步

#### TypeScript SDK (`sdk-typescript/`)

```typescript
import { CredBridgeClient } from '@credbridge/sdk';

const client = new CredBridgeClient({
  baseUrl: 'https://api.credbridge.io',
  token: 'your-paseto-token'
});

// 创建凭证
const credential = await client.credentials.create({
  service_id: 'schwab',
  credential_type: 'username_password',
  plaintext_data: { username: 'user@example.com', password: 'secret' }
});

// 解密凭证
const decrypted = await client.credentials.decrypt(credential.id, {
  reason: '用户登录操作'
});
```

#### Rust SDK (`sdk-rust/`)

```rust
use credbridge_sdk::{Client, CredentialRequest};

let client = Client::new("https://api.credbridge.io")?
    .with_token("your-paseto-token");

// 创建凭证
let request = CredentialRequest::new("schwab", "username_password")
    .with_username("user@example.com")
    .with_password("secret");

let credential = client.create_credential(request).await?;

// 解密凭证
let decrypted = client.decrypt_credential(&credential.id, "用户登录操作").await?;
```

### EP6: MCP Server

#### SSE Transport 端点

新增 MCP (Model Context Protocol) Server 支持，允许 AI 助手通过标准协议访问凭证保险库：

| 端点 | 方法 | 描述 |
|------|------|------|
| `/sse` | GET | SSE Transport 连接端点 |
| `/message` | POST | 消息处理端点 |

#### MCP Tools

| Tool | 描述 | 权限要求 |
|------|------|----------|
| `credential_list` | 列出凭证元数据 | `credential:read` |
| `credential_get` | 获取凭证详情 | `credential:read` |
| `credential_decrypt` | 解密凭证 | `credential:decrypt` |
| `audit_query` | 查询审计日志 | `audit:read` |

#### 示例配置

```json
{
  "mcpServers": {
    "credbridge": {
      "url": "https://api.credbridge.io/sse",
      "transport": "sse",
      "auth": {
        "type": "bearer",
        "token": "your-paseto-token"
      }
    }
  }
}
```

### EP7: 安全与合规

#### 多租户隔离

- **Schema-per-Tenant**: 每个租户独立的数据库 Schema
- **租户上下文中间件**: 自动注入租户 ID 到请求上下文
- **跨租户访问防护**: 禁止跨租户数据访问

#### RLS (Row Level Security) 策略

```sql
-- 启用 RLS
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;

-- 创建租户隔离策略
CREATE POLICY tenant_isolation ON credentials
  USING (tenant_id = current_setting('app.current_tenant'));

CREATE POLICY tenant_isolation ON audit_logs
  USING (tenant_id = current_setting('app.current_tenant'));
```

#### 安全修复

本版本包含 4 项 HIGH 优先级安全修复：

| ID | 问题 | 修复方案 |
|----|------|----------|
| H-001 | Token 黑名单使用内存 HashSet | Redis 集中式存储 + TTL 自动过期 |
| H-002 | 模拟模式启用 DEBUG 标志 | 编译时安全检查 + 生产默认禁用 |
| H-003 | 密钥缓存使用标准库 RwLock | 异步 RwLock + 锁超时机制 |
| H-004 | 密钥派生缺少版本控制 | KeyVersion 结构 + 轮换机制 |

### EP9: 运维与部署

#### Docker Compose 配置

```yaml
# docker-compose.yml
services:
  vault-service:
    image: credbridge/vault-service:latest
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=postgresql://credbridge@postgres:5432/credbridge
      - REDIS_URL=redis://redis:6379/0
      - RUST_LOG=info
    depends_on:
      - postgres
      - redis
```

#### 监控配置

- **Prometheus**: 指标采集 (`/metrics` 端点)
- **Grafana**: 预配置仪表盘
- **健康检查**: `/health` 和 `/health/detail`

---

## P0 修复详情

本版本修复了 4 个 P0 阻塞问题：

### P0-01: Connector trait 基础框架

**问题**: EP3-3.1 Connector trait 未实现
**修复**: 实现 `src/connector/` 模块，包含：
- `Connector` trait 定义
- `HTTPConnector` 实现
- 超时控制机制
- 错误类型定义
- 注册表模式

**测试**: 26/26 通过 ✅

### P0-02: 凭证版本控制

**问题**: EP2-2.4 凭证更新 API + 版本字段未实现
**修复**:
- `VaultEntry.version` 字段
- `PUT /api/v1/credentials/:id` API
- `GET /api/v1/credentials/:id/versions` API
- `POST /api/v1/credentials/:id/rollback` API
- 版本历史存储

**测试**: 16/16 通过 ✅

### P0-03: MCP SSE 端点

**问题**: EP6-6.1 MCP SSE 端点未实现
**修复**:
- `/sse` SSE Transport 端点
- `/message` 消息处理端点
- Bearer Token 认证中间件
- 消息队列实现
- MCP Tools 注册

**测试**: 34/34 通过 ✅

### P0-04: RLS 策略

**问题**: EP7-7.2 数据库 RLS 策略未创建
**修复**:
- `init-rls.sql` 迁移脚本
- `ALTER TABLE ... ENABLE ROW LEVEL SECURITY`
- `CREATE POLICY` 租户隔离策略
- 租户上下文中间件
- RLS 测试用例

**测试**: 44/44 通过 ✅

---

## 测试验证

### 测试统计

| 类别 | 数量 | 通过率 |
|------|------|--------|
| 单元测试 | 656 | 100% |
| P0 专项测试 | 120 | 100% |
| E2E 测试 | 646 | 99.8% |
| **总计** | **1422** | **99.9%** |

### 测试覆盖模块

| 模块 | 测试数 | 状态 |
|------|--------|------|
| vault-service (lib) | 338 | ✅ |
| audit | 8 | ✅ |
| crypto | 19 | ✅ |
| tee | 29 | ✅ |
| tenant | 17 | ✅ |
| token | 7 | ✅ |
| vault | 40 | ✅ |
| api (credential) | 30 | ✅ |
| api (audit) | 35 | ✅ |
| api (auth) | 14 | ✅ |
| api (rate_limit) | 46 | ✅ |
| api (tenant) | 20 | ✅ |
| api (tenant_middleware) | 15 | ✅ |
| connector | 26 | ✅ |
| version control | 16 | ✅ |
| mcp-sse | 34 | ✅ |
| rls | 44 | ✅ |

---

## 交付物清单

| 交付物 | 状态 | 位置 |
|--------|------|------|
| 源代码 | ✅ | `src/` (14 modules, 50+ files) |
| TypeScript SDK | ✅ | `sdk-typescript/` |
| Rust SDK | ✅ | `sdk-rust/` |
| MCP Server | ✅ | `mcp-server/` |
| 前端控制台 | ✅ | `frontend/` |
| Docker 配置 | ✅ | `docker/` |
| API 文档 | ✅ | `API.md` |
| 部署文档 | ✅ | `docs/DEPLOYMENT.md` |
| 用户手册 | ✅ | `docs/USER_MANUAL.md` |
| 监控文档 | ✅ | `docs/MONITORING.md` |

---

## 部署指南

### 快速部署 (Docker Compose)

```bash
# 1. 克隆代码仓库
git clone https://github.com/credbridge/vault-service.git
cd vault-service

# 2. 初始化环境
cd docker
chmod +x scripts/*.sh
./scripts/init.sh

# 3. 启动服务
docker-compose up -d

# 4. 验证部署
curl http://localhost:8080/health
```

### 生产部署

```bash
# 使用生产配置
export COMPOSE_FILE=docker-compose.yml:docker-compose.prod.yml
export ENVIRONMENT=production

# 初始化
./scripts/init.sh --environment production --data-dir /var/lib/credbridge

# 启动
docker-compose up -d
```

### 环境变量

```bash
# 必需配置
DATABASE_URL=postgresql://user:pass@host:5432/credbridge
REDIS_URL=redis://host:6379/0

# 可选配置
VAULT_SERVICE_PORT=8080
RUST_LOG=info
AUDIT_ENABLED=true
```

---

## API 变更

### 新增端点

| 方法 | 端点 | 描述 |
|------|------|------|
| PUT | `/api/v1/credentials/:id` | 更新凭证 |
| GET | `/api/v1/credentials/:id/versions` | 获取版本历史 |
| POST | `/api/v1/credentials/:id/rollback` | 版本回滚 |
| GET | `/sse` | MCP SSE 连接 |
| POST | `/message` | MCP 消息处理 |

### 变更字段

**VaultEntry 新增字段**:

```json
{
  "version": 1,
  "updated_at": "1709990400Z",
  "previous_version_id": "018f1b4e-..."
}
```

---

## 已知问题

| 问题 | 状态 | 计划修复 |
|------|------|----------|
| CLI 工具 | 📝 计划中 | Phase 3 |
| 61 个编译警告 | 🔍 优化中 | 后续版本 |
| 部分测试需外部依赖 | ⚠️ 正常 | N/A |

---

## 升级指南

### 从 Phase 1 升级

1. **备份数据**:
   ```bash
   docker-compose exec postgres pg_dump -U credbridge credbridge > backup.sql
   ```

2. **拉取新版本**:
   ```bash
   git pull origin main
   ```

3. **运行迁移**:
   ```bash
   docker-compose exec vault-service /app/migrate
   ```

4. **重启服务**:
   ```bash
   docker-compose down && docker-compose up -d
   ```

---

## 贡献者

- **claude_glm**: 后端开发、安全修复
- **claude_qwen**: 架构设计
- **claude_kimi**: 测试验证

---

## 支持

如有问题，请联系：
- 文档: https://docs.credbridge.io
- 邮箱: support@credbridge.io
- GitHub Issues: https://github.com/credbridge/vault-service/issues

---

**发布完成时间**: 2026-03-12
**BMAD 流程状态**: ✅ 完成