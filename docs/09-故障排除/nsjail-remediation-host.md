# nsjail 映射问题修复方案（宿主机 / 运行节点侧）

## 目标

修复 `nsjail` 在运行节点上创建 user namespace 时失败的问题，典型错误如下：

```text
newgidmap: gid range [0-1] -> [1000-1001] not allowed
[E] gidMapExternal() '/usr/bin/newgidmap' failed
[E] initParent(): Couldn't initialize user namespace
[E] standaloneMode(): Couldn't launch the child process
```

这类错误的核心含义是：

- 应用已经调用了 `nsjail`
- `nsjail` 尝试创建 user namespace
- 但**目标节点**不允许当前 UID/GID 映射

结论：

- 这是**宿主机 / 节点 / 容器运行时策略**问题
- 不能仅靠 API 调用方式或 token 调整解决

---

## 适用范围

本方案适用于以下部署形态：

- Docker 宿主机
- Kubernetes 节点
- 直接在 Linux 服务器上运行容器
- 使用 containerd / Docker / CRI-O 的运行节点

---

## 当前问题本质

结合代码库配置，当前 `nsjail` 默认会请求如下映射模式：

- inside uid/gid：`0`
- outside uid/gid：默认 `1000`

见 [src/tee/sandbox/config.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/config.rs:542)。

因此，当节点返回：

```text
gid range [0-1] -> [1000-1001] not allowed
```

通常说明以下几类问题之一：

1. 宿主机没有为对应用户配置 `/etc/subgid`
2. `newgidmap` 没有正确的 setuid root 权限
3. 节点禁用了 unprivileged user namespace
4. 容器运行时或 K8s 安全策略拦截了该能力
5. 运行用户与映射文件中的用户不一致

### 已观测到的实际状态

基于容器内检查结果，当前环境已经明确了几件事：

- 运行用户是 `root`
- `/etc/subuid` 和 `/etc/subgid` 中只有 `appuser:100000:65536`
- `kernel.unprivileged_userns_clone=1`
- `/sys/fs/cgroup` 已挂载
- `nsjail` 已成功执行到 user namespace 初始化阶段
- 失败点稳定落在 `newgidmap`

因此，本次故障优先级最高的根因不是内核总开关，而是：

- **为错误的用户配置了 subordinate id 范围**
- 或者说：**运行身份与授权身份不一致**

宿主机方案需要优先围绕这点处理。

---

## 宿主机侧可落地修复项

## 方案 1：修复 `newuidmap` / `newgidmap` 权限

目标：确保内核允许通过这些 helper 配置 UID/GID 映射。

在节点上检查：

```sh
ls -l /usr/bin/newuidmap /usr/bin/newgidmap
```

预期应看到 setuid 位，例如类似：

```text
-rwsr-xr-x root root ... /usr/bin/newuidmap
-rwsr-xr-x root root ... /usr/bin/newgidmap
```

如果没有 setuid 位，修复：

```sh
sudo chown root:root /usr/bin/newuidmap /usr/bin/newgidmap
sudo chmod 4755 /usr/bin/newuidmap /usr/bin/newgidmap
```

注意：

- 不同发行版可能由 `uidmap` 包自动提供
- 若被安全基线移除了 setuid，需要和平台团队确认是否允许恢复

---

## 方案 2：为运行用户配置 `/etc/subuid` 和 `/etc/subgid`

目标：允许该用户申请从属 UID/GID 范围。

先确认实际是谁在运行容器或 `nsjail`：

```sh
ps -ef | grep vault-service
ps -ef | grep nsjail
id <runtime-user>
```

然后检查：

```sh
grep '^<runtime-user>:' /etc/subuid
grep '^<runtime-user>:' /etc/subgid
```

如果没有条目，添加类似：

```text
<runtime-user>:100000:65536
```

添加方式：

```sh
echo '<runtime-user>:100000:65536' | sudo tee -a /etc/subuid
echo '<runtime-user>:100000:65536' | sudo tee -a /etc/subgid
```

要求：

- `subuid` 和 `subgid` 都要有
- 范围不要和系统已有分配冲突

结合当前实测，当前最直接的修复动作应是：

```sh
echo 'root:100000:65536' | sudo tee -a /etc/subuid
echo 'root:100000:65536' | sudo tee -a /etc/subgid
```

前提：

- 容器/进程继续以 `root` 运行
- 节点安全策略允许给 `root` 分配 subordinate id

