# Intel SGX TEE 部署需求文档

## 文档信息

- **项目名称**: CredBridge TEE 安全增强
- **文档版本**: 1.0.0
- **适用对象**: 运维工程师、系统管理员
- **部署环境**: 生产环境 (Intel SGX 硬件)
- **创建日期**: 2026-03-20

---

## 一、部署概述

### 1.1 项目简介

CredBridge 是一个基于 Intel SGX TEE (Trusted Execution Environment) 的凭证安全桥接系统。通过在 SGX Enclave 中执行敏感操作，实现：

- **硬件级隔离**: 敏感数据和处理过程在 Enclave 中运行，操作系统和 Hypervisor 无法访问
- **远程认证**: 客户端可验证 Enclave 身份和完整性
- **密钥保护**: 使用 SGX Sealing 机制保护密钥材料
- **防篡改**: Enclave 代码和数据的完整性由 CPU 硬件保证

### 1.2 部署目标

在 Intel SGX 硬件服务器上完成以下部署：

1. 安装和配置 Intel SGX 驱动及依赖库
2. 部署 CredBridge 应用（启用 TEE 硬件模式）
3. 配置 Intel DCAP 远程认证服务
4. 验证 TEE 功能完整性和安全性

### 1.3 部署范围

- **服务器**: 支持 Intel SGX 的物理服务器或云服务器
- **操作系统**: Linux (Ubuntu 20.04/22.04 或兼容发行版)
- **网络**: 需要访问 Intel PCS 证书服务（生产环境）

---

## 二、硬件要求

### 2.1 CPU 要求

| 要求项 | 最低配置 | 推荐配置 | 验证方法 |
|--------|---------|---------|---------|
| CPU 型号 | Intel 第 6 代酷睿 | Intel 第 10 代酷睿或更新 | `cat /proc/cpuinfo` |
| SGX 支持 | SGX1 | SGX1 + SGX2 | `grep -E "sgx|sgx_lc" /proc/cpuinfo` |
| FLC 支持 | 必需 | 必需 | `grep sgx_lc /proc/cpuinfo` |
| 虚拟化 | VT-x 启用 | VT-x + VT-d 启用 | BIOS 设置 |

### 2.2 内存要求

| 要求项 | 最低配置 | 推荐配置 | 说明 |
|--------|---------|---------|------|
| 系统内存 | 4GB | 8GB+ | 包含 EPC 内存开销 |
| EPC 内存 | 64MB | 128MB+ | Enclave 页面缓存，BIOS 中分配 |
| Swap | 2GB | 4GB+ | 防止内存溢出 |

### 2.3 存储要求

| 要求项 | 要求 | 说明 |
|--------|------|------|
| 系统盘 | 50GB+ SSD | 包含应用和依赖 |
| 数据盘 | 根据业务需求 | 持久化数据（建议加密） |

### 2.4 网络要求

