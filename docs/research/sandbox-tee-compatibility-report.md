# TEE内沙箱解决方案技术调查报告

**调查日期**: 2026-03-16
**调查人**: Backend Specialist Agent
**项目**: CredBridge TEE安全执行环境

---

## 执行摘要

本报告调查了6种主流开源沙箱解决方案在TEE（可信执行环境）内的运行可行性。核心发现：

| 方案 | TEE兼容性 | 推荐度 | 关键限制 |
|------|-----------|--------|----------|
| **gVisor** | 中等 | 首选 | 需要部分host内核支持 |
| **nsjail** | 高 | 次选 | 功能相对简单 |
| **Isolate** | 高 | 备选 | 仅适合特定场景 |
| **Kata Containers** | 低 | 不推荐 | 依赖KVM，TEE内不可用 |
| **Firecracker** | 低 | 不推荐 | 依赖KVM |
| **Chrome Sandbox** | 中等 | 参考 | 非通用方案 |

**核心结论**: TEE内无法运行完整VM（KVM不可用），因此基于microVM的方案（Firecracker、Kata）不可行。推荐采用用户空间内核方案（gVisor）或轻量级namespace方案（nsjail）。

---

## 1. Firecracker (AWS MicroVM)

### 1.1 架构概述

```
┌─────────────────────────────────────────────────────────────┐
│                        Host OS                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Firecracker VMM (用户空间)              │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │   │
│  │  │  MicroVM 1  │  │  MicroVM 2  │  │  MicroVM N  │ │   │
│  │  │  (Guest OS) │  │  (Guest OS) │  │  (Guest OS) │ │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘ │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│                    KVM (内核模块)                           │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 技术依赖分析

| 依赖项 | 要求 | TEE支持 | 说明 |
|--------|------|---------|------|
| KVM | 必需 | 否 | TEE内无法加载内核模块 |
| /dev/kvm | 设备访问 | 否 | Nitro Enclaves无此设备 |
| 内核虚拟化 | 硬件支持 | 受限 | SGX/SEV不支持嵌套虚拟化 |
| seccomp | 推荐 | 部分 | 依赖host内核配置 |

### 1.3 TEE兼容性评估

**Intel SGX**: 不兼容
- SGX enclave无法运行完整操作系统
- 不支持KVM嵌套虚拟化
- 缺乏必要的ring 0权限

**AMD SEV**: 不兼容
- SEV-SNP提供VM加密，但Firecracker本身需要KVM
- 嵌套虚拟化支持有限
- 启动流程冲突

**AWS Nitro Enclaves**: 不兼容
- Nitro Enclaves本身就是轻量级VM
- 内部无KVM支持
- 设计目标与Firecracker重叠但互斥

### 1.4 资源开销

| 指标 | 数值 | 说明 |
|------|------|------|
| 内存开销 | 5-15MB | 每个MicroVM |
| 启动时间 | <125ms | 冷启动 |
| 二进制大小 | ~10MB | 静态链接 |
| CPU | 1-2 vCPU | 典型配置 |

### 1.5 结论

**不可行**。Firecracker依赖KVM，而TEE环境内无法提供KVM支持。这是一个根本性架构冲突。

---

## 2. gVisor (Google用户空间内核)

### 2.1 架构概述

```
┌─────────────────────────────────────────────────────────────┐
│                        Host OS                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    gVisor                           │   │
│  │  ┌─────────────────────────────────────────────┐   │   │
│  │  │           Sentry (用户空间内核)              │   │   │
│  │  │  ┌─────────┐ ┌─────────┐ ┌─────────┐       │   │   │
│  │  │  │ App 1   │ │ App 2   │ │ App N   │       │   │   │
│  │  │  │ (Guest) │ │ (Guest) │ │ (Guest) │       │   │   │
│  │  │  └─────────┘ └─────────┘ └─────────┘       │   │   │
│  │  └─────────────────────────────────────────────┘   │   │
│  │           │                                         │   │
│  │      Gofer (文件系统代理)                           │   │
│  └───────────┼─────────────────────────────────────────┘   │
│              │                                              │
│      ptrace/seccomp (系统调用拦截)                         │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 运行模式

gVisor提供三种运行模式，TEE兼容性不同：

| 模式 | 机制 | TEE兼容性 | 性能 | 说明 |
|------|------|-----------|------|------|
| **KVM** | 使用KVM进行上下文切换 | 低 | 高 | 与Firecracker类似问题 |
| **ptrace** | 使用ptrace拦截syscall | 高 | 中 | 纯用户空间，TEE友好 |
| **systrap** | 混合模式 | 中 | 中高 | 需要部分内核支持 |

