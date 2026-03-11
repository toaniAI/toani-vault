//! 远程认证 API
//!
//! 提供 REST API 端点用于 DCAP 远程认证，包括：
//! - Quote 获取
//! - 认证验证
//! - 认证报告查询
//! - 挑战-响应协议
//! - 认证状态查询
//!
//! # API 端点
//!
//! ```text
//! GET  /api/v1/attestation/quote          - 获取当前 Quote
//! POST /api/v1/attestation/verify         - 验证 Quote
//! GET  /api/v1/attestation/report         - 获取认证报告
//! POST /api/v1/attestation/challenge      - 创建认证挑战（返回 nonce 和 Quote）
//! POST /api/v1/attestation/verify-response - 验证挑战响应
//! GET  /api/v1/attestation/status         - 获取认证状态
//! POST /api/v1/attestation/refresh        - 刷新 Quote
//! ```

use crate::tee::{
    attestation::{AttestationService, AttestationState as TeeAttestationState},
    challenge::{Challenge, ChallengeError, ChallengeMetadata, ChallengeProtocol, ChallengeResponse, ChallengeStatus, ProverProtocol},
    dcap::{
        DcapConfig, DcapService,
        INTEL_PCS_BASE_URL_PROD, INTEL_PCS_BASE_URL_TEST,
    },
    enclave::{Enclave, EnclaveConfig},
    quote::QuoteSerializer,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// 认证状态
pub struct AttestationState {
    /// DCAP 服务
    dcap_service: Arc<RwLock<DcapService>>,

    /// Enclave 实例
    enclave: Arc<RwLock<Enclave>>,

    /// 服务配置
    config: AttestationApiConfig,

    /// 挑战-响应协议处理器
    challenge_protocol: Arc<RwLock<ChallengeProtocol>>,

    /// Prover 协议处理器（用于生成 Quote）
    prover_protocol: Arc<RwLock<ProverProtocol>>,

    /// 认证服务
    attestation_service: Arc<RwLock<AttestationService>>,
}

impl std::fmt::Debug for AttestationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttestationState")
            .field("config", &self.config)
            .field("dcap_service", &"<DcapService>")
            .field("enclave", &"<Enclave>")
            .field("challenge_protocol", &"<ChallengeProtocol>")
            .field("prover_protocol", &"<ProverProtocol>")
            .field("attestation_service", &"<AttestationService>")
            .finish()
    }
}

impl Clone for AttestationState {
    fn clone(&self) -> Self {
        Self {
            dcap_service: Arc::clone(&self.dcap_service),
            enclave: Arc::clone(&self.enclave),
            config: self.config.clone(),
            challenge_protocol: Arc::clone(&self.challenge_protocol),
            prover_protocol: Arc::clone(&self.prover_protocol),
            attestation_service: Arc::clone(&self.attestation_service),
        }
    }
}

/// API 配置
#[derive(Debug, Clone)]
pub struct AttestationApiConfig {
    /// 是否启用模拟模式
    pub simulation_mode: bool,

    /// 是否需要 API 密钥
    pub require_api_key: bool,

    /// Quote 最大有效期（秒）
    pub quote_max_age: u64,

    /// 是否启用 PCS 注册
    pub enable_pcs_registration: bool,
}

impl Default for AttestationApiConfig {
    fn default() -> Self {
        Self {
            simulation_mode: false,
            require_api_key: false,
            quote_max_age: 3600,
            enable_pcs_registration: true,
        }
    }
}

/// 创建认证路由
pub fn attestation_routes(state: Arc<AttestationState>) -> Router {
    Router::new()
        .route("/quote", get(get_quote))
        .route("/verify", post(verify_quote))
        .route("/report", get(get_report))
        .route("/challenge", post(create_challenge))
        .route("/verify-response", post(verify_challenge_response))
        .route("/status", get(get_attestation_status))
        .route("/refresh", post(refresh_quote))
        .route("/health", get(health_check))
        .with_state(state)
}

/// Quote 响应
#[derive(Debug, Serialize)]
pub struct QuoteResponse {
    pub success: bool,
    pub data: Option<QuoteData>,
    pub error: Option<String>,
}

/// Quote 数据
#[derive(Debug, Serialize)]
pub struct QuoteData {
    pub version: u16,
    pub sign_type: u16,
    pub mrenclave: String,
    pub mrsigner: String,
    pub timestamp: u64,
    pub quote_b64: String,
}

