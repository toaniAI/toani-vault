//! Toani Vault Rust SDK - 基础使用示例
//!
//! 展示凭证创建、获取、解密和删除的基本操作，
//! 以及交易所 sandbox `http_request` 模板请求的构造方式

use toani_vault_sdk::{
    CreateCredentialRequest, CreateSandboxSessionRequest, CredentialCustomFunction,
    ExecuteSandboxOperationRequest,
};
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};
use toani_vault_sdk::types::{CredentialProvider, CredentialType, SandboxOperationType};
use serde_json::json;
use std::collections::HashMap;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (base_url, token) = crate::get_config();

    // 初始化 SDK
    let config = CredBridgeConfig::new(base_url)
        .with_token(token);

    let sdk = ToaniVaultSDK::new(config)?;

    println!("=== Toani Vault Rust SDK 基础示例 ===\n");

    let mut created_ids = Vec::new();

    // 1. 创建用户名密码凭证
    println!("1. 创建用户名密码凭证...");
    let user_credential = sdk.credentials()
        .create_username_password(
            "schwab",
            "user@example.com",
            "SecurePassword123!",
            Some(chrono::Utc::now().timestamp() + 86400 * 30),
            None,
        )
        .await?;

    println!("   创建成功! ID: {}", user_credential.credential_id);
    created_ids.push(user_credential.credential_id.clone());

    // 2. 创建 API Key 凭证
    println!("\n2. 创建 API Key 凭证...");
    let api_credential = sdk.credentials()
        .create_api_key(
            "stripe",
            "sk_live_51H...",
            Some("sk_secret_..."),
            Some(chrono::Utc::now().timestamp() + 86400 * 90),
            None,
        )
        .await?;

    println!("   创建成功! ID: {}", api_credential.credential_id);
    created_ids.push(api_credential.credential_id.clone());

    println!("\n2b. 创建 OKX 交易所 API Key 凭证...");
    let mut okx_plaintext = HashMap::new();
    okx_plaintext.insert("api_key".to_string(), json!("okx_api_key"));
    okx_plaintext.insert("secret_key".to_string(), json!("okx_secret_key"));
    okx_plaintext.insert("passphrase".to_string(), json!("okx_passphrase"));

    let okx_credential = sdk.credentials()
        .create_with_request(
            CreateCredentialRequest {
                service_id: "okx-trading".to_string(),
                credential_type: CredentialType::ApiKey,
                plaintext_data: okx_plaintext,
                expires_at: Some(chrono::Utc::now().timestamp() + 86400 * 90),
                requires_approval: Some(false),
                provider: Some(CredentialProvider::Okx),
                allowed_domains: vec![
                    "www.okx.com:443".to_string(),
                    "*.okx.com:443".to_string(),
                ],
                custom_functions: vec![CredentialCustomFunction {
                    function_name: "normalize_symbol".to_string(),
                    function_description: Some("Formats symbols for template inputs".to_string()),
                    function_body: "export default function func(input) { return String(input).toUpperCase(); }".to_string(),
                }],
            },
            None,
        )
        .await?;

    println!("   创建成功! ID: {}", okx_credential.credential_id);
    println!("   Provider: {:?}", okx_credential.provider);
    println!("   Allowed domains: {:?}", okx_credential.allowed_domains);
    created_ids.push(okx_credential.credential_id.clone());

    // 3. 获取凭证列表
    println!("\n3. 获取凭证列表...");
    let (credentials, total) = sdk.credentials().list(None, None).await?;
    println!("   共 {} 个凭证:", total);
    for cred in &credentials {
        println!("   - {} ({:?})", cred.credential_id, cred.credential_type);
    }

    println!("\n3b. 构造 sandbox http_request 模板参数...");
    let okx_session = CreateSandboxSessionRequest {
        credential_id: okx_credential.credential_id.clone(),
        original_intent: "Fetch OKX balance via template-rendered REST request".to_string(),
        metadata: None,
    };
    let okx_http_request = ExecuteSandboxOperationRequest {
        operation_type: SandboxOperationType::HttpRequest,
        description: "GET OKX account balance".to_string(),
        parameters: HashMap::from([
            ("method".to_string(), json!("GET")),
            (
                "url".to_string(),
                json!("https://www.okx.com/api/v5/account/balance"),
            ),
            (
                "headers".to_string(),
                json!({
                    "OK-ACCESS-KEY": "${credential.api_key}",
                    "OK-ACCESS-TIMESTAMP": "${functions.okx_timestamp()}",
                    "OK-ACCESS-PASSPHRASE": "${credential.passphrase}",
                    "OK-ACCESS-SIGN": "${functions.okx_sign()}",
                }),
            ),
        ]),
    };
    let binance_http_request = ExecuteSandboxOperationRequest {
        operation_type: SandboxOperationType::HttpRequest,
        description: "GET Binance account information".to_string(),
        parameters: HashMap::from([
            ("method".to_string(), json!("GET")),
            (
                "url".to_string(),
                json!("https://api.binance.com/api/v3/account"),
            ),
            (
                "query".to_string(),
                json!({
                    "timestamp": "${functions.binance_timestamp()}",
                    "recvWindow": "5000",
                    "signature": "${functions.binance_sign()}",
                }),
            ),
            (
                "headers".to_string(),
                json!({
                    "X-MBX-APIKEY": "${credential.api_key}",
                }),
            ),
        ]),
    };
    println!("   Session request: {:?}", okx_session);
    println!(
        "   {} OKX parameters: {}",
        SandboxOperationType::HttpRequest,
        serde_json::to_string_pretty(&okx_http_request.parameters)?
    );
    println!(
        "   {} Binance parameters: {}",
        SandboxOperationType::HttpRequest,
        serde_json::to_string_pretty(&binance_http_request.parameters)?
    );

    // 4. 获取凭证详情
    println!("\n4. 获取凭证详情...");
    let credential = sdk.credentials()
        .get(&user_credential.credential_id, None)
        .await?;
    println!("   服务ID: {}", credential.service_id);
    println!("   类型: {}", credential.credential_type);
    println!("   创建时间: {}", credential.created_at);

    // 5. 解密凭证
    println!("\n5. 解密凭证...");
    let decrypted = sdk.credentials()
        .decrypt(&user_credential.credential_id, Some("演示解密操作"), None)
        .await?;

    if let Some(username) = decrypted.plaintext_data.get("username") {
        println!("   用户名: {}", username);
    }
    println!("   密码: ***隐藏***");

    // 6. 检查 Token 信息
    println!("\n6. 检查 Token 信息...");
    if let Some(token_info) = sdk.token().get_token_info() {
        println!("   租户ID: {}", token_info.tenant_id);
        println!("   用户ID: {}", token_info.user_id);
        println!("   权限: {:?}", token_info.scopes);
        println!("   剩余时间: {}", sdk.token().get_remaining_time_formatted());
    }

    // 7. 删除凭证
    println!("\n7. 删除凭证...");
    for id in &created_ids {
        sdk.credentials().delete(id, None).await?;
        println!("   删除: {}", id);
    }
    println!("   删除成功!");

    println!("\n=== 基础示例执行完成 ===");

    Ok(())
}