### 2.3 TEE兼容性评估

**Intel SGX**: 部分兼容
- **ptrace模式**: 可行。纯用户空间实现，无需特权操作
- 已有研究项目：Graphene-SGX + gVisor集成
- 挑战：SGX内部分syscall受限（如mmap、线程创建）

**AMD SEV**: 部分兼容
- SEV VM内可运行gVisor
- ptrace模式无需嵌套虚拟化
- 需要验证seccomp-bpf支持

**AWS Nitro Enclaves**: 部分兼容
- Nitro Enclaves支持ptrace
- 已有AWS官方文档提及gVisor可能性
- 限制：无KVM模式，只能用ptrace

### 2.4 资源开销

| 指标 | 数值 | 说明 |
|------|------|------|
| 内存开销 | 50-200MB | 取决于应用 |
| 启动时间 | 100-500ms | ptrace模式 |
| 二进制大小 | ~50MB | runsc |
| syscall开销 | 2-10x | 相比原生 |

### 2.5 Rust集成

```rust
// gVisor OCI运行时集成示例
use std::process::Command;

pub struct GVisorSandbox {
    runtime_path: PathBuf,
    rootfs: PathBuf,
}

impl GVisorSandbox {
    pub async fn run_container(&self, image: &str) -> Result<Container, Error> {
        Command::new(&self.runtime_path)
            .arg("run")
            .arg("--runtime=ptrace")  // TEE兼容模式
            .arg(image)
            .output()
            .await?
    }
}
```

### 2.6 安全特性

- **隔离级别**: 进程级 + 用户空间内核
- **攻击面**: 仅Sentry和Gofer两个进程
- **Capability**: 可drop所有capability运行
- **seccomp**: 内置严格seccomp策略

### 2.7 结论

**推荐首选**。gVisor的ptrace模式完全在用户空间运行，不依赖KVM，是TEE内最成熟的沙箱方案。已有学术研究和工业实践支持。

---

## 3. Kata Containers

### 3.1 架构概述

```
┌─────────────────────────────────────────────────────────────┐
│                        Host OS                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           Containerd / CRI-O                        │   │
│  │                    │                                │   │
│  │              Kata Runtime                          │   │
│  │  ┌─────────────────────────────────────────────┐   │   │
│  │  │              轻量级VM (QEMU/Cloud-Hypervisor)│   │   │
│  │  │  ┌─────────────────────────────────────┐   │   │   │
│  │  │  │      Guest Kernel + Container       │   │   │   │
│  │  │  │  ┌─────────┐ ┌─────────┐ ┌────────┐ │   │   │   │
│  │  │  │  │ App 1   │ │ App 2   │ │  ...   │ │   │   │   │
│  │  │  │  └─────────┘ └─────────┘ └────────┘ │   │   │   │
│  │  │  └─────────────────────────────────────┘   │   │   │
│  │  └─────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│                    KVM (必需)                              │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 技术依赖分析

| 依赖项 | 要求 | TEE支持 | 说明 |
|--------|------|---------|------|
| KVM | 必需 | 否 | 核心依赖 |
| QEMU/Cloud-Hypervisor | 必需 | 否 | 需要VMM |
| 内核模块 | vhost等 | 否 | 多模块依赖 |
| OCI Runtime | containerd | 部分 | 可移植 |

### 3.3 TEE兼容性评估

**Intel SGX**: 不兼容
- 需要完整VM支持
- SGX无法运行QEMU

**AMD SEV**: 理论兼容（外部）
- SEV可加密Kata的VM
- 但这是在TEE外运行Kata，非题目要求

**AWS Nitro Enclaves**: 不兼容
- Nitro Enclaves本身就是轻量级VM
- 无法在Enclave内再运行VM

### 3.4 资源开销

| 指标 | 数值 | 说明 |
|------|------|------|
| 内存开销 | 128-512MB | 每个Pod |
| 启动时间 | 1-3s | 冷启动 |
| 存储 | ~100MB | 镜像大小 |

### 3.5 结论

**不可行**。Kata Containers本质是在VM中运行容器，依赖KVM。与Firecracker类似，无法在TEE内运行。

---

## 4. Chrome/Chromium Sandbox

### 4.1 架构概述

```
┌─────────────────────────────────────────────────────────────┐
│                     Browser Process                         │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Renderer Process (Sandboxed)           │   │
│  │  ┌─────────────────────────────────────────────┐   │   │
│  │  │           Sandbox IPC Layer                 │   │   │
│  │  │  ┌─────────┐ ┌─────────┐ ┌─────────┐       │   │   │
│  │  │  │  Blink  │ │   V8    │ │  ...    │       │   │   │
│  │  │  └─────────┘ └─────────┘ └─────────┘       │   │   │
│  │  └─────────────────────────────────────────────┘   │   │
│  │           │                                         │   │
│  │      seccomp-bpf + namespace                      │   │
│  └───────────┼─────────────────────────────────────────┘   │
│              │                                              │
│    setuid sandbox / namespace sandbox                      │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 多层沙箱架构

