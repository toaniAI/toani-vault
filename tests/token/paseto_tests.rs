#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]

//! PASETO Token 集成测试
//!
//! 测试 Token 签发、验证、过期和 Scope 权限验证

use vault_service::token::{
    ClaimsError, DEFAULT_TOKEN_TTL_SECONDS, PasetoToken, ScopeValidator, TokenClaims, TokenError,
    TokenValidationResult, quick_verify, scopes,
};

/// 模拟撤销检查器
struct MockRevocationChecker {
    revoked_jtis: Vec<String>,
}

impl MockRevocationChecker {
    fn new() -> Self {
        Self {
            revoked_jtis: Vec::new(),
        }
    }

    fn revoke(&mut self, jti: &str) {
        self.revoked_jtis.push(jti.to_string());
    }
}

impl vault_service::token::TokenRevocationChecker for MockRevocationChecker {
    fn is_revoked(&self, jti: &str) -> bool {
        self.revoked_jtis.contains(&jti.to_string())
    }
}

mod token_generation_tests {
    use super::*;

    #[test]
    fn test_generate_token_with_all_claims() {
        let key = PasetoToken::generate_key();
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read credential:write",
            true,
            900,
        );

        let token = PasetoToken::sign(&claims, &key).unwrap();

        // 验证 Token 格式
        assert!(token.starts_with("v4.local."));

        // 验证可以解析
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "v4");
        assert_eq!(parts[1], "local");

        // 验证长度合理（payload 应该被加密）
        assert!(!parts[2].is_empty());
    }

    #[test]
    fn test_generate_multiple_tokens_different_keys() {
        let key1 = PasetoToken::generate_key();
        let key2 = PasetoToken::generate_key();

        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token1 = PasetoToken::sign(&claims, &key1).unwrap();
        let token2 = PasetoToken::sign(&claims, &key2).unwrap();

        // 不同密钥应该产生不同的 Token
        assert_ne!(token1, token2);

        // 各自验证通过
        assert!(PasetoToken::verify(&token1, &key1, "tenant_456").is_ok());
        assert!(PasetoToken::verify(&token2, &key2, "tenant_456").is_ok());

        // 交叉验证失败
        assert!(PasetoToken::verify(&token1, &key2, "tenant_456").is_err());
        assert!(PasetoToken::verify(&token2, &key1, "tenant_456").is_err());
    }

    #[test]
    fn test_token_contains_all_claims() {
        let key = PasetoToken::generate_key();
        let claims = TokenClaims::new(
            "user_789",
            "tenant_abc",
            "credential:read credential:decrypt admin",
            false,
            1800,
        );

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let verified = PasetoToken::verify(&token, &key, "tenant_abc").unwrap();

        assert_eq!(verified.iss, "credbridge-vault");
        assert_eq!(verified.sub, "user_789");
        assert_eq!(verified.aud, "tenant_abc");
        assert_eq!(verified.scope, "credential:read credential:decrypt admin");
        assert!(!verified.mfa_verified);
        assert_eq!(verified.jti, claims.jti);
    }

    #[test]
    fn test_default_ttl_is_2_hours() {
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let expected_exp = claims.iat.unwrap() + DEFAULT_TOKEN_TTL_SECONDS;
        assert_eq!(claims.exp, expected_exp);
    }

    #[test]
    fn test_custom_ttl() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            3600, // 1 hour
        );

        let expected_exp = claims.iat.unwrap() + 3600;
        assert_eq!(claims.exp, expected_exp);
    }
}

mod token_verification_tests {
    use super::*;

    #[test]
    fn test_verify_valid_token() {
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let result = PasetoToken::verify(&token, &key, "tenant_456");

        assert!(result.is_ok());

        let verified = result.unwrap();
        assert_eq!(verified.sub, "user_123");
        assert_eq!(verified.aud, "tenant_456");
    }

    #[test]
    fn test_verify_wrong_audience_fails() {
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let result = PasetoToken::verify(&token, &key, "wrong_tenant");

        assert!(result.is_err());
    }

    #[test]
    fn test_verify_tampered_token_fails() {
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();

        // 篡改 Token（修改最后一个字符）
        let mut tampered = token.clone();
        tampered.pop();
        tampered.push('X');

        let result = PasetoToken::verify(&tampered, &key, "tenant_456");
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_corrupted_format_fails() {
        let key = PasetoToken::generate_key();

        let invalid_tokens = vec![
            "invalid",
            "v4.local",
            "v4.local.",
            "v3.local.payload",  // wrong version
            "v4.public.payload", // wrong purpose
            "",
        ];

        for token in invalid_tokens {
            let result = PasetoToken::verify(token, &key, "tenant_456");
            assert!(
                result.is_err(),
                "Token '{}' should fail verification",
                token
            );
        }
    }
}

mod token_expiration_tests {
    use super::*;

