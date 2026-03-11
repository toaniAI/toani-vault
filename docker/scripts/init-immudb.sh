#!/bin/bash
# immudb 初始化脚本
# 创建数据库和表结构

set -e

echo "=== immudb 初始化开始 ==="

# 等待 immudb 启动
echo "等待 immudb 启动..."
sleep 5

# 创建 credbridge_audit 数据库（如果不存在）
echo "创建审计数据库..."
./immuadmin database create credbridge_audit --if-not-exists || true

# 显示数据库列表
echo "当前数据库列表:"
./immuclient database list || true

echo "=== immudb 初始化完成 ==="
