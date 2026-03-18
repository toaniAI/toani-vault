//! Phase 2: 凭证解密失败问题复现测试
//!
//! 验证 Phase 1 分析报告中确认的三个根本原因：
//! 1. Credential ID 不一致：加密用临时 ID，解密用存储 ID
//! 2. AAD 不一致：加密用原始 user_id，解密用 hash
//! 3. L2 派生参数不一致：加密用原始 user_id，解密用 hash

use vault_service::crypto::cipher::{EncryptedBlob, decrypt_credential, encrypt_credential};
use vault_service::crypto::hkdf::KeyHierarchy;
use vault_service::crypto::keys::{HardwareRootKey, KeyPurpose};
use vault_service::vault::models::{CredentialId, UserId};

/// 测试配置
struct TestConfig {
    tenant_id: String,
    user_id_raw: String,
    user_id_hash: String,
}

impl TestConfig {
    fn new() -> Self {
        let user_id = UserId::new("test_user_123");
        Self {
            tenant_id: "test_tenant_456".to_string(),
            user_id_raw: "test_user_123".to_string(),
            user_id_hash: user_id.hash().to_string(),
        }
    }
}

/// 模拟加密流程（当前实现）
fn simulate_current_encrypt(
    hierarchy: &KeyHierarchy,
    config: &TestConfig,
    plaintext: &[u8],
) -> (EncryptedBlob, String, String, String) {
    println!("\n========== 加密流程 ==========");

    // 1. 生成临时 credential_id（问题点 1）
    let temp_cred_id = uuid::Uuid::now_v7().to_string();
    println!("[ENCRYPT] credential_id (临时): {}", temp_cred_id);

    // 2. 派生 L2 密钥（问题点 3：使用原始 user_id）
    let l2_key = hierarchy
        .derive_user_vault_key(&config.tenant_id, &config.user_id_raw)
        .unwrap();
    println!(
        "[ENCRYPT] L2 派生参数: tenant_id={}, user_id={}",
        config.tenant_id, config.user_id_raw
    );
    println!(
        "[ENCRYPT] L2 info: user-vault-key:v1:{}:{}",
        config.tenant_id, config.user_id_raw
    );

    // 3. 派生 L3 密钥
    let l3_key = hierarchy
        .derive_credential_key(&l2_key, &temp_cred_id, KeyPurpose::CredentialEncryption)
        .unwrap();
    println!("[ENCRYPT] L3 派生参数: credential_id={}", temp_cred_id);
    println!(
        "[ENCRYPT] L3 info: credential-key:v1:{}:CredentialEncryption",
        temp_cred_id
    );

    // 4. 构建 AAD（问题点 2：使用原始 user_id）
    let aad = format!("{}:{}", config.tenant_id, config.user_id_raw);
    println!("[ENCRYPT] AAD: {}", aad);

    // 5. 加密
    let blob = encrypt_credential(&l3_key, plaintext, Some(aad.as_bytes())).unwrap();

    (blob, temp_cred_id, config.user_id_raw.clone(), aad)
}

/// 模拟解密流程（当前实现）
fn simulate_current_decrypt(
    hierarchy: &KeyHierarchy,
    config: &TestConfig,
    blob: &EncryptedBlob,
    stored_cred_id: &str,
) -> Result<Vec<u8>, String> {
    println!("\n========== 解密流程 ==========");

    // 1. 使用存储的 credential_id
    println!("[DECRYPT] credential_id (存储): {}", stored_cred_id);

    // 2. 派生 L2 密钥（问题点 3：使用 hash 后的 user_id）
    let l2_key = hierarchy
        .derive_user_vault_key(&config.tenant_id, &config.user_id_hash)
        .map_err(|e| format!("L2 派生失败: {}", e))?;
    println!(
        "[DECRYPT] L2 派生参数: tenant_id={}, user_id={}",
        config.tenant_id, config.user_id_hash
    );
    println!(
        "[DECRYPT] L2 info: user-vault-key:v1:{}:{}",
        config.tenant_id, config.user_id_hash
    );

    // 3. 派生 L3 密钥
    let l3_key = hierarchy
        .derive_credential_key(&l2_key, stored_cred_id, KeyPurpose::CredentialEncryption)
        .map_err(|e| format!("L3 派生失败: {}", e))?;
    println!("[DECRYPT] L3 派生参数: credential_id={}", stored_cred_id);
    println!(
        "[DECRYPT] L3 info: credential-key:v1:{}:CredentialEncryption",
        stored_cred_id
    );

    // 4. 构建 AAD（问题点 2：使用 hash 后的 user_id）
    let aad = format!("{}:{}", config.tenant_id, config.user_id_hash);
    println!("[DECRYPT] AAD: {}", aad);

    // 5. 解密
    decrypt_credential(&l3_key, blob, Some(aad.as_bytes()))
        .map_err(|e| format!("解密失败: {:?}", e))
}

