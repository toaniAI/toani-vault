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

use crate::config::TeeRuntimeConfig;
use crate::tee::{
    SelfCheckItem, SelfCheckStatus, SharedEnclave, TeeCapabilities,
    challenge::{Challenge, ChallengeMetadata},
    dcap::{DcapConfig, DcapService},
    quote::QuoteSerializer,
    validate_runtime_requirements,
};
use async_trait::async_trait;
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use redis::{AsyncCommands, Client as RedisClient, Script};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock as AsyncRwLock;

use super::response::ApiErrorResponse;

/// 认证状态
pub struct AttestationState {
    /// DCAP 服务
    dcap_service: Arc<RwLock<DcapService>>,

    /// Enclave 实例
    enclave: SharedEnclave,

    /// 服务配置
    config: AttestationApiConfig,

    /// 运行时能力探测结果
    tee_capabilities: TeeCapabilities,

    /// 待验证 challenge 存储。默认要求 Redis 持久化，多实例共享。
    challenge_store: Arc<dyn ChallengeStore>,

    /// 最近一次 quote/challenge 校验成功时间。
    last_verified_at: Arc<RwLock<Option<u64>>>,
}

impl std::fmt::Debug for AttestationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttestationState")
            .field("config", &self.config)
            .field("dcap_service", &"<DcapService>")
            .field("enclave", &"<Enclave>")
            .field("challenge_store", &"<dyn ChallengeStore>")
            .field("last_verified_at", &"<Option<u64>>")
            .finish()
    }
}

impl Clone for AttestationState {
    fn clone(&self) -> Self {
        Self {
            dcap_service: Arc::clone(&self.dcap_service),
            enclave: Arc::clone(&self.enclave),
            config: self.config.clone(),
            tee_capabilities: self.tee_capabilities.clone(),
            challenge_store: Arc::clone(&self.challenge_store),
            last_verified_at: Arc::clone(&self.last_verified_at),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum ChallengeStoreError {
    #[error("Redis 连接错误: {0}")]
    RedisConnection(String),

    #[error("Redis 操作错误: {0}")]
    RedisOperation(String),

    #[error("序列化错误: {0}")]
    Serialization(String),
}

#[async_trait]
trait ChallengeStore: Send + Sync {
    async fn put(&self, challenge: &Challenge) -> Result<(), ChallengeStoreError>;
    async fn take(&self, challenge_id: &str) -> Result<Option<Challenge>, ChallengeStoreError>;
}

#[derive(Debug, Default)]
struct InMemoryChallengeStore {
    challenges: Arc<AsyncRwLock<HashMap<String, Challenge>>>,
}

#[async_trait]
impl ChallengeStore for InMemoryChallengeStore {
    async fn put(&self, challenge: &Challenge) -> Result<(), ChallengeStoreError> {
        let mut challenges = self.challenges.write().await;
        challenges.retain(|_, existing| !existing.is_expired());
        challenges.insert(challenge.id.clone(), challenge.clone());
        Ok(())
    }

    async fn take(&self, challenge_id: &str) -> Result<Option<Challenge>, ChallengeStoreError> {
        let mut challenges = self.challenges.write().await;
        challenges.retain(|_, existing| !existing.is_expired());
        Ok(challenges.remove(challenge_id))
    }
}

#[derive(Debug)]
struct RedisChallengeStore {
    client: RedisClient,
    key_prefix: String,
}

impl RedisChallengeStore {
    fn new(redis_url: &str) -> Result<Self, ChallengeStoreError> {
        let client = RedisClient::open(redis_url)
            .map_err(|error| ChallengeStoreError::RedisConnection(error.to_string()))?;
        Ok(Self {
            client,
            key_prefix: "credbridge:attestation:challenge:".to_string(),
        })
    }

    async fn health_check(&self) -> Result<(), ChallengeStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| ChallengeStoreError::RedisConnection(error.to_string()))?;

        redis::cmd("PING")
            .query_async::<_, String>(&mut conn)
            .await
            .map_err(|error| ChallengeStoreError::RedisOperation(error.to_string()))?;

        Ok(())
    }

    fn build_key(&self, challenge_id: &str) -> String {
        format!("{}{}", self.key_prefix, challenge_id)
    }
}

#[async_trait]
impl ChallengeStore for RedisChallengeStore {
    async fn put(&self, challenge: &Challenge) -> Result<(), ChallengeStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| ChallengeStoreError::RedisConnection(error.to_string()))?;

