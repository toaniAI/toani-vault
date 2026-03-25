#!/bin/bash
# SGX 环境检查脚本
# 用于验证 SGX 硬件和软件环境是否就绪

set -e

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "========================================"
echo "SGX 硬件环境检查"
echo "========================================"
echo ""

# 检查操作系统
echo "1. 操作系统检查"
echo "----------------------------------------"
if [ "$(uname)" != "Linux" ]; then
    echo -e "${RED}✗ 仅支持 Linux 系统${NC}"
    exit 1
fi
echo -e "${GREEN}✓ 操作系统：$(uname -s) $(uname -r)${NC}"
echo ""

# 检查 CPU SGX 支持
echo "2. CPU SGX 支持检查"
echo "----------------------------------------"
if grep -q "sgx" /proc/cpuinfo; then
    echo -e "${GREEN}✓ CPU 支持 SGX${NC}"
    
    if grep -q "sgx_lc" /proc/cpuinfo; then
        echo -e "${GREEN}✓ 支持 FLC (Flexible Launch Control)${NC}"
    else
        echo -e "${YELLOW}⚠ 不支持 FLC，DCAP 可能受限${NC}"
    fi
    
    if grep -q "sgx_dcap" /proc/cpuinfo; then
        echo -e "${GREEN}✓ 支持 DCAP${NC}"
    else
        echo -e "${YELLOW}⚠ 未检测到 DCAP 标志${NC}"
    fi
else
    echo -e "${RED}✗ CPU 不支持 SGX 或 BIOS 中未启用${NC}"
    echo ""
    echo "请检查 BIOS 设置，确保 SGX 设置为 'Enabled' 或 'Software Controlled'"
    exit 1
fi
echo ""

# 检查 SGX 驱动
echo "3. SGX 驱动检查"
echo "----------------------------------------"
if lsmod | grep -q "intel_sgx"; then
    echo -e "${GREEN}✓ SGX 驱动已加载 (intel_sgx)${NC}"
    lsmod | grep intel_sgx
elif lsmod | grep -q "isgx"; then
    echo -e "${GREEN}✓ SGX 驱动已加载 (isgx)${NC}"
    lsmod | grep isgx
else
    echo -e "${YELLOW}⚠ SGX 驱动未加载${NC}"
    echo "尝试加载驱动..."
    if sudo modprobe intel_sgx 2>/dev/null; then
        echo -e "${GREEN}✓ 驱动加载成功${NC}"
    else
        echo -e "${RED}✗ 驱动加载失败${NC}"
        echo "请安装 SGX 驱动："
        echo "  Ubuntu/Debian: sudo apt install intel-sgx-sdk"
        echo "  或从 https://github.com/intel/linux-sgx-driver 编译安装"
    fi
fi
echo ""

# 检查 SGX 设备节点
echo "4. SGX 设备节点检查"
echo "----------------------------------------"
SGX_DEVICES=0

if [ -c "/dev/sgx_enclave" ]; then
    echo -e "${GREEN}✓ /dev/sgx_enclave 存在${NC}"
    ls -la /dev/sgx_enclave
    ((SGX_DEVICES++))
else
    echo -e "${RED}✗ /dev/sgx_enclave 不存在${NC}"
fi

if [ -c "/dev/sgx_provision" ]; then
    echo -e "${GREEN}✓ /dev/sgx_provision 存在${NC}"
    ls -la /dev/sgx_provision
    ((SGX_DEVICES++))
else
    echo -e "${YELLOW}⚠ /dev/sgx_provision 不存在${NC}"
fi

if [ -c "/dev/sgx" ]; then
    echo -e "${GREEN}✓ /dev/sgx 存在${NC}"
    ls -la /dev/sgx
    ((SGX_DEVICES++))
fi

if [ $SGX_DEVICES -eq 0 ]; then
    echo -e "${RED}✗ 未找到任何 SGX 设备节点${NC}"
    exit 1
fi
echo ""

# 检查 AESM 服务
echo "5. AESM 服务检查"
echo "----------------------------------------"
if systemctl is-active --quiet aesmd 2>/dev/null; then
    echo -e "${GREEN}✓ AESM 服务运行中${NC}"
    systemctl status aesmd --no-pager -l
elif systemctl is-active --quiet sgx-aesm-service 2>/dev/null; then
    echo -e "${GREEN}✓ sgx-aesm-service 运行中${NC}"
    systemctl status sgx-aesm-service --no-pager -l
else
    echo -e "${RED}✗ AESM 服务未运行${NC}"
    echo "启动 AESM 服务："
    echo "  sudo systemctl start aesmd"
    echo "  sudo systemctl enable aesmd"