/// 使用一致参数解密（正确实现）
fn simulate_correct_decrypt(
    hierarchy: &KeyHierarchy,
    config: &TestConfig,
    blob: &EncryptedBlob,
    temp_cred_id: &str,
    encrypt_user_id: &str,
    encrypt_aad: &str,
) -> Result<Vec<u8>, String> {
    println!("\n========== 正确解密流程（使用一致参数） ==========");

    // 1. 使用与加密相同的 credential_id
    println!("[DECRYPT-CORRECT] credential_id: {}", temp_cred_id);

    // 2. 使用与加密相同的 user_id 参数
    let l2_key = hierarchy
        .derive_user_vault_key(&config.tenant_id, encrypt_user_id)
        .map_err(|e| format!("L2 派生失败: {}", e))?;
    println!(
        "[DECRYPT-CORRECT] L2 派生参数: tenant_id={}, user_id={}",
        config.tenant_id, encrypt_user_id
    );

    // 3. 使用与加密相同的 credential_id
    let l3_key = hierarchy
        .derive_credential_key(&l2_key, temp_cred_id, KeyPurpose::CredentialEncryption)
        .map_err(|e| format!("L3 派生失败: {}", e))?;
    println!(
        "[DECRYPT-CORRECT] L3 派生参数: credential_id={}",
        temp_cred_id
    );

    // 4. 使用与加密相同的 AAD
    println!("[DECRYPT-CORRECT] AAD: {}", encrypt_aad);

    // 5. 解密
    decrypt_credential(&l3_key, blob, Some(encrypt_aad.as_bytes()))
        .map_err(|e| format!("解密失败: {:?}", e))
}

/// 打印参数对比
fn print_parameter_comparison(
    encrypt_cred_id: &str,
    decrypt_cred_id: &str,
    encrypt_user_id: &str,
    decrypt_user_id: &str,
    encrypt_aad: &str,
    decrypt_aad: &str,
) {
    println!("\n========== 参数对比 ==========");

    // Credential ID 对比
    println!("\n[1] Credential ID:");
    println!("  加密时: {}", encrypt_cred_id);
    println!("  解密时: {}", decrypt_cred_id);
    if encrypt_cred_id == decrypt_cred_id {
        println!("  结果: ✓ 一致");
    } else {
        println!("  结果: ✗ 不一致（根本原因 1）");
    }

    // User ID（L2 派生参数）对比
    println!("\n[2] L2 派生参数 user_id:");
    println!("  加密时: {}", encrypt_user_id);
    println!("  解密时: {}", decrypt_user_id);
    if encrypt_user_id == decrypt_user_id {
        println!("  结果: ✓ 一致");
    } else {
        println!("  结果: ✗ 不一致（根本原因 3）");
    }

    // AAD 对比
    println!("\n[3] AAD:");
    println!("  加密时: {}", encrypt_aad);
    println!("  解密时: {}", decrypt_aad);
    if encrypt_aad == decrypt_aad {
        println!("  结果: ✓ 一致");
    } else {
        println!("  结果: ✗ 不一致（根本原因 2）");
    }
}