        let ttl_seconds = challenge
            .expires_at
            .saturating_sub(current_timestamp())
            .max(1);
        let payload = serde_json::to_string(challenge)
            .map_err(|error| ChallengeStoreError::Serialization(error.to_string()))?;

        conn.set_ex::<_, _, ()>(self.build_key(&challenge.id), payload, ttl_seconds)
            .await
            .map_err(|error| ChallengeStoreError::RedisOperation(error.to_string()))?;
        Ok(())
    }

    async fn take(&self, challenge_id: &str) -> Result<Option<Challenge>, ChallengeStoreError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|error| ChallengeStoreError::RedisConnection(error.to_string()))?;

        let script = Script::new(
            r#"
local value = redis.call("GET", KEYS[1])
if value then
  redis.call("DEL", KEYS[1])
end
return value
"#,
        );

        let payload: Option<String> = script
            .key(self.build_key(challenge_id))
            .invoke_async(&mut conn)
            .await
            .map_err(|error| ChallengeStoreError::RedisOperation(error.to_string()))?;

        payload
            .map(|value| {
                serde_json::from_str::<Challenge>(&value)
                    .map_err(|error| ChallengeStoreError::Serialization(error.to_string()))
            })
            .transpose()
    }
}

impl AttestationState {
    fn record_successful_verification(&self, timestamp: u64) {
        if let Ok(mut last_verified_at) = self.last_verified_at.write() {
            *last_verified_at = Some(timestamp);
        }
    }

    async fn generate_challenge(
        &self,
        enclave_id: Option<String>,
    ) -> Result<Challenge, &'static str> {
        let challenge = Challenge::with_metadata(
            format!("chal_{}", uuid::Uuid::new_v4().simple()),
            self.config.quote_max_age,
            ChallengeMetadata {
                client_ip: None,
                user_agent: None,
                request_id: None,
                extra: HashMap::new(),
            },
        )
        .map_err(|_| "Failed to construct attestation challenge")?;

