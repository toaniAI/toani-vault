/**
 * Toani Vault SDK TypeScript - 基础使用示例
 *
 * 展示凭证创建、获取、解密和删除的基本操作
 */

import { ToaniVaultSDK, CredentialType } from '@toani/vault-sdk';

// 配置
const BASE_URL = process.env.TOANI_VAULT_BASE_URL || 'https://api.toani.io';
const TOKEN = process.env.TOANI_VAULT_TOKEN || 'your-api-token';

async function main() {
  // 初始化 SDK
  const sdk = new ToaniVaultSDK({
    baseUrl: BASE_URL,
    token: TOKEN,
    timeout: 30000,
    maxRetries: 3,
  });

  console.log('=== Toani Vault SDK TypeScript 基础示例 ===\n');

  try {
    // 1. 创建用户名密码凭证
    console.log('1. 创建用户名密码凭证...');
    const userCredential = await sdk.credentials.createUsernamePassword(
      'schwab',
      'user@example.com',
      'SecurePassword123!',
      { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 30 }
    );
    console.log('   创建成功! ID:', userCredential.credential_id);

    // 2. 创建 API Key 凭证
    console.log('\n2. 创建 API Key 凭证...');
    const apiCredential = await sdk.credentials.createApiKey(
      'stripe',
      'sk_live_51H...',
      'sk_secret_...',
      { expiresAt: Math.floor(Date.now() / 1000) + 86400 * 90 }
    );
    console.log('   创建成功! ID:', apiCredential.credential_id);

    // 3. 获取凭证列表
    console.log('\n3. 获取凭证列表...');
    const { credentials, total } = await sdk.credentials.list();
    console.log(`   共 ${total} 个凭证:`);
    for (const cred of credentials) {
      console.log(`   - ${(cred as any).credential_id || (cred as any).credentialId} (${(cred as any).credential_type || (cred as any).credentialType})`);
    }

    // 4. 获取凭证详情
    console.log('\n4. 获取凭证详情...');
    const credential = await sdk.credentials.get(userCredential.credential_id);
    console.log('   服务ID:', (credential as any).service_id || credential.serviceId);
    console.log('   类型:', (credential as any).credential_type || credential.credentialType);
    console.log('   创建时间:', credential.createdAt);

    // 5. 解密凭证
    console.log('\n5. 解密凭证...');
    const decrypted = await sdk.credentials.decrypt(
      userCredential.credential_id,
      '演示解密操作'
    );
    console.log('   用户名:', (decrypted.plaintext_data as any).username);
    console.log('   密码:', '***隐藏***');

    // 6. 检查 Token 信息
    console.log('\n6. 检查 Token 信息...');
    const tokenInfo = sdk.token.getTokenInfo();
    if (tokenInfo) {
      console.log('   租户ID:', tokenInfo.tenantId);
      console.log('   用户ID:', tokenInfo.userId);
      console.log('   权限:', tokenInfo.scopes.join(', '));
      console.log('   剩余时间:', sdk.token.getRemainingTimeFormatted());
    }

    // 7. 删除凭证
    console.log('\n7. 删除凭证...');
    await sdk.credentials.delete(userCredential.credential_id);
    await sdk.credentials.delete(apiCredential.credential_id);
    console.log('   删除成功!');

    console.log('\n=== 示例执行完成 ===');

  } catch (error) {
    console.error('错误:', error);
    process.exit(1);
  }
}

main();
