#!/bin/bash
#
# CredBridge Docker 初始化脚本
# 用于初始化 Docker 环境、创建必要目录和配置文件
#

set -euo pipefail

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 脚本目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(dirname "$DOCKER_DIR")"

# 默认配置
ENVIRONMENT="${ENVIRONMENT:-development}"
DATA_DIR="${DATA_DIR:-/var/lib/credbridge}"

# =============================================================================
# 日志函数
# =============================================================================
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# =============================================================================
# 检查依赖
# =============================================================================
check_dependencies() {
    log_info "检查依赖..."

    local deps=("docker" "docker-compose")
    for dep in "${deps[@]}"; do
        if ! command -v "$dep" &>/dev/null; then
            log_error "$dep 未安装"
            exit 1
        fi
    done

    # 检查 Docker 守护进程
    if ! docker info &>/dev/null; then
        log_error "Docker 守护进程未运行"
        exit 1
    fi

    log_success "所有依赖已就绪"
}

# =============================================================================
# 创建目录结构
# =============================================================================
create_directories() {
    log_info "创建目录结构..."

    local dirs=(
        "$DOCKER_DIR/config/vault"
        "$DOCKER_DIR/config/vault-agent"
        "$DOCKER_DIR/config/postgres"
        "$DOCKER_DIR/config/redis"
        "$DOCKER_DIR/config/immudb"
        "$DOCKER_DIR/config/nginx/conf.d"
        "$DOCKER_DIR/config/nginx/ssl"
        "$DOCKER_DIR/config/prometheus"
        "$DOCKER_DIR/config/grafana"
        "$DOCKER_DIR/data/certbot/conf"
        "$DOCKER_DIR/data/certbot/www"
    )

    for dir in "${dirs[@]}"; do
        mkdir -p "$dir"
        log_info "创建目录: $dir"
    done

    log_success "目录结构创建完成"
}

# =============================================================================
# 生成环境文件
# =============================================================================
generate_env_file() {
    log_info "生成环境配置文件..."

    local env_file="$PROJECT_ROOT/.env"

    if [[ -f "$env_file" ]]; then
        log_warn ".env 文件已存在，跳过生成"
        return 0
    fi

    # 生成随机密码
    local db_password=$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)
    local redis_password=$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)
    local immudb_password=$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)
    local vault_token=$(openssl rand -base64 32 2>/dev/null || head -c 32 /dev/urandom | base64)
    local grafana_password=$(openssl rand -base64 16 2>/dev/null || head -c 16 /dev/urandom | base64)

    cat > "$env_file" << EOF
# CredBridge Docker 环境配置
# 生成时间: $(date)
# 警告: 请勿提交此文件到版本控制

# =============================================================================
# 服务版本
# =============================================================================
VAULT_SERVICE_VERSION=latest

# =============================================================================
# 数据库配置
# =============================================================================
DB_HOST=postgres
DB_PORT=5432
DB_NAME=credbridge
DB_USER=credbridge
DB_PASSWORD=$db_password
DATABASE_URL=postgresql://credbridge:${db_password}@postgres:5432/credbridge

# =============================================================================
# Redis 配置
# =============================================================================
REDIS_HOST=redis
REDIS_PORT=6379
REDIS_PASSWORD=$redis_password
REDIS_URL=redis://:${redis_password}@redis:6379/0

# =============================================================================
# immudb 配置
# =============================================================================
IMMUDB_HOST=immudb
IMMUDB_PORT=3322
IMMUDB_DATABASE=credbridge_audit
IMMUDB_USERNAME=credbridge
IMMUDB_PASSWORD=$immudb_password

# =============================================================================
# Vault 配置
# =============================================================================
VAULT_ADDR=http://vault:8200
VAULT_TOKEN=$vault_token

# =============================================================================
# Grafana 配置
# =============================================================================
GRAFANA_PASSWORD=$grafana_password

# =============================================================================
# TEE 配置
# =============================================================================
TEE_MODE=simulation
TEE_DEBUG=true
EOF

    chmod 600 "$env_file"
    log_success "环境配置文件已生成: $env_file"
    log_warn "请修改 .env 文件中的配置，特别是生产环境密码"
}

# =============================================================================
# 生成 Vault 配置
# =============================================================================
generate_vault_config() {
    log_info "生成 Vault 配置文件..."

    # 开发环境配置
    cat > "$DOCKER_DIR/config/vault/vault.hcl" << 'EOF'
ui = true

storage "file" {
  path = "/vault/file"
}

listener "tcp" {
  address     = "0.0.0.0:8200"
  tls_disable = true
}

default_lease_ttl = "168h"
max_lease_ttl = "720h"
EOF

    # 生产环境配置
    mkdir -p "$DOCKER_DIR/config/vault"
    cat > "$DOCKER_DIR/config/vault/vault.prod.hcl" << 'EOF'
ui = true
api_addr = "http://vault:8200"
cluster_addr = "https://vault:8201"

storage "file" {
  path = "/vault/file"
}

listener "tcp" {
  address         = "0.0.0.0:8200"
  tls_disable     = false
  tls_cert_file   = "/vault/config/server.crt"
  tls_key_file    = "/vault/config/server.key"
  tls_min_version = "tls12"
}

default_lease_ttl = "168h"
max_lease_ttl = "720h"

# 禁用内存锁定（容器环境）
disable_mlock = true

# 性能调优
max_request_duration = "90s"
max_request_size = 33554432
EOF

    log_success "Vault 配置文件已生成"
}