        let mut challenge = challenge;
        challenge.enclave_id = enclave_id;
        self.challenge_store
            .put(&challenge)
            .await
            .map_err(|_| "Failed to persist attestation challenge")?;
        Ok(challenge)
    }

    async fn take_challenge(&self, challenge_id: &str) -> Result<Option<Challenge>, &'static str> {
        self.challenge_store
            .take(challenge_id)
            .await
            .map_err(|_| "Failed to load attestation challenge")
    }

    pub async fn runtime_snapshot(&self) -> AttestationRuntimeSnapshot {
        let requested_mode = self.config.tee_runtime.mode.to_string();
        let effective_mode = self.config.tee_runtime.mode.to_string();
        let last_verified_at = self.last_verified_at.read().ok().and_then(|value| *value);

        let enclave = self.enclave.lock().await;
        let enclave_running = enclave.is_running();
        let enclave_state = enclave.state().to_string();
        let mrenclave = if enclave_running {
            hex::encode(enclave.mrenclave())
        } else {
            String::new()
        };
        let mrsigner = if enclave_running {
            hex::encode(enclave.mrsigner())
        } else {
            String::new()
        };

        if !enclave_running {
            return AttestationRuntimeSnapshot {
                status: ApiAttestationStatus::Uninitialized,
                health_status: SelfCheckStatus::Failed,
                requested_mode,
                effective_mode,
                root_key_source: self.config.root_key_source.clone(),
                detected_type: self
                    .tee_capabilities
                    .detected_type
                    .description()
                    .to_string(),
                hardware_available: self.tee_capabilities.hardware_available,
                remote_attestation_available: self.tee_capabilities.remote_attestation_available,
                enclave_state,
                enclave_running,
                mrenclave,
                mrsigner,
                quote_valid: false,
                quote_expires_at: None,
                last_quote_generated_at: None,
                last_verified_at,
                error: Some("Enclave not running".to_string()),
            };
        }

        let dcap_service = match self.dcap_service.read() {
            Ok(service) => service,
            Err(_) => {
                return AttestationRuntimeSnapshot {
                    status: ApiAttestationStatus::Failed,
                    health_status: SelfCheckStatus::Failed,
                    requested_mode,
                    effective_mode,
                    root_key_source: self.config.root_key_source.clone(),
                    detected_type: self
                        .tee_capabilities
                        .detected_type
                        .description()
                        .to_string(),
                    hardware_available: self.tee_capabilities.hardware_available,
                    remote_attestation_available: self
                        .tee_capabilities
                        .remote_attestation_available,
                    enclave_state,
                    enclave_running,
                    mrenclave,
                    mrsigner,
                    quote_valid: false,
                    quote_expires_at: None,
                    last_quote_generated_at: None,
                    last_verified_at,
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                };
            }
        };

        match dcap_service.get_current_quote() {
            Ok(quote) => {
                let quote_expires_at = quote.timestamp.checked_add(self.config.quote_max_age);
                let quote_valid =
                    quote_expires_at.is_some_and(|expires_at| current_timestamp() <= expires_at);

                let (status, health_status, error) = if quote_valid {
                    (
                        ApiAttestationStatus::Authenticated,
                        SelfCheckStatus::Ready,
                        None,
                    )
                } else {
                    (
                        ApiAttestationStatus::Expired,
                        SelfCheckStatus::Degraded,
                        Some("Cached quote expired".to_string()),
                    )
                };

                AttestationRuntimeSnapshot {
                    status,
                    health_status,
                    requested_mode,
                    effective_mode,
                    root_key_source: self.config.root_key_source.clone(),
                    detected_type: self
                        .tee_capabilities
                        .detected_type
                        .description()
                        .to_string(),
                    hardware_available: self.tee_capabilities.hardware_available,
                    remote_attestation_available: self
                        .tee_capabilities
                        .remote_attestation_available,
                    enclave_state,
                    enclave_running,
                    mrenclave,
                    mrsigner,
                    quote_valid,
                    quote_expires_at,
                    last_quote_generated_at: Some(quote.timestamp),
                    last_verified_at,
                    error,
                }
            }
            Err(error) => {
                let (status, health_status) = if self.config.tee_runtime.is_hardware() {
                    (ApiAttestationStatus::Failed, SelfCheckStatus::Failed)
                } else {
                    (
                        ApiAttestationStatus::PendingVerification,
                        SelfCheckStatus::Degraded,
                    )
                };

                AttestationRuntimeSnapshot {
                    status,
                    health_status,
                    requested_mode,
                    effective_mode,
                    root_key_source: self.config.root_key_source.clone(),
                    detected_type: self
                        .tee_capabilities
                        .detected_type
                        .description()
                        .to_string(),
                    hardware_available: self.tee_capabilities.hardware_available,
                    remote_attestation_available: self
                        .tee_capabilities
                        .remote_attestation_available,
                    enclave_state,
                    enclave_running,
                    mrenclave,
                    mrsigner,
                    quote_valid: false,
                    quote_expires_at: None,
                    last_quote_generated_at: None,
                    last_verified_at,
                    error: Some(format!("No valid quote available: {error}")),
                }
            }
        }
    }
}

/// API 配置
#[derive(Debug, Clone)]
pub struct AttestationApiConfig {
    /// TEE 运行时配置
    pub tee_runtime: TeeRuntimeConfig,

    /// 当前进程实际使用的 L0 根密钥来源
    pub root_key_source: String,

    /// 是否需要 API 密钥
    pub require_api_key: bool,

    /// Quote 最大有效期（秒）
    pub quote_max_age: u64,

    /// 是否启用 PCS 注册
    pub enable_pcs_registration: bool,

    /// 仅测试/显式 fallback 使用内存 challenge store。
    pub allow_memory_challenge_store: bool,
}