fi
echo ""

# 检查 DCAP 库
echo "6. DCAP 库检查"
echo "----------------------------------------"
DCAP_LIBS=0

if ldconfig -p 2>/dev/null | grep -q "libsgx_dcap_ql"; then
    echo -e "${GREEN}✓ libsgx_dcap_ql 已安装${NC}"
    ((DCAP_LIBS++))
else
    echo -e "${RED}✗ libsgx_dcap_ql 未安装${NC}"
fi

if ldconfig -p 2>/dev/null | grep -q "libsgx_quote_ex"; then
    echo -e "${GREEN}✓ libsgx_quote_ex 已安装${NC}"
    ((DCAP_LIBS++))
else
    echo -e "${YELLOW}⚠ libsgx_quote_ex 未安装${NC}"
fi

if ldconfig -p 2>/dev/null | grep -q "libsgx_dcap_default_qpl"; then
    echo -e "${GREEN}✓ libsgx_dcap_default_qpl 已安装${NC}"
    ((DCAP_LIBS++))
else
    echo -e "${YELLOW}⚠ libsgx_dcap_default_qpl 未安装${NC}"
fi

if [ $DCAP_LIBS -eq 0 ]; then
    echo -e "${RED}✗ DCAP 库未正确安装${NC}"
    echo "安装 DCAP 库："
    echo "  sudo apt install libsgx-dcap-ql libsgx-dcap-ql-dev"
fi
echo ""

# 检查 SGX SDK
echo "7. SGX SDK 检查"
echo "----------------------------------------"
if [ -f "/opt/intel/sgxsdk/environment" ]; then
    echo -e "${GREEN}✓ Intel SGX SDK 已安装${NC}"
    source /opt/intel/sgxsdk/environment
    echo "  SDK 版本：$SGX_SDK_VERSION"
elif command -v sgx_sign &> /dev/null; then
    echo -e "${GREEN}✓ SGX SDK 工具可用${NC}"
    sgx_sign version
else
    echo -e "${YELLOW}⚠ SGX SDK 未找到${NC}"
    echo "安装 SGX SDK："
    echo "  https://github.com/intel/linux-sgx"
fi
echo ""

# 检查 EPC 内存
echo "8. EPC 内存检查"
echo "----------------------------------------"
if [ -f "/sys/devices/system/cpu/sgx/epc_size" ]; then
    EPC_SIZE=$(cat /sys/devices/system/cpu/sgx/epc_size)
    echo -e "${GREEN}✓ EPC 内存：$((EPC_SIZE / 1024 / 1024)) MB${NC}"
elif dmesg | grep -i "sgx.*epc" | tail -1 > /dev/null; then
    echo -e "${GREEN}✓ EPC 内存信息:${NC}"
    dmesg | grep -i "sgx.*epc" | tail -1
else
    echo -e "${YELLOW}⚠ 无法获取 EPC 内存大小${NC}"
fi
echo ""

# 检查 PCCS 配置（可选）
echo "9. PCCS 配置检查（可选）"
echo "----------------------------------------"
if [ -f "/etc/sgx_default_qcnl.conf" ]; then
    echo -e "${GREEN}✓ PCCS 配置文件存在${NC}"
    echo "配置内容："
    cat /etc/sgx_default_qcnl.conf
else
    echo -e "${YELLOW}⚠ PCCS 配置文件不存在${NC}"
    echo "创建配置文件：/etc/sgx_default_qcnl.conf"
fi
echo ""

# 总结
echo "========================================"
echo "检查总结"
echo "========================================"

ISSUES=0

if ! grep -q "sgx" /proc/cpuinfo; then
    echo -e "${RED}[严重]${NC} CPU 不支持 SGX"
    ((ISSUES++))
fi

if [ $SGX_DEVICES -eq 0 ]; then
    echo -e "${RED}[严重]${NC} 未找到 SGX 设备节点"
    ((ISSUES++))
fi

if ! systemctl is-active --quiet aesmd 2>/dev/null; then
    echo -e "${RED}[严重]${NC} AESM 服务未运行"
    ((ISSUES++))
fi

if [ $DCAP_LIBS -eq 0 ]; then
    echo -e "${YELLOW}[警告]${NC} DCAP 库未完全安装"
fi

echo ""
if [ $ISSUES -eq 0 ]; then
    echo -e "${GREEN}✓ 所有检查通过，SGX 环境就绪${NC}"
    exit 0
else
    echo -e "${RED}✗ 发现 $ISSUES 个问题需要解决${NC}"
    exit 1
fi