/// 验证请求
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub quote_b64: String,
    pub nonce: Option<String>,
}

/// 验证响应
#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub success: bool,
    pub valid: bool,
    pub mrenclave: String,
    pub mrsigner: String,
    pub timestamp: u64,
    pub error: Option<String>,
}

/// 认证报告响应
#[derive(Debug, Serialize)]
pub struct ReportResponse {
    pub success: bool,
    pub data: Option<ReportData>,
    pub error: Option<String>,
}

/// 报告数据
#[derive(Debug, Serialize)]
pub struct ReportData {
    pub version: String,
    pub mrenclave: String,
    pub mrsigner: String,
    pub security_version: u16,
    pub product_id: u16,
    pub attributes: String,
    pub timestamp: u64,
    pub quote_b64: String,
    pub certificate_info: CertificateInfoData,
}

/// 证书信息数据
#[derive(Debug, Serialize)]
pub struct CertificateInfoData {
    pub subject: String,
    pub issuer: String,
    pub not_before: String,
    pub not_after: String,
    pub fingerprint: String,
}

/// 挑战请求（创建新挑战）
#[derive(Debug, Deserialize)]
pub struct CreateChallengeRequest {
    /// 可选的 Enclave ID
    pub enclave_id: Option<String>,
}

/// 挑战响应数据
#[derive(Debug, Serialize)]
pub struct ChallengeResponseData {
    pub success: bool,
    pub challenge_id: String,
    pub nonce: String,
    pub quote_b64: String,
    pub expires_at: u64,
    pub mrenclave: String,
    pub mrsigner: String,
    pub error: Option<String>,
}

/// 验证挑战响应请求
#[derive(Debug, Deserialize)]
pub struct VerifyChallengeResponseRequest {
    pub challenge_id: String,
    pub quote_b64: String,
    pub signature_b64: Option<String>,
}

/// 验证挑战响应结果
#[derive(Debug, Serialize)]
pub struct VerifyChallengeResponseResult {
    pub success: bool,
    pub verified: bool,
    pub mrenclave: String,
    pub mrsigner: String,
    pub timestamp: u64,
    pub error: Option<String>,
}

/// 认证状态响应
#[derive(Debug, Serialize)]
pub struct AttestationStatusResponse {
    pub success: bool,
    pub status: String,
    pub enclave_state: String,
    pub mrenclave: String,
    pub mrsigner: String,
    pub quote_valid: bool,
    pub quote_expires_at: Option<u64>,
    pub last_verified_at: Option<u64>,
    pub error: Option<String>,
}

/// 认证状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiAttestationStatus {
    /// 已认证
    Authenticated,
    /// 待验证
    PendingVerification,
    /// 已过期
    Expired,
    /// 未初始化
    Uninitialized,
}

/// 刷新响应
#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub success: bool,
    pub new_quote_b64: Option<String>,
    pub timestamp: u64,
    pub error: Option<String>,
}

/// 健康检查响应
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub enclave_state: String,
    pub dcap_version: String,
    pub quote_valid: bool,
}

/// 获取当前 Quote
///
/// GET /api/v1/attestation/quote
///
/// 返回当前的 DCAP Quote，包含 MRENCLAVE 和 MRSIGNER
async fn get_quote(
    State(state): State<Arc<AttestationState>>,
) -> impl IntoResponse {
    let dcap_service = match state.dcap_service.read() {
        Ok(service) => service,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(QuoteResponse {
                    success: false,
                    data: None,
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                }),
            );
        }
    };

    match dcap_service.get_current_quote() {
        Ok(quote) => {
            match QuoteSerializer::serialize(&quote) {
                Ok(quote_bytes) => {
                    let quote_b64 = base64::encode(&quote_bytes);
                    (
                        StatusCode::OK,
                        Json(QuoteResponse {
                            success: true,
                            data: Some(QuoteData {
                                version: quote.version,
                                sign_type: quote.sign_type,
                                mrenclave: hex::encode(&quote.report_body.mrenclave),
                                mrsigner: hex::encode(&quote.report_body.mrsigner),
                                timestamp: quote.timestamp,
                                quote_b64,
                            }),
                            error: None,
                        }),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(QuoteResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Failed to serialize quote: {}", e)),
                    }),
                ),
            }
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(QuoteResponse {
                success: false,
                data: None,
                error: Some(format!("No valid quote available: {}", e)),
            }),
        ),
    }
}

