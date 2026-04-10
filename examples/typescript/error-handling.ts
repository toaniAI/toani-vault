/**
 * Toani Vault SDK TypeScript - 错误处理示例
 *
 * 展示各种错误场景的处理方式
 */

import {
  CredBridgeClient,
  CredBridgeError,
  CredBridgeErrorCode,
} from "@toani/vault-sdk";

const BASE_URL = process.env.TOANI_VAULT_BASE_URL || "https://api.toani.io";
const TOKEN = process.env.TOANI_VAULT_TOKEN || "your-api-token";

async function main() {
  const client = new CredBridgeClient({
    baseUrl: BASE_URL,
    token: TOKEN,
  });

  console.log("=== Toani Vault 错误处理示例 ===\n");

  // 示例 1: 凭证不存在
  console.log("1. 处理凭证不存在错误...");
  try {
    await client.credentials.get("non-existent-id");
  } catch (error) {
    if (error instanceof CredBridgeError) {
      if (error.code === CredBridgeErrorCode.NotFound) {
        console.log("   ✓ 正确处理: 凭证不存在");
      } else {
        console.error("   ✗ 未预期的错误:", error.message);
      }
    }
  }

  // 示例 2: 权限不足
  console.log("\n2. 处理权限不足错误...");
  try {
    // 尝试使用只有 read 权限的 Token 进行 decrypt 操作
    const readOnlyClient = new CredBridgeClient({
      baseUrl: BASE_URL,
      token: "read-only-token", // 假设这是只有 read 权限的 Token
    });
    await readOnlyClient.credentials.decrypt("some-id", "测试");
  } catch (error) {
    if (error instanceof CredBridgeError) {
      if (
        error.code === CredBridgeErrorCode.InsufficientScope ||
        error.code === CredBridgeErrorCode.Forbidden
      ) {
        console.log("   ✓ 正确处理: 权限不足");
      }
    }
  }

  // 示例 3: 网络错误重试
  console.log("\n3. 网络错误重试...");
  try {
    // 使用错误的 URL 触发网络错误
    const badClient = new CredBridgeClient({
      baseUrl: "https://invalid.toani.io",
      token: TOKEN,
      maxRetries: 2, // 限制重试次数
    });
    await badClient.credentials.list();
  } catch (error) {
    if (error instanceof CredBridgeError) {
      if (error.isNetworkError()) {
        console.log("   ✓ 正确处理: 网络错误");
        console.log("   是否可重试:", error.isRetryable());
      }
    }
  }

  // 示例 4: Token 过期
  console.log("\n4. 处理 Token 过期...");
  try {
    const expiredClient = new CredBridgeClient({
      baseUrl: BASE_URL,
      token: "expired-token",
    });
    await expiredClient.credentials.list();
  } catch (error) {
    if (error instanceof CredBridgeError) {
      if (error.code === CredBridgeErrorCode.TokenExpired) {
        console.log("   ✓ 正确处理: Token 已过期");
        console.log("   建议: 刷新 Token 或重新登录");
      } else if (error.code === CredBridgeErrorCode.Unauthorized) {
        console.log("   ✓ 正确处理: 未授权");
      }
    }
  }

  // 示例 5: 通用的错误处理函数
  console.log("\n5. 通用错误处理函数...");

  async function safeOperation<T>(
    operation: () => Promise<T>,
    operationName: string,
  ): Promise<T | null> {
    try {
      return await operation();
    } catch (error) {
      if (error instanceof CredBridgeError) {
        const errorMessages: Record<CredBridgeErrorCode, string> = {
          [CredBridgeErrorCode.NotFound]: `${operationName} 失败: 资源不存在`,
          [CredBridgeErrorCode.Unauthorized]: `${operationName} 失败: 未授权，请重新登录`,
          [CredBridgeErrorCode.Forbidden]: `${operationName} 失败: 权限不足`,
          [CredBridgeErrorCode.NetworkError]: `${operationName} 失败: 网络错误`,
          [CredBridgeErrorCode.Timeout]: `${operationName} 失败: 请求超时`,
          [CredBridgeErrorCode.InternalError]: `${operationName} 失败: 服务器内部错误`,
        };

        const message =
          errorMessages[error.code] ||
          `${operationName} 失败: ${error.message}`;
        console.log("   处理结果:", message);
        console.log("   请求ID:", error.requestId);
      } else {
        console.error("   未预期的错误:", error);
      }
      return null;
    }
  }

  // 使用通用错误处理函数
  await safeOperation(() => client.credentials.get("invalid-id"), "获取凭证");

  // 示例 6: 批量错误处理
  console.log("\n6. 批量操作错误处理...");
  const ids = ["valid-id-1", "valid-id-2", "invalid-id"];

  const results = await Promise.allSettled(
    ids.map((id) => client.credentials.get(id)),
  );

  results.forEach((result, index) => {
    if (result.status === "fulfilled") {
      console.log(`   ✓ ${ids[index]}: 成功`);
    } else {
      const error = result.reason;
      if (error instanceof CredBridgeError) {
        console.log(`   ✗ ${ids[index]}: ${error.code} - ${error.message}`);
      }
    }
  });

  console.log("\n=== 错误处理示例完成 ===");
}

main();