Chrome使用多层沙箱策略：

| 层级 | 机制 | 目的 | TEE兼容性 |
|------|------|------|-----------|
| L1 | setuid helper | 初始特权降级 | 部分 |
| L2 | PID namespace | 进程隔离 | 高 |
| L3 | Network namespace | 网络隔离 | 高 |
| L4 | seccomp-bpf | 系统调用过滤 | 部分 |
| L5 | chroot | 文件系统隔离 | 高 |

### 4.3 TEE兼容性评估

**Intel SGX**: 部分兼容
- 已有项目：Fortanix EnclaveOS使用类似技术
- 挑战：SGX内seccomp-bpf支持不确定
- 需要自定义syscall broker

**AMD SEV**: 兼容
- SEV VM内可完整运行Chrome沙箱
- 所有Linux特性可用

**AWS Nitro Enclaves**: 部分兼容
- namespace支持完整
- seccomp需要验证
- 无setuid（单用户环境）

### 4.4 资源开销

| 指标 | 数值 | 说明 |
|------|------|------|
| 内存开销 | 20-50MB | 每个Renderer |
| 启动时间 | 50-100ms | 进程启动 |
| 代码复杂度 | 高 | 大量平台特定代码 |

### 4.5 Rust集成

```rust
// Chrome沙箱模式启发的设计
pub struct ChromeStyleSandbox {
    namespace_flags: CloneFlags,
    seccomp_policy: SeccompPolicy,
}

impl ChromeStyleSandbox {
    pub fn new() -> Self {
        Self {
            namespace_flags: CLONE_NEWPID | CLONE_NEWNET | CLONE_NEWNS,
            seccomp_policy: SeccompPolicy::strict(),
        }
    }

    pub fn enter(&self) -> Result<(), Error> {
        // 1. 创建namespace
        unshare(self.namespace_flags)?;

        // 2. 加载seccomp策略
        self.seccomp_policy.load()?;

        // 3. chroot到空目录
        chroot("/var/empty")?;
        chdir("/")?;

        // 4. drop privileges
        drop_capabilities()?;

        Ok(())
    }
}
```

### 4.6 结论

**参考设计**。Chrome沙箱设计优秀，但不是通用沙箱解决方案。其多层隔离思想值得借鉴，但直接复用困难。适合作为设计参考，而非直接采用。

---

## 5. nsjail (Namespace Jail)

### 5.1 架构概述