    #[test]
    fn test_expired_token_fails_verification() {
        let key = PasetoToken::generate_key();
        let mut claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        // 设置过期时间为过去
        claims.exp = 1; // Unix epoch + 1 second

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let result = PasetoToken::verify(&token, &key, "tenant_456");

        // 应该验证失败（过期）
        assert!(result.is_err());
    }

    #[test]
    fn test_is_expired_method() {
        let mut claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        // 未过期
        assert!(!claims.is_expired());

        // 设置为已过期
        claims.exp = 1;
        assert!(claims.is_expired());
    }

    #[test]
    fn test_remaining_ttl() {
        let claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            300, // 5 minutes
        );

        let ttl = claims.remaining_ttl();
        assert!(ttl > 295 && ttl <= 300); // 允许小误差
    }

    #[test]
    fn test_expired_token_has_zero_ttl() {
        let mut claims = TokenClaims::new(
            "user_123",
            "tenant_456",
            "credential:read",
            true,
            1, // 1 second
        );

        claims.exp = 1; // 强制过期

        assert_eq!(claims.remaining_ttl(), 0);
    }

    #[test]
    fn test_quick_verify_expired() {
        let key = PasetoToken::generate_key();
        let mut claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        // 强制过期
        claims.exp = 1;

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let result = quick_verify(&token, &key, "tenant_456");

        // 检查返回的是 Expired 或 Invalid(包含过期信息)
        match &result {
            TokenValidationResult::Expired => {}
            TokenValidationResult::Invalid(msg)
                if msg.contains("expired") || msg.contains("Exp") => {}
            _ => panic!("Expected Expired, got {:?}", result),
        }
    }
}

mod scope_verification_tests {
    use super::*;

    #[test]
    fn test_single_scope() {
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let verified = PasetoToken::verify(&token, &key, "tenant_456").unwrap();

        assert!(verified.has_scope("credential:read"));
        assert!(!verified.has_scope("credential:write"));
        assert!(!verified.has_scope("admin"));
    }

    #[test]
    fn test_multiple_scopes() {
        let key = PasetoToken::generate_key();
        let claims = TokenClaims::with_default_ttl(
            "user_123",
            "tenant_456",
            "credential:read credential:write credential:decrypt",
            true,
        );

        let token = PasetoToken::sign(&claims, &key).unwrap();
        let verified = PasetoToken::verify(&token, &key, "tenant_456").unwrap();

        let scopes = verified.scopes();
        assert_eq!(scopes.len(), 3);
        assert!(verified.has_scope("credential:read"));
        assert!(verified.has_scope("credential:write"));
        assert!(verified.has_scope("credential:decrypt"));
    }

    #[test]
    fn test_admin_scope_has_all_permissions() {
        let token_scopes = vec!["admin"];

        assert!(ScopeValidator::can_access(
            &token_scopes,
            scopes::CREDENTIAL_READ
        ));
        assert!(ScopeValidator::can_access(
            &token_scopes,
            scopes::CREDENTIAL_DECRYPT
        ));
        assert!(ScopeValidator::can_access(
            &token_scopes,
            scopes::CREDENTIAL_WRITE
        ));
        assert!(ScopeValidator::can_access(
            &token_scopes,
            scopes::CREDENTIAL_DELETE
        ));
        assert!(ScopeValidator::can_access(
            &token_scopes,
            scopes::TOKEN_MANAGE
        ));
        assert!(ScopeValidator::can_access(
            &token_scopes,
            scopes::AUDIT_READ
        ));
    }

    #[test]
    fn test_read_only_scope() {
        let token_scopes = vec!["credential:read"];

        assert!(ScopeValidator::can_access(
            &token_scopes,
            scopes::CREDENTIAL_READ
        ));
        assert!(!ScopeValidator::can_access(
            &token_scopes,
            scopes::CREDENTIAL_DECRYPT
        ));
        assert!(!ScopeValidator::can_access(
            &token_scopes,
            scopes::CREDENTIAL_WRITE
        ));
        assert!(!ScopeValidator::can_access(
            &token_scopes,
            scopes::TOKEN_MANAGE
        ));
    }

    #[test]
    fn test_scope_hierarchy() {
        // decrypt 包含 read
        let decrypt_scopes = vec!["credential:decrypt"];
        assert!(ScopeValidator::can_access(
            &decrypt_scopes,
            scopes::CREDENTIAL_READ
        ));

        // write 包含 delete
        let write_scopes = vec!["credential:write"];
        assert!(ScopeValidator::can_access(
            &write_scopes,
            scopes::CREDENTIAL_DELETE
        ));
    }

