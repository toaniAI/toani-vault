// 测试 SDK 的 URL 构建
const BASE_URL = "http://localhost:8082";
const path = "/credentials";
const url = `${BASE_URL.replace(/\/$/, "")}/api/v1${path}`;
console.log("URL:", url);

// 测试 fetch
const token = process.env.CREDBRIDGE_TOKEN || "your-token-here";

fetch(url, {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    Authorization: "Bearer " + token,
  },
  body: JSON.stringify({
    service_id: "test",
    credential_type: "username_password",
    plaintext_data: { username: "test", password: "test123" },
  }),
})
  .then(async (r) => {
    const data = await r.json();
    console.log("Status:", r.status);
    console.log("Response:", data);
  })
  .catch((e) => console.error("Fetch Error:", e.message));
