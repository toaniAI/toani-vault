# nsjail 映射问题修复方案（代码库内可落地）

## 目标

在**不依赖直接修改宿主机**的前提下，尽量把 `nsjail` 的 UID/GID 映射前置检查、镜像依赖、部署参数和发布闸门补齐，降低以下错误进入运行时的概率：

```text
newgidmap: gid range [0-1] -> [100000-100001] not allowed
[E] gidMapExternal() '/usr/bin/newgidmap' failed
[E] initParent(): Couldn't initialize user namespace
```

说明：

- 该方案只能修复**镜像层、容器层、CI/CD 层**的问题。
- 如果目标节点本身不允许 user namespace 映射，本方案只能做到**更早失败、给出更清晰错误**，不能单独保证彻底修复。

---

## 当前代码库现状

已确认的现状：

- 运行时镜像已安装 `uidmap`，并校验 `newuidmap/newgidmap` 存在，见 [docker/base/runtime.Dockerfile](/Users/yvan/AIWorkspace/credbridge/docker/base/runtime.Dockerfile:49)。
- 默认 `nsjail` UID/GID 映射会把容器内 `0` 映射到外部 `100000`，见 [src/tee/sandbox/config.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/config.rs:439) 和 [src/tee/sandbox/config.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/config.rs:460)。
- `docker-compose` 开发环境已经启用了 `privileged`、`/sys/fs/cgroup` 挂载和 `seccomp:unconfined`，见 [docker/docker-compose.yml](/Users/yvan/AIWorkspace/credbridge/docker/docker-compose.yml:68) 和 [docker/docker-compose.yml](/Users/yvan/AIWorkspace/credbridge/docker/docker-compose.yml:81)。
- 当前 [docker/scripts/runtime-preflight.sh](/Users/yvan/AIWorkspace/credbridge/docker/scripts/runtime-preflight.sh) 主要检查 SGX/DCAP 与 Playwright，**没有检查 userns / uidmap / subuid / subgid**。
- 仓库内目前**没有**现成的 `.drone.yml`，因此 Drone 发布闸门需要新增。

### 已观测到的实际失败形态

基于容器内实测，当前环境表现为：

- 当前运行用户是 `root`
- `/etc/subuid` 和 `/etc/subgid` 里只有 `appuser:100000:65536`
- `kernel.unprivileged_userns_clone=1`
- `/sys/fs/cgroup` 已挂载
- `nsjail` smoke test 失败于：

```text
newgidmap: gid range [0-1) -> [100000-100001) not allowed
```

这说明当前最直接的问题不是：

- `uidmap` 包缺失
- `nsjail` 二进制缺失
- `cgroup` 未挂载

而是：

- **容器实际以 `root` 运行**
- **映射授权却只配置给了 `appuser`**
- 同时应用默认要求把 inside gid `0` 映射到 outside gid `100000`

因此，代码库内方案的核心目标需要从“泛化增强”收敛为：

1. 统一运行用户与映射授权用户
2. 把默认 UID/GID 映射做成可配置
3. 在启动前明确识别“当前用户与 `/etc/subgid` 条目不匹配”

---

## 方案总览

代码库内建议分 4 个改动面：

1. 运行时镜像补齐 `newuidmap/newgidmap` 权限与映射文件准备
2. `runtime-preflight.sh` 增加 `nsjail` / userns 预检
3. 部署配置显式声明运行约束
4. Drone 流水线增加发布前阻断检查

---

## 方案 1：强化运行时镜像

目标：确保镜像不是“工具存在但不可用”的半成品。

建议修改文件：

- [docker/base/runtime.Dockerfile](/Users/yvan/AIWorkspace/credbridge/docker/base/runtime.Dockerfile)
- 如仍使用另一套运行时镜像，也同步检查 [docker/Dockerfile](/Users/yvan/AIWorkspace/credbridge/docker/Dockerfile)

建议改动：

### 1. 显式校验 `newuidmap` / `newgidmap` 权限位

在镜像构建阶段增加检查：

```sh
ls -l /usr/bin/newuidmap /usr/bin/newgidmap
test -u /usr/bin/newuidmap
test -u /usr/bin/newgidmap
```

目的：

- 仅检查“命令存在”不够。
- `newuidmap/newgidmap` 如果没有正确的 setuid 位，运行时仍然会失败。

### 2. 为容器内运行用户准备 `/etc/subuid` 和 `/etc/subgid`

