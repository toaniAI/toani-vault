#!/bin/bash
# CredBridge TypeScript SDK 示例运行脚本

# 设置环境变量
export CREDBRIDGE_BASE_URL="http://localhost:8082"

# 获取 Token
echo "正在获取 Token..."
RESPONSE=$(curl -s -X POST http://localhost:8082/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}')

export CREDBRIDGE_TOKEN=$(echo "$RESPONSE" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)

if [ -z "$CREDBRIDGE_TOKEN" ]; then
  echo "错误: 无法获取 Token，请确保后端服务运行在 http://localhost:8082"
  exit 1
fi

echo "Token 获取成功!"
echo ""

# 运行示例
case "$1" in
  basic)
    echo "=== 运行基础使用示例 ==="
    npx tsx basic-usage.ts
    ;;
  batch)
    echo "=== 运行批量操作示例 ==="
    npx tsx batch-operations.ts
    ;;
  token)
    echo "=== 运行 Token 管理示例 ==="
    npx tsx token-management.ts
    ;;
  error)
    echo "=== 运行错误处理示例 ==="
    npx tsx error-handling.ts
    ;;
  express)
    echo "=== 运行 Express 集成示例 ==="
    npx tsx express-integration.ts
    ;;
  *)
    echo "使用方法: ./run-examples.sh [basic|batch|token|error|express]"
    echo ""
    echo "可用示例:"
    echo "  basic   - 基础使用示例（创建、获取、解密、删除凭证）"
    echo "  batch   - 批量操作示例"
    echo "  token   - Token 管理示例"
    echo "  error   - 错误处理示例"
    echo "  express - Express 集成示例"
    echo ""
    echo "默认运行基础示例..."
    npx tsx basic-usage.ts
    ;;
esac
