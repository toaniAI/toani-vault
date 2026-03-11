# CredBridge Docker 部署指南

本目录包含 CredBridge Vault Service 的 Docker 容器化配置。

## 目录结构

```
docker/
├── Dockerfile                  # Vault Service 多阶段构建镜像
├── docker-compose.yml          # 开发环境服务编排
├── docker-compose.prod.yml     # 生产环境配置
├── .dockerignore               # Docker 构建忽略文件
├── README.md                   # 本文件
├── scripts/
│   ├── init.sh                 # 初始化脚本
│   └── healthcheck.sh          # 健康检查脚本
└── config/                     # 配置文件目录（由 init.sh 生成）
    ├── vault/
    ├── postgres/
    ├── redis/
    ├── nginx/
    └── prometheus/
```

## 快速开始

### 1. 初始化环境

```bash
cd docker
chmod +x scripts/*.sh
./scripts/init.sh
```

这将生成：
- `.env` 文件（包含随机生成的密码）
- 必要的配置文件
- 目录结构

### 2. 启动服务

**开发环境：**
```bash
docker-compose up -d
```

**生产环境：**
```bash
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

### 3. 验证服务

```bash
# 查看服务状态
docker-compose ps

# 查看日志
docker-compose logs -f vault-service

# 健康检查
curl http://localhost:8080/health
```

## 服务清单

| 服务 | 端口 | 用途 |
|------|------|------|
| vault-service | 8080 | CredBridge 主服务 |
| postgres | 5432 | 主数据库 |
| redis | 6379 | 缓存与会话存储 |
| immudb | 3322 | 不可变审计日志 |
| vault | 8200 | HashiCorp Vault 密钥管理 |
| grafana | 3000 | 监控仪表盘（可选） |
| prometheus | 9090 | 指标收集（可选） |

## 环境变量

### 核心服务配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `VAULT_SERVICE_HOST` | 0.0.0.0 | 服务绑定地址 |
| `VAULT_SERVICE_PORT` | 8080 | 服务端口 |
| `VAULT_SERVICE_ENV` | development | 环境类型 |
| `RUST_LOG` | debug | 日志级别 |

### 数据库配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DATABASE_URL` | - | PostgreSQL 连接字符串 |
| `DB_USER` | credbridge | 数据库用户名 |
| `DB_PASSWORD` | - | 数据库密码 |
| `DB_NAME` | credbridge | 数据库名称 |

### Redis 配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `REDIS_URL` | - | Redis 连接字符串 |
| `REDIS_HOST` | redis | Redis 主机 |
| `REDIS_PORT` | 6379 | Redis 端口 |
| `REDIS_PASSWORD` | - | Redis 密码 |

### immudb 配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `IMMUDB_HOST` | immudb | immudb 主机 |
| `IMMUDB_PORT` | 3322 | immudb 端口 |
| `IMMUDB_DATABASE` | credbridge_audit | 审计数据库名 |
| `IMMUDB_USERNAME` | credbridge | immudb 用户名 |
| `IMMUDB_PASSWORD` | - | immudb 密码 |

### Vault 配置

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `VAULT_ADDR` | http://vault:8200 | Vault 地址 |
| `VAULT_TOKEN` | - | Vault Root Token |

## 常用命令

### 启动与停止

```bash
# 启动所有服务
docker-compose up -d

# 启动指定服务
docker-compose up -d vault-service postgres

# 停止所有服务
docker-compose down

# 停止并删除数据卷（谨慎使用！）
docker-compose down -v
```

### 日志管理

```bash
# 查看所有服务日志
docker-compose logs

# 查看指定服务日志
docker-compose logs vault-service

# 实时跟踪日志
docker-compose logs -f vault-service

# 查看最近 100 行日志
docker-compose logs --tail=100 vault-service
```

### 服务管理

```bash
# 重启服务
docker-compose restart vault-service

# 重建并启动
docker-compose up -d --build vault-service

# 扩展服务副本（仅支持 Swarm 模式）
docker-compose up -d --scale vault-service=3
```

### 数据管理

```bash
# 备份 PostgreSQL 数据
docker-compose exec postgres pg_dump -U credbridge credbridge > backup.sql

# 恢复 PostgreSQL 数据
cat backup.sql | docker-compose exec -T postgres psql -U credbridge credbridge

# 备份 Redis 数据
docker-compose exec redis redis-cli BGSAVE

# 查看 immudb 状态
docker-compose exec immudb /app/immudb version
```

## 安全配置

### 开发环境

开发环境使用默认密码和宽松的安全策略。请勿在生产环境使用！

### 生产环境

生产环境配置包含以下安全增强：

1. **只读根文件系统**
2. **非特权用户运行**
3. **资源限制**（CPU、内存）
4. **健康检查**
5. **安全头部**
6. **TLS/SSL 加密**
7. **Docker Secrets 密码管理**

### 密码管理

建议使用 Docker Secrets 管理敏感信息：

```bash
# 创建密码 secret
echo "my_secure_password" | docker secret create db_password -

# 在 docker-compose.prod.yml 中使用
secrets:
  - db_password
```

## 监控与日志

### 启用监控（可选）

```bash
# 启动包含监控的服务
docker-compose --profile monitoring up -d
```

访问地址：
- Grafana: http://localhost:3000 (admin/admin)
- Prometheus: http://localhost:9090

### 日志收集

默认使用 `json-file` 日志驱动，配置日志轮转：

```yaml
logging:
  driver: "json-file"
  options:
    max-size: "100m"
    max-file: "3"
```

## 故障排查

### 服务无法启动

```bash
# 检查端口占用
sudo lsof -i :8080

# 查看详细日志
docker-compose logs --no-color vault-service

# 检查容器状态
docker-compose ps
```

### 数据库连接失败

```bash
# 检查 PostgreSQL 是否就绪
docker-compose exec postgres pg_isready -U credbridge

# 检查网络连接
docker-compose exec vault-service nc -zv postgres 5432
```

### 内存不足

```bash
# 检查容器内存使用
docker stats

# 调整资源限制（docker-compose.prod.yml）
deploy:
  resources:
    limits:
      memory: 2G
```

## 升级指南

### 升级 Vault Service

```bash
# 拉取最新镜像
docker-compose pull vault-service

# 重启服务
docker-compose up -d vault-service
```

### 升级依赖服务

```bash
# 停止服务
docker-compose down

# 更新镜像版本（在 docker-compose.yml 中修改）
# 然后重新启动
docker-compose up -d
```

## 开发与调试

### 本地开发

```bash
# 使用本地代码构建
docker-compose up -d --build

# 进入容器调试
docker-compose exec vault-service sh

# 运行测试
docker-compose exec vault-service cargo test
```

### 调试模式

```bash
# 启用调试日志
RUST_LOG=debug docker-compose up -d

# 查看调试日志
docker-compose logs -f vault-service
```

## 许可证

Copyright (c) 2026 CredBridge. All rights reserved.