/// 创建自定义凭证示例
pub async fn create_custom_credential() -> Result<(), Box<dyn std::error::Error>> {
    let (base_url, token) = crate::get_config();

    let config = CredBridgeConfig::new(base_url)
        .with_token(token);

    let sdk = ToaniVaultSDK::new(config)?;

    // 创建 OAuth 刷新令牌
    println!("\n创建 OAuth 刷新令牌凭证...");
    let oauth_credential = sdk.credentials()
        .create_oauth_refresh(
            "google",
            "1//0dYVjK7V7V7V7V7V7V7V7V7V7V7V...",
            Some(chrono::Utc::now().timestamp() + 86400 * 180),
            None,
        )
        .await?;

    println!("   创建成功! ID: {}", oauth_credential.credential_id);

    // 创建自定义凭证
    println!("\n创建自定义会话 Cookie 凭证...");
    let mut data = HashMap::new();
    data.insert("session_id".to_string(), json!("sess_123456"));
    data.insert("csrf_token".to_string(), json!("csrf_abcdef"));
    data.insert("user_agent".to_string(), json!("Mozilla/5.0..."));

    let custom_credential = sdk.credentials()
        .create(
            "web-session",
            CredentialType::SessionCookie,
            data,
            Some(chrono::Utc::now().timestamp() + 3600), // 1小时过期
            None,
        )
        .await?;

    println!("   创建成功! ID: {}", custom_credential.credential_id);

    // 清理
    sdk.credentials().delete(&oauth_credential.credential_id, None).await?;
    sdk.credentials().delete(&custom_credential.credential_id, None).await?;

    Ok(())
}
