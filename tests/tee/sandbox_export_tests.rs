//! TEE 沙箱安全导出功能集成测试
//!
//! 测试页面冻结、安全截图、内容审核和脱敏功能的集成

use std::sync::Arc;
use vault_service::crypto::{EnclaveKeyManager, KeyAlgorithm, KeyConfig, KeyState};
use vault_service::tee::sandbox::export::{
    ContentReviewer, ExportFormat, ExportRequest, ExportService, FreezeState, ImageFormat,
    PageStateFreezer, RedactionService, ReviewConfig, RiskLevel, ScreenshotConfig,
    ScreenshotRequest, ScreenshotService, SensitiveType,
};
use vault_service::tee::sandbox::types::SessionId;

/// 测试页面冻结器的基本功能
#[tokio::test]
async fn test_page_state_freezer_basic() {
    let session_id = SessionId::new();
    let freezer = PageStateFreezer::new(session_id);

    // 初始状态检查
    assert!(!freezer.is_frozen().await);
    assert_eq!(freezer.state().await, FreezeState::Unfrozen);

    // 创建页面信息
    let page_info = vault_service::tee::sandbox::export::freezer::PageInfo {
        url: "https://example.com/test".to_string(),
        dom_hash: "abc123def456".to_string(),
        viewport: vault_service::tee::sandbox::export::freezer::ViewportInfo {
            width: 1920,
            height: 1080,
            scroll_x: 0.0,
            scroll_y: 100.0,
            device_scale_factor: 1.0,
        },
        metadata: Default::default(),
    };

    // 冻结页面
    let frozen = freezer.freeze(page_info).await.unwrap();
    assert_eq!(frozen.session_id, session_id);
    assert!(freezer.is_frozen().await);

    // 验证冻结状态
    let stored_state = freezer.get_frozen_state().await;
    assert!(stored_state.is_some());
    assert_eq!(stored_state.unwrap().session_id, session_id);

    // 解冻页面
    freezer.unfreeze().await.unwrap();
    assert!(!freezer.is_frozen().await);
    assert!(freezer.get_frozen_state().await.is_none());
}

/// 测试页面冻结超时机制
#[tokio::test]
async fn test_freezer_timeout() {
    use std::time::Duration;

    let session_id = SessionId::new();
    let freezer = PageStateFreezer::with_timeout(session_id, Duration::from_millis(100));

    let page_info = vault_service::tee::sandbox::export::freezer::PageInfo {
        url: "https://example.com".to_string(),
        dom_hash: "hash123".to_string(),
        viewport: vault_service::tee::sandbox::export::freezer::ViewportInfo {
            width: 1920,
            height: 1080,
            scroll_x: 0.0,
            scroll_y: 0.0,
            device_scale_factor: 1.0,
        },
        metadata: Default::default(),
    };

    // 冻结页面
    freezer.freeze(page_info).await.unwrap();
    assert!(freezer.is_frozen().await);

    // 验证冻结有效
    assert!(freezer.validate_freeze().await.is_ok());

    // 等待超时
    tokio::time::sleep(Duration::from_millis(200)).await;

    // 冻结应该已超时
    assert!(freezer.validate_freeze().await.is_err());
}

/// 测试重复冻结错误处理
#[tokio::test]
async fn test_double_freeze_error() {
    let session_id = SessionId::new();
    let freezer = PageStateFreezer::new(session_id);

    let page_info = vault_service::tee::sandbox::export::freezer::PageInfo {
        url: "https://example.com".to_string(),
        dom_hash: "hash".to_string(),
        viewport: vault_service::tee::sandbox::export::freezer::ViewportInfo {
            width: 1920,
            height: 1080,
            scroll_x: 0.0,
            scroll_y: 0.0,
            device_scale_factor: 1.0,
        },
        metadata: Default::default(),
    };

    // 第一次冻结
    freezer.freeze(page_info.clone()).await.unwrap();

    // 第二次冻结应该失败
    let result = freezer.freeze(page_info).await;
    assert!(result.is_err());
}

