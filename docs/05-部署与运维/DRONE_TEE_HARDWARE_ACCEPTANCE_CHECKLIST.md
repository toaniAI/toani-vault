# Drone 部署后 TEE Hardware 验收清单

> 适用场景：`backend-build-deploy` 已经通过 Drone 把当前镜像部署到 TEE 节点之后，需要在 **SGX 机器 / 可访问集群的运维终端** 上一次性完成 hardware-only 验收。
>
> 本清单关注的是 **主服务链路** 在 `TEE_MODE=hardware` 下是否真正跑通 SGX/DCAP 就绪链路，而不是普通 simulation 回归。

## 1. 验收目标

完成以下检查后，才能认为本次 Drone 部署在 TEE 环境中达到了最小硬件验收口径：

- Pod 已成功 rollout 到目标命名空间。
- Pod 实际落在 TEE/SGX 节点。
- 运行时使用 `TEE_MODE=hardware`，且未静默降级到 simulation。
- 服务的 `liveness`、`readiness`、attestation health 语义与当前实现一致。
- `sgx_hardware_tests` 能在目标环境执行通过。

## 2. 前置条件

执行前请确认：

- Drone 已完成镜像构建与 Helm/K8s 部署。
- 你知道本次部署的命名空间、Deployment 名称、镜像 tag 或 Drone build number。
- 目标节点已准备 SGX 依赖：`/dev/sgx_enclave`、`/dev/sgx_provision`、AESM、PCCS/PCS 可达。
- 容器内配置了 `TEE_MODE=hardware` 及本次环境所需数据库、Redis、Vault、immudb 连接信息。

## 3. 建议先设置的环境变量

将下面变量替换为本次部署的真实值：

```bash
export NS=zkme-test
export DEPLOYMENT=credbridge-go
export APP_LABEL=app=credbridge-go
export CONTAINER=credbridge-go
export PORT=8080
```

如果本次在开发环境验收，可以把 `NS` 改成 `zkme-dev`。

## 4. 执行步骤

### 步骤 1：确认 Drone 对应部署已经 rollout 成功

```bash
kubectl -n "$NS" rollout status deployment/"$DEPLOYMENT" --timeout=600s
```

预期输出：

```text
deployment "credbridge-go" successfully rolled out
```

如果这里失败，不要继续做 TEE 验收，先处理镜像、Helm 或配置问题。

### 步骤 2：定位最新 Pod，并确认它落在 TEE 节点

```bash
kubectl -n "$NS" get pods -l "$APP_LABEL" -o wide
```

预期输出：

- 至少有一个 `READY` 为 `1/1` 的 Pod。
- `NODE` 列对应的应是 SGX/TEE 节点。

可进一步核对节点标签：

```bash
export POD=$(kubectl -n "$NS" get pods -l "$APP_LABEL" -o jsonpath='{.items[0].metadata.name}')
export NODE=$(kubectl -n "$NS" get pod "$POD" -o jsonpath='{.spec.nodeName}')
kubectl get node "$NODE" --show-labels | rg 'sgx|tee'
```

预期输出：

- 能看到与 `sgx` 或 `tee` 相关的节点标签，或至少确认该节点就是你们的 SGX 专用节点。

### 步骤 3：确认运行时配置确实请求 hardware 模式

```bash
kubectl -n "$NS" exec "$POD" -c "$CONTAINER" -- printenv TEE_MODE
```

预期输出：

```text
hardware
```

如果输出为空或为 `simulation`，本次验收直接失败。

### 步骤 4：确认 Pod 内可见 SGX 设备节点

```bash
kubectl -n "$NS" exec "$POD" -c "$CONTAINER" -- sh -lc 'ls -l /dev/sgx_enclave /dev/sgx_provision'
```

预期输出：

- 两个设备节点都存在。
- 命令退出码为 `0`。

若命令报 `No such file or directory`，说明设备挂载或节点调度不正确。

### 步骤 5：检查应用启动日志，确认没有 hardware -> simulation 静默降级

```bash
kubectl -n "$NS" logs "$POD" -c "$CONTAINER" --since=10m | rg 'requested_mode|effective_mode|root_key_source|fail-closed|attestation|ready'
```

预期输出重点：

- 能看到 `requested_mode=hardware`
- `effective_mode=hardware`
- `root_key_source` 不是 `simulation`
- 不应出现“fall back to simulation”一类日志

如果出现 fail-closed 日志，需要结合 readiness 接口判断是否符合预期阻断。

### 步骤 6：验证 liveness 端点

推荐使用端口转发：

```bash
kubectl -n "$NS" port-forward pod/"$POD" 18080:"$PORT"
```

另开一个终端执行：

```bash
curl -sS http://127.0.0.1:18080/health
```

预期输出：

