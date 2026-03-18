import { CredBridgeSDK, CredentialType } from '@credbridge/sdk';

const BASE_URL = process.env.CREDBRIDGE_BASE_URL || 'http://localhost:8082';
const TOKEN = process.env.CREDBRIDGE_TOKEN || '';

async function main() {
  console.log('BASE_URL:', BASE_URL);
  console.log('TOKEN length:', TOKEN.length);
  console.log('TOKEN prefix:', TOKEN.substring(0, 50));
  
  const sdk = new CredBridgeSDK({
    baseUrl: BASE_URL,
    token: TOKEN,
    timeout: 30000,
    maxRetries: 0,
  });

  // 检查 token 信息
  const tokenInfo = sdk.token.getTokenInfo();
  console.log('\nToken Info:', tokenInfo);

  try {
    console.log('\n尝试创建凭证...');
    const credential = await sdk.credentials.create({
      serviceId: 'schwab',
      credentialType: CredentialType.UsernamePassword,
      plaintextData: { username: 'user@example.com', password: 'test123' },
    });
    console.log('创建成功! ID:', credential.credentialId);
  } catch (error: any) {
    console.error('\n错误类型:', error.constructor.name);
    console.error('错误码:', error.code);
    console.error('错误消息:', error.message);
    console.error('状态码:', error.statusCode);
    console.error('请求ID:', error.requestId);
  }
}

main();