/// 验证 Quote
///
/// POST /api/v1/attestation/verify
///
/// 验证客户端提供的 Quote 的有效性
async fn verify_quote(
    State(state): State<Arc<AttestationState>>,
    Json(request): Json<VerifyRequest>,
) -> impl IntoResponse {
    let dcap_service = match state.dcap_service.read() {
        Ok(service) => service,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyResponse {
                    success: false,
                    valid: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                }),
            );
        }
    };

    // 解码 Quote
    let quote_bytes = match base64::decode(&request.quote_b64) {
        Ok(bytes) => bytes,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse {
                    success: false,
                    valid: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some(format!("Invalid base64 quote: {}", e)),
                }),
            );
        }
    };

    // 验证 nonce（如果提供）
    let nonce = request.nonce.as_ref().map(|n| base64::decode(n).ok()).flatten();

    // 执行验证
    match dcap_service.verify_attestation(&quote_bytes, nonce.as_deref()) {
        Ok(report) => (
            StatusCode::OK,
            Json(VerifyResponse {
                success: true,
                valid: true,
                mrenclave: report.mrenclave_hex,
                mrsigner: report.mrsigner_hex,
                timestamp: report.timestamp,
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(VerifyResponse {
                success: true,
                valid: false,
                mrenclave: String::new(),
                mrsigner: String::new(),
                timestamp: 0,
                error: Some(format!("Verification failed: {}", e)),
            }),
        ),
    }
}

/// 获取认证报告
///
/// GET /api/v1/attestation/report
///
/// 返回详细的认证报告
async fn get_report(
    State(state): State<Arc<AttestationState>>,
) -> impl IntoResponse {
    let dcap_service = match state.dcap_service.read() {
        Ok(service) => service,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ReportResponse {
                    success: false,
                    data: None,
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                }),
            );
        }
    };

    match dcap_service.get_attestation_report() {
        Ok(report) => {
            let response_data = ReportData {
                version: report.version,
                mrenclave: report.mrenclave_hex,
                mrsigner: report.mrsigner_hex,
                security_version: report.security_version,
                product_id: report.product_id,
                attributes: report.attributes,
                timestamp: report.timestamp,
                quote_b64: report.quote_b64,
                certificate_info: CertificateInfoData {
                    subject: report.certificate_info.subject,
                    issuer: report.certificate_info.issuer,
                    not_before: report.certificate_info.not_before,
                    not_after: report.certificate_info.not_after,
                    fingerprint: report.certificate_info.fingerprint,
                },
            };

            (
                StatusCode::OK,
                Json(ReportResponse {
                    success: true,
                    data: Some(response_data),
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ReportResponse {
                success: false,
                data: None,
                error: Some(format!("Failed to get report: {}", e)),
            }),
        ),
    }
}

/// 创建认证挑战
///
/// POST /api/v1/attestation/challenge
///
/// 创建一个新的认证挑战，返回挑战 nonce 和 Quote
/// 用于远程认证流程中的第一步
async fn create_challenge(
    State(state): State<Arc<AttestationState>>,
    Json(request): Json<CreateChallengeRequest>,
) -> impl IntoResponse {
    // 获取 Enclave 实例
    let enclave = match state.enclave.read() {
        Ok(enclave) => enclave,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChallengeResponseData {
                    success: false,
                    challenge_id: String::new(),
                    nonce: String::new(),
                    quote_b64: String::new(),
                    expires_at: 0,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    error: Some("Failed to acquire Enclave lock".to_string()),
                }),
            );
        }
    };

    // 检查 Enclave 状态
    if !enclave.is_running() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ChallengeResponseData {
                success: false,
                challenge_id: String::new(),
                nonce: String::new(),
                quote_b64: String::new(),
                expires_at: 0,
                mrenclave: String::new(),
                mrsigner: String::new(),
                error: Some("Enclave not running".to_string()),
            }),
        );
    }

    // 生成挑战
    let challenge_protocol = match state.challenge_protocol.read() {
        Ok(protocol) => protocol,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChallengeResponseData {
                    success: false,
                    challenge_id: String::new(),
                    nonce: String::new(),
                    quote_b64: String::new(),
                    expires_at: 0,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    error: Some("Failed to acquire challenge protocol lock".to_string()),
                }),
            );
        }
    };

    let metadata = ChallengeMetadata {
        client_ip: None,
        user_agent: None,
        request_id: None,
        extra: std::collections::HashMap::new(),
    };

    let challenge = match challenge_protocol.generate_challenge(request.enclave_id.clone(), Some(metadata)) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChallengeResponseData {
                    success: false,
                    challenge_id: String::new(),
                    nonce: String::new(),
                    quote_b64: String::new(),
                    expires_at: 0,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    error: Some(format!("Failed to generate challenge: {}", e)),
                }),
            );
        }
    };

    // 使用 Prover 协议生成 Quote
    let prover_protocol = match state.prover_protocol.read() {
        Ok(protocol) => protocol,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChallengeResponseData {
                    success: false,
                    challenge_id: String::new(),
                    nonce: String::new(),
                    quote_b64: String::new(),
                    expires_at: 0,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    error: Some("Failed to acquire prover protocol lock".to_string()),
                }),
            );
        }
    };

    let challenge_response = match prover_protocol.respond_to_challenge(&*enclave, &challenge) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ChallengeResponseData {
                    success: false,
                    challenge_id: String::new(),
                    nonce: String::new(),
                    quote_b64: String::new(),
                    expires_at: 0,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    error: Some(format!("Failed to generate quote: {}", e)),
                }),
            );
        }
    };

    // 序列化 Quote (使用 Quote 结构体的 to_bytes 方法)
    let quote_bytes = challenge_response.quote.to_bytes();

    (
        StatusCode::OK,
        Json(ChallengeResponseData {
            success: true,
            challenge_id: challenge.id.clone(),
            nonce: hex::encode(&challenge.nonce),
            quote_b64: base64::encode(&quote_bytes),
            expires_at: challenge.expires_at,
            mrenclave: hex::encode(&challenge_response.quote.mrenclave()),
            mrsigner: hex::encode(&challenge_response.quote.mrsigner()),
            error: None,
        }),
    )
}

