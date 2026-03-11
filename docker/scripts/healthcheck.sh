#!/bin/sh
#
# CredBridge Vault Service 健康检查脚本
# 用于 Docker HEALTHCHECK 指令
#

set -e

# 配置
HEALTH_PORT="${VAULT_SERVICE_PORT:-8080}"
HEALTH_HOST="${VAULT_SERVICE_HOST:-localhost}"
HEALTH_ENDPOINT="${HEALTH_ENDPOINT:-/health}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-5}"

# 日志函数（兼容 sh）
log_debug() {
    if [ "${HEALTH_DEBUG:-}" = "true" ]; then
        echo "[DEBUG] $1" >&2
    fi
}

log_error() {
    echo "[ERROR] $1" >&2
}

# =============================================================================
# 健康检查函数
# =============================================================================

# 检查 HTTP 端点
check_http_endpoint() {
    log_debug "Checking HTTP endpoint: http://${HEALTH_HOST}:${HEALTH_PORT}${HEALTH_ENDPOINT}"

    if command -v curl >/dev/null 2>&1; then
        response=$(curl -fsS -o /dev/null -w "%{http_code}" \
            --max-time "$HEALTH_TIMEOUT" \
            "http://${HEALTH_HOST}:${HEALTH_PORT}${HEALTH_ENDPOINT}" 2>/dev/null) || {
            log_error "HTTP check failed: curl returned $?"
            return 1
        }

        if [ "$response" = "200" ]; then
            log_debug "HTTP check passed: status $response"
            return 0
        else
            log_error "HTTP check failed: unexpected status $response"
            return 1
        fi

    elif command -v wget >/dev/null 2>&1; then
        if wget -q -O /dev/null \
            --timeout="$HEALTH_TIMEOUT" \
            "http://${HEALTH_HOST}:${HEALTH_PORT}${HEALTH_ENDPOINT}" 2>/dev/null; then
            log_debug "HTTP check passed"
            return 0
        else
            log_error "HTTP check failed: wget returned $?"
            return 1
        fi

    else
        # 使用 /dev/tcp 进行基本连接测试
        log_debug "No curl/wget available, using /dev/tcp for connection test"
        if timeout "$HEALTH_TIMEOUT" sh -c "</dev/tcp/${HEALTH_HOST}/${HEALTH_PORT}" 2>/dev/null; then
            log_debug "TCP connection check passed"
            return 0
        else
            log_error "TCP connection check failed"
            return 1
        fi
    fi
}

# 检查进程是否存在
check_process() {
    log_debug "Checking if vault-service process is running"

    if pgrep -x "vault-service" >/dev/null 2>&1; then
        log_debug "Process check passed: vault-service is running"
        return 0
    else
        log_error "Process check failed: vault-service not found"
        return 1
    fi
}

# 检查磁盘空间
check_disk_space() {
    log_debug "Checking disk space"

    # 获取 /app 目录所在文件系统的可用空间百分比
    usage=$(df /app | awk 'NR==2 {print $5}' | sed 's/%//')

    if [ -n "$usage" ] && [ "$usage" -lt 90 ]; then
        log_debug "Disk check passed: ${usage}% used"
        return 0
    else
        log_error "Disk check failed: ${usage}% used (threshold: 90%)"
        return 1
    fi
}

# 检查内存使用
check_memory() {
    log_debug "Checking memory usage"

    # 检查可用内存是否足够（至少 50MB）
    if command -v free >/dev/null 2>&1; then
        available=$(free -m | awk 'NR==2{printf "%d", $7}')
        if [ "$available" -lt 50 ]; then
            log_error "Memory check failed: only ${available}MB available"
            return 1
        fi
        log_debug "Memory check passed: ${available}MB available"
    fi

    return 0
}

# =============================================================================
# 主检查逻辑
# =============================================================================
main() {
    log_debug "Starting health check at $(date -Iseconds 2>/dev/null || date)"

    # 执行所有检查
    failed=0

    if ! check_process; then
        failed=$((failed + 1))
    fi

    if ! check_http_endpoint; then
        failed=$((failed + 1))
    fi

    if ! check_disk_space; then
        failed=$((failed + 1))
    fi

    if ! check_memory; then
        failed=$((failed + 1))
    fi

    # 结果
    if [ $failed -eq 0 ]; then
        log_debug "All health checks passed"
        exit 0
    else
        log_error "$failed health check(s) failed"
        exit 1
    fi
}

# 运行主函数
main