#[test]
fn test_reproduce_decryption_failure() {
    println!("\n");
    println!("============================================================");
    println!("  Phase 2: 凭证解密失败问题复现测试");
    println!("============================================================");

    // 初始化
    let l0 = HardwareRootKey::for_simulation().unwrap();
    let mut hierarchy = KeyHierarchy::new();
    hierarchy.initialize_master_key(&l0).unwrap();

    let config = TestConfig::new();
    let plaintext = b"my secret credential data";

    println!("\n测试配置:");
    println!("  tenant_id: {}", config.tenant_id);
    println!("  user_id (raw): {}", config.user_id_raw);
    println!("  user_id (hash): {}", config.user_id_hash);

    // 步骤 1: 模拟加密
    let (blob, temp_cred_id, encrypt_user_id, encrypt_aad) =
        simulate_current_encrypt(&hierarchy, &config, plaintext);

    println!("\n加密成功！");
    println!("  temp_cred_id: {}", temp_cred_id);
    println!("  密文长度: {} bytes", blob.ciphertext.len());

    // 模拟存储后生成的新 credential_id
    let stored_cred_id = CredentialId::new();
    println!("\n存储后生成的 credential_id: {}", stored_cred_id.as_str());

    // 步骤 2: 尝试使用当前实现的解密流程
    println!("\n============================================================");
    println!("  测试 1: 使用当前实现解密（预期失败）");
    println!("============================================================");

    let decrypt_result =
        simulate_current_decrypt(&hierarchy, &config, &blob, stored_cred_id.as_str());

    // 打印参数对比
    print_parameter_comparison(
        &temp_cred_id,
        stored_cred_id.as_str(),
        &encrypt_user_id,
        &config.user_id_hash,
        &encrypt_aad,
        &format!("{}:{}", config.tenant_id, config.user_id_hash),
    );

    // 验证结果
    println!("\n========== 测试结果 ==========");
    match decrypt_result {
        Ok(_) => {
            println!("❌ 解密成功（与预期不符！）");
            panic!("预期解密失败，但成功了！");
        }
        Err(e) => {
            println!("✓ 解密失败（符合预期）");
            println!("  错误: {}", e);
        }
    }

    // 步骤 3: 使用一致参数解密（验证根本原因）
    println!("\n============================================================");
    println!("  测试 2: 使用一致参数解密（预期成功）");
    println!("============================================================");

    let correct_result = simulate_correct_decrypt(
        &hierarchy,
        &config,
        &blob,
        &temp_cred_id,
        &encrypt_user_id,
        &encrypt_aad,
    );

    match correct_result {
        Ok(decrypted) => {
            println!("✓ 解密成功");
            println!("  解密数据: {:?}", String::from_utf8_lossy(&decrypted));
            assert_eq!(decrypted, plaintext.to_vec(), "解密后数据应与原始数据一致");
        }
        Err(e) => {
            println!("❌ 解密失败: {}", e);
            panic!("使用一致参数应该解密成功！");
        }
    }
}

#[test]
fn test_reproduce_multiple_times() {
    println!("\n");
    println!("============================================================");
    println!("  多次复现测试（验证稳定性）");
    println!("============================================================");

    // 初始化
    let l0 = HardwareRootKey::for_simulation().unwrap();
    let mut hierarchy = KeyHierarchy::new();
    hierarchy.initialize_master_key(&l0).unwrap();

    let config = TestConfig::new();

    let mut success_count = 0;
    let mut failure_count = 0;

    for i in 1..=3 {
        println!("\n---------- 第 {} 次测试 ----------", i);

        let plaintext = format!("secret_data_{}", i);
        let plaintext_bytes = plaintext.as_bytes();

        // 加密
        let (blob, _temp_cred_id, _encrypt_user_id, _encrypt_aad) =
            simulate_current_encrypt(&hierarchy, &config, plaintext_bytes);

        // 模拟存储
        let stored_cred_id = CredentialId::new();

        // 解密
        let decrypt_result =
            simulate_current_decrypt(&hierarchy, &config, &blob, stored_cred_id.as_str());

        match decrypt_result {
            Ok(_) => {
                println!("结果: 解密成功（不符合预期）");
                success_count += 1;
            }
            Err(e) => {
                println!("结果: 解密失败 - {}", e);
                failure_count += 1;
            }
        }
    }

    println!("\n========== 统计结果 ==========");
    println!("解密成功次数: {}", success_count);
    println!("解密失败次数: {}", failure_count);

    assert_eq!(
        failure_count, 3,
        "应该所有 3 次测试都失败，验证问题稳定复现"
    );
}