| 要求项 | 要求 | 说明 |
|--------|------|------|
| 外网访问 | 必需 | 访问 Intel PCS 证书服务 (https://api.trustedservices.intel.com) |
| 防火墙 | 开放 443 端口 | HTTPS 出站流量 |
| 代理 | 如有需要配置 | 某些企业环境需要代理访问 Intel |

---

## 三、BIOS/UEFI 设置

### 3.1 设置步骤

**重要**: 以下设置需要在服务器 BIOS/UEFI 中配置，需要重启服务器。

1. **进入 BIOS 设置**
   ```
   重启服务器，在启动时按 Del/F2/F10 键进入 BIOS
   ```

2. **启用虚拟化技术**
   ```
   Advanced → CPU Configuration → Intel Virtualization Technology: Enabled
   Advanced → System Agent Configuration → VT-d: Enabled (如果可用)
   ```

3. **启用 SGX**
   ```
   Advanced → CPU Configuration → Intel SGX: Enabled
   或
   Security → Intel SGX: Enabled
   ```

4. **配置 SGX 控制**
   ```
   Advanced → CPU Configuration → SGX Control: Software Controlled
   或
   Advanced → CPU Configuration → SGX Control: Enabled
   ```

5. **分配 EPC 内存**
   ```
   Advanced → CPU Configuration → SGX EPC Size: 128MB (或最大值)
   注意：某些 BIOS 自动分配，无需手动设置
   ```

6. **启用 FLC (Flexible Launch Control)**
   ```
   Advanced → CPU Configuration → Flexible Launch Control: Enabled
   ```

7. **保存并退出**
   ```
   按 F10 保存设置并重启
   ```

### 3.2 验证设置

重启后执行以下命令验证：

```bash
# 检查 SGX 标志
grep -E "sgx|sgx_lc" /proc/cpuinfo

# 应该看到输出包含 sgx 和 sgx_lc
```

---

## 四、操作系统要求

### 4.1 支持的操作系统

| 操作系统 | 版本 | 内核版本 | 推荐度 |
|---------|------|---------|--------|
| Ubuntu Server | 22.04 LTS | 5.15+ | ★★★★★ |
| Ubuntu Server | 20.04 LTS | 5.11+ | ★★★★☆ |
| CentOS Stream | 9 | 5.14+ | ★★★★☆ |
| RHEL | 9.x | 5.14+ | ★★★★☆ |
| Debian | 11+ | 5.10+ | ★★★☆☆ |

**推荐**: Ubuntu Server 22.04 LTS（最佳兼容性和文档支持）

### 4.2 系统更新

```bash
# Ubuntu/Debian
sudo apt update && sudo apt upgrade -y

# CentOS/RHEL
sudo yum update -y
```

---

## 五、软件依赖安装

### 5.1 安装基础依赖

```bash
# Ubuntu/Debian
sudo apt update

sudo apt install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    cmake \
    curl \
    wget \
    git
```

### 5.2 安装 Intel SGX 驱动

#### 方案 A: 使用内核内置驱动（推荐，内核 >= 5.11）

```bash
# 检查内核版本
uname -r

# 如果内核 >= 5.11，SGX 驱动已内置
# 加载驱动模块
sudo modprobe intel_sgx

# 验证驱动加载
lsmod | grep sgx

# 应该看到 intel_sgx 模块
```

#### 方案 B: 安装 Intel 官方驱动（内核 < 5.11）

```bash
# 下载驱动
wget https://download.01.org/intel-sgx/sgx-linux/2.24/dists/ubuntu-22.04/sgx_linux_x64_driver_2.24.bin

# 添加执行权限
chmod +x sgx_linux_x64_driver_2.24.bin

# 安装驱动
sudo ./sgx_linux_x64_driver_2.24.bin

# 加载驱动
sudo modprobe intel_sgx
```

### 5.3 安装 SGX DCAP 库

```bash
# Ubuntu 22.04
sudo apt install -y \
    libsgx-dcap-ql \
    libsgx-dcap-ql-dev \
    libsgx-quote-ex \
    libsgx-quote-ex-dev \
    libsgx-dcap-default-qpl \
    libsgx-dcap-default-qpl-dev \
    sgx-aesm-service \
    libsgx-urts

# CentOS/RHEL
sudo yum install -y \
    libsgx-dcap-ql \
    libsgx-quote-ex \
    libsgx-dcap-default-qpl
```

### 5.4 安装 Intel SGX SDK（可选，用于开发）

```bash
# 添加 Intel 软件仓库
wget -qO - https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key | sudo apt-key add -

echo "deb https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main" | \
    sudo tee /etc/apt/sources.list.d/intel-sgx.list

sudo apt update

# 安装 SDK
sudo apt install -y libsgx-sdk libsgx-sdk-dev
```

### 5.5 安装 PCCS（可选，用于企业环境缓存证书）

```bash
# Ubuntu
sudo apt install -y sgx-dcap-pccs

# 配置 PCCS
sudo vim /etc/sgx-dcap-pccs.conf

# 设置 Intel PCS API Key（如果需要）
# 访问 https://api.portal.trustedservices.intel.com/ 注册获取

# 启动服务
sudo systemctl start pccs
sudo systemctl enable pccs
```

### 5.6 配置 AESM 服务

```bash
# 启动 AESM 服务
sudo systemctl start aesmd
sudo systemctl enable aesmd

# 检查服务状态
systemctl status aesmd

# 应该看到 active (running) 状态

# 查看日志
journalctl -u aesmd -f
```

---

## 六、环境验证

### 6.1 运行环境检查脚本

创建检查脚本 `/tmp/check_sgx_env.sh`:

```bash
#!/bin/bash
# SGX 环境检查脚本

echo "========================================"
echo "SGX 硬件环境检查"
echo "========================================"
echo ""

# 1. 操作系统检查
echo "1. 操作系统检查"
echo "----------------------------------------"
if [ -f /etc/os-release ]; then
    cat /etc/os-release | grep "PRETTY_NAME"
    echo "✓ 操作系统检查通过"
else
    echo "✗ 无法识别操作系统"
    exit 1
fi
echo ""

# 2. CPU SGX 支持检查
echo "2. CPU SGX 支持检查"
echo "----------------------------------------"
if grep -q "sgx" /proc/cpuinfo; then
    echo "✓ CPU 支持 SGX"
    grep -E "sgx|sgx_lc|sgx_dcap" /proc/cpuinfo | head -1
else
    echo "✗ CPU 不支持 SGX"
    exit 1
fi
echo ""

# 3. SGX 驱动检查
echo "3. SGX 驱动检查"
echo "----------------------------------------"
if lsmod | grep -q "intel_sgx\|isgx"; then
    echo "✓ SGX 驱动已加载"
    lsmod | grep -E "intel_sgx|isgx"
else
    echo "✗ SGX 驱动未加载"
    echo "尝试加载驱动：sudo modprobe intel_sgx"
    exit 1
fi
echo ""

# 4. SGX 设备节点检查
echo "4. SGX 设备节点检查"
echo "----------------------------------------"
if [ -c "/dev/sgx_enclave" ] && [ -c "/dev/sgx_provision" ]; then
    echo "✓ SGX 设备节点存在"
    ls -la /dev/sgx_*
else
    echo "✗ SGX 设备节点不存在"
    echo "检查驱动是否正确加载"
    exit 1
fi
echo ""

# 5. AESM 服务检查
echo "5. AESM 服务检查"
echo "----------------------------------------"
if systemctl is-active --quiet aesmd; then
    echo "✓ AESM 服务运行中"
else
    echo "✗ AESM 服务未运行"
    echo "启动服务：sudo systemctl start aesmd"
    exit 1
fi
echo ""

# 6. DCAP 库检查
echo "6. DCAP 库检查"
echo "----------------------------------------"
if ldconfig -p | grep -q "libsgx_dcap_ql"; then
    echo "✓ libsgx_dcap_ql 已安装"
else
    echo "✗ libsgx_dcap_ql 未安装"
    exit 1
fi

if ldconfig -p | grep -q "libsgx_quote_ex"; then
    echo "✓ libsgx_quote_ex 已安装"
else
    echo "✗ libsgx_quote_ex 未安装"
    exit 1
fi
echo ""

# 7. EPC 内存检查
echo "7. EPC 内存检查"
echo "----------------------------------------"
if [ -f "/sys/devices/system/cpu/sgx/epc_size" ]; then
    EPC_SIZE=$(cat /sys/devices/system/cpu/sgx/epc_size)
    echo "✓ EPC 内存：$((EPC_SIZE / 1024 / 1024)) MB"
    if [ $EPC_SIZE -lt 67108864 ]; then  # 64MB
        echo "⚠ 警告：EPC 内存小于 64MB，可能影响性能"
    fi
else
    echo "⚠ 无法读取 EPC 内存大小"
    dmesg | grep -i "sgx.*epc" | tail -1
fi
echo ""

# 8. 网络连通性检查
echo "8. 网络连通性检查 (Intel PCS)"
echo "----------------------------------------"
if curl -s --connect-timeout 5 https://api.trustedservices.intel.com >/dev/null; then
    echo "✓ 可访问 Intel PCS 服务"
else
    echo "⚠ 无法访问 Intel PCS 服务"
    echo "检查防火墙和代理设置"
fi
echo ""

echo "========================================"
echo "检查总结"
echo "========================================"
echo "✓ 所有检查通过，SGX 环境就绪"
echo ""
echo "下一步："
echo "1. 部署 CredBridge 应用"
echo "2. 配置 TEE_MODE=hardware"
echo "3. 运行功能验证测试"
```

执行检查：

```bash
chmod +x /tmp/check_sgx_env.sh
/tmp/check_sgx_env.sh
```

### 6.2 预期输出

```
========================================
SGX 硬件环境检查
========================================

1. 操作系统检查
----------------------------------------
✓ 操作系统检查通过

2. CPU SGX 支持检查
----------------------------------------
✓ CPU 支持 SGX
flags: ... sgx sgx_lc ...

3. SGX 驱动检查
----------------------------------------
✓ SGX 驱动已加载
intel_sgx    123456  0

4. SGX 设备节点检查
----------------------------------------
✓ SGX 设备节点存在
crw------- 1 root root 10, 125 Mar 20 10:00 /dev/sgx_enclave
crw------- 1 root root 10, 126 Mar 20 10:00 /dev/sgx_provision

5. AESM 服务检查
----------------------------------------
✓ AESM 服务运行中

6. DCAP 库检查
----------------------------------------
✓ libsgx_dcap_ql 已安装
✓ libsgx_quote_ex 已安装

7. EPC 内存检查
----------------------------------------
✓ EPC 内存：128 MB

8. 网络连通性检查 (Intel PCS)
----------------------------------------
✓ 可访问 Intel PCS 服务

========================================
检查总结
========================================
✓ 所有检查通过，SGX 环境就绪
```

---

## 七、CredBridge 应用部署

### 7.1 获取应用

```bash
# 从 Git 仓库克隆
git clone https://github.com/your-org/credbridge.git
cd credbridge

# 或下载预编译二进制
wget https://releases.credbridge.io/vault-service-latest-linux-x64.tar.gz
tar -xzf vault-service-latest-linux-x64.tar.gz
```

### 7.2 编译应用（如需要）

```bash
# 安装 Rust (如果未安装)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 编译释放版本
cargo build --release

# 二进制文件位置
./target/release/vault-service
```

### 7.3 配置环境变量

创建 `/etc/credbridge/.env` 文件：

```bash
# 基础配置
CREDBRIDGE_PORT=8080
RUST_LOG=info
RUST_BACKTRACE=0

# TEE 配置（关键）
TEE_MODE=hardware
TEE_DEBUG=false
TEE_INTEL_SGX_ENABLED=true
SGX_MODE=hardware

# 数据库配置
DATABASE_URL=postgresql://credbridge:PASSWORD@localhost:5432/credbridge
DATABASE_MAX_CONNECTIONS=20
DATABASE_MIN_CONNECTIONS=5

# Redis 配置
REDIS_URL=redis://localhost:6379
REDIS_CLUSTER_ENABLED=false

# DCAP 配置（可选）
DCAP_PCS_BASE_URL=https://api.trustedservices.intel.com/sgx/certification/v4
DCAP_USE_TEST_ENV=false

# 安全配置
SECURITY_HSTS_ENABLED=true
SECURITY_CSP_ENABLED=true
RATE_LIMIT_ENABLED=true
RATE_LIMIT_REQUESTS_PER_SECOND=100
```

### 7.4 创建系统服务

创建 `/etc/systemd/system/credbridge.service`:

```ini
[Unit]
Description=CredBridge TEE Security Service
After=network.target postgresql.service redis.service
Wants=postgresql.service redis.service

[Service]
Type=notify
User=credbridge
Group=credbridge

# 环境变量
EnvironmentFile=/etc/credbridge/.env

# 执行文件
ExecStart=/opt/credbridge/vault-service
WorkingDirectory=/opt/credbridge

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/credbridge /var/log/credbridge
PrivateTmp=true

# 资源限制
LimitNOFILE=65535
LimitNPROC=4096

# 重启策略
Restart=always
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

### 7.5 启动服务

```bash
# 创建用户和目录
sudo useradd -r -s /bin/false credbridge
sudo mkdir -p /opt/credbridge /var/lib/credbridge /var/log/credbridge
sudo chown -R credbridge:credbridge /opt/credbridge /var/lib/credbridge /var/log/credbridge

# 复制应用
sudo cp target/release/vault-service /opt/credbridge/
sudo cp -r migrations /opt/credbridge/

# 重新加载 systemd
sudo systemctl daemon-reload

# 启动服务
sudo systemctl start credbridge
sudo systemctl enable credbridge

# 检查状态
sudo systemctl status credbridge

# 查看日志
sudo journalctl -u credbridge -f
```

---

## 八、功能验证

### 8.1 运行 TEE 功能测试

```bash
cd /path/to/credbridge

# 运行 SGX 硬件测试
cargo test --test sgx_hardware_tests --release -- --test-threads=1

# 运行 DCAP 测试
cargo test test_dcap --release -- --nocapture

# 运行远程认证测试
cargo test test_attestation --release -- --nocapture
```

### 8.2 验证 API 端点

```bash
# 健康检查
curl http://localhost:8080/health

# TEE 状态检查
curl http://localhost:8080/api/v1/tee/status

# 获取 DCAP Quote
curl http://localhost:8080/api/v1/tee/quote

# 验证远程认证
curl -X POST http://localhost:8080/api/v1/tee/attest \
  -H "Content-Type: application/json" \
  -d '{"challenge": "random_challenge_string"}'
```

### 8.3 检查日志

```bash
# 查看应用日志
tail -f /var/log/credbridge/vault-service.log

# 应该看到类似输出：
# [INFO  vault_service::tee] SGX Enclave initialized successfully
# [INFO  vault_service::tee::dcap] DCAP Quote generated
# [INFO  vault_service::tee::attestation] Remote attestation successful
```

---

## 九、安全加固建议

### 9.1 系统安全

```bash
# 1. 禁用不必要的服务
sudo systemctl disable bluetooth cups modemmanager

# 2. 配置防火墙
sudo ufw enable
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 8080/tcp  # CredBridge API
sudo ufw allow 22/tcp   # SSH

# 3. 启用 SELinux/AppArmor
sudo setenforce 1  # CentOS/RHEL
# 或
sudo aa-enforce /etc/apparmor.d/*  # Ubuntu

# 4. 配置审计日志
sudo auditctl -w /opt/credbridge -p rwxa -k credbridge
```

### 9.2 SGX 安全配置

```bash
# 1. 限制设备节点访问
sudo chown root:credbridge /dev/sgx_enclave /dev/sgx_provision
sudo chmod 660 /dev/sgx_enclave /dev/sgx_provision

# 2. 禁用调试模式（生产环境必须）
# 在 /etc/credbridge/.env 中设置：
# TEE_DEBUG=false

# 3. 使用签名 Enclave（生产环境）
# 配置 Enclave 签名密钥
```

### 9.3 网络安全

```bash
# 1. 配置 SSL/TLS
# 使用 Nginx 反向代理
sudo apt install nginx

# 配置 /etc/nginx/sites-available/credbridge
server {
    listen 443 ssl;
    server_name credbridge.example.com;
    
    ssl_certificate /etc/ssl/certs/credbridge.crt;
    ssl_certificate_key /etc/ssl/private/credbridge.key;
    
    location / {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}

# 2. 启用防火墙规则
sudo ufw allow 443/tcp
```

---

## 十、监控和维护

### 10.1 监控指标

```bash
# 1. 服务健康状态
systemctl is-active credbridge

# 2. EPC 内存使用
cat /sys/devices/system/cpu/sgx/epc_size

# 3. AESM 服务状态
systemctl status aesmd

# 4. 应用日志
journalctl -u credbridge --since "1 hour ago"
```

### 10.2 日志轮转

创建 `/etc/logrotate.d/credbridge`:

```
/var/log/credbridge/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0640 credbridge credbridge
    postrotate
        systemctl reload credbridge
    endscript
}
```

### 10.3 定期更新

```bash
# 1. 系统更新
sudo apt update && sudo apt upgrade -y

# 2. SGX 驱动更新
# 关注 Intel 官方发布：https://download.01.org/intel-sgx/

# 3. DCAP 库更新
sudo apt upgrade libsgx-dcap-ql libsgx-quote-ex

# 4. 应用更新
cd /path/to/credbridge
git pull
cargo build --release
sudo systemctl restart credbridge
```

---

## 十一、故障排查

### 11.1 常见问题

#### 问题 1: SGX 设备节点不存在

**症状**:
```
Error: SGX device not found
```

**解决方案**:
```bash
# 检查驱动
lsmod | grep sgx

# 重新加载驱动
sudo modprobe -r intel_sgx
sudo modprobe intel_sgx

# 检查 BIOS 设置（需要重启）
```

#### 问题 2: AESM 服务失败

**症状**:
```
Error: AESM service not responding
```

**解决方案**:
```bash
# 重启服务
sudo systemctl restart aesmd

# 查看日志
journalctl -u aesmd -n 50

# 检查依赖
ldconfig -p | grep sgx
```

#### 问题 3: Quote 生成失败

**症状**:
```
Error: Quote generation failed
```

**解决方案**:
```bash
# 检查 DCAP 库
ldd /opt/credbridge/vault-service | grep sgx

# 检查 EPC 内存
cat /sys/devices/system/cpu/sgx/epc_size

# 关闭其他 SGX 应用释放内存
```

#### 问题 4: 无法访问 Intel PCS

**症状**:
```
Error: Failed to connect to Intel PCS
```

**解决方案**:
```bash
# 检查网络
curl -I https://api.trustedservices.intel.com

# 配置代理（如果需要）
export https_proxy=http://proxy.example.com:8080

# 使用本地 PCCS
# 安装并配置 sgx-dcap-pccs
```

### 11.2 收集诊断信息

```bash
# 创建诊断包
mkdir -p ~/credbridge_diagnosis

# 系统信息
uname -a > ~/credbridge_diagnosis/system.txt
cat /etc/os-release >> ~/credbridge_diagnosis/system.txt

# CPU 和 SGX 信息
lscpu > ~/credbridge_diagnosis/cpu.txt
grep -E "sgx|sgx_lc" /proc/cpuinfo >> ~/credbridge_diagnosis/cpu.txt

# 驱动信息
lsmod | grep sgx > ~/credbridge_diagnosis/drivers.txt
dmesg | grep -i sgx >> ~/credbridge_diagnosis/dmesg.txt

# 服务状态
systemctl status aesmd > ~/credbridge_diagnosis/aesmd.txt
systemctl status credbridge >> ~/credbridge_diagnosis/credbridge.txt

# 设备节点
ls -la /dev/sgx_* > ~/credbridge_diagnosis/devices.txt

# 应用日志
journalctl -u credbridge --no-pager > ~/credbridge_diagnosis/app.log

# 打包
tar czf credbridge_diagnosis_$(date +%Y%m%d).tar.gz ~/credbridge_diagnosis
```

---

## 十二、部署检查清单

### 12.1 部署前检查

- [ ] 硬件支持 SGX（CPU 标志包含 sgx, sgx_lc）
- [ ] BIOS 中启用 SGX、VT-x、FLC
- [ ] EPC 内存分配 >= 64MB
- [ ] 操作系统为支持的 Linux 发行版
- [ ] 系统已更新到最新补丁

### 12.2 安装检查

- [ ] SGX 驱动已加载（intel_sgx 模块）
- [ ] 设备节点存在（/dev/sgx_enclave, /dev/sgx_provision）
- [ ] AESM 服务已启动
- [ ] DCAP 库已安装
- [ ] 可访问 Intel PCS 服务

### 12.3 配置检查

- [ ] TEE_MODE=hardware
- [ ] TEE_DEBUG=false
- [ ] TEE_INTEL_SGX_ENABLED=true
- [ ] 数据库连接配置正确
- [ ] Redis 连接配置正确

### 12.4 验证检查

- [ ] 服务启动成功（systemctl status credbridge）
- [ ] 存活检查通过（`/health` 端点）
- [ ] 就绪检查通过（`/ready` 或 `/health/detail` 端点）
- [ ] SGX 硬件测试通过
- [ ] DCAP Quote 生成成功
- [ ] 远程认证功能正常

---

## 十三、联系支持

### 13.1 内部支持

- **技术支持邮箱**: support@credbridge.io
- **紧急联系**: +86-XXX-XXXX-XXXX
- **文档地址**: https://docs.credbridge.io

### 13.2 外部资源

- **Intel SGX 开发者文档**: https://www.intel.com/content/www/us/en/developer/tools/software-guard-extensions/overview.html
- **Intel DCAP 文档**: https://www.intel.com/content/www/us/en/developer/tools/software-guard-extensions/data-center-attestation-primitives.html
- **GitHub Issues**: https://github.com/credbridge/credbridge/issues

---

## 附录 A: 快速命令参考

```bash
# 环境检查
/tmp/check_sgx_env.sh

# 服务管理
sudo systemctl start|stop|restart|status credbridge
sudo systemctl start|stop|restart|status aesmd

# 日志查看
journalctl -u credbridge -f
journalctl -u aesmd -f

# SGX 状态
lsmod | grep sgx
cat /sys/devices/system/cpu/sgx/epc_size
ls -la /dev/sgx_*

# 运行测试
cargo fmt --check
cargo clippy --tests -- -D warnings
cargo test
TEE_MODE=hardware cargo test --test sgx_hardware_tests -- --ignored --test-threads=1

# 收集诊断信息
tar czf diagnosis.tar.gz ~/credbridge_diagnosis
```

---

## 附录 B: 版本兼容性

| CredBridge 版本 | Intel SGX Driver | DCAP Library | 最低内核 |
|----------------|------------------|--------------|---------|
| 0.1.x | 2.11+ | 1.15+ | 5.11 |
| 0.2.x | 2.20+ | 1.18+ | 5.14 |
| 最新 | 2.24+ | 1.20+ | 5.15 |

---

**文档结束**
