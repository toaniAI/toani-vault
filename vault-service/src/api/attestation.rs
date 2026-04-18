//! TEE 远程认证 API 模块
//!
//! 提供可信执行环境（TEE）远程认证端点，包括：
//! - Quote 生成与获取
//! - 认证状态查询
//!
//! 支持 SGX、TDX 等 TEE 类型的远程认证

use crate::config::TeeRuntimeConfig;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Attestation API 配置
#[derive(Debug, Clone)]
pub struct AttestationApiConfig {
    /// TEE 运行时配置
    pub tee_runtime: TeeRuntimeConfig,
    /// 是否需要 API Key 认证
    pub require_api_key: bool,
    /// Quote 最大有效期（秒）
    pub quote_max_age: u64,
}

impl Default for AttestationApiConfig {
    fn default() -> Self {
        Self {
            tee_runtime: TeeRuntimeConfig::from_env()
                .unwrap_or_else(|_| TeeRuntimeConfig::hardware()),
            require_api_key: false,
            quote_max_age: 3600, // 1 小时
        }
    }
}

/// Attestation 状态共享结构
#[derive(Debug)]
pub struct AttestationState {
    /// 配置
    pub config: AttestationApiConfig,
    /// TEE 类型
    pub tee_type: TeeType,
    /// Enclave 是否已初始化
    pub initialized: bool,
    /// 最后认证时间
    pub last_attestation_time: Option<u64>,
    /// MRENCLAVE 值（SGX）
    pub mrenclave: Option<String>,
    /// MRSIGNER 值（SGX）
    pub mrsigner: Option<String>,
    /// 当前实现是否对请求的硬件模式执行显式 fail-closed
    pub fail_closed: bool,
    /// fail-closed 的显式原因
    pub fail_closed_reason: Option<String>,
}

/// TEE 类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TeeType {
    /// Intel SGX
    Sgx,
    /// Intel TDX
    Tdx,
    /// AMD SEV-SNP
    SevSnp,
    /// 模拟环境（非 TEE）
    Simulation,
    /// 未知类型
    Unknown,
}

impl std::fmt::Display for TeeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeeType::Sgx => write!(f, "sgx"),
            TeeType::Tdx => write!(f, "tdx"),
            TeeType::SevSnp => write!(f, "sev-snp"),
            TeeType::Simulation => write!(f, "simulation"),
            TeeType::Unknown => write!(f, "unknown"),
        }
    }
}

impl AttestationState {
    /// 创建新的 Attestation 状态
    pub fn new(config: AttestationApiConfig) -> Self {
        let (tee_type, fail_closed, fail_closed_reason) = if config.tee_runtime.is_simulation() {
            (TeeType::Simulation, false, None)
        } else {
            (
                TeeType::Unknown,
                true,
                Some(
                    "real hardware attestation initialization is not implemented in vault-service yet; refusing simulated fallback"
                        .to_string(),
                ),
            )
        };

        Self {
            config,
            tee_type,
            initialized: false,
            last_attestation_time: None,
            mrenclave: None,
            mrsigner: None,
            fail_closed,
            fail_closed_reason,
        }
    }

    /// 设置 TEE 类型
    pub fn with_tee_type(mut self, tee_type: TeeType) -> Self {
        self.tee_type = tee_type;
        self
    }

    /// 设置初始化状态
    pub fn with_initialized(mut self, initialized: bool) -> Self {
        self.initialized = initialized;
        self
    }

    /// 设置 MRENCLAVE 值
    pub fn with_mrenclave(mut self, mrenclave: String) -> Self {
        self.mrenclave = Some(mrenclave);
        self
    }

    /// 设置 MRSIGNER 值
    pub fn with_mrsigner(mut self, mrsigner: String) -> Self {
        self.mrsigner = Some(mrsigner);
        self
    }
}

/// TEE Quote 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteResponse {
    /// Quote 数据（Base64 编码）
    pub quote: String,
    /// Quote 版本
    pub version: u32,
    /// TEE 类型
    pub tee_type: String,
    /// 生成时间戳（Unix 秒）
    pub timestamp: u64,
    /// 过期时间戳（Unix 秒）
    pub expires_at: u64,
    /// Quote 状态
    pub status: QuoteStatus,
}

