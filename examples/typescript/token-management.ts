/**
 * Toani Vault SDK TypeScript - Token 管理示例
 *
 * 展示 Token 验证、权限检查和刷新操作
 */

import { CredBridgeClient, CredBridgeError } from "@toani/vault-sdk";

const BASE_URL = process.env.TOANI_VAULT_BASE_URL || "https://api.toani.io";
const TOKEN = process.env.TOANI_VAULT_TOKEN || "your-api-token";

async function main() {
  const client = new CredBridgeClient({
    baseUrl: BASE_URL,
    token: TOKEN,
  });

  console.log("=== Toani Vault Token 管理示例 ===\n");

  try {
    // 1. 获取 Token 信息
    console.log("1. 获取 Token 信息...");
    const tokenInfo = client.getTokenInfo();
    if (tokenInfo) {
      console.log("   Token ID:", tokenInfo.tokenId);
      console.log("   主题:", tokenInfo.subject);
      console.log("   租户ID:", tokenInfo.tenantId);
      console.log("   用户ID:", tokenInfo.userId);
      console.log(
        "   颁发时间:",
        new Date(tokenInfo.issuedAt * 1000).toISOString(),
      );
      console.log(
        "   过期时间:",
        new Date(tokenInfo.expiresAt * 1000).toISOString(),
      );
    }

    // 2. 检查 Token 有效性
    console.log("\n2. 检查 Token 有效性...");
    console.log("   是否有效:", client.token.isValid());
    console.log("   是否即将过期:", client.token.isExpiringSoon());
    console.log("   剩余秒数:", client.token.getRemainingTime());
    console.log("   剩余时间:", client.token.getRemainingTimeFormatted());

    // 3. 检查权限
    console.log("\n3. 检查权限...");
    console.log(
      "   是否有 read 权限:",
      client.token.hasScope("credential:read"),
    );
    console.log(
      "   是否有 decrypt 权限:",
      client.token.hasScope("credential:decrypt"),
    );
    console.log(
      "   是否有 write 权限:",
      client.token.hasScope("credential:write"),
    );
    console.log("   所有权限:", client.token.getScopes());

    // 4. 权限组合检查
    console.log("\n4. 权限组合检查...");
    const readScopes = ["credential:read", "credential:decrypt"];
    const writeScopes = ["credential:read", "credential:write"];
    console.log(
      "   是否有 read+decrypt:",
      client.token.hasAllScopes(readScopes),
    );
    console.log(
      "   是否有 read+write:",
      client.token.hasAllScopes(writeScopes),
    );
    console.log(
      "   是否有任一 read/write:",
      client.token.hasAnyScope(readScopes),
    );

    // 5. 验证 Token（向服务器确认）
    console.log("\n5. 验证 Token（向服务器确认）...");
    const isValid = await client.token.verify();
    console.log("   服务器验证结果:", isValid);

    // 6. 设置 Token 过期提醒
    console.log("\n6. 设置 Token 过期提醒...");
    const unsubscribe = client.on("token_expiring", (event) => {
      console.log("   ⚠️ Token 即将过期!");
      console.log("   Token 信息:", event.data.tokenInfo);
    });

    // 7. 模拟 Token 更新
    console.log("\n7. 模拟 Token 更新...");
    client.on("token_refreshed", (event) => {
      console.log("   ✅ Token 已刷新");
      console.log("   新 Token:", event.data.token.substring(0, 20) + "...");
    });

    // 取消订阅示例（实际使用时调用）
    // unsubscribe();

    console.log("\n=== Token 管理示例完成 ===");
  } catch (error) {
    if (error instanceof CredBridgeError) {
      console.error("Toani Vault 错误:", error.code, error.message);
    } else {
      console.error("错误:", error);
    }
    process.exit(1);
  }
}

main();
