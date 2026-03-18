use crate::cli::CredentialCommands;
use crate::config::Config;
use crate::output::{confirm, input_secret, OutputFormatter};
use anyhow::{Context, Result};
use colored::Colorize;
use credbridge_sdk::types::CredentialType;
use serde_json::Value;

pub async fn execute(cmd: CredentialCommands, config: Config) -> Result<()> {
    if !config.is_configured() {
        anyhow::bail!("未配置，请先运行 'credbridge auth login'");
    }

    let sdk = create_sdk(&config)?;
    let formatter = OutputFormatter::new(config.output_format);

    match cmd {
        CredentialCommands::List {
            credential_type,
            limit,
            offset: _,
        } => list_credentials(&sdk, &formatter, credential_type, limit).await,
        CredentialCommands::Get { id, include_value: _ } => {
            get_credential(&sdk, &formatter, id).await
        }
        CredentialCommands::Create {
            name,
            credential_type,
            value,
            description: _,
            metadata,
        } => create_credential(&sdk, &formatter, name, credential_type, value, metadata).await,
        CredentialCommands::Update {
            id,
            name: _,
            value: _,
            description: _,
        } => update_credential(&formatter, id).await,
        CredentialCommands::Delete { id, force } => delete_credential(&sdk, id, force).await,
        CredentialCommands::Decrypt { id, version: _ } => {
            decrypt_credential(&sdk, &formatter, id).await
        }
        CredentialCommands::Versions { id: _ } => list_versions().await,
        CredentialCommands::Rollback { id: _, version: _ } => rollback_credential().await,
    }
}

fn create_sdk(config: &Config) -> Result<credbridge_sdk::CredBridgeSDK> {
    credbridge_sdk::CredBridgeSDK::new(
        credbridge_sdk::CredBridgeConfig::new(config.require_url()?)
            .with_token(config.require_token()?)
            .with_timeout_ms(config.timeout * 1000),
    )
    .context("创建 SDK 客户端失败")
}

async fn list_credentials(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    credential_type: Option<String>,
    _limit: u32,
) -> Result<()> {
    println!("🔍 正在获取凭证列表...\n");

    // 解析凭证类型
    let filter = if let Some(ct_str) = credential_type {
        let ct = parse_credential_type(&ct_str)?;
        Some(credbridge_sdk::types::CredentialFilter {
            credential_type: Some(ct),
            ..Default::default()
        })
    } else {
        None
    };

    let (credentials, total) = sdk.credentials().list(filter, None).await?;

    if credentials.is_empty() {
        println!("ℹ️  暂无凭证");
        return Ok(());
    }

    // 转换为可序列化的视图
    let view: Vec<_> = credentials
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": c.credential_id,
                "service_id": c.service_id,
                "type": format!("{:?}", c.credential_type),
                "tenant_id": c.tenant_id,
                "created_at": c.created_at,
            })
        })
        .collect();

    formatter.print_list(&view, &["ID", "Service", "Type", "Tenant", "Created At"])?;

    println!(
        "\n共 {} 条记录 (总计: {})",
        credentials.len().to_string().cyan(),
        total
    );
    Ok(())
}

async fn get_credential(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    id: String,
) -> Result<()> {
    let credential = sdk
        .credentials()
        .get(&id, None)
        .await
        .with_context(|| format!("获取凭证 {} 失败", id))?;

    // 手动格式化输出
    if formatter.is_table() {
        println!("凭证详情:\n");
        println!("  ID:           {}", credential.credential_id);
        println!("  Service:      {}", credential.service_id);
        println!("  Type:         {}", credential.credential_type);
        println!("  Created At:   {}", credential.created_at);
        if let Some(expires) = &credential.expires_at {
            println!("  Expires At:   {}", expires);
        }
        println!("  Deleted:      {}", credential.is_deleted);
        if credential.encrypted_payload.is_some() {
            println!("  Payload:      [encrypted]");
        }
    } else {
        // JSON 格式 - 构建可序列化的对象
        let view = serde_json::json!({
            "credential_id": credential.credential_id,
            "service_id": credential.service_id,
            "credential_type": credential.credential_type,
            "created_at": credential.created_at,
            "expires_at": credential.expires_at,
            "is_deleted": credential.is_deleted,
            "has_encrypted_payload": credential.encrypted_payload.is_some(),
        });
        formatter.print_object(&view)?;
    }
    Ok(())
}

