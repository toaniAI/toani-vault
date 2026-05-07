# 部署与运维

本目录包含 ToaniVault 的部署指南、监控配置和运维文档。

## 部署文档

### 基础部署

- [部署指南](DEPLOYMENT.md) - 完整的部署流程和配置
- [Docker 部署](../docker/README.md) - Docker 环境部署
- [Kubernetes 部署](../config/chart/) - Helm Chart 配置

### 环境配置

- [DCAP 设置](DCAP-SETUP.md) - Intel SGX DCAP 环境配置
- [ImmuDB 设置](IMMUDB_SETUP.md) - 审计日志数据库设置
- [Intel SGX 部署要求](INTEL_SGX_DEPLOYMENT_REQUIREMENTS.md) - SGX 硬件部署要求
- [Drone 部署后 TEE 硬件验收清单](DRONE_TEE_HARDWARE_ACCEPTANCE_CHECKLIST.md) - 在 TEE 节点完成 rollout 后的一次性验收步骤

## 监控与运维

### 启动存储策略

- 生产环境必须配置：`DATABASE_URL`（auth/tenant/sandbox）、`REDIS_URL`（token state）、`IMMUDB_*`（audit）。
- 缺失上述后端时服务会启动失败，不再静默降级到内存。
- 开发/测试若需内存回退，必须显式设置：
  - `CREDBRIDGE_AUTH_ALLOW_MEMORY_FALLBACK=true`
  - `CREDBRIDGE_TENANT_ALLOW_MEMORY_FALLBACK=true`
  - `CREDBRIDGE_SANDBOX_ALLOW_MEMORY_FALLBACK=true`
  - `CREDBRIDGE_AUDIT_ALLOW_MEMORY_FALLBACK=true`
  - `CREDBRIDGE_TOKEN_ALLOW_MEMORY_FALLBACK=true`

### 监控告警

- [监控告警](MONITORING.md) - Prometheus + Grafana 监控配置
- 健康检查端点：`/health`（liveness）, `/ready` / `/health/detail`（readiness）
- 指标端点：`/metrics`
- 若健康检查经过前端或外部网关代理，先对照 [HEALTH_ENDPOINT_PROXY_CHECKLIST.md](HEALTH_ENDPOINT_PROXY_CHECKLIST.md)
  确认 `/health`、`/ready`、`/health/detail` 没有错误回退到前端静态页

### 日志与审计

- 审计日志：ImmuDB 不可篡改存储
- 应用日志：JSON 格式，结构化输出
- 日志级别：ERROR, WARN, INFO, DEBUG, TRACE

## 性能调优

- Redis 缓存配置
- 数据库连接池优化
- TEE Enclave 内存优化
- 并发请求处理优化

## 维护手册

### 日常维护

- 定期备份数据库
- 监控磁盘空间使用
- 检查审计日志完整性
- 更新 SSL 证书

### 故障排查

- 查看应用日志
- 检查健康检查端点
- 验证 TEE Enclave 状态
- 检查 DCAP 驱动和 PCS 服务

---

**更新时间**: 2026-05-07