/// 验证挑战响应
///
/// POST /api/v1/attestation/verify-response
///
/// 验证客户端返回的挑战响应，确认客户端拥有正确的密钥
async fn verify_challenge_response(
    State(state): State<Arc<AttestationState>>,
    Json(request): Json<VerifyChallengeResponseRequest>,
) -> impl IntoResponse {
    // 解码 Quote
    let quote_bytes = match base64::decode(&request.quote_b64) {
        Ok(bytes) => bytes,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyChallengeResponseResult {
                    success: false,
                    verified: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some(format!("Invalid base64 quote: {}", e)),
                }),
            );
        }
    };

    // 反序列化 Quote
    let quote = match crate::tee::attestation::Quote::from_bytes(&quote_bytes) {
        Ok(q) => q,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyChallengeResponseResult {
                    success: false,
                    verified: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some(format!("Invalid quote format: {}", e)),
                }),
            );
        }
    };

    // 构建 ChallengeResponse 对象
    let challenge_response = ChallengeResponse::new(request.challenge_id, quote.clone());

    // 获取 Enclave 身份
    let enclave = match state.enclave.read() {
        Ok(enclave) => enclave,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyChallengeResponseResult {
                    success: false,
                    verified: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some("Failed to acquire Enclave lock".to_string()),
                }),
            );
        }
    };

    let enclave_identity = generate_enclave_identity(&*enclave);

    // 验证响应
    let challenge_protocol = match state.challenge_protocol.read() {
        Ok(protocol) => protocol,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyChallengeResponseResult {
                    success: false,
                    verified: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some("Failed to acquire challenge protocol lock".to_string()),
                }),
            );
        }
    };

    match challenge_protocol.verify_response(&challenge_response, &enclave_identity) {
        Ok(result) => (
            StatusCode::OK,
            Json(VerifyChallengeResponseResult {
                success: true,
                verified: result.success,
                mrenclave: hex::encode(&result.mrenclave),
                mrsigner: hex::encode(&result.mrsigner),
                timestamp: result.timestamp,
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(VerifyChallengeResponseResult {
                success: true,
                verified: false,
                mrenclave: String::new(),
                mrsigner: String::new(),
                timestamp: 0,
                error: Some(format!("Verification failed: {}", e)),
            }),
        ),
    }
}

/// 获取认证状态
///
/// GET /api/v1/attestation/status
///
/// 返回当前 Enclave 的认证状态（已认证/待验证/已过期）
async fn get_attestation_status(
    State(state): State<Arc<AttestationState>>,
) -> impl IntoResponse {
    let enclave = match state.enclave.read() {
        Ok(enclave) => enclave,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AttestationStatusResponse {
                    success: false,
                    status: "error".to_string(),
                    enclave_state: "unknown".to_string(),
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    quote_valid: false,
                    quote_expires_at: None,
                    last_verified_at: None,
                    error: Some("Failed to acquire Enclave lock".to_string()),
                }),
            );
        }
    };

    let dcap_service = match state.dcap_service.read() {
        Ok(service) => service,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AttestationStatusResponse {
                    success: false,
                    status: "error".to_string(),
                    enclave_state: "unknown".to_string(),
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    quote_valid: false,
                    quote_expires_at: None,
                    last_verified_at: None,
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                }),
            );
        }
    };

    let enclave_state = enclave.state().to_string();
    let mrenclave = hex::encode(&enclave.mrenclave());
    let mrsigner = hex::encode(&enclave.mrsigner());

    // 确定认证状态
    let (status, quote_valid, quote_expires_at) = if !enclave.is_running() {
        (ApiAttestationStatus::Uninitialized, false, None)
    } else {
        match dcap_service.get_current_quote() {
            Ok(quote) => {
                let now = current_timestamp();
                let age = now.saturating_sub(quote.timestamp);
                let max_age = state.config.quote_max_age;

                if age > max_age {
                    (ApiAttestationStatus::Expired, false, Some(quote.timestamp + max_age))
                } else {
                    (ApiAttestationStatus::Authenticated, true, Some(quote.timestamp + max_age))
                }
            }
            Err(_) => (ApiAttestationStatus::PendingVerification, false, None),
        }
    };

    (
        StatusCode::OK,
        Json(AttestationStatusResponse {
            success: true,
            status: format!("{:?}", status).to_lowercase(),
            enclave_state,
            mrenclave,
            mrsigner,
            quote_valid,
            quote_expires_at,
            last_verified_at: dcap_service.get_current_quote().ok().map(|q| q.timestamp),
            error: None,
        }),
    )
}