#[test]
fn test_root_cause_1_credential_id_mismatch() {
    println!("\n");
    println!("============================================================");
    println!("  根本原因 1: Credential ID 不一致");
    println!("============================================================");

    let l0 = HardwareRootKey::for_simulation().unwrap();
    let mut hierarchy = KeyHierarchy::new();
    hierarchy.initialize_master_key(&l0).unwrap();

    let tenant_id = "tenant_123";
    let user_id = "user_456";
    let plaintext = b"test data";

    // 派生 L2 密钥
    let l2_key = hierarchy.derive_user_vault_key(tenant_id, user_id).unwrap();

    // 使用临时 credential_id 加密
    let temp_cred_id = uuid::Uuid::now_v7().to_string();
    println!("加密时 credential_id: {}", temp_cred_id);

    let l3_key_encrypt = hierarchy
        .derive_credential_key(&l2_key, &temp_cred_id, KeyPurpose::CredentialEncryption)
        .unwrap();

    let aad = format!("{}:{}", tenant_id, user_id);
    let blob = encrypt_credential(&l3_key_encrypt, plaintext, Some(aad.as_bytes())).unwrap();

    // 使用不同 credential_id 解密
    let stored_cred_id = uuid::Uuid::now_v7().to_string();
    println!("解密时 credential_id: {}", stored_cred_id);
    println!("两个 ID 是否相同: {}", temp_cred_id == stored_cred_id);

    let l3_key_decrypt = hierarchy
        .derive_credential_key(&l2_key, &stored_cred_id, KeyPurpose::CredentialEncryption)
        .unwrap();

    // 说明：HKDF 是确定性 KDF，不同的 info 参数会产生不同的密钥
    println!("\nL3 密钥派生 info 对比:");
    println!(
        "  加密 info: credential-key:v1:{}:CredentialEncryption",
        temp_cred_id
    );
    println!(
        "  解密 info: credential-key:v1:{}:CredentialEncryption",
        stored_cred_id
    );

    let result = decrypt_credential(&l3_key_decrypt, &blob, Some(aad.as_bytes()));

    println!("\n结果: {:?}", result);
    assert!(
        result.is_err(),
        "使用不同 credential_id 派生的密钥应该无法解密"
    );
}

#[test]
fn test_root_cause_2_aad_mismatch() {
    println!("\n");
    println!("============================================================");
    println!("  根本原因 2: AAD 不一致");
    println!("============================================================");

    let l0 = HardwareRootKey::for_simulation().unwrap();
    let mut hierarchy = KeyHierarchy::new();
    hierarchy.initialize_master_key(&l0).unwrap();

    let tenant_id = "tenant_123";
    let user_id_raw = "user_456";
    let user_id = UserId::new(user_id_raw);
    let user_id_hash = user_id.hash();
    let plaintext = b"test data";
    let cred_id = "cred_789";

    println!("原始 user_id: {}", user_id_raw);
    println!("hash user_id: {}", user_id_hash);

    // 派生 L2 密钥
    let l2_key = hierarchy
        .derive_user_vault_key(tenant_id, user_id_raw)
        .unwrap();

    // 派生 L3 密钥
    let l3_key = hierarchy
        .derive_credential_key(&l2_key, cred_id, KeyPurpose::CredentialEncryption)
        .unwrap();

    // 使用原始 user_id 作为 AAD 加密
    let encrypt_aad = format!("{}:{}", tenant_id, user_id_raw);
    println!("\n加密时 AAD: {}", encrypt_aad);

    let blob = encrypt_credential(&l3_key, plaintext, Some(encrypt_aad.as_bytes())).unwrap();

    // 使用 hash user_id 作为 AAD 解密
    let decrypt_aad = format!("{}:{}", tenant_id, user_id_hash);
    println!("解密时 AAD: {}", decrypt_aad);
    println!("两个 AAD 是否相同: {}", encrypt_aad == decrypt_aad);

    let result = decrypt_credential(&l3_key, &blob, Some(decrypt_aad.as_bytes()));

    println!("\n结果: {:?}", result);
    assert!(result.is_err(), "使用不同 AAD 解密应该失败");
}