如果平台安全基线不允许给 `root` 分配 subordinate id，那么就不能继续让工作负载以 `root` 运行，而应改为让容器以 `appuser` 启动。

这意味着宿主机团队和应用团队需要在下面两条路径中选一条：

### 路径 A：继续使用 `root`

- 为 `root` 配置 `/etc/subuid`
- 为 `root` 配置 `/etc/subgid`
- 保持应用当前运行身份

### 路径 B：切换为 `appuser`

- 保持 `appuser` 的 subordinate id 配置
- 把容器主进程真正切到 `appuser`
- 同步修正 SGX 设备权限、挂载目录权限和运行用户

就当前恢复目标而言，**路径 A 更适合先验证故障是否消失**。

---

## 方案 3：启用 user namespace 能力

目标：避免节点从内核级别直接禁止该能力。

检查：

```sh
cat /proc/sys/kernel/unprivileged_userns_clone
```

如果结果是 `0`，在允许的前提下启用：

```sh
sudo sysctl -w kernel.unprivileged_userns_clone=1
```

持久化：

```sh
echo 'kernel.unprivileged_userns_clone=1' | sudo tee /etc/sysctl.d/99-credbridge-userns.conf
sudo sysctl --system
```

补充检查：

```sh
sysctl user.max_user_namespaces
```

如果过低，也可能导致异常。必要时调高：

```sh
sudo sysctl -w user.max_user_namespaces=15000
```

根据当前实测，这一项已基本满足，因为容器内读取到的是 `1`。因此它不是本次首要瓶颈，不应优先于 `/etc/subgid` 用户失配问题处理。

---

## 方案 4：确认容器运行时允许该能力

目标：避免 Docker / containerd / K8s 安全策略把 `nsjail` 卡住。

### Docker / Compose

至少确认：

- 容器是 `privileged`
- 挂载了 `/sys/fs/cgroup`
- seccomp 未阻止关键系统调用

代码库开发配置里已经体现这一点，见 [docker/docker-compose.yml](/Users/yvan/AIWorkspace/credbridge/docker/docker-compose.yml:81)。

如果线上没有这些条件，需要在部署平台补齐。

### Kubernetes

至少确认 Pod / Container 具备：

- `securityContext.privileged: true` 或等价能力
- `allowPrivilegeEscalation: true`
- 合适的 `capabilities`
- 未被 PodSecurity / PSP / OPA / Kyverno 拦截

还要检查：

- 节点是否允许挂载 `/sys/fs/cgroup`
- Admission Controller 是否拦截特权容器

---

## 方案 5：对 SGX / userns 共存环境做节点级验证

目标：避免 SGX 条件满足，但 `nsjail` 条件不满足。

在节点上建议执行以下联调检查：

### 推荐：在容器内执行的一键检查脚本

下面这段脚本适合直接在目标容器内执行，用来快速判断服务器是否满足当前 `nsjail` 运行前提：

```bash
sh -lc '
set -eu

echo "== basic binaries =="
command -v nsjail
command -v newuidmap
command -v newgidmap
ls -l /usr/bin/newuidmap /usr/bin/newgidmap

echo
echo "== current user =="
id
USER_NAME="$(id -un 2>/dev/null || true)"
echo "user=${USER_NAME:-unknown}"

echo
echo "== subuid/subgid =="
test -f /etc/subuid && cat /etc/subuid || echo "/etc/subuid missing"
test -f /etc/subgid && cat /etc/subgid || echo "/etc/subgid missing"
if [ -n "${USER_NAME:-}" ]; then
  grep -E "^${USER_NAME}:" /etc/subuid >/dev/null && echo "subuid entry ok" || echo "subuid entry missing for ${USER_NAME}"
  grep -E "^${USER_NAME}:" /etc/subgid >/dev/null && echo "subgid entry ok" || echo "subgid entry missing for ${USER_NAME}"
fi

echo
echo "== kernel / cgroup / sgx =="
test -r /proc/sys/kernel/unprivileged_userns_clone && cat /proc/sys/kernel/unprivileged_userns_clone || echo "unprivileged_userns_clone unreadable"
test -d /sys/fs/cgroup && echo "/sys/fs/cgroup mounted" || echo "/sys/fs/cgroup missing"
ls -ld /dev/sgx /dev/sgx/enclave /dev/sgx/provision /dev/sgx_enclave /dev/sgx_provision 2>/dev/null || echo "SGX device not present"

echo
echo "== nsjail smoke test =="
nsjail --mode o \
  --uid_mapping 0:1000:1 \
  --gid_mapping 0:1000:1 \
  -- /bin/sh -lc "id && echo NSJAIL_OK"
'
```