/// Quote 状态枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum QuoteStatus {
    /// 有效
    Valid,
    /// 已过期
    Expired,
    /// 模拟模式
    Simulated,
    /// 错误
    Error,
}

/// Attestation 状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationStatusResponse {
    /// TEE 类型
    pub tee_type: String,
    /// Enclave 是否已初始化
    pub initialized: bool,
    /// 当前认证状态
    pub attestation_status: AttestationStatus,
    /// 最后认证时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attestation_time: Option<u64>,
    /// MRENCLAVE（如果是 SGX）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mrenclave: Option<String>,
    /// MRSIGNER（如果是 SGX）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mrsigner: Option<String>,
    /// 请求的运行模式
    pub requested_mode: String,
    /// 实际生效的运行模式
    pub effective_mode: String,
    /// 服务版本
    pub version: String,
    /// 当前实现是否显式 fail-closed
    pub fail_closed: bool,
    /// fail-closed 的原因
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_closed_reason: Option<String>,
}

/// 认证状态枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AttestationStatus {
    /// 未认证
    NotAttested,
    /// 认证中
    Attesting,
    /// 已认证
    Attested,
    /// 认证失败
    Failed,
    /// 模拟模式
    Simulated,
}

/// 初始化 Attestation API
pub fn init_attestation_api(config: AttestationApiConfig) -> Result<Arc<AttestationState>, String> {
    if config.tee_runtime.is_hardware() {
        return Err(
            "TEE_MODE=hardware requested, but real hardware attestation initialization is not implemented in vault-service yet; refusing simulated fallback".to_string(),
        );
    }

    let state = AttestationState::new(config);

    // 在实际实现中，这里应该初始化 TEE 相关资源
    // 例如加载 Enclave、验证 TDX 环境等

    Ok(Arc::new(state))
}

/// 构建 Attestation 路由
pub fn attestation_routes() -> Router<Arc<AttestationState>> {
    Router::new()
        .route("/quote", get(get_quote))
        .route("/status", get(get_status))
}

/// GET /api/v1/attestation/quote - 获取 TEE Quote
pub async fn get_quote(State(state): State<Arc<AttestationState>>) -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let expires_at = timestamp + state.config.quote_max_age;

    // simulation 模式：返回模拟 Quote
    if state.config.tee_runtime.is_simulation() {
        let response = QuoteResponse {
            quote: base64_encode_simulated_quote(),
            version: 1,
            tee_type: state.tee_type.to_string(),
            timestamp,
            expires_at,
            status: QuoteStatus::Simulated,
        };
        return (StatusCode::OK, Json(response));
    }

    // 实际 TEE 环境：生成真实 Quote
    match generate_real_quote(&state) {
        Ok(quote) => {
            let response = QuoteResponse {
                quote,
                version: 1,
                tee_type: state.tee_type.to_string(),
                timestamp,
                expires_at,
                status: QuoteStatus::Valid,
            };
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            let response = QuoteResponse {
                quote: String::new(),
                version: 0,
                tee_type: state.tee_type.to_string(),
                timestamp,
                expires_at: 0,
                status: QuoteStatus::Error,
            };
            tracing::error!("Failed to generate quote: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(response))
        }
    }
}

/// GET /api/v1/attestation/status - 获取认证状态
pub async fn get_status(State(state): State<Arc<AttestationState>>) -> impl IntoResponse {
    let attestation_status = if state.config.tee_runtime.is_simulation() {
        AttestationStatus::Simulated
    } else if !state.initialized {
        AttestationStatus::NotAttested
    } else if state.last_attestation_time.is_some() {
        AttestationStatus::Attested
    } else {
        AttestationStatus::NotAttested
    };

    let response = AttestationStatusResponse {
        tee_type: state.tee_type.to_string(),
        initialized: state.initialized,
        attestation_status,
        last_attestation_time: state.last_attestation_time,
        mrenclave: state.mrenclave.clone(),
        mrsigner: state.mrsigner.clone(),
        requested_mode: state.config.tee_runtime.mode.to_string(),
        effective_mode: state.config.tee_runtime.mode.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        fail_closed: state.fail_closed,
        fail_closed_reason: state.fail_closed_reason.clone(),
    };

    (StatusCode::OK, Json(response))
}

