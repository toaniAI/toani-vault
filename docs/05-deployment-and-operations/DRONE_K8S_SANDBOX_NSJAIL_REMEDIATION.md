# Drone/K8s 发布环境沙箱缺失修复清单

## 背景

在 `TEE_MODE=hardware` 且运行于 Intel SGX 环境时，调用沙箱 API：

- `POST /api/v1/sandbox/sessions`

返回 `500 Internal Server Error`。

核心日志特征：

```text
Starting nsjail sandbox: <sandbox-id>
Running: /usr/bin/nsjail [...]
Failed to start nsjail sandbox <sandbox-id>: No such file or directory (os error 2)
Failed to create session: 进程错误: Failed to start nsjail: No such file or directory (os error 2)
```

同时伴随一条非阻塞告警：

```text
Skipping cgroup setup for sandbox ... Permission denied (os error 13)
```

## 结论

### 直接根因

Drone 发布流水线构建镜像时使用的是仓库根目录 `Dockerfile`，不是 `docker/Dockerfile`。

当前发布路径下：

- 根目录 `Dockerfile` 包含 SGX/DCAP 运行时依赖
- 根目录 `Dockerfile` 不包含 `nsjail` 安装或复制逻辑
- 服务启动后调用默认路径 `/usr/bin/nsjail`
- 镜像内不存在该文件，最终在运行时触发 `ENOENT`

### 次级风险

即使后续把 `nsjail` 打进镜像，K8s 发布配置也可能仍然缺少完整的沙箱运行条件，例如：

- `/sys/fs/cgroup` 可写挂载
- 与 `nsjail` 相关的 Linux capability
- 合适的 seccomp / namespace 权限

当前日志中的 `cgroup Permission denied` 就是这条风险的信号，但它不是本次 500 的首因。

## 影响范围

- 影响所有通过 Drone `backend-build-deploy` 流水线发布的环境
- 影响所有需要调用沙箱能力的接口与功能
- 不影响普通 SGX/DCAP 初始化路径
- 不影响未触发 sandbox session 创建的普通 API

## 运维修复目标

需要同时满足以下三类目标：

1. 镜像内必须存在可执行的 `nsjail`
2. 容器运行时必须具备 `nsjail` 所需权限与挂载
3. 发布后必须有显式验收，确认沙箱 API 可用

## 修复清单

### 一、修正 Drone 构建入口

确认 `backend-build-deploy` 流水线的 Docker 构建入口是否仍为仓库根 `Dockerfile`。

当前风险配置特征：

- Drone 使用 `dockerfile: Dockerfile`
- 本地 compose 使用 `docker/Dockerfile`
- 两条发布路径产物能力不一致

运维/平台处理项：

- 将发布流水线统一切换到包含 `nsjail` 打包逻辑的 Dockerfile
- 或者将根目录 `Dockerfile` 补齐 `nsjail` 构建与复制逻辑
- 保证 dev/test/prod 三套发布路径使用同一套运行时能力基线

验收标准：

- 镜像内存在 `/usr/bin/nsjail` 或 `/usr/local/bin/nsjail`
- 服务容器内执行 `nsjail -V` 成功

### 二、核对镜像内容

发布新镜像后，在目标环境或镜像仓库落地前，执行以下检查：

```bash
docker run --rm --entrypoint sh <image> -lc 'ls -l /usr/bin/nsjail /usr/local/bin/nsjail || true'
docker run --rm --entrypoint sh <image> -lc 'command -v nsjail || true'
docker run --rm --entrypoint sh <image> -lc 'ldd /usr/bin/nsjail || ldd /usr/local/bin/nsjail || true'
docker run --rm --entrypoint sh <image> -lc '/usr/bin/nsjail -V || /usr/local/bin/nsjail -V'
```

验收标准：

- 至少一个路径存在 `nsjail`
- `ldd` 不出现缺失共享库
- `nsjail -V` 返回成功

### 三、补齐 K8s 运行权限

需要比照当前 `docker-compose` 的沙箱运行条件，核对 Helm/K8s 部署是否具备等效能力。

重点检查项：

