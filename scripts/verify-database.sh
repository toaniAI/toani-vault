#!/bin/bash
# =============================================================================
# CredBridge 数据库验证脚本
# 用于验证 PostgreSQL 数据库连接和初始化
# =============================================================================

set -e

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 数据库配置
DB_HOST="${DB_HOST:-10.11.25.9}"
DB_PORT="${DB_PORT:-15432}"
DB_USER="${DB_USER:-dn}"
DB_PASSWORD="${DB_PASSWORD:-dnXcdYxcv56H}"
DB_NAME="${DB_NAME:-credbridge}"

echo "=========================================="
echo "CredBridge 数据库验证"
echo "=========================================="
echo ""
echo "数据库配置:"
echo "  Host: $DB_HOST"
echo "  Port: $DB_PORT"
echo "  User: $DB_USER"
echo "  Database: $DB_NAME"
echo ""

# 检查是否有 psql
if command -v psql &> /dev/null; then
    echo -e "${GREEN}✓ 找到 psql 命令${NC}"
    PSQL_CMD="psql"
else
    echo -e "${YELLOW}⚠ 未找到 psql，尝试使用 Docker...${NC}"

    # 检查 Docker
    if command -v docker &> /dev/null; then
        echo -e "${GREEN}✓ 找到 Docker${NC}"
        PSQL_CMD="docker run --rm -e PGPASSWORD='$DB_PASSWORD' postgres:16-alpine psql"
    else
        echo -e "${RED}✗ 未找到 psql 或 Docker${NC}"
        echo "请安装 PostgreSQL 客户端或 Docker"
        exit 1
    fi
fi

# 测试连接
echo ""
echo "测试数据库连接..."
if $PSQL_CMD -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "SELECT version();" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ 数据库连接成功${NC}"
else
    echo -e "${RED}✗ 数据库连接失败${NC}"
    echo "请检查:"
    echo "  1. PostgreSQL 服务是否运行"
    echo "  2. 连接地址和端口是否正确"
    echo "  3. 用户名和密码是否正确"
    exit 1
fi

# 检查数据库是否存在
echo ""
echo "检查数据库 '$DB_NAME' 是否存在..."
DB_EXISTS=$($PSQL_CMD -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -t -c "SELECT 1 FROM pg_database WHERE datname='$DB_NAME';" 2>/dev/null | xargs)

if [ "$DB_EXISTS" = "1" ]; then
    echo -e "${GREEN}✓ 数据库 '$DB_NAME' 已存在${NC}"
else
    echo -e "${YELLOW}⚠ 数据库 '$DB_NAME' 不存在，正在创建...${NC}"
    $PSQL_CMD -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d postgres -c "CREATE DATABASE \"$DB_NAME\";" > /dev/null 2>&1
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ 数据库 '$DB_NAME' 创建成功${NC}"
    else
        echo -e "${RED}✗ 数据库创建失败${NC}"
        exit 1
    fi
fi

# 检查数据库连接
echo ""
echo "测试 credbridge 数据库连接..."
if $PSQL_CMD -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT current_database(), current_user;" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ 数据库 '$DB_NAME' 连接成功${NC}"
else
    echo -e "${RED}✗ 数据库 '$DB_NAME' 连接失败${NC}"
    exit 1
fi

# 创建扩展
echo ""
echo "创建 PostgreSQL 扩展..."
$PSQL_CMD -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS pgcrypto; CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"; CREATE EXTENSION IF NOT EXISTS citext;" > /dev/null 2>&1
echo -e "${GREEN}✓ 扩展创建完成${NC}"

# 获取数据库统计信息
echo ""
echo "数据库统计信息:"
echo "------------------------------------------"
TABLE_COUNT=$($PSQL_CMD -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';" 2>/dev/null | xargs)
echo "  表数量: $TABLE_COUNT"

# 显示表列表
if [ "$TABLE_COUNT" -gt 0 ]; then
    echo ""
    echo "现有表:"
    $PSQL_CMD -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -c "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name;" 2>/dev/null | grep -v "^ table_name$" | grep -v "^--" | grep -v "^($" | sed 's/^/  - /'
fi

echo ""
echo "=========================================="
echo -e "${GREEN}数据库验证完成！${NC}"
echo "=========================================="
echo ""
echo "下一步:"
echo "  1. 启动应用程序: cargo run"
echo "  2. 应用程序会自动创建所需的表结构"
echo ""