```
┌─────────────────────────────────────────────────────────────┐
│                        Host OS                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    nsjail                           │   │
│  │  ┌─────────────────────────────────────────────┐   │   │
│  │  │              隔离环境                        │   │   │
│  │  │  ┌─────────┐                                │   │   │
│  │  │  │ Target  │                                │   │   │
│  │  │  │ Process │                                │   │   │
│  │  │  └─────────┘                                │   │   │
│  │  │       │                                     │   │   │
│  │  │  ┌────┴────┐                                │   │   │
│  │  │  │  Kafel  │  (seccomp策略)                 │   │   │
│  │  │  └─────────┘                                │   │   │
│  │  └─────────────────────────────────────────────┘   │   │
│  │           │                                         │   │
│  │  ┌────────┴────────┐                                │   │
│  │  │ Linux Namespaces │                               │   │
│  │  │ PID, NET, MNT, USER, IPC, UTS, CGROUP          │   │   │
│  │  └─────────────────┘                               │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 5.2 隔离机制

nsjail基于Linux namespace和seccomp：

| 机制 | 用途 | TEE兼容性 | 配置复杂度 |
|------|------|-----------|------------|
| PID namespace | 进程树隔离 | 高 | 低 |
| Mount namespace | 文件系统视图 | 高 | 中 |
| Network namespace | 网络隔离 | 高 | 低 |
| User namespace | UID/GID映射 | 部分 | 中 |
| IPC namespace | 信号量/共享内存 | 高 | 低 |
| UTS namespace | 主机名隔离 | 高 | 低 |
| Cgroup | 资源限制 | 部分 | 中 |
| seccomp | 系统调用过滤 | 部分 | 高 |

### 5.3 TEE兼容性评估

**Intel SGX**: 高兼容
- 纯用户空间实现
- 不依赖内核模块
- 已有OCCLUM等项目验证namespace在SGX可行
- 限制：SGX 2.0的EDMM支持动态内存，早期版本受限

**AMD SEV**: 高兼容
- SEV VM内namespace完整支持
- 所有Linux特性可用
- seccomp-bpf正常工作

**AWS Nitro Enclaves**: 高兼容
- Nitro Enclaves基于Linux
- namespace完整支持
- 需要验证seccomp配置

### 5.4 资源开销

| 指标 | 数值 | 说明 |
|------|------|------|
| 内存开销 | 1-5MB | 进程级开销 |
| 启动时间 | 10-50ms | 进程启动 |
| 二进制大小 | ~1MB | 静态链接 |
| 依赖 | 无 | 仅需libc |

### 5.5 Rust集成

```rust
// nsjail风格沙箱的Rust实现
use nix::sched::{unshare, CloneFlags};
use nix::unistd::{chroot, chdir, setuid, setgid};

pub struct NsjailSandbox {
    config: NsjailConfig,
}

#[derive(Debug, Clone)]
pub struct NsjailConfig {
    pub root_dir: PathBuf,
    pub uid: u32,
    pub gid: u32,
    pub namespaces: CloneFlags,
    pub seccomp_policy: Option<String>,
    pub rlimits: Vec<RLimit>,
}

impl NsjailSandbox {
    pub fn new(config: NsjailConfig) -> Self {
        Self { config }
    }

    pub fn enter(&self) -> Result<(), SandboxError> {
        // 1. 创建namespaces
        unshare(self.config.namespaces)?;

        // 2. 设置rootfs
        if self.config.root_dir.exists() {
            chroot(&self.config.root_dir)?;
            chdir("/")?;
        }

        // 3. 加载seccomp策略
        if let Some(ref policy) = self.config.seccomp_policy {
            load_seccomp_policy(policy)?;
        }

        // 4. 设置资源限制
        for rlimit in &self.config.rlimits {
            setrlimit(rlimit.resource, rlimit.soft, rlimit.hard)?;
        }

        // 5. 降级权限
        setgid(self.config.gid)?;
        setuid(self.config.uid)?;

        Ok(())
    }

    pub fn run<F, T>(&self, f: F) -> Result<T, SandboxError>
    where
        F: FnOnce() -> T,
    {
        // fork + 在子进程中进入沙箱
        match unsafe { fork() }? {
            ForkResult::Parent { child } => {
                waitpid(child, None)?;
            }
            ForkResult::Child => {
                self.enter()?;
                let result = f();
                exit(0);
            }
        }
    }
}
```

### 5.6 安全特性

- **隔离级别**: 进程级（namespace）
- **攻击面**: 内核syscall接口
- **Capability**: 可完全drop
- **审计**: 支持seccomp日志

### 5.7 结论

**推荐次选**。nsjail架构简单，纯用户空间，TEE兼容性极佳。适合对隔离要求不是极端严格的场景。配置相对简单，易于集成。

---

## 6. Isolate (IOI竞赛沙箱)

### 6.1 架构概述

```
┌─────────────────────────────────────────────────────────────┐
│                        Host OS                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                   isolate                           │   │
│  │  ┌─────────────────────────────────────────────┐   │   │
│  │  │              Control Group                   │   │   │
│  │  │  ┌─────────┐                                │   │   │
│  │  │  │  Box    │  (隔离环境)                     │   │   │
│  │  │  │ Process │                                │   │   │
│  │  │  └─────────┘                                │   │   │
│  │  │       │                                     │   │   │
│  │  │  ┌────┴────┐                                │   │   │
│  │  │  │  Cgroup │  (资源控制)                     │   │   │
│  │  │  │ v1/v2   │                                │   │   │
│  │  │  └─────────┘                                │   │   │
│  │  └─────────────────────────────────────────────┘   │   │
│  │           │                                         │   │
│  │  ┌────────┴────────┐                                │   │
│  │  │  Namespace + chroot                            │   │   │
│  │  └─────────────────┘                               │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 设计目标