/// 测试截图服务集成
#[tokio::test]
async fn test_screenshot_service_integration() {
    let session_id = SessionId::new();
    let freezer = PageStateFreezer::new(session_id);
    let service = ScreenshotService::with_freezer(freezer);

    // 捕获截图
    let request = ScreenshotRequest::default();
    let result = service.capture(request).await;

    assert!(result.is_ok());
    let screenshot = result.unwrap();
    assert_eq!(screenshot.session_id, session_id);
    assert!(screenshot.frozen_state.is_some());
    assert!(!screenshot.data.is_empty());
}

/// 测试不同图片格式的截图
#[tokio::test]
async fn test_screenshot_different_formats() {
    let session_id = SessionId::new();
    let freezer = PageStateFreezer::new(session_id);
    let service = ScreenshotService::with_freezer(freezer);

    for format in [ImageFormat::Png, ImageFormat::Jpeg] {
        let request = ScreenshotRequest {
            format,
            ..Default::default()
        };

        let result = service.capture(request).await;
        assert!(result.is_ok(), "Failed for format: {:?}", format);

        let screenshot = result.unwrap();
        assert_eq!(screenshot.format, format);
    }
}

/// 测试脱敏服务
#[test]
fn test_redaction_service_creation() {
    let service = RedactionService::new();
    // 基本功能测试
    assert_eq!(
        service.select_strategy_for_type(SensitiveType::Credential),
        vault_service::tee::sandbox::export::review::RedactionAction::Pixelate
    );
}

/// 测试导出服务
#[tokio::test]
async fn test_export_service_json() {
    let redaction_service = Arc::new(RedactionService::new());
    let export_service = ExportService::with_redaction_service(redaction_service);

    let data = serde_json::json!({
        "user": "test_user",
        "email": "test@example.com",
        "balance": 1000.50
    });

    let request = ExportRequest::new(ExportFormat::Json, data)
        .with_redaction(false)
        .with_metadata(true);

    let result = export_service.export(request, SessionId::new()).await;
    assert!(result.is_ok());

    let export = result.unwrap();
    assert_eq!(export.format, ExportFormat::Json);
    assert!(!export.data.is_empty());
}

/// 测试 CSV 导出
#[tokio::test]
async fn test_export_service_csv() {
    let redaction_service = Arc::new(RedactionService::new());
    let export_service = ExportService::with_redaction_service(redaction_service);

    let data = serde_json::json!([
        {"name": "Alice", "age": 30, "email": "alice@example.com"},
        {"name": "Bob", "age": 25, "email": "bob@example.com"}
    ]);

    let request = ExportRequest::new(ExportFormat::Csv, data).with_redaction(false);

    let result = export_service.export(request, SessionId::new()).await;
    assert!(result.is_ok());

    let export = result.unwrap();
    assert_eq!(export.format, ExportFormat::Csv);
    assert!(!export.data.is_empty());
}