```json
{
  "status": "alive",
  "service": "credbridge-vault",
  "version": "...",
  "message": "CredBridge service is running"
}
```

验收要点：

- HTTP 状态码应为 `200`
- `status` 应为 `alive`
- 这里仅表示进程存活，不代表 TEE 已完全就绪

### 步骤 7：验证 readiness 端点

```bash
curl -i -sS http://127.0.0.1:18080/ready
```

预期输出：

- HTTP 状态码为 `200 OK`
- JSON 中 `ready` 为 `true`
- `status` 为 `ready` 或至少不是 `failed`

可补充查看详细版本：

```bash
curl -sS http://127.0.0.1:18080/health/detail
```

预期输出中应看到：

- `live: true`
- `ready: true`
- `components.enclave` 为就绪态
- `components.attestation` 为就绪态或与当前环境一致的可接受状态

如果 `/ready` 返回 `503`，先不要执行后续业务验收，应回到日志和依赖环境排查。

### 步骤 8：验证 attestation 状态接口

```bash
curl -sS http://127.0.0.1:18080/api/v1/attestation/status
```

预期输出要点：

- `success: true`
- `requested_mode: "hardware"`
- `effective_mode: "hardware"`
- `root_key_source` 不为 `simulation`
- `hardware_available: true`
- `enclave_state: "Running"` 或等价运行态

补充健康检查：

```bash
curl -i -sS http://127.0.0.1:18080/api/v1/attestation/health
```

预期输出：

- HTTP 状态码为 `200 OK`
- `ready: true`
- `quote_valid: true`

如果 `quote_valid` 为 `false` 或返回 `503`，说明 DCAP/PCCS/PCS/证书链闭环还未真正打通。

### 步骤 9：验证监控指标与 readiness 语义一致

```bash
curl -sS http://127.0.0.1:18080/metrics | rg 'credbridge_ready|credbridge_attestation_quote_valid'
```

预期输出：

```text
credbridge_ready{status="ready"} 1
credbridge_attestation_quote_valid{} 1
```

如果这里仍显示 `0`，即使进程可访问，也不应判定为硬件验收通过。

### 步骤 10：执行 hardware-only 验收测试

如果测试在宿主机执行：

```bash
cd /path/to/credbridge
TEE_MODE=hardware cargo test --test sgx_hardware_tests -- --ignored --test-threads=1
```

如果测试需要在 Pod 内执行：

```bash
kubectl -n "$NS" exec "$POD" -c "$CONTAINER" -- sh -lc 'cd /app && TEE_MODE=hardware cargo test --test sgx_hardware_tests -- --ignored --test-threads=1'
```

预期输出：

```text
running ...
test result: ok.
```

验收要点：

- 所有 `hardware-only` 用例通过
- 不允许通过跳过 SGX 依赖来“伪通过”

## 5. 推荐记录模板

建议把本次验收记录成如下格式：

```text
部署背景：
- Drone build: <build-number>
- Namespace: <namespace>
- Deployment: <deployment>
- Image tag: <tag>

已执行验证：
- kubectl rollout status deployment/<deployment> --timeout=600s
- curl http://127.0.0.1:18080/health
- curl http://127.0.0.1:18080/ready
- curl http://127.0.0.1:18080/api/v1/attestation/status
- curl http://127.0.0.1:18080/api/v1/attestation/health
- curl http://127.0.0.1:18080/metrics | rg 'credbridge_ready|credbridge_attestation_quote_valid'
- TEE_MODE=hardware cargo test --test sgx_hardware_tests -- --ignored --test-threads=1

验收结论：
- rollout: pass/fail
- TEE_MODE=hardware: pass/fail
- SGX devices: pass/fail
- readiness: pass/fail
- attestation health: pass/fail
- hardware-only tests: pass/fail
```

## 6. 失败时的最小排查顺序

按下面顺序排查，避免混乱：

1. `kubectl rollout status` 是否成功
2. Pod 是否真的调度到 SGX/TEE 节点
3. `TEE_MODE` 是否为 `hardware`
4. `/dev/sgx_enclave` 与 `/dev/sgx_provision` 是否存在
5. `/ready` 是否返回 `200`
6. `/api/v1/attestation/health` 是否返回 `200` 且 `quote_valid=true`
7. `sgx_hardware_tests` 是否通过

## 7. 通过标准

以下条件同时满足，才判定本次 Drone 部署后的 TEE hardware-only 验收通过：

- rollout 成功
- Pod 落在 TEE 节点
- `TEE_MODE=hardware`
- `/health` 返回 `200 alive`
- `/ready` 返回 `200` 且 `ready=true`
- `/api/v1/attestation/health` 返回 `200` 且 `quote_valid=true`
- `credbridge_ready{status="ready"} 1`
- `credbridge_attestation_quote_valid{} 1`
- `sgx_hardware_tests` 通过