/// 刷新 Quote
///
/// POST /api/v1/attestation/refresh
///
/// 生成新的 Quote 并更新缓存
async fn refresh_quote(
    State(state): State<Arc<AttestationState>>,
) -> impl IntoResponse {
    let dcap_service = match state.dcap_service.write() {
        Ok(service) => service,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RefreshResponse {
                    success: false,
                    new_quote_b64: None,
                    timestamp: 0,
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                }),
            );
        }
    };

    let enclave = match state.enclave.read() {
        Ok(enclave) => enclave,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(RefreshResponse {
                    success: false,
                    new_quote_b64: None,
                    timestamp: 0,
                    error: Some("Failed to acquire Enclave lock".to_string()),
                }),
            );
        }
    };

    match dcap_service.refresh_quote(&*enclave) {
        Ok(quote) => {
            match QuoteSerializer::serialize(&quote) {
                Ok(quote_bytes) => (
                    StatusCode::OK,
                    Json(RefreshResponse {
                        success: true,
                        new_quote_b64: Some(base64::encode(&quote_bytes)),
                        timestamp: quote.timestamp,
                        error: None,
                    }),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(RefreshResponse {
                        success: false,
                        new_quote_b64: None,
                        timestamp: 0,
                        error: Some(format!("Failed to serialize quote: {}", e)),
                    }),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RefreshResponse {
                success: false,
                new_quote_b64: None,
                timestamp: 0,
                error: Some(format!("Failed to refresh quote: {}", e)),
            }),
        ),
    }
}

/// 健康检查
///
/// GET /api/v1/attestation/health
///
/// 返回认证服务健康状态
async fn health_check(
    State(state): State<Arc<AttestationState>>,
) -> impl IntoResponse {
    let enclave_state = match state.enclave.read() {
        Ok(enclave) => enclave.state().to_string(),
        Err(_) => "unknown".to_string(),
    };

    let quote_valid = match state.dcap_service.read() {
        Ok(service) => service.get_current_quote().is_ok(),
        Err(_) => false,
    };

    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "healthy".to_string(),
            enclave_state,
            dcap_version: "1.0.0".to_string(),
            quote_valid,
        }),
    )
}

/// 生成 Enclave 身份标识
fn generate_enclave_identity(enclave: &Enclave) -> Vec<u8> {
    let mut identity = Vec::with_capacity(64);
    identity.extend_from_slice(&enclave.mrenclave());
    identity.extend_from_slice(&enclave.mrsigner());
    identity
}

