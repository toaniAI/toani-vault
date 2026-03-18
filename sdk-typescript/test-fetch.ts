// 测试 SDK 的 URL 构建
const BASE_URL = 'http://localhost:8082';
const path = '/credentials';
const url = `${BASE_URL.replace(/\/$/, '')}/api/v1${path}`;
console.log('URL:', url);

// 测试 fetch
const token = 'v4.local.wy4uUIp_TVM3hxg7wIv29Z3ebYnrg725pLOoXagQecYY-ozWWTIpv1cV9XBNXak7r3v2G6-bstCahBF7uqq1kZUf79eQ9nWJnsSo1jCGlSjijAfucR_ne3gGr0lx2dCxkc74yfQpq37GqJVxFzzz6AwmbpYeMxybKMWp-S3cCfGNLEQXHuBxGf4X9v3kxKsM6EqF_Lyvrsw75vsptuOAbqwQ5Q6PO9sk4d-RpCw146NOMpKmw56_e8L_MtnBo0gg5ya7U7P1mk7KE3Q8HVLwZmz4HIcZcKipoC91INVnPriwk-UM6sD7UGHPBalFK3HYiLu5dfZHAzEziTHc537c2p5T8ltQB9D-kyYOq0oc7WCRw6S0h0Meva56zNwh';

fetch(url, {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    'Authorization': 'Bearer ' + token
  },
  body: JSON.stringify({
    service_id: 'test',
    credential_type: 'username_password',
    plaintext_data: {username:'test', password:'test123'}
  })
}).then(async r => {
  const data = await r.json();
  console.log('Status:', r.status);
  console.log('Response:', data);
}).catch(e => console.error('Fetch Error:', e.message));