/// 生成模拟 Quote（Base64 编码）
fn base64_encode_simulated_quote() -> String {
    // 模拟 Quote 数据
    // 在实际实现中，这应该是真实的 SGX/TDX Quote
    let simulated_data = format!(
        "SIMULATED_QUOTE_{}_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        uuid::Uuid::new_v4()
    );

    // Base64 编码
    use base64::{Engine as _, engine::general_purpose};
    general_purpose::STANDARD.encode(simulated_data.as_bytes())
}

/// 生成真实 Quote（TEE 硬件环境）
///
/// 此函数在真实 TEE 硬件上调用平台特定 SDK 生成 Quote。
/// 各 TEE 类型的 SDK 依赖通过 feature flags 隔离：
/// - `sgx`: 需要 Intel SGX DCAP 库（`sgx_urts`, `sgx_dcap_ql`）
/// - `tdx`: 需要 Intel TDX attestation 库（`tdx-attest`）
/// - `sev-snp`: 需要 AMD SEV-SNP 固件接口（`/dev/sev-guest`）
///
/// 在无硬件 TEE 的开发环境中，请将 `TEE_MODE=simulation`
/// 以获取格式正确的模拟 Quote（用于集成测试）。
fn generate_real_quote(state: &AttestationState) -> Result<String, String> {
    if !state.initialized {
        return Err(
            "TEE enclave is not initialized. Call init_attestation_api() with a valid TEE \
             configuration before requesting quotes."
                .to_string(),
        );
    }

    match state.tee_type {
        TeeType::Sgx => {
            // SGX Quote 生成需要 Intel DCAP 运行时库：
            //   cargo add sgx_dcap_ql --features dcap
            // 并在真实 SGX 硬件上运行。
            //
            // 示例调用（需要 `sgx` feature）：
            //   #[cfg(feature = "sgx")]
            //   return sgx_generate_quote(nonce).map(|q| base64_encode(q));
            Err(
                "SGX quote generation requires Intel SGX DCAP hardware and the sgx_urts/sgx_dcap_ql \
                 libraries. Run on SGX-capable hardware with the 'sgx' feature enabled, \
                 or set TEE_MODE=simulation for development."
                    .to_string(),
            )
        }
        TeeType::Tdx => {
            // TDX Quote 生成需要 Intel TDX 模块和 tdx-attest 库：
            //   cargo add tdx-attest --features tdx
            // 并在 TDX 支持的平台（第四代 Xeon 及以上）上运行。
            Err(
                "TDX quote generation requires Intel TDX-capable hardware (4th Gen Xeon+) and \
                 the tdx-attest library. Run on TDX-enabled platform with the 'tdx' feature \
                 enabled, or set TEE_MODE=simulation for development."
                    .to_string(),
            )
        }
        TeeType::SevSnp => {
            // SEV-SNP attestation 通过 /dev/sev-guest 固件接口获取：
            //   需要 AMD EPYC 7003+ 处理器和启用 SEV-SNP 的 BIOS 设置。
            Err(
                "SEV-SNP attestation requires AMD EPYC 7003+ (Milan/Genoa) with SEV-SNP enabled \
                 in BIOS and kernel support (/dev/sev-guest). Run on SEV-SNP-capable hardware, \
                 or set TEE_MODE=simulation for development."
                    .to_string(),
            )
        }
        TeeType::Simulation => {
            // 不应到达此处（simulation 模式已在调用方处理）
            Err("Use TEE_MODE=simulation to generate simulated quotes.".to_string())
        }
        TeeType::Unknown => Err(format!(
            "Unknown TEE type. Set the tee_type field to sgx, tdx, or sev-snp before \
             requesting a real quote. Current type: {:?}",
            state.tee_type
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tee_type_display() {
        assert_eq!(TeeType::Sgx.to_string(), "sgx");
        assert_eq!(TeeType::Tdx.to_string(), "tdx");
        assert_eq!(TeeType::SevSnp.to_string(), "sev-snp");
        assert_eq!(TeeType::Simulation.to_string(), "simulation");
    }

    #[test]
    fn test_attestation_state_new() {
        let config = AttestationApiConfig {
            tee_runtime: TeeRuntimeConfig::hardware(),
            ..Default::default()
        };
        let state = AttestationState::new(config.clone());

        assert_eq!(state.tee_type, TeeType::Unknown);
        assert!(!state.initialized);
        assert!(state.mrenclave.is_none());
    }

    #[test]
    fn test_attestation_state_simulation_mode() {
        let config = AttestationApiConfig {
            tee_runtime: TeeRuntimeConfig::simulation(),
            ..Default::default()
        };
        let state = AttestationState::new(config);

        assert_eq!(state.tee_type, TeeType::Simulation);
    }

    #[test]
    fn test_attestation_state_builder() {
        let state = AttestationState::new(AttestationApiConfig {
            tee_runtime: TeeRuntimeConfig::hardware(),
            ..Default::default()
        })
        .with_tee_type(TeeType::Sgx)
        .with_initialized(true)
        .with_mrenclave("abc123".to_string())
        .with_mrsigner("def456".to_string());

        assert_eq!(state.tee_type, TeeType::Sgx);
        assert!(state.initialized);
        assert_eq!(state.mrenclave, Some("abc123".to_string()));
        assert_eq!(state.mrsigner, Some("def456".to_string()));
    }

    #[test]
    fn test_quote_status_serialization() {
        let status = QuoteStatus::Valid;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"valid\"");

        let status = QuoteStatus::Simulated;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"simulated\"");
    }

    #[test]
    fn test_attestation_status_serialization() {
        let status = AttestationStatus::Attested;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"attested\"");

        let status = AttestationStatus::Simulated;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"simulated\"");
    }

    #[test]
    fn test_base64_encode_simulated_quote() {
        let quote = base64_encode_simulated_quote();
        assert!(!quote.is_empty());
        // 验证是有效的 Base64
        use base64::{Engine as _, engine::general_purpose};
        assert!(general_purpose::STANDARD.decode(&quote).is_ok());
    }

    #[tokio::test]
    async fn test_get_quote_simulation_mode() {
        let config = AttestationApiConfig {
            tee_runtime: TeeRuntimeConfig::simulation(),
            ..Default::default()
        };
        let state = Arc::new(AttestationState::new(config));

        let result = get_quote(State(state)).await;
        let response = result.into_response();

        assert_eq!(response.status(), StatusCode::OK);

        // 从响应体中提取 JSON
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: QuoteResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.status, QuoteStatus::Simulated);
        assert_eq!(json.tee_type, "simulation");
    }

    #[tokio::test]
    async fn test_get_status_simulation_mode() {
        let config = AttestationApiConfig {
            tee_runtime: TeeRuntimeConfig::simulation(),
            ..Default::default()
        };
        let state = Arc::new(AttestationState::new(config));

        let result = get_status(State(state)).await;
        let response = result.into_response();

        assert_eq!(response.status(), StatusCode::OK);

        // 从响应体中提取 JSON
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: AttestationStatusResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(json.requested_mode, "simulation");
        assert_eq!(json.effective_mode, "simulation");
        assert_eq!(json.attestation_status, AttestationStatus::Simulated);
    }

    #[test]
    fn test_init_attestation_api_hardware_mode_fails_closed() {
        let error = init_attestation_api(AttestationApiConfig {
            tee_runtime: TeeRuntimeConfig::hardware(),
            ..Default::default()
        })
        .unwrap_err();

        assert!(error.contains("TEE_MODE=hardware"));
        assert!(error.contains("refusing simulated fallback"));
    }
}