# =============================================================================
# 生成 PostgreSQL 初始化脚本
# =============================================================================
generate_postgres_init() {
    log_info "生成 PostgreSQL 初始化脚本..."

    cat > "$DOCKER_DIR/scripts/init-postgres.sql" << 'EOF'
-- CredBridge PostgreSQL 初始化脚本
-- 创建扩展和初始表结构

-- 启用 UUID 扩展
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- 创建 schema
CREATE SCHEMA IF NOT EXISTS credbridge;

-- 设置搜索路径
ALTER DATABASE credbridge SET search_path TO credbridge, public;

-- 审计日志表
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    event_type VARCHAR(50) NOT NULL,
    tenant_id VARCHAR(100) NOT NULL,
    user_id VARCHAR(100),
    action VARCHAR(100) NOT NULL,
    resource_type VARCHAR(100),
    resource_id VARCHAR(100),
    status VARCHAR(20) NOT NULL,
    details JSONB,
    ip_address INET,
    user_agent TEXT,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    immudb_ref VARCHAR(100)
);

-- 凭证表
CREATE TABLE IF NOT EXISTS credentials (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id VARCHAR(100) NOT NULL,
    user_id VARCHAR(100) NOT NULL,
    credential_type VARCHAR(50) NOT NULL,
    encrypted_data BYTEA NOT NULL,
    key_handle VARCHAR(256) NOT NULL,
    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT true,
    UNIQUE(tenant_id, user_id, credential_type)
);

-- 租户表
CREATE TABLE IF NOT EXISTS tenants (
    id VARCHAR(100) PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    config JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    is_active BOOLEAN DEFAULT true
);