async fn create_credential(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    name: String,
    credential_type: String,
    value: Option<String>,
    metadata: Vec<(String, String)>,
) -> Result<()> {
    // 如果没有提供 value，交互式输入
    let value = match value {
        Some(v) => v,
        None => input_secret("请输入凭证值")?,
    };

    println!("📝 正在创建凭证...\n");

    // 解析凭证类型
    let cred_type = parse_credential_type(&credential_type)?;

    // 构建凭证数据
    let mut plaintext_data: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    plaintext_data.insert("value".to_string(), Value::String(value));

    // 添加元数据
    for (k, v) in metadata {
        plaintext_data.insert(k, Value::String(v));
    }

    let credential = sdk
        .credentials()
        .create(name, cred_type, plaintext_data, None, None)
        .await?;

    formatter.print_success(&format!("凭证创建成功: {}", credential.credential_id));
    Ok(())
}

async fn update_credential(formatter: &OutputFormatter, _id: String) -> Result<()> {
    // SDK 暂不支持更新，这里提示用户
    formatter.print_error("更新凭证功能暂不可用");
    println!("   请使用 delete + create 来更新凭证");
    Ok(())
}

async fn delete_credential(
    sdk: &credbridge_sdk::CredBridgeSDK,
    id: String,
    force: bool,
) -> Result<()> {
    // 确认删除
    if !force {
        let confirm_msg = format!("确定要删除凭证 {} 吗? 此操作不可撤销。", id.red());
        if !confirm(&confirm_msg)? {
            println!("已取消");
            return Ok(());
        }
    }

    println!("🗑️  正在删除凭证...\n");

    sdk.credentials().delete(&id, None).await?;

    println!("✅ 凭证 {} 已删除", id);
    Ok(())
}

async fn decrypt_credential(
    sdk: &credbridge_sdk::CredBridgeSDK,
    formatter: &OutputFormatter,
    id: String,
) -> Result<()> {
    println!("🔓 正在解密凭证...\n");

    let decrypted = sdk
        .credentials()
        .decrypt(&id, Some::<String>("CLI 解密操作".into()), None)
        .await?;

    if formatter.is_table() {
        println!("凭证数据:\n");
        println!("  ID:       {}", decrypted.credential_id);
        println!("  Service:  {}", decrypted.service_id);
        println!("  Type:     {}", decrypted.credential_type);
        println!("  Data:");
        if let Ok(json_str) = serde_json::to_string_pretty(&decrypted.plaintext_data) {
            println!("{}", json_str.green());
        } else {
            println!("{:?}", decrypted.plaintext_data);
        }
    } else {
        // 构建可序列化的视图
        let view = serde_json::json!({
            "credential_id": decrypted.credential_id,
            "service_id": decrypted.service_id,
            "credential_type": decrypted.credential_type,
            "plaintext_data": decrypted.plaintext_data,
        });
        formatter.print_object(&view)?;
    }

    Ok(())
}

async fn list_versions() -> Result<()> {
    println!("ℹ️  版本历史功能暂未实现");
    Ok(())
}

async fn rollback_credential() -> Result<()> {
    println!("ℹ️  回滚功能暂未实现");
    Ok(())
}

fn parse_credential_type(s: &str) -> Result<CredentialType> {
    match s.to_lowercase().as_str() {
        "username_password" | "usernamepassword" => Ok(CredentialType::UsernamePassword),
        "api_key" | "apikey" => Ok(CredentialType::ApiKey),
        "oauth_refresh" | "oauthrefresh" => Ok(CredentialType::OAuthRefresh),
        "session_cookie" | "sessioncookie" => Ok(CredentialType::SessionCookie),
        "kyc_document" | "kycdocument" => Ok(CredentialType::KycDocument),
        "certificate" | "cert" => Ok(CredentialType::Certificate),
        "ssh_key" | "sshkey" => Ok(CredentialType::SshKey),
        "database_connection" | "databaseconnection" => Ok(CredentialType::DatabaseConnection),
        _ => anyhow::bail!(
            "无效的凭证类型: {}。可选类型: username_password, api_key, oauth_refresh, session_cookie, kyc_document, certificate, ssh_key, database_connection",
            s
        ),
    }
}
