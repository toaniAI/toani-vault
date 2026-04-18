//! 生成测试用 PASETO Token
//!
//! 运行: cargo run --release --bin generate_test_token

use pasetors::claims::Claims;
use pasetors::keys::SymmetricKey;
use pasetors::local;
use pasetors::version4::V4;
use std::time::Duration;
use uuid::Uuid;

fn main() {
    // 从环境变量获取密钥或使用默认测试密钥（与服务端 main.rs 保持一致）
    // 注意：服务端 main.rs:676 使用 vec![0u8; 32] 作为默认值
    let key_bytes: [u8; 32] = if let Ok(key) = std::env::var("TOKEN_SECRET_KEY") {
        let mut bytes = [0u8; 32];
        let secret_bytes = key.as_bytes();
        let copy_len = secret_bytes.len().min(32);
        bytes[..copy_len].copy_from_slice(&secret_bytes[..copy_len]);
        bytes
    } else {
        // 默认使用全零密钥，与服务端 main.rs 一致
        [0u8; 32]
    };

    // 创建对称密钥
    let sk: SymmetricKey<V4> = SymmetricKey::from(&key_bytes).expect("创建密钥失败");

    // Token 参数
    let user_id = "test_user";
    let tenant_id = "default";
    let scopes = "credential:read credential:write credential:decrypt audit:read sandbox:write sandbox:read sandbox:execute";

    // 创建 Claims
    let mut claims = Claims::new_expires_in(&Duration::from_secs(3600)).expect("创建 Claims 失败");

    claims.issuer("credbridge-vault").expect("设置 iss 失败");
    claims
        .subject(&format!("{tenant_id}:{user_id}"))
        .expect("设置 sub 失败");
    claims.audience(tenant_id).expect("设置 aud 失败");
    claims
        .token_identifier(&Uuid::now_v7().to_string())
        .expect("设置 jti 失败");
    claims
        .add_additional("scope", serde_json::json!(scopes))
        .expect("添加 scope 失败");

    // 加密 Token
    let token = local::encrypt(&sk, &claims, None, None).expect("Token 加密失败");

    println!("=== CredBridge 测试 Token ===");
    println!();
    println!("Token: {token}");
    println!();
    println!("参数:");
    println!("  用户ID: {user_id}");
    println!("  租户ID: {tenant_id}");
    println!("  权限: {scopes}");
    println!("  有效期: 1 小时");
    println!();
    println!("使用方法:");
    println!("  curl -H \"Authorization: Bearer {token}\" http://localhost:8082/health");
}