    #[test]
    fn test_has_any_scope() {
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read admin", true);

        assert!(claims.has_any_scope(&["credential:read", "credential:write"]));
        assert!(claims.has_any_scope(&["admin", "nonexistent"]));
        assert!(!claims.has_any_scope(&["credential:decrypt", "token:manage"]));
    }

    #[test]
    fn test_has_all_scopes() {
        let claims = TokenClaims::with_default_ttl(
            "user_123",
            "tenant_456",
            "credential:read credential:write admin",
            true,
        );

        assert!(claims.has_all_scopes(&["credential:read", "credential:write"]));
        assert!(claims.has_all_scopes(&["credential:read"]));
        assert!(!claims.has_all_scopes(&["credential:read", "token:manage"]));
    }
}

mod revocation_tests {
    use super::*;
    use vault_service::token::RevocableTokenValidator;

    #[test]
    fn test_revocable_token_validator() {
        let checker = MockRevocationChecker::new();
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();

        // 未撤销时验证通过
        let validator = RevocableTokenValidator::new(checker);
        let result = validator.verify(&token, &key, "tenant_456");
        assert!(result.is_ok());
    }

    #[test]
    fn test_revocable_token_validator_revoked() {
        let mut checker = MockRevocationChecker::new();
        let key = PasetoToken::generate_key();
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", true);

        let token = PasetoToken::sign(&claims, &key).unwrap();

        // 撤销后验证失败
        checker.revoke(&claims.jti);
        let validator = RevocableTokenValidator::new(checker);
        let result = validator.verify(&token, &key, "tenant_456");
        assert!(matches!(result, Err(TokenError::Revoked(_))));
    }
}

mod key_derivation_tests {
    use super::*;

    #[test]
    fn test_derive_key_from_master_deterministic() {
        let master_key = [0x42; 32];

        let key1 = PasetoToken::derive_key_from_master(&master_key, "tenant_1").unwrap();
        let key2 = PasetoToken::derive_key_from_master(&master_key, "tenant_1").unwrap();

        // 相同输入产生相同输出
        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derive_key_different_context() {
        let master_key = [0x42; 32];

        let key1 = PasetoToken::derive_key_from_master(&master_key, "tenant_1").unwrap();
        let key2 = PasetoToken::derive_key_from_master(&master_key, "tenant_2").unwrap();

        // 不同上下文产生不同密钥
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_derived_key_usable_for_signing() {
        let master_key = [0x42; 32];
        let token_key = PasetoToken::derive_key_from_master(&master_key, "tenant_1").unwrap();

        let claims = TokenClaims::with_default_ttl("user_123", "tenant_1", "credential:read", true);

        let token = PasetoToken::sign(&claims, &token_key).unwrap();
        let verified = PasetoToken::verify(&token, &token_key, "tenant_1").unwrap();

        assert_eq!(verified.sub, "user_123");
    }
}

mod error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_key_length() {
        let result = PasetoToken::key_from_bytes(&[0x42; 16]); // 16 bytes instead of 32
        assert!(matches!(result, Err(TokenError::KeyError(_))));

        let result = PasetoToken::key_from_bytes(&[0x42; 64]); // 64 bytes
        assert!(matches!(result, Err(TokenError::KeyError(_))));
    }

    #[test]
    fn test_error_types() {
        let key = PasetoToken::generate_key();

        // Invalid format
        let result = PasetoToken::verify("bad.token", &key, "tenant");
        assert!(matches!(result, Err(TokenError::InvalidFormat)));

        // Wrong key
        let claims = TokenClaims::with_default_ttl("user", "tenant", "read", true);
        let token = PasetoToken::sign(&claims, &key).unwrap();

        let wrong_key = PasetoToken::generate_key();
        let result = PasetoToken::verify(&token, &wrong_key, "tenant");
        assert!(matches!(result, Err(TokenError::VerificationError(_))));
    }

    #[test]
    fn test_claims_error_conversion() {
        let claims_error = ClaimsError::Expired;
        let token_error: TokenError = claims_error.into();

        assert!(matches!(
            token_error,
            TokenError::ClaimsError(ClaimsError::Expired)
        ));
    }
}

mod serialization_tests {
    use super::*;

    #[test]
    fn test_claims_json_roundtrip() {
        let claims = TokenClaims::with_default_ttl(
            "user_123",
            "tenant_456",
            "credential:read credential:write",
            true,
        );

        let json = claims.to_json().unwrap();
        let deserialized = TokenClaims::from_json(&json).unwrap();

        assert_eq!(claims, deserialized);
    }