/// 测试 Enclave 密钥管理器
#[tokio::test]
async fn test_enclave_key_manager_basic() {
    use std::path::PathBuf;
    use vault_service::tee::sealing::SealedStorage;

    // 创建临时存储目录
    let temp_dir =
        std::env::temp_dir().join(format!("vault_service_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let storage = Arc::new(SealedStorage::new(temp_dir.to_string_lossy().to_string()));
    let config = KeyConfig::default();
    let mut manager = EnclaveKeyManager::new(storage, config);

    // 生成密钥
    let key = manager.generate_key().await.unwrap();
    assert_eq!(key.state, KeyState::Active);
    assert!(key.version > 0);
    assert!(!key.key_id.is_empty());
}

/// 测试密钥签名和验证
#[tokio::test]
async fn test_enclave_key_sign_and_verify() {
    use vault_service::tee::sealing::SealedStorage;

    let temp_dir =
        std::env::temp_dir().join(format!("vault_service_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let storage = Arc::new(SealedStorage::new(temp_dir.to_string_lossy().to_string()));
    let config = KeyConfig::default();
    let mut manager = EnclaveKeyManager::new(storage, config);

    // 生成密钥
    manager.generate_key().await.unwrap();

    // 签名数据
    let data = b"test data to sign";
    let signature = manager.sign(data).await.unwrap();

    assert_eq!(signature.algorithm, "Ed25519");
    assert!(!signature.key_id.is_empty());
    assert!(!signature.data.is_empty());
    assert!(!signature.public_key.is_empty());

    // 验证签名
    let valid = manager.verify(data, &signature).await.unwrap();
    assert!(valid);

    // 验证错误的数据
    let invalid = manager.verify(b"wrong data", &signature).await.unwrap();
    assert!(!invalid);
}

/// 测试密钥轮换
#[tokio::test]
async fn test_key_rotation() {
    use vault_service::tee::sealing::SealedStorage;

    let temp_dir =
        std::env::temp_dir().join(format!("vault_service_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let storage = Arc::new(SealedStorage::new(temp_dir.to_string_lossy().to_string()));
    let config = KeyConfig::default();
    let mut manager = EnclaveKeyManager::new(storage, config);

    // 生成初始密钥
    let key1 = manager.generate_key().await.unwrap();
    let key1_id = key1.key_id.clone();

    // 使用初始密钥签名
    let data = b"test data";
    let sig1 = manager.sign(data).await.unwrap();
    assert_eq!(sig1.key_id, key1_id);

    // 轮换密钥
    manager.rotate_key().await.unwrap();

    // 检查新密钥
    let key2 = manager.current_key().await.unwrap();
    assert_ne!(key2.key_id, key1_id);
    assert_eq!(key2.version, 2);

    // 旧签名仍然可以验证
    let valid = manager.verify(data, &sig1).await.unwrap();
    assert!(valid);

    // 新签名
    let sig2 = manager.sign(data).await.unwrap();
    assert_eq!(sig2.key_id, key2.key_id);
}

/// 测试风险等级排序
#[test]
fn test_risk_level_ordering() {
    use vault_service::tee::sandbox::export::review::RiskLevel;

    assert!(RiskLevel::None < RiskLevel::Low);
    assert!(RiskLevel::Low < RiskLevel::Medium);
    assert!(RiskLevel::Medium < RiskLevel::High);
    assert!(RiskLevel::High < RiskLevel::Critical);
}

/// 测试敏感类型到脱敏策略的映射
#[test]
fn test_sensitive_type_to_strategy_mapping() {
    use vault_service::tee::sandbox::export::{
        RedactionService,
        review::{RedactionAction, SensitiveType},
    };

    let service = RedactionService::new();

    // 凭证应该使用像素化
    assert_eq!(
        service.select_strategy_for_type(SensitiveType::Credential),
        RedactionAction::Pixelate
    );

    // 信用卡应该使用遮罩
    assert_eq!(
        service.select_strategy_for_type(SensitiveType::CreditCard),
        RedactionAction::Blackout
    );

    // 地址应该使用模糊
    assert_eq!(
        service.select_strategy_for_type(SensitiveType::Address),
        RedactionAction::Blur
    );
}

/// 测试导出请求构建器
#[test]
fn test_export_request_builder() {
    let data = serde_json::json!({"test": "data"});

    let request = ExportRequest::new(ExportFormat::Json, data.clone())
        .with_redaction(true)
        .with_metadata(false);

    assert!(request.redact_sensitive);
    assert!(!request.include_metadata);
    assert_eq!(request.format, ExportFormat::Json);
}

/// 测试图片格式属性
#[test]
fn test_image_format_properties() {
    assert_eq!(ImageFormat::Png.mime_type(), "image/png");
    assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
    assert_eq!(ImageFormat::Webp.to_string(), "webp");
}

/// 测试导出格式属性
#[test]
fn test_export_format_properties() {
    assert_eq!(ExportFormat::Json.mime_type(), "application/json");
    assert_eq!(ExportFormat::Csv.extension(), "csv");
    assert_eq!(ExportFormat::Pdf.to_string(), "pdf");
}