impl Default for AttestationApiConfig {
    fn default() -> Self {
        Self {
            tee_runtime: TeeRuntimeConfig::hardware(),
            root_key_source: "unknown".to_string(),
            require_api_key: false,
            quote_max_age: 3600,
            enable_pcs_registration: true,
            allow_memory_challenge_store: false,
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
    pub health_status: String,
    pub ready: bool,
    pub requested_mode: String,
    pub effective_mode: String,
    pub root_key_source: String,
    pub detected_type: String,
    pub hardware_available: bool,
    pub remote_attestation_available: bool,
    pub enclave_state: String,
    pub mrenclave: String,
    pub mrsigner: String,
    pub quote_valid: bool,
    pub quote_expires_at: Option<u64>,
    pub last_quote_generated_at: Option<u64>,
    pub last_verified_at: Option<u64>,
    pub error: Option<String>,
}

/// 认证状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiAttestationStatus {
    /// 已认证
    Authenticated,
    /// 待验证
    PendingVerification,
    /// 已过期
    Expired,
    /// 运行期失败
    Failed,
    /// 未初始化
    Uninitialized,
}

impl ApiAttestationStatus {
    fn as_str(self) -> &'static str {
        match self {
            ApiAttestationStatus::Authenticated => "authenticated",
            ApiAttestationStatus::PendingVerification => "pending_verification",
            ApiAttestationStatus::Expired => "expired",
            ApiAttestationStatus::Failed => "failed",
            ApiAttestationStatus::Uninitialized => "uninitialized",
        }
    }
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
    pub ready: bool,
    pub requested_mode: String,
    pub effective_mode: String,
    pub root_key_source: String,
    pub detected_type: String,
    pub enclave_state: String,
    pub dcap_version: String,
    pub quote_valid: bool,
    pub quote_expires_at: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttestationRuntimeSnapshot {
    pub status: ApiAttestationStatus,
    pub health_status: SelfCheckStatus,
    pub requested_mode: String,
    pub effective_mode: String,
    pub root_key_source: String,
    pub detected_type: String,
    pub hardware_available: bool,
    pub remote_attestation_available: bool,
    pub enclave_state: String,
    pub enclave_running: bool,
    pub mrenclave: String,
    pub mrsigner: String,
    pub quote_valid: bool,
    pub quote_expires_at: Option<u64>,
    pub last_quote_generated_at: Option<u64>,
    pub last_verified_at: Option<u64>,
    pub error: Option<String>,
}

impl AttestationRuntimeSnapshot {
    pub fn ready(&self) -> bool {
        self.health_status.is_ready()
    }

    pub fn health_label(&self) -> &'static str {
        self.health_status.as_str()
    }

    pub fn as_readiness_check(&self) -> SelfCheckItem {
        match self.health_status {
            SelfCheckStatus::Ready => SelfCheckItem::ready("attestation"),
            SelfCheckStatus::Degraded => SelfCheckItem::degraded(
                "attestation",
                self.error
                    .clone()
                    .unwrap_or_else(|| "attestation degraded".to_string()),
            ),
            SelfCheckStatus::Failed => SelfCheckItem::failed(
                "attestation",
                self.error
                    .clone()
                    .unwrap_or_else(|| "attestation failed".to_string()),
            ),
        }
    }
}