#[test]
fn test_root_cause_3_l2_derivation_mismatch() {
    println!("\n");
    println!("============================================================");
    println!("  根本原因 3: L2 密钥派生参数不一致");
    println!("============================================================");

    let l0 = HardwareRootKey::for_simulation().unwrap();
    let mut hierarchy = KeyHierarchy::new();
    hierarchy.initialize_master_key(&l0).unwrap();

    let tenant_id = "tenant_123";
    let user_id_raw = "user_456";
    let user_id = UserId::new(user_id_raw);
    let user_id_hash = user_id.hash();
    let plaintext = b"test data";
    let cred_id = "cred_789";

    println!("原始 user_id: {}", user_id_raw);
    println!("hash user_id: {}", user_id_hash);

    // 使用原始 user_id 派生 L2 密钥
    let l2_key_encrypt = hierarchy
        .derive_user_vault_key(tenant_id, user_id_raw)
        .unwrap();
    println!(
        "\n加密时 L2 info: user-vault-key:v1:{}:{}",
        tenant_id, user_id_raw
    );

    // 使用 hash user_id 派生 L2 密钥
    let l2_key_decrypt = hierarchy
        .derive_user_vault_key(tenant_id, user_id_hash)
        .unwrap();
    println!(
        "解密时 L2 info: user-vault-key:v1:{}:{}",
        tenant_id, user_id_hash
    );

    // 说明：HKDF 是确定性 KDF，不同的 info 参数会产生不同的密钥
    println!("\nL2 密钥派生 info 对比:");
    println!(
        "  加密 L2 info: user-vault-key:v1:{}:{}",
        tenant_id, user_id_raw
    );
    println!(
        "  解密 L2 info: user-vault-key:v1:{}:{}",
        tenant_id, user_id_hash
    );

    // 使用加密 L2 派生 L3 并加密
    let l3_key_encrypt = hierarchy
        .derive_credential_key(&l2_key_encrypt, cred_id, KeyPurpose::CredentialEncryption)
        .unwrap();

    let aad = format!("{}:{}", tenant_id, user_id_raw);
    let blob = encrypt_credential(&l3_key_encrypt, plaintext, Some(aad.as_bytes())).unwrap();

    // 使用解密 L2 派生 L3 并解密
    let l3_key_decrypt = hierarchy
        .derive_credential_key(&l2_key_decrypt, cred_id, KeyPurpose::CredentialEncryption)
        .unwrap();

    // 打印 L3 info 对比
    println!("\nL3 密钥派生 info 对比:");
    println!(
        "  加密 L3 info: credential-key:v1:{}:CredentialEncryption",
        cred_id
    );
    println!(
        "  解密 L3 info: credential-key:v1:{}:CredentialEncryption",
        cred_id
    );
    println!("  注意：虽然 L3 info 相同，但由于 L2 密钥不同，最终 L3 密钥也不同");

    let result = decrypt_credential(&l3_key_decrypt, &blob, Some(aad.as_bytes()));

    println!("\n结果: {:?}", result);
    assert!(
        result.is_err(),
        "使用不同 L2 密钥派生的 L3 密钥应该无法解密"
    );
}