Isolate专为算法竞赛设计：

| 特性 | 实现 | 说明 |
|------|------|------|
| 时间限制 | CPU cgroup + setitimer | 精确到ms |
| 内存限制 | Memory cgroup | 含swap控制 |
| 输出限制 | Filesystem quota | 防止磁盘炸弹 |
| 进程限制 | PID cgroup | 防止fork炸弹 |
| 网络隔离 | Network namespace | 完全无网络 |
| 文件隔离 | chroot + bind mount | 只读rootfs |

### 6.3 TEE兼容性评估

**Intel SGX**: 部分兼容
- cgroup需要cgroupfs，SGX内可能受限
- namespace支持良好
- 时间/内存限制功能需要验证

**AMD SEV**: 高兼容
- SEV VM内cgroup完整支持
- 所有限制功能可用

**AWS Nitro Enclaves**: 部分兼容
- cgroup v2支持需要验证
- namespace完整支持
- 可能需要调整配置

### 6.4 资源开销

| 指标 | 数值 | 说明 |
|------|------|------|
| 内存开销 | <1MB | 极轻量 |
| 启动时间 | 5-20ms | 进程启动 |
| 二进制大小 | ~100KB | 极简 |
| 依赖 | cgroupfs | 需要挂载 |

### 6.5 Rust集成

```rust
// Isolate风格沙箱的Rust实现
pub struct IsolateSandbox {
    box_id: u32,
    config: IsolateConfig,
}

#[derive(Debug, Clone)]
pub struct IsolateConfig {
    pub time_limit_ms: u64,
    pub memory_limit_kb: u64,
    pub fsize_limit_kb: u64,
    pub processes_limit: u32,
    pub wall_time_limit_ms: u64,
    pub extra_time_ms: u64,
    pub cgroup_version: CgroupVersion,
}

pub struct SandboxResult {
    pub status: ExitStatus,
    pub time_used_ms: u64,
    pub time_used_sys_ms: u64,
    pub memory_used_kb: u64,
    pub exit_code: i32,
    pub message: Option<String>,
}

impl IsolateSandbox {
    pub fn new(box_id: u32, config: IsolateConfig) -> Self {
        Self { box_id, config }
    }

    pub fn init(&self) -> Result<(), Error> {
        // 创建cgroup
        let cgroup_path = format!("/sys/fs/cgroup/isolate/{}`, self.box_id);
        fs::create_dir_all(&cgroup_path)?;

        // 设置资源限制
        self.set_cgroup_limit("memory.max", self.config.memory_limit_kb * 1024)?;
        self.set_cgroup_limit("cpu.max", format!("{} 1000000", self.config.time_limit_ms))?;
        self.set_cgroup_limit("pids.max", self.config.processes_limit)?;

        // 创建chroot目录
        let box_dir = format!("/var/local/lib/isolate/{}`, self.box_id);
        fs::create_dir_all(&box_dir)?;

        Ok(())
    }

    pub fn run(&self, command: &[&str]) -> Result<SandboxResult, Error> {
        // fork + 在cgroup中运行
        // 收集统计信息
        // 返回结果
    }

    pub fn cleanup(&self) -> Result<(), Error> {
        // 清理cgroup
        // 删除chroot
    }
}
```

### 6.6 结论

**备选方案**。Isolate设计精简，专为受控执行设计。如果CredBridge的需求与竞赛沙箱类似（严格的资源限制、短期任务），Isolate是很好的选择。但通用性不如gVisor和nsjail。

---

## 综合对比

### 架构兼容性矩阵

| 方案 | SGX | SEV | Nitro | 特权要求 | 内核依赖 |
|------|-----|-----|-------|----------|----------|
| Firecracker | 否 | 否 | 否 | root | KVM |
| gVisor | 部分 | 部分 | 部分 | 可选 | 可选 |
| Kata | 否 | 否 | 否 | root | KVM |
| Chrome | 部分 | 是 | 部分 | 初始 | seccomp |
| nsjail | 是 | 是 | 是 | 初始 | namespace |
| Isolate | 部分 | 是 | 部分 | 初始 | cgroup |

### 资源开销对比

```
内存开销 (MB)
│
200 ┤                                          ┌── gVisor
    │                                          │