-- Token 撤销列表
CREATE TABLE IF NOT EXISTS token_revocations (
    token_jti VARCHAR(100) PRIMARY KEY,
    revoked_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    reason VARCHAR(200)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant ON audit_logs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp ON audit_logs(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type ON audit_logs(event_type);
CREATE INDEX IF NOT EXISTS idx_credentials_tenant ON credentials(tenant_id);
CREATE INDEX IF NOT EXISTS idx_credentials_user ON credentials(user_id);
CREATE INDEX IF NOT EXISTS idx_token_revocations_expires ON token_revocations(expires_at);

-- 创建更新时间触发器
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_credentials_updated_at
    BEFORE UPDATE ON credentials
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tenants_updated_at
    BEFORE UPDATE ON tenants
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
EOF

    log_success "PostgreSQL 初始化脚本已生成"
}

# =============================================================================
# 生成 Nginx 配置
# =============================================================================
generate_nginx_config() {
    log_info "生成 Nginx 配置文件..."

    # 主配置
    cat > "$DOCKER_DIR/config/nginx/nginx.conf" << 'EOF'
user nginx;
worker_processes auto;
error_log /var/log/nginx/error.log warn;
pid /var/run/nginx.pid;

events {
    worker_connections 1024;
    use epoll;
    multi_accept on;
}

http {
    include /etc/nginx/mime.types;
    default_type application/octet-stream;

    log_format main '$remote_addr - $remote_user [$time_local] "$request" '
                    '$status $body_bytes_sent "$http_referer" '
                    '"$http_user_agent" "$http_x_forwarded_for" '
                    'rt=$request_time uct="$upstream_connect_time" '
                    'uht="$upstream_header_time" urt="$upstream_response_time"';

    access_log /var/log/nginx/access.log main;

    sendfile on;
    tcp_nopush on;
    tcp_nodelay on;
    keepalive_timeout 65;
    types_hash_max_size 2048;
    server_tokens off;

    # Gzip
    gzip on;
    gzip_vary on;
    gzip_min_length 1024;
    gzip_types text/plain text/css text/xml text/javascript application/javascript application/xml+rss application/json;

    # 安全头部
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;

    # 限制请求体大小
    client_max_body_size 10M;

    include /etc/nginx/conf.d/*.conf;
}
EOF

    # Vault Service 反向代理配置
    cat > "$DOCKER_DIR/config/nginx/conf.d/vault-service.conf" << 'EOF'
upstream vault_service {
    least_conn;
    server vault-service:8080 max_fails=3 fail_timeout=30s;
    keepalive 32;
}

server {
    listen 80;
    server_name _;

    # 健康检查端点
    location /health {
        access_log off;
        return 200 "healthy\n";
        add_header Content-Type text/plain;
    }

    # 主服务
    location / {
        proxy_pass http://vault_service;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        proxy_connect_timeout 5s;
        proxy_send_timeout 60s;
        proxy_read_timeout 60s;

        proxy_buffering on;
        proxy_buffer_size 4k;
        proxy_buffers 8 4k;
    }
}
EOF

    log_success "Nginx 配置文件已生成"
}

# =============================================================================
# 生成 Prometheus 配置
# =============================================================================
generate_prometheus_config() {
    log_info "生成 Prometheus 配置文件..."

    cat > "$DOCKER_DIR/config/prometheus/prometheus.yml" << 'EOF'
global:
  scrape_interval: 15s
  evaluation_interval: 15s
  external_labels:
    cluster: 'credbridge'
    replica: '{{.ExternalURL}}'

alerting:
  alertmanagers:
    - static_configs:
        - targets: []

rule_files: []

scrape_configs:
  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']

  - job_name: 'vault-service'
    static_configs:
      - targets: ['vault-service:8080']
    metrics_path: '/metrics'
    scrape_interval: 30s

  - job_name: 'postgres'
    static_configs:
      - targets: ['postgres:5432']

  - job_name: 'redis'
    static_configs:
      - targets: ['redis:6379']

  - job_name: 'vault'
    static_configs:
      - targets: ['vault:8200']
    metrics_path: '/v1/sys/metrics'
    params:
      format: ['prometheus']
EOF

    log_success "Prometheus 配置文件已生成"
}

# =============================================================================
# 生成 Docker Secrets（生产环境）
# =============================================================================
generate_docker_secrets() {
    log_info "生成 Docker Secrets..."

    if [[ "$ENVIRONMENT" != "production" ]]; then
        log_info "非生产环境，跳过 Docker Secrets 生成"
        return 0
    fi

    # 创建 secrets 目录
    mkdir -p "$DOCKER_DIR/secrets"

    # 生成数据库密码
    openssl rand -base64 32 > "$DOCKER_DIR/secrets/db_password.txt"
    openssl rand -base64 32 > "$DOCKER_DIR/secrets/db_root_password.txt"

    log_warn "生产环境密码已生成到 $DOCKER_DIR/secrets/"
    log_warn "请使用以下命令创建 Docker Secrets:"
    log_warn "  docker secret create db_password $DOCKER_DIR/secrets/db_password.txt"
    log_warn "  docker secret create db_root_password $DOCKER_DIR/secrets/db_root_password.txt"
}

# =============================================================================
# 显示使用信息
# =============================================================================
show_usage() {
    echo ""
    echo "╔════════════════════════════════════════════════════════════╗"
    echo "║          CredBridge Docker 初始化完成                     ║"
    echo "╚════════════════════════════════════════════════════════════╝"
    echo ""
    echo "使用方法:"
    echo ""
    echo "  1. 启动开发环境:"
    echo "     cd docker && docker-compose up -d"
    echo ""
    echo "  2. 启动生产环境:"
    echo "     cd docker && docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d"
    echo ""
    echo "  3. 查看日志:"
    echo "     docker-compose logs -f vault-service"
    echo ""
    echo "  4. 停止服务:"
    echo "     docker-compose down"
    echo ""
    echo "服务地址:"
    echo "  - Vault Service: http://localhost:8080"
    echo "  - PostgreSQL:    localhost:5432"
    echo "  - Redis:         localhost:6379"
    echo "  - immudb:        localhost:3322"
    echo "  - Vault:         http://localhost:8200"
    echo "  - Grafana:       http://localhost:3000 (admin/admin)"
    echo "  - Prometheus:    http://localhost:9090"
    echo ""
    echo "重要文件:"
    echo "  - 环境配置: $PROJECT_ROOT/.env"
    echo "  - 数据目录: $DATA_DIR"
    echo ""
}

# =============================================================================
# 主函数
# =============================================================================
main() {
    log_info "开始初始化 CredBridge Docker 环境..."
    log_info "项目根目录: $PROJECT_ROOT"
    log_info "环境: $ENVIRONMENT"

    check_dependencies
    create_directories
    generate_env_file
    generate_vault_config
    generate_postgres_init
    generate_nginx_config
    generate_prometheus_config
    generate_docker_secrets

    log_success "初始化完成!"
    show_usage
}

# 解析命令行参数
while [[ $# -gt 0 ]]; do
    case $1 in
        -e|--environment)
            ENVIRONMENT="$2"
            shift 2
            ;;
        -d|--data-dir)
            DATA_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "用法: $0 [选项]"
            echo ""
            echo "选项:"
            echo "  -e, --environment   环境类型 (development/production) [默认: development]"
            echo "  -d, --data-dir      数据目录 [默认: /var/lib/credbridge]"
            echo "  -h, --help          显示帮助"
            exit 0
            ;;
        *)
            log_error "未知选项: $1"
            exit 1
            ;;
    esac
done

# 执行主函数
main
