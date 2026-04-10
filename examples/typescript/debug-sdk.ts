import { ToaniVaultSDK, CredentialType } from "@toani/vault-sdk";

const BASE_URL = process.env.TOANI_VAULT_BASE_URL || "http://localhost:8082";
const TOKEN = process.env.TOANI_VAULT_TOKEN || "";

async function main() {
  console.log("BASE_URL:", BASE_URL);
  console.log("TOKEN:", TOKEN.substring(0, 50) + "...");

  const sdk = new ToaniVaultSDK({
    baseUrl: BASE_URL,
    token: TOKEN,
    timeout: 30000,
    maxRetries: 0,
  });

  try {
    console.log("\n尝试创建凭证...");
    const credential = await sdk.credentials.createUsernamePassword(
      "schwab",
      "user@example.com",
      "SecurePassword123!",
      { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30 },
    );
    console.log("创建成功! ID:", credential.credentialId);
  } catch (error: any) {
    console.error("错误类型:", error.constructor.name);
    console.error("错误码:", error.code);
    console.error("错误消息:", error.message);
    console.error("状态码:", error.statusCode);
    console.error("完整错误:", error);
  }
}

main();