判定标准：

- `newuidmap` / `newgidmap` 必须存在，且通常应带 `setuid`
- 当前运行用户必须在 `/etc/subuid` 和 `/etc/subgid` 中有条目
- `kernel.unprivileged_userns_clone` 最好为 `1`
- `nsjail` smoke test 必须输出 `NSJAIL_OK`

如果检查结果像当前案例这样：

- `current user = root`
- `/etc/subuid`、`/etc/subgid` 里只有 `appuser:...`
- smoke test 报 `newgidmap ... not allowed`

那么优先结论就是：

- 运行身份与 subordinate id 授权身份不一致
- 应先修 `root` / `appuser` 对应关系，而不是继续怀疑 SGX 或 cgroup

### 推荐：宿主机侧快速对照命令

如果已经拿到容器内检查结果，宿主机上可以再执行这组命令做对照：

```bash
ls -l /usr/bin/newuidmap /usr/bin/newgidmap
grep '^root:' /etc/subuid /etc/subgid
grep '^appuser:' /etc/subuid /etc/subgid
cat /proc/sys/kernel/unprivileged_userns_clone
```

### 1. SGX 设备

```sh
ls -l /dev/sgx /dev/sgx/enclave /dev/sgx/provision
```

### 2. userns helper

```sh
command -v newuidmap
command -v newgidmap
ls -l /usr/bin/newuidmap /usr/bin/newgidmap
```

### 3. subuid/subgid

```sh
grep '<runtime-user>' /etc/subuid /etc/subgid
```

对当前问题，建议直接执行：

```sh
grep '^root:' /etc/subuid /etc/subgid
grep '^appuser:' /etc/subuid /etc/subgid
```

目的：

- 验证当前到底授权给了谁
- 避免只看到 `appuser` 存在就误以为配置完成

### 4. 最小 `nsjail` 验证

在节点上执行最小 smoke test：

```sh
nsjail --mode o \
  --uid_mapping 0:1000:1 \
  --gid_mapping 0:1000:1 \
  -- /bin/sh -c 'id && echo ok'
```

如果这一步都失败，说明问题在节点，不在应用。

---

## 推荐实施顺序

1. 确认容器实际运行用户是否为 `root`
2. 为该实际运行用户配置 `/etc/subuid`、`/etc/subgid`
3. 修正 `newuidmap/newgidmap` 权限
4. 重新执行 `nsjail` 最小 smoke test
5. 再验证容器运行时 / K8s 安全策略
6. 最后回到应用层验证 `create-session`

---

## 验收标准

宿主机修复完成后，应满足以下条件：

1. 节点上执行 `nsjail` 最小 smoke test 成功
2. 容器内执行 `newuidmap/newgidmap` 不再报 `not allowed`
3. CredBridge 服务启动时不再出现 user namespace 初始化错误
4. TEE sandbox `create-session` 可以稳定进入 `ready/running`

---

## 失败时的定位方法

如果修复后仍失败，按下面顺序定位：

### 1. 先看节点

检查：

- `/etc/subuid`
- `/etc/subgid`
- `newuidmap/newgidmap` 权限
- `kernel.unprivileged_userns_clone`

并且优先核对：

- 当前容器/进程实际运行用户是谁
- `/etc/subuid` 和 `/etc/subgid` 条目是不是写给了同一个用户

### 2. 再看容器

检查：

- 是否真正以预期用户运行
- 是否具备 `privileged` / 必需 capability
- 是否有 `/sys/fs/cgroup`

### 3. 最后看应用

检查：

- `nsjail` outside uid/gid 是否仍固定为 `1000`
- 是否需要按环境调整映射目标

---

## 边界说明

宿主机方案解决的是“节点是否允许 `nsjail` 做映射”。

它**不负责**解决：

- token scope 不足
- 凭证读取失败
- SGX attestation 失败
- sandbox API 授权失败

因此，实际恢复业务链路时，还需要同时确保 sandbox token 至少具备：

- `credential:decrypt`
- `sandbox:write`
- `sandbox:read`
- `sandbox:execute`

相关依据见 [docs/03-API 参考/REST-API.md](/Users/yvan/AIWorkspace/credbridge/docs/03-API%20参考/REST-API.md:1288)。