/// 生成随机挑战（用于测试）
fn _generate_challenge() -> Vec<u8> {
    use rand::RngCore;
    let mut challenge = vec![0u8; 32];
    let mut rng = rand::thread_rng();
    rng.fill_bytes(&mut challenge);
    challenge
}

/// 生成挑战 ID（用于测试）
fn _generate_challenge_id(challenge: &[u8]) -> String {
    use ring::digest::{digest, SHA256};
    let hash = digest(&SHA256, challenge);
    format!("chal_{}", hex::encode(&hash.as_ref()[..8]))
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 初始化认证 API
///
/// 创建并初始化认证状态
pub fn init_attestation_api(
    config: AttestationApiConfig,
) -> Result<Arc<AttestationState>, AttestationInitError> {
    // 创建 Enclave
    let enclave_config = EnclaveConfig {
        debug_mode: config.simulation_mode,
        ..Default::default()
    };

    let mut enclave = Enclave::new(enclave_config);
    enclave
        .initialize()
        .map_err(|e| AttestationInitError::EnclaveError(e.to_string()))?;

    // 创建 DCAP 服务
    let dcap_config = DcapConfig {
        simulation_mode: config.simulation_mode,
        pcs_base_url: if config.simulation_mode {
            INTEL_PCS_BASE_URL_TEST.to_string()
        } else {
            INTEL_PCS_BASE_URL_PROD.to_string()
        },
        ..Default::default()
    };

    let dcap_service =
        DcapService::new(dcap_config).map_err(|e| AttestationInitError::DcapError(e.to_string()))?;

    // 初始化 DCAP（生成 Quote）
    dcap_service
        .initialize(&enclave)
        .map_err(|e| AttestationInitError::DcapError(e.to_string()))?;

    // 创建认证服务
    let attestation_service = AttestationService::new()
        .allow_simulation(config.simulation_mode);

    // 创建挑战-响应协议处理器
    let challenge_protocol = ChallengeProtocol::new(attestation_service.clone())
        .with_ttl(config.quote_max_age);

    // 创建 Prover 协议处理器
    let prover_protocol = ProverProtocol::new(attestation_service.clone());

    Ok(Arc::new(AttestationState {
        dcap_service: Arc::new(RwLock::new(dcap_service)),
        enclave: Arc::new(RwLock::new(enclave)),
        config,
        challenge_protocol: Arc::new(RwLock::new(challenge_protocol)),
        prover_protocol: Arc::new(RwLock::new(prover_protocol)),
        attestation_service: Arc::new(RwLock::new(attestation_service)),
    }))
}

/// 认证初始化错误
#[derive(Debug)]
pub enum AttestationInitError {
    EnclaveError(String),
    DcapError(String),
    ConfigurationError(String),
}

impl std::fmt::Display for AttestationInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttestationInitError::EnclaveError(msg) => write!(f, "Enclave error: {}", msg),
            AttestationInitError::DcapError(msg) => write!(f, "DCAP error: {}", msg),
            AttestationInitError::ConfigurationError(msg) => {
                write!(f, "Configuration error: {}", msg)
            }
        }
    }
}

impl std::error::Error for AttestationInitError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_challenge() {
        let challenge1 = _generate_challenge();
        let challenge2 = _generate_challenge();

        assert_eq!(challenge1.len(), 32);
        assert_eq!(challenge2.len(), 32);
        assert_ne!(challenge1, challenge2);
    }

    #[test]
    fn test_generate_challenge_id() {
        let challenge = b"test_challenge_12345";
        let id = _generate_challenge_id(challenge);

        assert!(id.starts_with("chal_"));
        assert_eq!(id.len(), 5 + 16); // "chal_" + 16 hex chars
    }

    #[test]
    fn test_attestation_api_config_default() {
        let config = AttestationApiConfig::default();
        assert!(!config.simulation_mode);
        assert!(!config.require_api_key);
        assert_eq!(config.quote_max_age, 3600);
        assert!(config.enable_pcs_registration);
    }

    #[test]
    fn test_health_response() {
        let response = HealthResponse {
            status: "healthy".to_string(),
            enclave_state: "running".to_string(),
            dcap_version: "1.0.0".to_string(),
            quote_valid: true,
        };

        assert_eq!(response.status, "healthy");
        assert!(response.quote_valid);
    }
}