如果容器内实际由固定用户启动 `nsjail`，建议在镜像里显式写入：

```text
appuser:100000:65536
```

或与实际用户一致的范围。

注意：

- 这一步只能确保**容器文件系统里有映射定义**。
- 是否真正允许映射，还要看宿主机/运行时是否支持。
- 根据当前实测结果，**只给 `appuser` 写条目是不够的**，因为容器实际以 `root` 启动主进程。

基于当前仓库，必须二选一，不能继续混用：

### 方案 A：保持进程以 `root` 运行

那就必须为 `root` 也准备对应条目，例如：

```text
root:100000:65536
appuser:100000:65536
```

并在 preflight 中校验当前 `id -un` 对应的条目存在。

### 方案 B：改为真正以 `appuser` 运行

如果要保留只给 `appuser` 授权，那么主进程和 `nsjail` 调用链就必须统一切到 `appuser`，而不是继续让容器以 `root` 启动。

当前更推荐先落地 **方案 A**，因为：

- 改动更小
- 不会立刻引入 `SGX` 设备访问权限、文件属主、端口绑定等连锁问题
- 可以先验证 `newgidmap` 故障是否消失

### 3. 固化运行用户策略

当前根 Dockerfile 里创建了 `appuser`，见 [Dockerfile](/Users/yvan/AIWorkspace/credbridge/Dockerfile:86)。

建议统一：

- 谁启动主进程
- 谁调用 `nsjail`
- `nsjail` 的 outside uid/gid 映射到谁

避免出现：

- 镜像里创建的是 `appuser`
- 实际运行的是 `root`
- 映射文件写给了另一个用户

这不是理论风险，而是当前已发生的真实状态。

建议把这条改成硬性约束：

- `runtime-preflight.sh` 必须打印当前运行用户
- 若当前运行用户在 `/etc/subuid` 或 `/etc/subgid` 中没有条目，直接 `fail`
- 镜像构建时不要只写 `appuser` 条目却默认仍用 `root` 运行

### 4. 把 UID/GID 映射做成可配置项

建议在应用配置里新增环境变量支持，例如：

- `NSJAIL_OUTSIDE_UID`
- `NSJAIL_OUTSIDE_GID`
- `NSJAIL_UID_COUNT`
- `NSJAIL_GID_COUNT`

由 [src/tee/sandbox/config.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/config.rs) 读取，而不是把 `1000` 写死为默认外部映射目标。

价值：

- 允许不同环境按节点策略调整映射
- 降低“代码默认值和节点策略冲突”的概率

这项现在是必要项，不再只是优化项。当前失败日志表明默认值 `outside_gid=1000` 已经与当前容器实际授权状态冲突。

最低要求：

- 默认值保留，但必须允许环境覆盖
- 启动日志打印最终生效的 inside/outside uid/gid 映射
- preflight 对将要使用的映射值做一次 smoke test，而不是写死 `0:100000:1`

---

## 方案 2：为 runtime-preflight 增加 userns 预检

目标：在服务启动前失败，而不是等用户触发 sandbox API 时才失败。

建议修改文件：

- [docker/scripts/runtime-preflight.sh](/Users/yvan/AIWorkspace/credbridge/docker/scripts/runtime-preflight.sh)

建议新增检查：

### 1. 检查基础命令和权限

检查项：

- `command -v nsjail`
- `command -v newuidmap`
- `command -v newgidmap`
- `test -u /usr/bin/newuidmap`
- `test -u /usr/bin/newgidmap`

### 2. 检查映射文件

检查项：

- `/etc/subuid` 是否存在
- `/etc/subgid` 是否存在
- 当前运行用户在这两个文件里是否有条目

这一项要升级成强失败条件。根据当前实测，最关键的失配就是：

- 当前用户：`root`
- 条目用户：`appuser`

只要出现这种失配，就不应继续启动服务。

### 3. 检查内核 userns 能力

如果运行环境暴露了该内核参数，可检查：

```sh
cat /proc/sys/kernel/unprivileged_userns_clone
```

说明：

- 若为 `0`，大概率无法使用当前 `nsjail` 映射策略
- 若容器内不可见，也应打印告警而不是静默略过

### 4. 做一次最小化 `nsjail` 自检

建议在 preflight 里运行一个极小命令，例如：

