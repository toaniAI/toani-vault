import { CredBridgeClient } from "./src/client.js";
import { CredBridgeError, CredBridgeErrorCode } from "./src/types.js";

// 测试无效的 token 格式
try {
  const client = new CredBridgeClient({
    baseUrl: "http://localhost:8080",
    token: "invalid-token-format", // 无效的 token
  });
  console.log("FAIL: 应该抛出 CredBridgeError");
  process.exit(1);
} catch (error) {
  if (error instanceof CredBridgeError) {
    if (error.code === CredBridgeErrorCode.InvalidToken) {
      console.log("PASS: 正确抛出 InvalidToken 错误");
      console.log("错误消息:", error.message);
      process.exit(0);
    } else {
      console.log("FAIL: 错误码不匹配，期望 InvalidToken，实际:", error.code);
      process.exit(1);
    }
  } else {
    console.log("FAIL: 应该抛出 CredBridgeError，实际:", error);
    process.exit(1);
  }
}
