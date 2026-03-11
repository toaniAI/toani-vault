//! CredBridge Rust SDK - 批量操作示例
//!
//! 展示批量创建、获取和删除凭证的操作

use credbridge_sdk::{CredBridgeConfig, CredBridgeSDK};
use credbridge_sdk::types::CredentialFilter;
use futures::future::join_all;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (base_url, token) = crate::get_config();

    let config = CredBridgeConfig::new(base_url)
        .with_token(token);

    let sdk = CredBridgeSDK::new(config)?;

    println!("=== CredBridge 批量操作示例 ===\n");

    let mut created_ids = Vec::new();

    // 1. 批量创建凭证
    println!("1. 批量创建凭证...");
    let services = vec![
        ("bank-of-america", "user1@example.com", "pass1"),
        ("chase", "user2@example.com", "pass2"),
        ("wells-fargo", "user3@example.com", "pass3"),
    ];

    for (service, username, password) in services {
        let credential = sdk.credentials()
            .create_username_password(service, username, password, None, None)
            .await?;

        created_ids.push(credential.credential_id.clone());
        println!("   创建成功: {} -> {}", service, credential.credential_id);
    }

    // 2. 批量获取凭证详情（并发）
    println!("\n2. 批量获取凭证详情（并发）...");
    let futures: Vec<_> = created_ids
        .iter()
        .map(|id| {
            let sdk = sdk.clone();
            let id = id.clone();
            async move {
                sdk.credentials().get(&id, None).await
            }
        })
        .collect();

    let results = join_all(futures).await;
    println!("   获取结果: {}/{} 成功", results.iter().filter(|r| r.is_ok()).count(), results.len());

    // 3. 按服务过滤
    println!("\n3. 按服务过滤凭证...");
    let filter = CredentialFilter {
        service_id: Some("chase".to_string()),
        ..Default::default()
    };
    let (chase_creds, count) = sdk.credentials().list(Some(filter), None).await?;
    println!("   Chase 凭证数: {}", count);
    for cred in &chase_creds {
        println!("   - {} ({:?})", cred.credential_id, cred.credential_type);
    }

    // 4. 并发解密多个凭证
    println!("\n4. 并发解密多个凭证...");
    let decrypt_futures: Vec<_> = created_ids
        .iter()
        .map(|id| {
            let sdk = sdk.clone();
            let id = id.clone();
            async move {
                sdk.credentials()
                    .decrypt(&id, Some("批量操作示例"), None)
                    .await
                    .map(|d| (id, d.plaintext_data))
            }
        })
        .collect();

    let decrypt_results = join_all(decrypt_futures).await;
    for result in decrypt_results {
        if let Some((id, data)) = result {
            if let Some(username) = data.get("username") {
                println!("   {}: username={}", id, username);
            }
        }
    }

    // 5. 批量删除
    println!("\n5. 批量删除凭证...");
    let delete_futures: Vec<_> = created_ids
        .iter()
        .map(|id| {
            let sdk = sdk.clone();
            let id = id.clone();
            async move {
                sdk.credentials()
                    .delete(&id, None)
                    .await
                    .map(|r| (id, r.deleted))
                    .map_err(|e| (id, e.to_string()))
            }
        })
        .collect();

    let delete_results = join_all(delete_futures).await;
    for result in delete_results {
        match result {
            Ok((id, success)) => println!("   删除 {}: {}", id, if success { "成功" } else { "失败" }),
            Err((id, err)) => println!("   删除 {}: 错误 - {}", id, err),
        }
    }

    println!("\n=== 批量操作示例完成 ===");

    Ok(())
}

/// 凭证轮换示例
pub async fn rotate_credentials_example() -> Result<(), Box<dyn std::error::Error>> {
    let (base_url, token) = crate::get_config();

    let config = CredBridgeConfig::new(base_url)
        .with_token(token);

    let sdk = CredBridgeSDK::new(config)?;

    println!("\n=== 凭证轮换示例 ===\n");

    // 先创建一些测试凭证
    let service_id = "test-service";
    let cred1 = sdk.credentials()
        .create_username_password(service_id, "user1", "old_pass1", None, None)
        .await?;
    let cred2 = sdk.credentials()
        .create_username_password(service_id, "user2", "old_pass2", None, None)
        .await?;

    println!("创建测试凭证: {}, {}", cred1.credential_id, cred2.credential_id);

    // 获取服务的所有凭证
    let (credentials, _) = sdk.credentials()
        .get_by_service(service_id, None)
        .await?;

    println!("\n开始轮换 {} 个凭证...", credentials.len());

    // 轮换每个凭证
    for cred in credentials {
        // 获取并解密旧凭证
        let old = sdk.credentials()
            .decrypt(&cred.credential_id, Some("Credential rotation"), None)
            .await?;

        // 创建新凭证（密码保持不变或生成新密码）
        let new_cred = sdk.credentials()
            .create(
                &cred.service_id,
                cred.credential_type,
                old.plaintext_data,
                Some(chrono::Utc::now().timestamp() + 86400 * 90),
                None,
            )
            .await?;

        // 删除旧凭证
        sdk.credentials().delete(&cred.credential_id, None).await?;

        println!("   轮换: {} -> {}", cred.credential_id, new_cred.credential_id);
    }

    println!("\n=== 凭证轮换示例完成 ===");

    Ok(())
}