```sh
nsjail --mode o \
  --uid_mapping "${INSIDE_UID}:${OUTSIDE_UID}:${UID_COUNT}" \
  --gid_mapping "${INSIDE_GID}:${OUTSIDE_GID}:${GID_COUNT}" \
  -- /bin/sh -c 'id'
```

要求：

- 失败即阻止主服务启动
- 把 stderr 原样输出到日志

价值：

- 把“延迟到业务请求时爆炸”变成“启动即失败”
- 对运维更友好
- 避免出现“preflight 使用一套映射，运行时实际使用另一套映射”的假阳性

---

## 方案 3：补齐部署配置约束

目标：让部署配置明确表达 `nsjail` 的运行前提，而不是只靠口头约定。

建议覆盖的配置面：

- `docker-compose`
- Helm chart values / deployment manifest
- Kubernetes `securityContext`

### 1. Compose 侧

开发环境已经有这些设置：

- `privileged: true`
- `cap_add`
- `/sys/fs/cgroup:/sys/fs/cgroup:rw`
- `seccomp:unconfined`

建议把这些约束写入专门的部署说明文档，避免生产部署遗漏。

### 2. Helm / K8s 侧

当前 [config/chart/values.yaml](/Users/yvan/AIWorkspace/credbridge/config/chart/values.yaml) 基本没有 sandbox runtime 的安全上下文字段。

建议新增 values：

- `sandboxRuntime.privileged`
- `sandboxRuntime.allowPrivilegeEscalation`
- `sandboxRuntime.capabilities`
- `sandboxRuntime.seccompProfile`
- `sandboxRuntime.hostPaths`
- `sandboxRuntime.runAsUser`
- `sandboxRuntime.runAsGroup`

并在 Deployment 模板中消费。

注意：

- 如果 K8s 节点或 Admission Policy 不允许这些权限，说明问题仍在运行环境，不在应用代码。

---

## 方案 4：新增 Drone 发布前阻断

目标：让不满足 `nsjail` 前提的镜像/部署配置在 CI 阶段失败，而不是发布后由业务流量触发。

由于仓库当前没有 `.drone.yml`，建议新增一份包含以下阶段的流水线：

### 阶段 A：镜像静态检查

检查项：

- `newuidmap/newgidmap` 存在
- 二进制权限正确
- `/etc/subuid`、`/etc/subgid` 已准备
- `nsjail -V` 可执行

### 阶段 B：容器内 preflight 检查

以构建出的 runtime image 运行：

```sh
/app/runtime-preflight.sh true
```

并增加 `nsjail` 最小自检脚本。

### 阶段 C：部署清单检查

检查 K8s manifest 或 Helm values 中是否包含：

- privileged / capability 设置
- cgroup 挂载
- seccomp 策略

### 阶段 D：发布阻断

若任一检查失败：

- 不允许推送生产镜像
- 不允许进入部署阶段

---

## 推荐实施顺序

### 第一阶段：先做低风险高收益项

1. 修改 `runtime-preflight.sh`，增加 userns 预检
2. 在 `docker/base/runtime.Dockerfile` 中增加权限与映射文件检查，并修复当前 `root`/`appuser` 条目失配
3. 新增 Drone 阻断步骤

### 第二阶段：做配置抽象

1. 把 outside uid/gid 做成环境变量
2. 在 Helm values 中显式建模 sandbox runtime 配置

### 第三阶段：补回归验证

建议新增自动化检查覆盖：

- `nsjail` 最小启动 smoke test
- `runtime-preflight` 失败分支测试
- 映射配置从 env 覆盖默认值的单元测试

---

## 成功标准

满足以下条件，才算代码库内方案完成：

1. 镜像构建阶段能发现 `newuidmap/newgidmap` 权限缺陷
2. 服务启动前能发现“当前运行用户与 `/etc/subuid` `/etc/subgid` 条目不匹配”
3. Drone 在发布前能阻断不满足条件的构建
4. 运行时日志能明确区分：
   - 镜像问题
   - 容器权限问题
   - 节点 userns 问题

---

## 边界说明

这份方案的上限是：

- 修复镜像内缺失
- 修复配置表达不完整
- 把问题前移到启动期和 CI 阶段

这份方案的下限是：

- **不能替代宿主机 user namespace 策略修复**
- **不能绕过节点内核/容器运行时对 `newuidmap/newgidmap` 的限制**

因此，这份方案必须和宿主机修复方案配套执行。