/// 获取当前 Quote
///
/// GET /api/v1/attestation/quote
///
/// 返回当前的 DCAP Quote，包含 MRENCLAVE 和 MRSIGNER
async fn get_quote(State(state): State<Arc<AttestationState>>) -> impl IntoResponse {
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
        Ok(quote) => match QuoteSerializer::serialize(&quote) {
            Ok(quote_bytes) => {
                let quote_b64 = STANDARD.encode(&quote_bytes);
                (
                    StatusCode::OK,
                    Json(QuoteResponse {
                        success: true,
                        data: Some(QuoteData {
                            version: quote.version,
                            sign_type: quote.sign_type,
                            mrenclave: hex::encode(quote.report_body.mrenclave),
                            mrsigner: hex::encode(quote.report_body.mrsigner),
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
                    error: Some(format!("Failed to serialize quote: {e}")),
                }),
            ),
        },
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(QuoteResponse {
                success: false,
                data: None,
                error: Some(format!("No valid quote available: {e}")),
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
    let quote_bytes = match STANDARD.decode(&request.quote_b64) {
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
                    error: Some(format!("Invalid base64 quote: {e}")),
                }),
            );
        }
    };

    // 验证 nonce（如果提供）
    let nonce = request.nonce.as_ref().and_then(|n| STANDARD.decode(n).ok());

    // 执行验证
    match dcap_service.verify_attestation(&quote_bytes, nonce.as_deref()) {
        Ok(report) => {
            state.record_successful_verification(report.timestamp);
            (
                StatusCode::OK,
                Json(VerifyResponse {
                    success: true,
                    valid: true,
                    mrenclave: report.mrenclave_hex,
                    mrsigner: report.mrsigner_hex,
                    timestamp: report.timestamp,
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::OK,
            Json(VerifyResponse {
                success: true,
                valid: false,
                mrenclave: String::new(),
                mrsigner: String::new(),
                timestamp: 0,
                error: Some(format!("Verification failed: {e}")),
            }),
        ),
    }
}

/// 获取认证报告
///
/// GET /api/v1/attestation/report
///
/// 返回详细的认证报告
async fn get_report(State(state): State<Arc<AttestationState>>) -> impl IntoResponse {
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
                error: Some(format!("Failed to get report: {e}")),
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
    let enclave = state.enclave.lock().await;
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

    let challenge = match state.generate_challenge(request.enclave_id.clone()).await {
        Ok(challenge) => challenge,
        Err(error) => {
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
                    error: Some(error.to_string()),
                }),
            );
        }
    };

    let dcap_service = match state.dcap_service.read() {
        Ok(service) => service,
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
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                }),
            );
        }
    };

    let quote = match dcap_service.generate_quote_for_challenge(&enclave, &challenge.nonce) {
        Ok(quote) => quote,
        Err(e) => {
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
                    error: Some(format!("Failed to generate quote: {e}")),
                }),
            );
        }
    };

    let quote_bytes = match QuoteSerializer::serialize(&quote) {
        Ok(bytes) => bytes,
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
                    error: Some(format!("Failed to serialize quote: {e}")),
                }),
            );
        }
    };

    (
        StatusCode::OK,
        Json(ChallengeResponseData {
            success: true,
            challenge_id: challenge.id.clone(),
            nonce: hex::encode(challenge.nonce),
            quote_b64: STANDARD.encode(&quote_bytes),
            expires_at: challenge.expires_at,
            mrenclave: hex::encode(quote.report_body.mrenclave),
            mrsigner: hex::encode(quote.report_body.mrsigner),
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
) -> Response {
    let quote_bytes = match STANDARD.decode(&request.quote_b64) {
        Ok(bytes) => bytes,
        Err(e) => {
            // BUG-18354: 统一非法 payload 响应格式为 {"error":"invalid_request"}
            return ApiErrorResponse::invalid_request(format!("Invalid base64 quote: {e}"))
                .into_response();
        }
    };

    let challenge = match state.take_challenge(&request.challenge_id).await {
        Ok(Some(challenge)) => challenge,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(VerifyChallengeResponseResult {
                    success: true,
                    verified: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some("Verification failed: Challenge not found or expired".to_string()),
                }),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyChallengeResponseResult {
                    success: false,
                    verified: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some(error.to_string()),
                }),
            )
                .into_response();
        }
    };

    if challenge.is_expired() {
        return (
            StatusCode::OK,
            Json(VerifyChallengeResponseResult {
                success: true,
                verified: false,
                mrenclave: String::new(),
                mrsigner: String::new(),
                timestamp: 0,
                error: Some("Verification failed: Challenge expired".to_string()),
            }),
        )
            .into_response();
    }

    let dcap_service = match state.dcap_service.read() {
        Ok(service) => service,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyChallengeResponseResult {
                    success: false,
                    verified: false,
                    mrenclave: String::new(),
                    mrsigner: String::new(),
                    timestamp: 0,
                    error: Some("Failed to acquire DCAP service lock".to_string()),
                }),
            )
                .into_response();
        }
    };

    match dcap_service.verify_attestation(&quote_bytes, Some(&challenge.nonce)) {
        Ok(result) => {
            state.record_successful_verification(result.timestamp);
            (
                StatusCode::OK,
                Json(VerifyChallengeResponseResult {
                    success: true,
                    verified: result.result.success,
                    mrenclave: result.mrenclave_hex,
                    mrsigner: result.mrsigner_hex,
                    timestamp: result.timestamp,
                    error: None,
                }),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::OK,
            Json(VerifyChallengeResponseResult {
                success: true,
                verified: false,
                mrenclave: String::new(),
                mrsigner: String::new(),
                timestamp: 0,
                error: Some(format!("Verification failed: {e}")),
            }),
        )
            .into_response(),
    }
}

