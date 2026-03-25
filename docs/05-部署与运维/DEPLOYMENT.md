# CredBridge 部署文档

本文档详细介绍 CredBridge 系统的各种部署方式，包括 Docker Compose、Kubernetes 以及生产环境最佳实践。

## 目录

1. [系统要求](#系统要求)
2. [Docker Compose 部署](#docker-compose-部署)
3. [生产环境部署](#生产环境部署)
4. [Kubernetes 部署](#kubernetes-部署)
5. [安全配置](#安全配置)
6. [监控与告警](#监控与告警)
7. [备份与恢复](#备份与恢复)
8. [故障排查](#故障排查)

---

## 系统要求

### 最低配置

| 组件 | CPU | 内存 | 存储 | 网络 |
|------|-----|------|------|------|
| Vault Service | 1核 | 1GB | 10GB | 100Mbps |
| PostgreSQL | 1核 | 2GB | 50GB | 100Mbps |
| Redis | 0.5核 | 512MB | 5GB | 100Mbps |
| immudb | 1核 | 2GB | 100GB | 100Mbps |
| HashiCorp Vault | 0.5核 | 512MB | 5GB | 100Mbps |

**总计：4核 CPU，6GB 内存，170GB 存储**

### 推荐配置

| 组件 | CPU | 内存 | 存储 | 说明 |
|------|-----|------|------|------|
| Vault Service | 2核 | 2GB | 20GB | 支持水平扩展 |
| PostgreSQL | 2核 | 4GB | 200GB | SSD 存储，主从复制 |
| Redis | 1核 | 1GB | 10GB | 主从 + Sentinel |
| immudb | 2核 | 4GB | 500GB | SSD 存储 |
| HashiCorp Vault | 1核 | 1GB | 10GB | 集群模式 |

**总计：8核 CPU，12GB 内存，740GB SSD 存储**

### 软件要求

- Docker 24.0+
- Docker Compose 2.20+
- Linux 内核 5.10+（推荐 Ubuntu 22.04 LTS / Debian 12）
- OpenSSL 3.0+

---

## Docker Compose 部署

### 快速开始

#### 1. 克隆代码仓库

```bash
git clone https://github.com/credbridge/vault-service.git
cd vault-service
```

#### 2. 初始化环境

```bash
cd docker
chmod +x scripts/*.sh
./scripts/init.sh
```

初始化脚本会：
- 检查 Docker 环境
- 生成随机密码
- 创建必要的目录结构
- 生成配置文件

#### 3. 启动服务

```bash
docker-compose up -d
```

#### 4. 验证部署

```bash
# 查看服务状态
docker-compose ps

# 等待所有服务就绪
./scripts/wait-for-services.sh

# 测试健康检查端点
curl http://localhost:8080/health
```

### 开发环境配置

开发环境使用 `docker-compose.yml`，包含以下特性：

- **自动端口映射**：所有服务映射到本地端口
- **热重载**：代码修改自动重建
- **调试日志**：RUST_LOG=debug 详细日志
- **宽松安全策略**：便于开发和测试

#### 服务端口

| 服务 | 端口 | 访问地址 |
|------|------|----------|
| Vault Service | 8080 | http://localhost:8080 |
| PostgreSQL | 5432 | localhost:5432 |
| Redis | 6379 | localhost:6379 |
| immudb | 3322 | localhost:3322 |
| HashiCorp Vault | 8200 | http://localhost:8200 |

### 环境变量配置

编辑 `.env` 文件自定义配置：

```bash
# 服务配置
VAULT_SERVICE_PORT=8080
VAULT_SERVICE_ENV=development
RUST_LOG=debug

# 数据库密码（自动生成的随机密码）
DB_PASSWORD=xxx
REDIS_PASSWORD=xxx
IMMUDB_PASSWORD=xxx
VAULT_TOKEN=xxx
```

---

## 生产环境部署

### 部署前准备

#### 1. 准备服务器

```bash
# 更新系统
sudo apt update && sudo apt upgrade -y

# 安装 Docker
sudo apt install -y docker.io docker-compose-plugin

# 创建数据目录
sudo mkdir -p /var/lib/credbridge/{postgres,redis,immudb,vault}
sudo chown -R 1000:1000 /var/lib/credbridge
```

#### 2. 配置防火墙

```bash
# 仅开放必要端口
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow ssh
sudo ufw allow 80/tcp   # HTTP
sudo ufw allow 443/tcp  # HTTPS
sudo ufw enable
```

#### 3. 配置 Docker Secrets

```bash
# 生成强密码
openssl rand -base64 32 | docker secret create db_password -
openssl rand -base64 32 | docker secret create db_root_password -
openssl rand -base64 32 | docker secret create redis_password -
openssl rand -base64 32 | docker secret create vault_token -
```

### 生产环境部署步骤

#### 1. 使用生产配置启动

```bash
cd docker

# 设置环境变量
export COMPOSE_FILE=docker-compose.yml:docker-compose.prod.yml
export ENVIRONMENT=production

# 初始化生产环境
./scripts/init.sh --environment production --data-dir /var/lib/credbridge

# 启动服务
docker-compose up -d
```

#### 2. 验证部署

```bash
# 检查服务状态
docker-compose ps

# 检查资源使用
docker stats --no-stream

# 检查健康状态
docker-compose exec vault-service /app/healthcheck.sh
```

### 生产环境特性

生产环境配置（`docker-compose.prod.yml`）包含以下安全增强：

#### 1. 容器安全

- **非特权用户**：服务以 credbridge 用户运行（UID 1000）
- **只读根文件系统**：防止运行时修改容器
- **资源限制**：CPU 和内存硬限制
- **安全选项**：no-new-privileges 等

#### 2. 网络安全

- **内部网络隔离**：服务间通信使用内部网络
- **端口限制**：仅开放必要端口到宿主机
- **TLS 加密**：Vault 和 API 使用 TLS

#### 3. 数据安全

- **持久化卷**：数据存储在宿主机
- **定期备份**：自动备份脚本
- **加密存储**：敏感数据加密存储

#### 4. 高可用性

- **健康检查**：定期健康检查和自动重启
- **滚动更新**：零停机部署
- **副本配置**：支持多副本运行

### SSL/TLS 配置

#### 使用 Let's Encrypt

```bash
# 启动包含 certbot 的服务
docker-compose --profile with-ssl up -d
```

#### 使用自签名证书

```bash
# 生成证书
mkdir -p docker/config/nginx/ssl
openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout docker/config/nginx/ssl/server.key \
  -out docker/config/nginx/ssl/server.crt \
  -subj "/CN=your-domain.com"
```

---

## Kubernetes 部署

### 部署架构

```
┌─────────────────────────────────────────────────────────────┐
│                        Kubernetes Cluster                    │
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ Vault Svc   │  │ Vault Svc   │  │ Vault Svc   │          │
│  │   Pod 1     │  │   Pod 2     │  │   Pod 3     │          │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘          │
│         └─────────────────┴─────────────────┘                │
│                           │                                  │
│                    ┌──────┴──────┐                          │
│                    │   Service   │                          │
│                    │   (LB)      │                          │
│                    └──────┬──────┘                          │
│                           │                                  │
│  ┌────────────────────────┼────────────────────────┐         │
│  │                        │                        │         │
│  ▼                        ▼                        ▼         │
│ ┌──────────┐        ┌──────────┐          ┌──────────┐      │
│ │PostgreSQL│        │  Redis   │          │ immudb   │      │
│ │ Stateful │        │ Cluster  │          │StatefulSe│      │
│ │   Set    │        │          │          │   t      │      │
│ └──────────┘        └──────────┘          └──────────┘      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 部署文件

Kubernetes 部署文件位于 `k8s/` 目录（需要单独创建）：

```bash
mkdir -p k8s/{base,overlays/{development,production}}
```

#### Namespace

```yaml
# k8s/base/namespace.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: credbridge
  labels:
    name: credbridge
    environment: production
```

#### ConfigMap

```yaml
# k8s/base/configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: vault-service-config
  namespace: credbridge
data:
  VAULT_SERVICE_PORT: "8080"
  VAULT_SERVICE_ENV: "production"
  RUST_LOG: "info"
  DATABASE_URL: "postgresql://credbridge@postgres:5432/credbridge"
  REDIS_URL: "redis://redis:6379/0"
  IMMUDB_HOST: "immudb"
  VAULT_ADDR: "http://vault:8200"
```

#### Secret

```yaml
# k8s/base/secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: vault-service-secrets
  namespace: credbridge
type: Opaque
stringData:
  DB_PASSWORD: "<base64-encoded-password>"
  REDIS_PASSWORD: "<base64-encoded-password>"
  VAULT_TOKEN: "<base64-encoded-token>"
```

#### Deployment

```yaml
# k8s/base/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: vault-service
  namespace: credbridge
  labels:
    app: vault-service
spec:
  replicas: 3
  selector:
    matchLabels:
      app: vault-service
  template:
    metadata:
      labels:
        app: vault-service
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
      containers:
      - name: vault-service
        image: credbridge/vault-service:latest
        imagePullPolicy: Always
        ports:
        - containerPort: 8080
          name: http
        envFrom:
        - configMapRef:
            name: vault-service-config
        - secretRef:
            name: vault-service-secrets
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          capabilities:
            drop:
            - ALL
        resources:
          requests:
            memory: "1Gi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
        livenessProbe:
          exec:
            command:
            - /app/healthcheck.sh
          initialDelaySeconds: 60
          periodSeconds: 30
          timeoutSeconds: 10
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5
          timeoutSeconds: 3
          failureThreshold: 3
        volumeMounts:
        - name: tmp
          mountPath: /tmp
        - name: data
          mountPath: /app/data
      volumes:
      - name: tmp
        emptyDir: {}
      - name: data
        persistentVolumeClaim:
          claimName: vault-service-data
```

#### Service

```yaml
# k8s/base/service.yaml
apiVersion: v1
kind: Service
metadata:
  name: vault-service
  namespace: credbridge
spec:
  type: ClusterIP
  ports:
  - port: 8080
    targetPort: 8080
    protocol: TCP
    name: http
  selector:
    app: vault-service
```

#### Ingress

```yaml
# k8s/base/ingress.yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: vault-service
  namespace: credbridge
  annotations:
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    nginx.ingress.kubernetes.io/proxy-body-size: "10m"
    cert-manager.io/cluster-issuer: "letsencrypt"
spec:
  tls:
  - hosts:
    - api.credbridge.io
    secretName: vault-service-tls
  rules:
  - host: api.credbridge.io
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: vault-service
            port:
              number: 8080
```

### Kustomize 配置

```yaml
# k8s/overlays/production/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: credbridge

resources:
- ../../base

replicas:
- name: vault-service
  count: 5

images:
- name: credbridge/vault-service
  newTag: v1.2.3

patchesStrategicMerge:
- deployment-patch.yaml

configMapGenerator:
- name: vault-service-config
  behavior: merge
  literals:
  - VAULT_SERVICE_ENV=production
  - RUST_LOG=info
```

### 部署命令

```bash
# 部署到开发环境
kubectl apply -k k8s/overlays/development

# 部署到生产环境
kubectl apply -k k8s/overlays/production

# 查看部署状态
kubectl get pods -n credbridge
kubectl get svc -n credbridge
kubectl get ingress -n credbridge
```

---

## 安全配置

### 1. 密钥管理

#### HashiCorp Vault 集成

```bash
# 初始化 Vault
docker-compose exec vault vault operator init

# 解封 Vault
docker-compose exec vault vault operator unseal <unseal-key>

# 启用密钥引擎
docker-compose exec vault vault secrets enable -path=credbridge kv-v2
```

#### 自动解封（开发环境）

生产环境建议使用自动解封机制：
- AWS KMS
- Azure Key Vault
- GCP Cloud KMS
- Kubernetes 自动解封

### 2. 网络隔离

#### Docker 网络

```yaml
networks:
  credbridge_internal:
    internal: true  # 无外部访问
  credbridge_external:
    driver: bridge
```

#### Kubernetes 网络策略

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: vault-service-policy
  namespace: credbridge
spec:
  podSelector:
    matchLabels:
      app: vault-service
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - namespaceSelector:
        matchLabels:
          name: ingress-nginx
    ports:
    - protocol: TCP
      port: 8080
  egress:
  - to:
    - podSelector:
        matchLabels:
          app: postgres
    ports:
    - protocol: TCP
      port: 5432
```

### 3. 审计日志

审计日志配置在 `docker-compose.yml`：

```yaml
environment:
  - AUDIT_ENABLED=true
  - AUDIT_LEVEL=info
  - AUDIT_OUTPUT=immudb
```

### 4. 安全扫描

```bash
# 镜像安全扫描
docker scan credbridge/vault-service:latest

# 或 Trivy
trivy image credbridge/vault-service:latest
```

---

## 监控与告警

### Prometheus 配置

```yaml
# docker/config/prometheus/prometheus.yml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'vault-service'
    static_configs:
      - targets: ['vault-service:8080']
    metrics_path: '/metrics'
```

### Grafana 仪表盘

导入预配置的仪表盘：
- Vault Service 概览
- 数据库性能
- Redis 监控
- 系统资源

### 告警规则

```yaml
# Prometheus 告警规则
groups:
- name: vault-service
  rules:
  - alert: HighErrorRate
    expr: rate(http_requests_total{status=~"5.."}[5m]) > 0.1
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "High error rate detected"
```

---

## 备份与恢复

### PostgreSQL 备份

```bash
# 自动备份脚本
#!/bin/bash
BACKUP_DIR="/backup/postgres/$(date +%Y%m%d)"
mkdir -p "$BACKUP_DIR"

docker-compose exec -T postgres pg_dump \
  -U credbridge credbridge \
  | gzip > "$BACKUP_DIR/credbridge-$(date +%H%M%S).sql.gz"

# 保留最近 7 天备份
find /backup/postgres -type d -mtime +7 -exec rm -rf {} +
```

### immudb 备份

```bash
# immudb 支持增量备份
docker-compose exec immudb /app/immudb backup \
  --database=credbridge_audit \
  --target=/backup/immudb/$(date +%Y%m%d)
```

### 完全恢复

```bash
# 停止服务
docker-compose down

# 恢复数据卷（从备份）
# ... 恢复操作 ...

# 重新启动
docker-compose up -d
```

---

## 故障排查

### 常见问题

#### 1. 服务无法启动

```bash
# 检查日志
docker-compose logs vault-service

# 检查端口占用
sudo lsof -i :8080

# 检查磁盘空间
df -h
```

#### 2. 数据库连接失败

```bash
# 测试数据库连接
docker-compose exec vault-service nc -zv postgres 5432

# 检查 PostgreSQL 状态
docker-compose exec postgres pg_isready
```

#### 3. 内存不足

```bash
# 查看内存使用
docker stats --no-stream

# 调整资源限制
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

### 调试模式

```bash
# 启用详细日志
export RUST_LOG=debug
docker-compose up -d

# 进入容器调试
docker-compose exec vault-service sh
```

---

## 参考文档

- [Docker Compose 官方文档](https://docs.docker.com/compose/)
- [HashiCorp Vault 部署指南](https://learn.hashicorp.com/vault)
- [PostgreSQL Docker 镜像](https://hub.docker.com/_/postgres)
- [Redis Docker 镜像](https://hub.docker.com/_/redis)
- [immudb 文档](https://docs.immudb.io/)

---

## 支持

如有问题，请联系：
- 邮箱：support@credbridge.io
- 工单：https://support.credbridge.io
- 社区论坛：https://community.credbridge.io
