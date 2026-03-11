/**
 * CredBridge TypeScript SDK - 批量操作示例
 *
 * 展示批量创建、获取和删除凭证的操作
 */

import { CredBridgeClient, CredentialType } from '@credbridge/sdk';

const BASE_URL = process.env.CREDBRIDGE_BASE_URL || 'https://api.credbridge.io';
const TOKEN = process.env.CREDBRIDGE_TOKEN || 'your-api-token';

async function main() {
  const client = new CredBridgeClient({
    baseUrl: BASE_URL,
    token: TOKEN,
  });

  console.log('=== CredBridge 批量操作示例 ===\n');

  const createdIds: string[] = [];

  try {
    // 1. 批量创建凭证
    console.log('1. 批量创建凭证...');
    const services = [
      { service: 'bank-of-america', username: 'user1@example.com', password: 'pass1' },
      { service: 'chase', username: 'user2@example.com', password: 'pass2' },
      { service: 'wells-fargo', username: 'user3@example.com', password: 'pass3' },
    ];

    for (const svc of services) {
      const credential = await client.credentials.createUsernamePassword(
        svc.service,
        svc.username,
        svc.password
      );
      createdIds.push(credential.credentialId);
      console.log(`   创建成功: ${svc.service} -> ${credential.credentialId}`);
    }

    // 2. 批量获取凭证详情
    console.log('\n2. 批量获取凭证详情...');
    const details = await Promise.all(
      createdIds.map(id =>
        client.credentials.get(id).catch(err => ({ error: err.message, credentialId: id }))
      )
    );
    console.log('   获取结果:', details.length);

    // 3. 按服务过滤
    console.log('\n3. 按服务过滤凭证...');
    const { credentials: chaseCreds } = await client.credentials.list({
      serviceId: 'chase'
    });
    console.log(`   Chase 凭证数: ${chaseCreds.length}`);

    // 4. 按类型过滤
    console.log('\n4. 按类型过滤凭证...');
    const { credentials: passwordCreds } = await client.credentials.list({
      credentialType: CredentialType.UsernamePassword
    });
    console.log(`   用户名密码凭证数: ${passwordCreds.length}`);

    // 5. 批量删除
    console.log('\n5. 批量删除凭证...');
    const deleteResults = await Promise.all(
      createdIds.map(id =>
        client.credentials.delete(id).then(() => ({ id, success: true }))
          .catch(err => ({ id, success: false, error: err.message }))
      )
    );
    console.log('   删除结果:', deleteResults);

    console.log('\n=== 批量操作示例完成 ===');

  } catch (error) {
    console.error('错误:', error);
    // 清理已创建的凭证
    console.log('\n清理已创建的凭证...');
    for (const id of createdIds) {
      try {
        await client.credentials.delete(id);
      } catch {
        // 忽略删除错误
      }
    }
    process.exit(1);
  }
}

main();