/// 获取认证状态
///
/// GET /api/v1/attestation/status
///
/// 返回当前 Enclave 的认证状态（已认证/待验证/已过期）
async fn get_attestation_status(State(state): State<Arc<AttestationState>>) -> impl IntoResponse {
    let snapshot = state.runtime_snapshot().await;

    (
        StatusCode::OK,
        Json(AttestationStatusResponse {
            success: true,
            status: snapshot.status.as_str().to_string(),
            health_status: snapshot.health_label().to_string(),
            ready: snapshot.ready(),
            requested_mode: snapshot.requested_mode,
            effective_mode: snapshot.effective_mode,
            root_key_source: snapshot.root_key_source,
            detected_type: snapshot.detected_type,
            hardware_available: snapshot.hardware_available,
            remote_attestation_available: snapshot.remote_attestation_available,
            enclave_state: snapshot.enclave_state,
            mrenclave: snapshot.mrenclave,
            mrsigner: snapshot.mrsigner,
            quote_valid: snapshot.quote_valid,
            quote_expires_at: snapshot.quote_expires_at,
            last_quote_generated_at: snapshot.last_quote_generated_at,
            last_verified_at: snapshot.last_verified_at,
            error: snapshot.error,
        }),
    )
}

/// 刷新 Quote
///
/// POST /api/v1/attestation/refresh
///
/// 生成新的 Quote 并更新缓存
async fn refresh_quote(State(state): State<Arc<AttestationState>>) -> impl IntoResponse {
    let enclave = state.enclave.lock().await;

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

    match dcap_service.refresh_quote(&enclave) {
        Ok(quote) => match QuoteSerializer::serialize(&quote) {
            Ok(quote_bytes) => (
                StatusCode::OK,
                Json(RefreshResponse {
                    success: true,
                    new_quote_b64: Some(STANDARD.encode(&quote_bytes)),
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
                    error: Some(format!("Failed to serialize quote: {e}")),
                }),
            ),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RefreshResponse {
                success: false,
                new_quote_b64: None,
                timestamp: 0,
                error: Some(format!("Failed to refresh quote: {e}")),
            }),
        ),
    }
}