- Pod 是否以可执行 `nsjail` 的身份运行
- 是否允许创建 namespace
- 是否允许操作 cgroup
- 是否挂载 `/sys/fs/cgroup`
- 是否配置了所需 capability
- 是否存在过严的 seccomp / AppArmor / PSP / PSA / admission policy 限制

建议核查项：

- `securityContext.privileged`
- `allowPrivilegeEscalation`
- `capabilities.add`
- `seccompProfile`
- `hostPath` 或其他方式提供 `/sys/fs/cgroup`

最低验收标准：

- 容器内能够访问并写入目标 cgroup 路径，或系统明确接受“无 cgroup 限流降级”
- 若策略要求 fail-closed，则必须启用 cgroup 并确保创建成功

### 四、明确 cgroup 策略

当前程序逻辑允许在 cgroup 创建失败时继续运行沙箱。

运维需与研发确认生产策略：

- 策略 A：允许降级运行
- 策略 B：必须 fail-closed，禁止无 cgroup 运行

如果选择策略 B：

- 设置 `CREDBRIDGE_SANDBOX_CGROUP_REQUIRED=true`
- 同时确保集群环境具备 cgroup 创建能力

如果选择策略 A：

- 接受日志中出现 `Skipping cgroup setup`
- 但必须确认 `nsjail` 本身可以正常拉起

### 五、补充发布前自检

当前启动前脚本主要检查 SGX/DCAP 条件，没有检查 `nsjail`。

建议运维与研发协同增加以下预检项：

- `command -v nsjail`
- `nsjail -V`
- 若启用沙箱，检查 `/sys/fs/cgroup` 可用性
- 若启用硬件模式，保留现有 SGX/DCAP 检查

目标：

- 问题在 Pod 启动阶段暴露
- 不要等到业务调用 `/api/v1/sandbox/sessions` 才发现镜像缺件

### 六、发布后验收

完成部署后，至少执行以下验收：

#### 1. 容器内自检

```bash
kubectl -n <namespace> exec -it <pod> -- sh -lc 'id && command -v nsjail && nsjail -V'
kubectl -n <namespace> exec -it <pod> -- sh -lc 'ls -ld /sys/fs/cgroup /sys/fs/cgroup/credbridge || true'
kubectl -n <namespace> exec -it <pod> -- sh -lc 'test -S /var/run/aesmd/aesm.socket && echo ok || echo missing'
```

#### 2. API 验收

使用有效 token 调用：

```bash
curl -i -X POST 'https://<host>/api/v1/sandbox/sessions' \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  --data '{"credential_id":"<credential-id>","original_intent":"sandbox smoke test"}'
```

期望结果：

- 返回 `201` 或成功创建会话的响应
- 服务日志中不再出现 `Failed to start nsjail ... No such file or directory`

#### 3. 日志验收

重点检查以下关键字：

- `Starting nsjail sandbox`
- `Nsjail sandbox started`
- `Skipping cgroup setup`
- `Failed to start nsjail`
- `Permission denied`

判定标准：

- 不能再出现 `No such file or directory (os error 2)`
- 若仍有 `Skipping cgroup setup`，需按既定策略确认是否接受

## 推荐处置顺序

1. 先修正 Drone 构建入口，确保镜像内带上 `nsjail`
2. 再核对 K8s 权限、cgroup、capability 和 seccomp
3. 最后执行沙箱 API 冒烟验证

## 回滚条件

若修复后出现以下情况，应停止继续放量并回滚：

- Pod 无法启动
- `runtime-preflight` 新增检查导致硬件环境误判
- `nsjail` 存在但因权限不足导致所有沙箱请求失败
- 新增权限策略触发集群安全基线告警

## 运维交付物

修复完成后，请保留以下证据：

- Drone 构建日志中实际使用的 Dockerfile 路径
- 新镜像内 `nsjail` 存在与版本输出截图
- K8s Deployment 或 Helm values 中与沙箱权限相关的最终配置
- 沙箱 API 成功创建 session 的请求与响应样例
- 对应 Pod 日志中 `Nsjail sandbox started` 的证据

## 备注

本次 500 的首因是镜像缺少 `nsjail`，不是 Intel SGX 本身失效。

IntelTEE 环境只说明硬件模式已进入业务执行路径；沙箱失败点发生在宿主侧进程隔离组件未随发布镜像正确交付。
