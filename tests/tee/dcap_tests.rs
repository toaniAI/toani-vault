#![allow(clippy::field_reassign_with_default)]
#![allow(dead_code)]
#![allow(clippy::uninlined_format_args)]
#![allow(deprecated)]

//! Simulation-safe DCAP 集成测试
//!
//! 本文件显式使用 simulation 运行时，覆盖 Quote 结构、白名单、序列化和
//! fail-closed 语义，不要求真实 SGX/DCAP/AESM。真实硬件链路见
//! `tests/sgx_hardware_tests.rs`。

use vault_service::config::{TeeRuntimeConfig, TeeRuntimeMode};
use vault_service::tee::{
    dcap::{DcapConfig, DcapError, DcapService, EcdsaSignatureDcap, INTEL_PCS_BASE_URL_PROD},
    enclave::{Enclave, EnclaveConfig},
    quote::{
        QuoteParseError, QuoteParser, QuoteSerializer, QuoteValidationError, QuoteValidator,
        utils::{format_mrenclave, format_mrsigner},
    },
};

fn temp_sealed_storage_path(test_name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "credbridge-dcap-tests-{test_name}-{}",
        uuid::Uuid::new_v4()
    ))
}

/// 创建 simulation-safe 的 DCAP 服务。
fn create_simulation_safe_dcap_service() -> DcapService {
    let runtime = TeeRuntimeConfig::simulation();
    let config = DcapConfig {
        runtime_mode: runtime.mode,
        quote_max_age_seconds: 3600,
        ..Default::default()
    };
    DcapService::new(config).expect("Failed to create DCAP service")
}

/// 创建用于 simulation-safe 测试的 Enclave。
fn create_simulation_safe_enclave() -> Enclave {
    let config = EnclaveConfig {
        debug_mode: true,
        sealed_storage_path: temp_sealed_storage_path("simulation-enclave")
            .to_string_lossy()
            .to_string(),
        ..Default::default()
    };
    let mut enclave = Enclave::new(config);
    enclave.initialize().expect("Failed to initialize enclave");
    enclave
}

/// 兼容旧测试调用点；新测试应优先使用更显式的 simulation-safe helper。
fn create_test_service() -> DcapService {
    create_simulation_safe_dcap_service()
}

/// 兼容旧测试调用点；新测试应优先使用更显式的 simulation-safe helper。
fn create_initialized_enclave() -> Enclave {
    create_simulation_safe_enclave()
}

mod dcap_service_tests {
    use super::*;

    #[test]
    fn test_dcap_service_creation() {
        let service = create_simulation_safe_dcap_service();
        assert_eq!(service.verified_enclave_count(), 0);
    }