    #[test]
    fn test_claims_json_structure() {
        let claims =
            TokenClaims::with_default_ttl("user_123", "tenant_456", "credential:read", false);

        let json = claims.to_json().unwrap();

        // 验证 JSON 包含所有必要字段
        assert!(json.contains("\"iss\":\"credbridge-vault\""));
        assert!(json.contains("\"sub\":\"user_123\""));
        assert!(json.contains("\"aud\":\"tenant_456\""));
        assert!(json.contains("\"jti\""));
        assert!(json.contains("\"scope\":\"credential:read\""));
        assert!(json.contains("\"mfa_verified\":false"));
    }

    #[test]
    fn test_invalid_json_deserialization() {
        let result = TokenClaims::from_json("invalid json");
        assert!(result.is_err());

        let result = TokenClaims::from_json("{}");
        assert!(result.is_err());
    }
}

mod integration_tests {
    use super::*;

    #[test]
    fn test_full_token_lifecycle() {
        // 1. 生成密钥
        let key = PasetoToken::generate_key();

        // 2. 创建 Claims
        let claims = TokenClaims::with_default_ttl(
            "user_123",
            "tenant_456",
            "credential:read credential:decrypt",
            true,
        );

        // 3. 签发 Token
        let token = PasetoToken::sign(&claims, &key).unwrap();
        assert!(!token.is_empty());

        // 4. 验证 Token
        let verified = PasetoToken::verify(&token, &key, "tenant_456").unwrap();

        // 5. 验证所有字段
        assert_eq!(verified.iss, "credbridge-vault");
        assert_eq!(verified.sub, "user_123");
        assert_eq!(verified.aud, "tenant_456");
        assert_eq!(verified.jti, claims.jti);
        assert_eq!(verified.scope, "credential:read credential:decrypt");
        assert!(verified.mfa_verified);

        // 6. 验证 Scope
        assert!(verified.has_scope("credential:read"));
        assert!(verified.has_scope("credential:decrypt"));
        assert!(ScopeValidator::can_access(
            &verified.scopes(),
            "credential:read"
        ));

        // 7. 验证未过期
        assert!(!verified.is_expired());
        assert!(verified.remaining_ttl() > 0);
    }

    #[test]
    fn test_cross_tenant_isolation() {
        let key = PasetoToken::generate_key();

        // 租户 A 的 Token
        let claims_a =
            TokenClaims::with_default_ttl("user_123", "tenant_a", "credential:read", true);
        let token_a = PasetoToken::sign(&claims_a, &key).unwrap();

        // 租户 B 的 Token
        let claims_b =
            TokenClaims::with_default_ttl("user_456", "tenant_b", "credential:write", true);
        let token_b = PasetoToken::sign(&claims_b, &key).unwrap();

        // 租户 A 的 Token 不能用租户 B 的 audience 验证
        assert!(PasetoToken::verify(&token_a, &key, "tenant_b").is_err());

        // 租户 B 的 Token 不能用租户 A 的 audience 验证
        assert!(PasetoToken::verify(&token_b, &key, "tenant_a").is_err());

        // 各自验证通过
        assert!(PasetoToken::verify(&token_a, &key, "tenant_a").is_ok());
        assert!(PasetoToken::verify(&token_b, &key, "tenant_b").is_ok());
    }

    #[test]
    fn test_multiple_tokens_same_user() {
        let key = PasetoToken::generate_key();
        let user_id = "user_123";
        let tenant_id = "tenant_456";

        // 同一用户的多个 Token
        let token1 = PasetoToken::sign(
            &TokenClaims::with_default_ttl(user_id, tenant_id, "credential:read", true),
            &key,
        )
        .unwrap();

        let token2 = PasetoToken::sign(
            &TokenClaims::with_default_ttl(user_id, tenant_id, "credential:write", true),
            &key,
        )
        .unwrap();

        let token3 = PasetoToken::sign(
            &TokenClaims::with_default_ttl(user_id, tenant_id, "admin", true),
            &key,
        )
        .unwrap();

        // 每个 Token 都有唯一的 jti
        let v1 = PasetoToken::verify(&token1, &key, tenant_id).unwrap();
        let v2 = PasetoToken::verify(&token2, &key, tenant_id).unwrap();
        let v3 = PasetoToken::verify(&token3, &key, tenant_id).unwrap();

        assert_ne!(v1.jti, v2.jti);
        assert_ne!(v2.jti, v3.jti);
        assert_ne!(v1.jti, v3.jti);

        // 每个 Token 有不同的 scope
        assert!(v1.has_scope("credential:read"));
        assert!(v2.has_scope("credential:write"));
        assert!(v3.has_scope("admin"));
    }
}