/// 健康检查
///
/// GET /api/v1/attestation/health
///
/// 返回认证服务健康状态
async fn health_check(State(state): State<Arc<AttestationState>>) -> impl IntoResponse {
    let snapshot = state.runtime_snapshot().await;
    let status_code = if snapshot.ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(HealthResponse {
            status: snapshot.health_label().to_string(),
            ready: snapshot.ready(),
            requested_mode: snapshot.requested_mode,
            effective_mode: snapshot.effective_mode,
            root_key_source: snapshot.root_key_source,
            detected_type: snapshot.detected_type,
            enclave_state: snapshot.enclave_state,
            dcap_version: "1.0.0".to_string(),
            quote_valid: snapshot.quote_valid,
            quote_expires_at: snapshot.quote_expires_at,
            error: snapshot.error,
        }),
    )
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
    use ring::digest::{SHA256, digest};
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
pub async fn init_attestation_api(
    mut config: AttestationApiConfig,
    enclave: SharedEnclave,
    tee_capabilities: Option<TeeCapabilities>,
) -> Result<Arc<AttestationState>, AttestationInitError> {
    if config.root_key_source.is_empty() {
        config.root_key_source = if config.tee_runtime.is_simulation() {
            "simulation".to_string()
        } else {
            "unknown".to_string()
        };
    }

    if config.tee_runtime.is_hardware() && config.root_key_source == "simulation" {
        return Err(AttestationInitError::ConfigurationError(
            "TEE_MODE=hardware requested, but attestation API was configured with a simulation root key source".to_string(),
        ));
    }

    let tee_capabilities = tee_capabilities.map(Ok).unwrap_or_else(|| {
        validate_runtime_requirements(&config.tee_runtime)
            .map_err(|e| AttestationInitError::ConfigurationError(e.to_string()))
    })?;

    // 创建 DCAP 服务
    let dcap_config = DcapConfig::from_runtime(&config.tee_runtime);

    let dcap_service = DcapService::new(dcap_config)
        .map_err(|e| AttestationInitError::DcapError(e.to_string()))?;

    let challenge_store: Arc<dyn ChallengeStore> = if config.allow_memory_challenge_store {
        Arc::new(InMemoryChallengeStore::default())
    } else {
        let redis_url = std::env::var("REDIS_URL").map_err(|_| {
            AttestationInitError::ConfigurationError(
                "attestation challenge state 要求 REDIS_URL，内存回退已禁用".to_string(),
            )
        })?;
        let store = RedisChallengeStore::new(&redis_url)
            .map_err(|error| AttestationInitError::ConfigurationError(error.to_string()))?;
        store
            .health_check()
            .await
            .map_err(|error| AttestationInitError::ConfigurationError(error.to_string()))?;
        Arc::new(store)
    };

    // 初始化 DCAP（生成 Quote）
    {
        let enclave_guard = enclave.lock().await;
        dcap_service
            .initialize(&enclave_guard)
            .map_err(|e| AttestationInitError::DcapError(e.to_string()))?;
    }

    Ok(Arc::new(AttestationState {
        dcap_service: Arc::new(RwLock::new(dcap_service)),
        enclave,
        config,
        tee_capabilities,
        challenge_store,
        last_verified_at: Arc::new(RwLock::new(None)),
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
            AttestationInitError::EnclaveError(msg) => write!(f, "Enclave error: {msg}"),
            AttestationInitError::DcapError(msg) => write!(f, "DCAP error: {msg}"),
            AttestationInitError::ConfigurationError(msg) => {
                write!(f, "Configuration error: {msg}")
            }
        }
    }
}

impl std::error::Error for AttestationInitError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TeeRuntimeMode;
    use crate::tee::{Enclave, EnclaveConfig};
    use tokio::sync::Mutex;

    async fn make_initialized_enclave() -> SharedEnclave {
        let mut enclave = Enclave::new(EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        });
        enclave.initialize().unwrap();
        Arc::new(Mutex::new(enclave))
    }

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
        assert_eq!(config.tee_runtime.mode, TeeRuntimeMode::Hardware);
        assert!(!config.require_api_key);
        assert_eq!(config.quote_max_age, 3600);
        assert!(config.enable_pcs_registration);
    }

    #[test]
    fn test_health_response() {
        let response = HealthResponse {
            status: "ready".to_string(),
            ready: true,
            requested_mode: "simulation".to_string(),
            effective_mode: "simulation".to_string(),
            root_key_source: "simulation".to_string(),
            detected_type: "Software Simulation".to_string(),
            enclave_state: "running".to_string(),
            dcap_version: "1.0.0".to_string(),
            quote_valid: true,
            quote_expires_at: Some(42),
            error: None,
        };

        assert_eq!(response.status, "ready");
        assert!(response.ready);
        assert!(response.quote_valid);
    }

    #[tokio::test]
    async fn test_init_attestation_api_hardware_mode_fails_closed() {
        let enclave = make_initialized_enclave().await;
        let error = init_attestation_api(AttestationApiConfig::default(), enclave, None)
            .await
            .unwrap_err();
        let message = error.to_string();

        assert!(
            message.contains("TEE_MODE=hardware")
                || message.contains("real SGX DCAP quote generation"),
            "unexpected error: {message}"
        );
    }

    #[tokio::test]
    async fn test_init_attestation_api_reuses_shared_enclave_state() {
        let enclave = make_initialized_enclave().await;
        let state = init_attestation_api(
            AttestationApiConfig {
                tee_runtime: TeeRuntimeConfig::simulation(),
                root_key_source: "simulation".to_string(),
                allow_memory_challenge_store: true,
                ..Default::default()
            },
            enclave.clone(),
            Some(validate_runtime_requirements(&TeeRuntimeConfig::simulation()).unwrap()),
        )
        .await
        .unwrap();

        {
            let mut shared = enclave.lock().await;
            shared.shutdown().unwrap();
        }

        let snapshot = state.runtime_snapshot().await;
        assert_eq!(snapshot.status, ApiAttestationStatus::Uninitialized);
        assert_eq!(snapshot.health_status, SelfCheckStatus::Failed);
        assert_eq!(snapshot.enclave_state, "shutdown");
        assert!(!snapshot.quote_valid);
    }
}