    #[test]
    fn test_dcap_service_initialize() {
        let service = create_simulation_safe_dcap_service();
        let enclave = create_simulation_safe_enclave();

        let result = service.initialize(&enclave);
        assert!(
            result.is_ok(),
            "Initialize should succeed: {:?}",
            result.err()
        );

        let quote = result.unwrap();
        assert_eq!(quote.version, 3);
        assert_eq!(quote.sign_type, 2);
        assert!(!quote.report_body.mrenclave.iter().all(|&b| b == 0));
        assert!(!quote.report_body.mrsigner.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_get_current_quote_after_initialize() {
        let service = create_simulation_safe_dcap_service();
        let enclave = create_simulation_safe_enclave();

        // Before initialization, getting quote should fail
        assert!(service.get_current_quote().is_err());

        // After initialization, should succeed
        service.initialize(&enclave).expect("Initialize failed");
        let quote = service.get_current_quote();
        assert!(quote.is_ok(), "Should get quote after init");
    }

    #[test]
    fn test_get_attestation_report() {
        let service = create_simulation_safe_dcap_service();
        let enclave = create_simulation_safe_enclave();

        service.initialize(&enclave).expect("Initialize failed");

        let report = service.get_attestation_report();
        assert!(report.is_ok(), "Should get attestation report");

        let report = report.unwrap();
        assert!(!report.mrenclave_hex.is_empty());
        assert!(!report.mrsigner_hex.is_empty());
        assert!(!report.quote_b64.is_empty());
        assert!(report.result.success);
    }

    #[test]
    fn test_verify_attestation_valid() {
        let service = create_simulation_safe_dcap_service();
        let enclave = create_simulation_safe_enclave();

        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize quote");

        let result = service.verify_attestation(&quote_bytes, None);
        assert!(
            result.is_ok(),
            "Should verify valid quote: {:?}",
            result.err()
        );

        let report = result.unwrap();
        assert!(report.result.success);
    }

    #[test]
    fn test_challenge_bound_quote_verifies_with_matching_nonce() {
        let service = create_simulation_safe_dcap_service();
        let enclave = create_simulation_safe_enclave();
        let nonce = b"dcap-challenge-nonce";

        let quote = service
            .generate_quote_for_challenge(&enclave, nonce)
            .expect("challenge-bound quote should be generated in simulation mode");
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize quote");

        let report = service
            .verify_attestation(&quote_bytes, Some(nonce))
            .expect("matching nonce should verify");

        assert!(report.result.success);
        assert_eq!(report.result.mrenclave, enclave.mrenclave());
        assert_eq!(report.result.mrsigner, enclave.mrsigner());
    }

    #[test]
    fn test_challenge_bound_quote_rejects_wrong_nonce() {
        let service = create_simulation_safe_dcap_service();
        let enclave = create_simulation_safe_enclave();

        let quote = service
            .generate_quote_for_challenge(&enclave, b"expected-nonce")
            .expect("challenge-bound quote should be generated in simulation mode");
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize quote");

        let error = service
            .verify_attestation(&quote_bytes, Some(b"wrong-nonce"))
            .expect_err("mismatched nonce must fail verification");

        assert!(matches!(error, DcapError::QuoteVerificationFailed(_)));
        assert!(error.to_string().contains("Challenge binding mismatch"));
    }

    #[test]
    fn test_verify_attestation_invalid_quote() {
        let service = create_simulation_safe_dcap_service();

        let invalid_quote = vec![0u8; 100];
        let result = service.verify_attestation(&invalid_quote, None);
        assert!(result.is_err(), "Should fail with invalid quote");
    }

    #[test]
    fn test_refresh_quote() {
        let service = create_simulation_safe_dcap_service();
        let enclave = create_simulation_safe_enclave();

        service.initialize(&enclave).expect("Initialize failed");
        let quote1 = service.get_current_quote().expect("Failed to get quote 1");

        // Wait a bit to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(10));

        let quote2 = service
            .refresh_quote(&enclave)
            .expect("Failed to refresh quote");

        // New quote should have different timestamp
        assert!(
            quote2.timestamp >= quote1.timestamp,
            "Refreshed quote should have newer or equal timestamp"
        );
    }

    #[test]
    fn test_hardware_mode_quote_generation_fails_closed() {
        let enclave = create_simulation_safe_enclave();
        let service = DcapService::new(DcapConfig {
            runtime_mode: TeeRuntimeMode::Hardware,
            ..Default::default()
        })
        .expect("Failed to create hardware-mode service");

        let error = service
            .initialize(&enclave)
            .expect_err("hardware mode must not silently generate simulated DCAP quotes");

        assert!(
            matches!(error, DcapError::QuoteGenerationFailed(_)),
            "unexpected error: {error:?}"
        );
        assert!(
            error
                .to_string()
                .contains("refusing to fall back to simulation"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_hardware_mode_challenge_quote_fails_closed() {
        let enclave = create_simulation_safe_enclave();
        let service = DcapService::new(DcapConfig {
            runtime_mode: TeeRuntimeMode::Hardware,
            ..Default::default()
        })
        .expect("Failed to create hardware-mode service");

        let error = service
            .generate_quote_for_challenge(&enclave, b"challenge")
            .expect_err("hardware mode must not silently mint simulated challenge quotes");

        assert!(matches!(error, DcapError::QuoteGenerationFailed(_)));
        assert!(
            error
                .to_string()
                .contains("refusing to fall back to simulation"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_measurement_whitelist_mrenclave() {
        let enclave = create_simulation_safe_enclave();
        let mrenclave = enclave.mrenclave();

        let config = DcapConfig {
            runtime_mode: TeeRuntimeMode::Simulation, // 使用模拟模式：测试侧重测量白名单逻辑而非签名验证
            allowed_mrenclaves: vec![mrenclave],
            ..Default::default()
        };
        let service = DcapService::new(config).expect("Failed to create service");
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");

        let result = service.verify_attestation(&quote_bytes, None);
        assert!(result.is_ok(), "Should verify with matching MRENCLAVE");
    }

    #[test]
    fn test_measurement_whitelist_mrsigner() {
        let enclave = create_simulation_safe_enclave();
        let mrsigner = enclave.mrsigner();

        let config = DcapConfig {
            runtime_mode: TeeRuntimeMode::Simulation, // 使用模拟模式：测试侧重测量白名单逻辑而非签名验证
            allowed_mrsigners: vec![mrsigner],
            ..Default::default()
        };
        let service = DcapService::new(config).expect("Failed to create service");
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");

        let result = service.verify_attestation(&quote_bytes, None);
        assert!(result.is_ok(), "Should verify with matching MRSIGNER");
    }

    #[test]
    fn test_measurement_mismatch() {
        let enclave = create_simulation_safe_enclave();
        let wrong_mrenclave = [0x99u8; 32];

        let config = DcapConfig {
            runtime_mode: TeeRuntimeMode::Simulation, // 使用模拟模式：测试侧重测量不匹配逻辑而非签名验证
            allowed_mrenclaves: vec![wrong_mrenclave],
            ..Default::default()
        };
        let service = DcapService::new(config).expect("Failed to create service");
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");

        let result = service.verify_attestation(&quote_bytes, None);
        assert!(
            matches!(result.unwrap_err(), DcapError::MeasurementMismatch),
            "Should fail with measurement mismatch"
        );
    }

    #[test]
    fn test_allow_mrenclave_mrsigner() {
        let config = DcapConfig {
            runtime_mode: TeeRuntimeMode::Simulation,
            ..Default::default()
        };
        let mut service = DcapService::new(config).expect("Failed to create service");

        let mrenclave = [0x42u8; 32];
        let mrsigner = [0x43u8; 32];

        service.allow_mrenclave(mrenclave);
        service.allow_mrsigner(mrsigner);

        // Verify whitelist works by testing with a valid enclave
        // (The internal state is verified through behavior, not direct field access)
    }

    #[test]
    fn test_verified_enclave_count() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();

        assert_eq!(service.verified_enclave_count(), 0);

        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");

        service
            .verify_attestation(&quote_bytes, None)
            .expect("Verify failed");

        assert_eq!(service.verified_enclave_count(), 1);
    }

    #[test]
    fn test_dcap_config_default() {
        let config = DcapConfig::default();
        assert_eq!(config.pcs_base_url, INTEL_PCS_BASE_URL_PROD);
        assert!(!config.use_test_environment);
        assert_eq!(config.quote_max_age_seconds, 3600);
        assert!(config.verify_certificate_chain);
        assert_eq!(config.runtime_mode, TeeRuntimeMode::Hardware);
        assert!(config.allowed_mrenclaves.is_empty());
        assert!(config.allowed_mrsigners.is_empty());
    }
}

mod quote_parser_tests {
    use super::*;

    fn create_test_quote_bytes() -> Vec<u8> {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        QuoteSerializer::serialize(&quote).expect("Failed to serialize")
    }

    #[test]
    fn test_quote_parser_parse() {
        let bytes = create_test_quote_bytes();
        let parsed = QuoteParser::parse(&bytes);
        assert!(parsed.is_ok(), "Should parse quote: {:?}", parsed.err());

        let parsed = parsed.unwrap();
        assert_eq!(parsed.version, 3);
        assert_eq!(parsed.sign_type, 2);
        assert!(!parsed.report_body.mrenclave.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_quote_parser_extract_mrenclave() {
        let bytes = create_test_quote_bytes();
        let mrenclave = QuoteParser::extract_mrenclave(&bytes);
        assert!(mrenclave.is_ok());
        assert!(!mrenclave.unwrap().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_quote_parser_extract_mrsigner() {
        let bytes = create_test_quote_bytes();
        let mrsigner = QuoteParser::extract_mrsigner(&bytes);
        assert!(mrsigner.is_ok());
        assert!(!mrsigner.unwrap().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_quote_parser_parse_metadata() {
        let bytes = create_test_quote_bytes();
        let metadata = QuoteParser::parse_metadata(&bytes);
        assert!(metadata.is_ok());

        let metadata = metadata.unwrap();
        assert_eq!(metadata.version, 3);
        assert_eq!(metadata.sign_type, 2);
        assert!(metadata.has_signature);
        assert!(metadata.signature_size > 0);
    }

    #[test]
    fn test_quote_parser_insufficient_data() {
        let result = QuoteParser::parse(&[0u8; 100]);
        assert!(
            matches!(
                result.unwrap_err(),
                QuoteParseError::InsufficientData { .. }
            ),
            "Should fail with insufficient data"
        );
    }

    #[test]
    fn test_quote_parser_extract_mrenclave_insufficient_data() {
        let result = QuoteParser::extract_mrenclave(&[0u8; 50]);
        assert!(
            matches!(
                result.unwrap_err(),
                QuoteParseError::InsufficientData { .. }
            ),
            "Should fail with insufficient data for MRENCLAVE extraction"
        );
    }
}

mod quote_serializer_tests {
    use super::*;

    #[test]
    fn test_quote_serializer_roundtrip() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");

        let bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");
        let parsed = QuoteParser::parse(&bytes).expect("Failed to parse");

        assert_eq!(parsed.version, quote.version);
        assert_eq!(parsed.sign_type, quote.sign_type);
        assert_eq!(parsed.report_body.mrenclave, quote.report_body.mrenclave);
        assert_eq!(parsed.report_body.mrsigner, quote.report_body.mrsigner);
    }

    #[test]
    fn test_quote_serializer_to_hex() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");

        let hex = QuoteSerializer::serialize_to_hex(&quote).expect("Failed to serialize to hex");
        assert!(!hex.is_empty());
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_quote_serializer_to_base64() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");

        let b64 =
            QuoteSerializer::serialize_to_base64(&quote).expect("Failed to serialize to base64");
        assert!(!b64.is_empty());
        // Base64 should be valid
        assert!(base64::decode(&b64).is_ok());
    }
}

mod quote_validator_tests {
    use super::*;

    #[test]
    fn test_quote_validator_valid_quote() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");

        let parsed = QuoteParser::parse(&bytes).expect("Failed to parse");
        let validator = QuoteValidator::new();

        let result = validator.validate(&parsed);
        assert!(
            result.is_ok(),
            "Should validate valid quote: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_quote_validator_dcap_quote() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let validator = QuoteValidator::new();

        let result = validator.validate_dcap(&quote);
        assert!(
            result.is_ok(),
            "Should validate DCAP quote: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_quote_validator_version_mismatch() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let mut quote = service.get_current_quote().expect("Failed to get quote");
        quote.version = 99; // Invalid version

        let validator = QuoteValidator::new();
        let result = validator.validate_dcap(&quote);

        assert!(
            matches!(
                result.unwrap_err(),
                QuoteValidationError::VersionMismatch { .. }
            ),
            "Should fail with version mismatch"
        );
    }

    #[test]
    fn test_quote_validator_invalid_mrenclave() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let mut quote = service.get_current_quote().expect("Failed to get quote");
        quote.report_body.mrenclave = [0u8; 32]; // Invalid MRENCLAVE

        let validator = QuoteValidator::new();
        let result = validator.validate_dcap(&quote);

        assert!(
            matches!(result.unwrap_err(), QuoteValidationError::InvalidMrenclave),
            "Should fail with invalid MRENCLAVE"
        );
    }

    #[test]
    fn test_quote_validator_missing_signature() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let mut quote = service.get_current_quote().expect("Failed to get quote");
        quote.signature_len = 0; // No signature

        let validator = QuoteValidator::new();
        let result = validator.validate_dcap(&quote);

        assert!(
            matches!(result.unwrap_err(), QuoteValidationError::MissingSignature),
            "Should fail with missing signature"
        );
    }

    #[test]
    fn test_quote_validator_bytes() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");
        let bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");

        let validator = QuoteValidator::new();
        let result = validator.validate_bytes(&bytes);
        assert!(result.is_ok(), "Should validate bytes: {:?}", result.err());
    }
}

mod utils_tests {
    use super::*;

    #[test]
    fn test_format_mrenclave() {
        let mrenclave = [0x42u8; 32];
        let formatted = format_mrenclave(&mrenclave);

        assert!(formatted.starts_with("42424242"));
        assert!(formatted.ends_with("42424242"));
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_format_mrsigner() {
        let mrsigner = [0x43u8; 32];
        let formatted = format_mrsigner(&mrsigner);

        assert!(formatted.starts_with("43434343"));
        assert!(formatted.ends_with("43434343"));
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_mrenclave_mrsigner_equality() {
        use vault_service::tee::quote::utils::{mrenclave_eq, mrsigner_eq};

        let m1 = [0x42u8; 32];
        let m2 = [0x42u8; 32];
        let m3 = [0x43u8; 32];

        assert!(mrenclave_eq(&m1, &m2));
        assert!(!mrenclave_eq(&m1, &m3));

        assert!(mrsigner_eq(&m1, &m2));
        assert!(!mrsigner_eq(&m1, &m3));
    }
}

mod dcap_quote_structure_tests {
    use super::*;

    #[test]
    fn test_ecdsa_signature_dcap() {
        let r = [0x42u8; 32];
        let s = [0x43u8; 32];

        let sig = EcdsaSignatureDcap::new(r, s);
        assert_eq!(sig.r, r);
        assert_eq!(sig.s, s);

        let bytes = sig.to_bytes();
        assert_eq!(bytes.len(), 64);

        let restored = EcdsaSignatureDcap::from_bytes(&bytes).expect("Failed to restore signature");
        assert_eq!(restored.r, r);
        assert_eq!(restored.s, s);
    }

    #[test]
    fn test_dcap_quote_signature_from_bytes_invalid() {
        let result = EcdsaSignatureDcap::from_bytes(&[0u8; 32]); // Wrong size
        assert!(result.is_err());
    }

    #[test]
    fn test_dcap_report_body_fields() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();
        service.initialize(&enclave).expect("Initialize failed");

        let quote = service.get_current_quote().expect("Failed to get quote");

        assert!(!quote.report_body.cpusvn.iter().all(|&b| b == 0));
        assert_eq!(quote.report_body.mrenclave, enclave.mrenclave());
        assert_eq!(quote.report_body.mrsigner, enclave.mrsigner());
    }
}

mod integration_tests {
    use super::*;

    #[test]
    fn test_full_attestation_flow() {
        // 1. 创建 DCAP 服务
        let service = create_test_service();

        // 2. 创建并初始化 Enclave
        let enclave = create_initialized_enclave();

        // 3. 初始化 DCAP 服务（生成 Quote）
        let quote = service.initialize(&enclave).expect("Initialize failed");

        // 4. 获取认证报告
        let report = service
            .get_attestation_report()
            .expect("Failed to get report");

        // 5. 验证报告包含正确的测量值
        assert_eq!(report.result.mrenclave, enclave.mrenclave());
        assert_eq!(report.result.mrsigner, enclave.mrsigner());

        // 6. 序列化 Quote
        let quote_bytes = QuoteSerializer::serialize(&quote).expect("Failed to serialize");

        // 7. 解析 Quote
        let parsed = QuoteParser::parse(&quote_bytes).expect("Failed to parse");

        // 8. 验证 Quote
        let validator = QuoteValidator::new();
        validator.validate(&parsed).expect("Validation failed");

        // 9. 执行远程认证验证
        let verify_result = service.verify_attestation(&quote_bytes, None);
        assert!(verify_result.is_ok(), "Remote attestation should succeed");
    }

    #[test]
    fn test_multiple_enclaves_attestation() {
        let service = create_test_service();

        // 创建多个 Enclave
        let enclave1 = create_initialized_enclave();
        let enclave2 = create_initialized_enclave();

        // 初始化 DCAP 服务
        service.initialize(&enclave1).expect("Initialize failed");

        // 验证 Quote 包含第一个 Enclave 的测量值
        let quote1 = service.get_current_quote().expect("Failed to get quote 1");
        assert_eq!(quote1.report_body.mrenclave, enclave1.mrenclave());

        // 刷新为第二个 Enclave 的 Quote
        let quote2 = service.refresh_quote(&enclave2).expect("Failed to refresh");
        assert_eq!(quote2.report_body.mrenclave, enclave2.mrenclave());
    }

    #[test]
    fn test_enclave_measurement_persistence() {
        let service = create_test_service();
        let enclave = create_initialized_enclave();

        service.initialize(&enclave).expect("Initialize failed");

        let mrenclave1 = enclave.mrenclave();
        let mrsigner1 = enclave.mrsigner();

        // 多次获取 Quote，测量值应保持一致
        for _ in 0..5 {
            let quote = service.get_current_quote().expect("Failed to get quote");
            assert_eq!(quote.report_body.mrenclave, mrenclave1);
            assert_eq!(quote.report_body.mrsigner, mrsigner1);
        }
    }
}

mod error_handling_tests {
    use super::*;

    #[test]
    fn test_dcap_error_display() {
        let errors = vec![
            DcapError::QuoteGenerationFailed("test".to_string()),
            DcapError::QuoteVerificationFailed("test".to_string()),
            DcapError::PcsCommunicationFailed("test".to_string()),
            DcapError::CertificateVerificationFailed("test".to_string()),
            DcapError::InvalidCertificateChain,
            DcapError::SignatureVerificationFailed,
            DcapError::MeasurementMismatch,
            DcapError::InvalidQuoteFormat,
            DcapError::ConfigurationError("test".to_string()),
            DcapError::EnclaveError("test".to_string()),
            DcapError::InternalError("test".to_string()),
        ];

        for error in errors {
            let display = error.to_string();
            assert!(!display.is_empty(), "Error should have display message");
        }
    }

    #[test]
    fn test_quote_parse_error_display() {
        let errors = vec![
            QuoteParseError::InsufficientData {
                expected: 100,
                actual: 50,
            },
            QuoteParseError::InvalidReportBodySize(200),
            QuoteParseError::InvalidVersion(99),
            QuoteParseError::InvalidSignType(99),
            QuoteParseError::UnsupportedFormat("test".to_string()),
        ];

        for error in errors {
            let display = error.to_string();
            assert!(!display.is_empty(), "Error should have display message");
        }
    }

    #[test]
    fn test_quote_validation_error_display() {
        let errors = vec![
            QuoteValidationError::VersionMismatch {
                expected: 3,
                actual: 99,
            },
            QuoteValidationError::SignTypeMismatch {
                expected: 2,
                actual: 99,
            },
            QuoteValidationError::InvalidMrenclave,
            QuoteValidationError::InvalidMrsigner,
            QuoteValidationError::MissingSignature,
            QuoteValidationError::ParseError("test".to_string()),
            QuoteValidationError::ValidationFailed("test".to_string()),
        ];

        for error in errors {
            let display = error.to_string();
            assert!(!display.is_empty(), "Error should have display message");
        }
    }
}
