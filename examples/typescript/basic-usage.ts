/**
 * CredBridge TypeScript SDK - 基础使用示例
 *
 * 展示凭证创建、获取、解密和删除的基本操作
 */

import { CredBridgeClient, CredentialType } from '@credbridge/sdk';

// 配置
const BASE_URL = process.env.CREDBRIDGE_BASE_URL || 'https://api.credbridge.io';
const TOKEN = process.env.CREDBRIDGE_TOKEN || 'your-api-token';

async function main() {
  // 初始化客户端
  const client = new CredBridgeClient({
    baseUrl: BASE_URL,
    token: TOKEN,
    timeout: 30000,
    maxRetries: 3,
  });

  console.log('=== CredBridge TypeScript SDK 基础示例 ===\n');

  try {
    // 1. 创建用户名密码凭证
    console.log('1. 创建用户名密码凭证...');
    const userCredential = await client.credentials.createUsernamePassword(
      'schwab',
      'user@example.com',
      'SecurePassword123!',
      { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30 }
    );
    console.log('   创建成功! ID:', userCredential.credentialId);

    // 2. 创建 API Key 凭证
    console.log('\n2. 创建 API Key 凭证...');
    const apiCredential = await client.credentials.createApiKey(
      'stripe',
      'sk_live_51H...',
      'sk_secret_...',
      { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 90 }
    );
    console.log('   创建成功! ID:', apiCredential.credentialId);

    // 3. 获取凭证列表
    console.log('\n3. 获取凭证列表...');
    const { credentials, total } = await client.credentials.list();
    console.log(`   共 ${total} 个凭证:`);
    for (const cred of credentials) {
      console.log(`   - ${cred.credentialId} (${cred.credentialType})`);
    }

    // 4. 获取凭证详情
    console.log('\n4. 获取凭证详情...');
    const credential = await client.credentials.get(userCredential.credentialId);
    console.log('   服务ID:', credential.serviceId);
    console.log('   类型:', credential.credentialType);
    console.log('   创建时间:', credential.createdAt);

    // 5. 解密凭证
    console.log('\n5. 解密凭证...');
    const decrypted = await client.credentials.decrypt(
      userCredential.credentialId,
      '演示解密操作'
    );
    console.log('   用户名:', decrypted.plaintextData.username);
    console.log('   密码:', '***隐藏***');

    // 6. 检查 Token 信息
    console.log('\n6. 检查 Token 信息...');
    const tokenInfo = client.getTokenInfo();
    if (tokenInfo) {
      console.log('   租户ID:', tokenInfo.tenantId);
      console.log('   用户ID:', tokenInfo.userId);
      console.log('   权限:', tokenInfo.scopes.join(', '));
      console.log('   剩余时间:', client.token.getRemainingTimeFormatted());
    }

    // 7. 删除凭证
    console.log('\n7. 删除凭证...');
    await client.credentials.delete(userCredential.credentialId);
    await client.credentials.delete(apiCredential.credentialId);
    console.log('   删除成功!');

    console.log('\n=== 示例执行完成 ===');

  } catch (error) {
    console.error('错误:', error);
    process.exit(1);
  }
}

main();
