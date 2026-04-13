use std::env;

use serde_json::json;
use toani_vault_sdk::{CredBridgeConfig, ToaniVaultSDK};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url =
        env::var("CREDBRIDGE_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let token = env::var("CREDBRIDGE_TOKEN")
        .map_err(|_| "CREDBRIDGE_TOKEN is required for the real-chain SDK smoke test")?;
    let access_token = env::var("CREDBRIDGE_ACCESS_TOKEN")
        .map_err(|_| "CREDBRIDGE_ACCESS_TOKEN is required for the real-chain SDK smoke test")?;

    let sdk = ToaniVaultSDK::new(
        CredBridgeConfig::new(&base_url)
            .with_token(token)
            .with_timeout_ms(30_000)
            .with_max_retries(1),
    )?;

    let me: serde_json::Value = sdk.client().get("/users/me").await?;
    sdk.client().set_token(access_token);

    let access_me: serde_json::Value = sdk.client().get("/users/me").await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "baseUrl": base_url,
            "automationToken": {
                "userId": me["id"],
            },
            "accessToken": {
                "userId": access_me["id"],
            }
        }))?
    );

    Ok(())
}
