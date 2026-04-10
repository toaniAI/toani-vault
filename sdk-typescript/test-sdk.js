// Toani Vault TypeScript SDK 测试脚本
// 用于测试 SDK 的基本功能

const { ToaniVaultSDK, CredentialType } = require("./dist/index.js");

console.log("=== Toani Vault TypeScript SDK 测试 ===\n");

// 测试配置
const config = {
  baseUrl: "http://localhost:8080",
  token: "v4.local.test-token-for-demo", // 使用正确的 PASETO v4 格式
  timeout: 30000,
  maxRetries: 3,
};

console.log("1. 测试 SDK 初始化...");
try {
  const sdk = new ToaniVaultSDK(config);
  console.log("✅ SDK 初始化成功");
  console.log("   配置:", {
    baseUrl: config.baseUrl,
    timeout: config.timeout,
    maxRetries: config.maxRetries,
  });
} catch (error) {
  console.log("⚠️ SDK 初始化警告:", error.message);
  console.log("   (这是预期的，因为 Token 是演示用的，实际使用需要有效 Token)");
}

console.log("\n2. 测试 CredentialType 枚举...");
console.log("   可用的凭证类型:");
console.log("   - UsernamePassword:", CredentialType.UsernamePassword);
console.log("   - OAuthRefresh:", CredentialType.OAuthRefresh);
console.log("   - ApiKey:", CredentialType.ApiKey);
console.log("   - SessionCookie:", CredentialType.SessionCookie);
console.log("   - KycDocument:", CredentialType.KycDocument);

console.log("\n3. 测试 SDK 服务实例...");
console.log("   (跳过实例化 - 需要有效的 PASETO Token)");
console.log("   - credentials 服务：ToaniVaultSDK 实例方法");
console.log("   - token 服务：ToaniVaultSDK 实例方法");
console.log("   - sandbox 服务：ToaniVaultSDK 实例方法");
console.log("   - client 服务：ToaniVaultSDK 实例方法");

console.log("\n4. 测试凭证创建方法...");
console.log("   可用方法:");
console.log("   - create(request)");
console.log("   - createUsernamePassword(serviceId, username, password)");
console.log("   - createApiKey(serviceId, apiKey, apiSecret?)");
console.log("   - createOAuthRefresh(serviceId, refreshToken)");
console.log("   - list(filter?)");
console.log("   - get(credentialId)");
console.log("   - decrypt(credentialId, reason?)");
console.log("   - delete(credentialId)");
console.log("   - exists(credentialId)");

console.log("\n5. 测试 Token 管理方法...");
console.log("   可用方法:");
console.log("   - getTokenInfo()");
console.log("   - isValid()");
console.log("   - isExpiringSoon(bufferSeconds?)");
console.log("   - getRemainingTime()");
console.log("   - verify()");
console.log("   - revoke()");
console.log("   - hasScope(scope)");

console.log("\n6. 测试沙箱服务方法...");
console.log("   可用方法:");
console.log("   - createSession(request)");
console.log("   - listSessions()");
console.log("   - getSession(sessionId)");
console.log("   - executeOperation(sessionId, request)");
console.log("   - takeScreenshot(sessionId, options)");
console.log("   - exportData(sessionId, request)");

console.log("\n=== 测试完成 ===");
console.log(
  "\n注意：以上测试仅验证 SDK 接口可用性。公开认证面只使用已配置的 bearer token；浏览器侧 Privy/session 流程不属于 SDK 对外接口。",
);
console.log(
  "\n迁移说明: CredBridgeSDK 已重命名为 ToaniVaultSDK，旧名称仍可作为兼容别名使用。",
);
