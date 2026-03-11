# HashiCorp Vault 配置文件
# 开发模式配置 - 生产环境请使用生产级配置

# 存储后端配置
storage "file" {
  path = "/vault/data"
}

# 监听配置
listener "tcp" {
  address     = "0.0.0.0:8200"
  tls_disable = true

  # 生产环境应启用 TLS
  # tls_cert_file = "/vault/certs/server.crt"
  # tls_key_file  = "/vault/certs/server.key"
}

# API 地址配置
api_addr = "http://0.0.0.0:8200"
cluster_addr = "http://0.0.0.0:8201"

# UI 启用
ui = true

# 默认 lease 配置
default_lease_ttl = "168h"
max_lease_ttl = "720h"

# 日志级别
log_level = "info"

# 性能调优
disable_cache = false
disable_mlock = false

# 遥测配置（可选）
# telemetry {
#   prometheus_retention_time = "30s"
#   disable_hostname          = true
# }
