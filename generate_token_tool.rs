//! 生成 PASETO Token 工具
//!
//! 用于生成测试用的 PASETO v4.local Token

use vault_service::token::{PasetoToken, TokenClaims, PasetoKey};

fn main() {
    // 生成密钥
    let key = PasetoToken::generate_key();

    // 创建 Claims - 包含所有需要的 scopes
    let claims = TokenClaims::new(
        "test_user",           // subject (user_id)
        "default",             // tenant_id
        "credential:read credential:write credential:decrypt audit:read", // scopes
        true,                  // mfa_verified
        86400,                 // TTL: 24 hours
    );

    // 签发 Token
    let token = PasetoToken::sign(&claims, &key).expect("Failed to sign token");

    println!("=== PASETO Token 生成成功 ===");
    println!();
    println!("Token:");
    println!("{}", token);
    println!();
    println!("密钥 (hex):");
    println!("{}", hex::encode(key.as_bytes()));
    println!();
    println!("Token 信息:");
    println!("  - 用户ID: {}", claims.sub);
    println!("  - 租户ID: {}", claims.aud);
    println!("  - 权限: {}", claims.scope);
    println!("  - JTI: {}", claims.jti);
    println!();
    println!("注意: Token 仅在本次运行时有效，密钥未保存。");
}
