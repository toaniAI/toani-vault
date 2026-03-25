# Phase 2 部署检查清单

**版本**: v1.0.0-phase2
**更新日期**: 2026-03-12

---

## 部署前检查

### 系统要求

| 组件 | 最低配置 | 推荐配置 |
|------|----------|----------|
| CPU | 4 核 | 8 核 |
| 内存 | 6 GB | 12 GB |
| 存储 | 170 GB | 740 GB SSD |
| Docker | 24.0+ | 24.0+ |
| Docker Compose | 2.20+ | 2.20+ |

### 必需服务

- [x] PostgreSQL 15+
- [x] Redis 7+
- [x] immudb (审计日志)
- [x] HashiCorp Vault (密钥管理)

---

## 新增配置项

### Phase 2 必需配置

```bash
# .env 新增配置

# MCP Server (EP6)
MCP_ENABLED=true
MCP_SSE_PATH=/sse
MCP_MESSAGE_PATH=/message

# RLS 策略 (EP7)
RLS_ENABLED=true

# Token 黑名单 (安全修复 H-001)
REDIS_URL=redis://redis:6379/0
TOKEN_BLACKLIST_TTL=900  # 15 分钟

# 密钥版本控制 (安全修复 H-004)
KEY_ROTATION_INTERVAL_DAYS=90
```

### Redis 连接配置

Phase 2 要求 Redis 用于 Token 黑名单存储：

```yaml
# docker-compose.yml
services:
  redis:
    image: redis:7-alpine
    command: redis-server --requirepass ${REDIS_PASSWORD}
    volumes:
      - redis_data:/data
    networks:
      - credbridge_internal
```

---

## 部署步骤

### 1. 备份现有数据

```bash
# PostgreSQL 备份
docker-compose exec -T postgres pg_dump -U credbridge credbridge | gzip > backup_$(date +%Y%m%d).sql.gz

# immudb 备份
docker-compose exec immudb /app/immudb backup --target=/backup/$(date +%Y%m%d)
```

### 2. 拉取 Phase 2 代码

```bash
git fetch origin
git checkout v1.0.0-phase2
```

### 3. 更新配置文件

```bash
# 复制新的环境变量模板
cp .env.example .env

# 编辑配置
nano .env
```

### 4. 运行数据库迁移

```bash
# 执行 RLS 迁移
docker-compose exec postgres psql -U credbridge -d credbridge -f /migrations/init-rls.sql
```

### 5. 重启服务

```bash
# 停止服务
docker-compose down

# 启动服务
docker-compose up -d

# 查看日志
docker-compose logs -f vault-service
```

### 6. 验证部署

```bash
# 健康检查
curl http://localhost:8080/health

# 详细健康检查
curl http://localhost:8080/health/detail

# 验证 MCP SSE 端点
curl -N http://localhost:8080/sse -H "Authorization: Bearer YOUR_TOKEN"
```

---

## 功能验证

### EP5: SDK 测试

**TypeScript SDK**:
```bash
cd sdk-typescript
npm install
npm test
```

**Rust SDK**:
```bash
cd sdk-rust
cargo test
```

### EP6: MCP Server 测试

```bash
# 测试 SSE 连接
curl -N -H "Authorization: Bearer $TOKEN" http://localhost:8080/sse

# 测试 MCP Tools
# 使用 MCP 客户端连接测试
```

### EP7: RLS 策略验证

```sql
-- 验证 RLS 已启用
SELECT schemaname, tablename, rowsecurity
FROM pg_tables
WHERE tablename IN ('credentials', 'audit_logs');

-- 测试租户隔离
SET app.current_tenant = 'tenant_123';
SELECT * FROM credentials;  -- 应仅返回 tenant_123 的数据
```

### EP9: 监控验证

```bash
# Prometheus 指标
curl http://localhost:8080/metrics

# Grafana 访问
open http://localhost:3000
```

---

## 回滚计划

如果部署失败，执行以下步骤回滚：

```bash
# 1. 停止服务
docker-compose down

# 2. 切换到上一版本
git checkout v1.0.0-phase1

# 3. 恢复数据库
gunzip < backup_YYYYMMDD.sql.gz | docker-compose exec -T postgres psql -U credbridge credbridge

# 4. 重启服务
docker-compose up -d
```

---

## 常见问题

### Q: Redis 连接失败

```bash
# 检查 Redis 状态
docker-compose exec redis redis-cli ping

# 检查网络
docker-compose exec vault-service nc -zv redis 6379
```

### Q: RLS 策略未生效

```sql
-- 检查 RLS 是否启用
SELECT * FROM pg_policies WHERE tablename = 'credentials';

-- 手动启用
ALTER TABLE credentials ENABLE ROW LEVEL SECURITY;
```

### Q: MCP SSE 连接断开

检查 Token 是否有效：
```bash
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/credentials
```

---

## 监控指标

### 新增 Prometheus 指标

| 指标 | 描述 | 告警阈值 |
|------|------|----------|
| `credbridge_token_blacklist_hits` | Token 黑名单命中次数 | > 100/min |
| `credbridge_key_version` | 当前密钥版本 | 版本变化告警 |
| `credbridge_rls_denied` | RLS 拒绝访问次数 | > 10/min |
| `credbridge_mcp_connections` | MCP 活跃连接数 | > 100 |

### Grafana 仪表盘

导入以下仪表盘：
- Vault Service Overview (ID: credbridge-overview)
- Tenant Metrics (ID: credbridge-tenant)
- Security Events (ID: credbridge-security)

---

## 联系支持

如有部署问题，请联系：
- 邮箱: support@credbridge.io
- 工单: https://support.credbridge.io