100 ┤                    ┌── Kata              │
    │                    │                     │
 50 ┤      ┌── Firecracker                    │
    │      │           │                     │
 20 ┤──────┼───────────┼─────────────────────┤
    │      │           │                     │
 10 ┤      │           │                     │
    │  ┌───┘           │                     │
  5 ┤  │ Chrome        │                     │
    │  │               │                     │
  1 ┤──┼───────────────┼─────────────────────┤── nsjail
    │  │               │                     │   Isolate
  0 ┼──┴───────────────┴─────────────────────┴──────────
       Firecracker  Kata    gVisor   Chrome   nsjail  Isolate
```

### 安全隔离级别

| 方案 | 隔离级别 | 攻击面 | 逃逸难度 |
|------|----------|--------|----------|
| Firecracker | VM级 | 很小 | 极高 |
| gVisor | 用户空间内核 | 小 | 高 |
| Kata | VM级 | 很小 | 极高 |
| Chrome | 进程级 | 中 | 中 |
| nsjail | 进程级 | 中 | 中 |
| Isolate | 进程级 | 中 | 中 |

---

## 推荐方案

### 方案1: gVisor (首选)

**适用场景**: 需要运行完整Linux应用，兼容OCI容器

**优势**:
- 成熟的用户空间内核实现
- 支持ptrace模式，TEE兼容
- 良好的Rust生态（可通过OCI集成）
- Google持续维护

**实施路径**:
```
1. 在TEE内编译gVisor (ptrace模式)
2. 实现Rust OCI runtime绑定
3. 配置最小化seccomp策略
4. 集成到CredBridge执行引擎
```

### 方案2: nsjail (次选)

**适用场景**: 轻量级隔离，简单应用执行

**优势**:
- 极轻量，资源开销小
- 配置简单
- 完全用户空间
- TEE兼容性最佳

**实施路径**:
```
1. 用Rust重写核心功能（或绑定C库）
2. 设计声明式配置格式
3. 实现资源限制和监控
4. 集成到CredBridge
```

### 方案3: 混合方案 (推荐)

结合gVisor和nsjail的优点：

```
┌─────────────────────────────────────────────────────────────┐
│                    CredBridge TEE                           │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Sandbox Manager (Rust)                 │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐ │   │
│  │  │  gVisor     │  │   nsjail    │  │  Custom     │ │   │
│  │  │  Runtime    │  │   Runtime   │  │  Runtime    │ │   │
│  │  │  (复杂应用)  │  │  (简单任务)  │  │  (特殊场景)  │ │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘ │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## 实施建议

### 短期（1-2周）

1. **POC验证**: 在目标TEE环境中测试gVisor ptrace模式
2. **nsjail评估**: 测试nsjail在TEE内的完整功能
3. **性能基准**: 建立syscall开销、启动时间基准

### 中期（1个月）

1. **Rust绑定**: 为选定方案开发Rust API
2. **配置设计**: 设计CredBridge沙箱配置DSL
3. **安全审计**: 审查沙箱配置的安全性

### 长期（2-3个月）

1. **生产集成**: 完整集成到CredBridge执行引擎
2. **监控告警**: 实现沙箱逃逸检测
3. **文档完善**: 编写运维和开发文档

---

## 参考资源

### gVisor
- 官方文档: https://gvisor.dev/docs/
- GitHub: https://github.com/google/gvisor
- TEE相关研究: "Graphene-SGX: A Practical Library OS for Unmodified Applications on SGX"

### nsjail
- GitHub: https://github.com/google/nsjail
- Kafel (seccomp DSL): https://github.com/google/kafel

### Isolate
- GitHub: https://github.com/ioi/isolate
- 文档: https://www.ucw.cz/moe/isolate.html

### Chrome Sandbox
- 设计文档: https://chromium.googlesource.com/chromium/src/+/main/docs/linux/sandboxing.md
- 源码: https://source.chromium.org/chromium/chromium/src/+/main:sandbox/

---

## 附录: TEE环境限制总结

| 限制 | SGX | SEV | Nitro | 影响 |
|------|-----|-----|-------|------|
| KVM | 否 | 否 | 否 | 排除Firecracker/Kata |
| 内核模块 | 否 | 部分 | 否 | 限制驱动支持 |
| 特权操作 | 受限 | 部分 | 受限 | 需要设计降级策略 |
| 内存动态扩展 | 2.0+ | 是 | 是 | 影响大内存应用 |
| 多线程 | 支持 | 支持 | 支持 | 无显著影响 |
| 文件系统 | 需定制 | 完整 | 受限 | 需要libOS或定制 |

---

*报告完成*
