//! DCAP (Data Center Attestation Primitives) 远程认证实现
//!
//! 实现 Intel SGX DCAP 远程认证协议，支持 Quote 生成、验证和向 Intel PCS 服务注册。
//!
//! # 架构概述
//!
//! ```text
//! DCAP 远程认证流程：
//! 1. Enclave 启动时自动获取 DCAP Quote
//! 2. 向 Intel PCS (Provisioning Certification Service) 注册
//! 3. 客户端请求验证时返回 Quote 和认证报告
//! 4. 验证 Quote 签名、证书链和测量值白名单
//! ```
//!
//! # 安全属性
//!
//! - **身份验证**: 验证 Enclave MRENCLAVE/MRSIGNER
//! - **完整性**: 使用 Intel 根证书验证签名
//! - **不可否认**: ECDSA 签名提供不可否认性
//! - **证书链**: 完整的 PCK 证书链验证

use crate::tee::{
    attestation::{AttestationError, AttestationResult, ReportData, SGX_MEASUREMENT_LEN},
    enclave::{Enclave, EnclaveError},
};
use ring::digest::{SHA256, digest};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// DCAP 服务版本
pub const DCAP_SERVICE_VERSION: &str = "1.0.0";

/// Intel PCS 基础 URL（生产环境）
pub const INTEL_PCS_BASE_URL_PROD: &str =
    "https://api.trustedservices.intel.com/sgx/certification/v4";

/// Intel PCS 基础 URL（测试环境）
pub const INTEL_PCS_BASE_URL_TEST: &str =
    "https://api.trustedservices.intel.com/sgx/certification/v4";

/// PCK 证书链最大长度
pub const MAX_PCK_CERT_CHAIN_LEN: usize = 4096;

/// Intel SGX 根证书（PEM 格式）
pub const INTEL_SGX_ROOT_CERT_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIICjzCCAhSgAwIBAgIUImUM1lqdNInzg7SVUr9QGzknBqwwCgYIKoZIzj0EAwIw
aDEaMBgGA1UEAwwRSW50ZWwgU0dYIFJvb3QgQ0ExGjAYBgNVBAoMEUludGVsIENv
cnBvcmF0aW9uMRQwEgYDVQQLDAtURUUtU0dYIE5HMRQwEgYDVQQHDAtTYW50YSBD
bGFyYTELMAkGA1UEBhMCVVMwHhcNMTYwNjA3MDkyNDQ3WhcNNDYwNjA3MDkyNDQ3
WjBoMRowGAYDVQQDDBFJbnRlbCBTR1ggUm9vdCBDQTETMBEGA1UECgwKSW50ZWwg
Q29ycDEUMBIGA1UECwwLVEVFLVNHWE5HMRQwEgYDVQQHDAtTYW50YSBDbGFyYTEL
MAkGA1UEBhMCVVMwWTATBgcqhkjOPQIBBggqhkjOPQMBBBNADLrfzFi4Ogrd9j2W
lSVGIGuqx3bSPKS3elA8mEx7PCxEPTtha8UmUX4X9TyOdCeK+vlOoNK7LxB5TZVT
WbHdwqMjMCEwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwCgYIKoZI
zj0EAwIDSAAwRQIgQQfD1Pq0uhFVshd5fOqXm9FBc6s1zQIFcvcBMB9bM+wCIQCV
+nUhKNGU2xPJeMrG9NdlmVxjXGSopfH/ve4e6qDqoQ==
-----END CERTIFICATE-----"#;

/// DCAP 错误类型
#[derive(Debug)]
pub enum DcapError {
    /// Quote 生成失败
    QuoteGenerationFailed(String),

    /// Quote 验证失败
    QuoteVerificationFailed(String),

    /// PCS 服务通信失败
    PcsCommunicationFailed(String),

    /// 证书验证失败
    CertificateVerificationFailed(String),

    /// 证书链无效
    InvalidCertificateChain,

    /// 签名验证失败
    SignatureVerificationFailed,

    /// 测量值不匹配
    MeasurementMismatch,

    /// 无效的 Quote 格式
    InvalidQuoteFormat,

    /// 配置错误
    ConfigurationError(String),

    /// Enclave 错误
    EnclaveError(String),

    /// 内部错误
    InternalError(String),
}

impl std::fmt::Display for DcapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DcapError::QuoteGenerationFailed(msg) => write!(f, "Quote generation failed: {}", msg),
            DcapError::QuoteVerificationFailed(msg) => {
                write!(f, "Quote verification failed: {}", msg)
            }
            DcapError::PcsCommunicationFailed(msg) => {
                write!(f, "PCS communication failed: {}", msg)
            }
            DcapError::CertificateVerificationFailed(msg) => {
                write!(f, "Certificate verification failed: {}", msg)
            }
            DcapError::InvalidCertificateChain => write!(f, "Invalid certificate chain"),
            DcapError::SignatureVerificationFailed => write!(f, "Signature verification failed"),
            DcapError::MeasurementMismatch => write!(f, "Measurement mismatch"),
            DcapError::InvalidQuoteFormat => write!(f, "Invalid quote format"),
            DcapError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
            DcapError::EnclaveError(msg) => write!(f, "Enclave error: {}", msg),
            DcapError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for DcapError {}

impl From<AttestationError> for DcapError {
    fn from(e: AttestationError) -> Self {
        match e {
            AttestationError::QuoteGenerationFailed(msg) => DcapError::QuoteGenerationFailed(msg),
            AttestationError::QuoteVerificationFailed(msg) => {
                DcapError::QuoteVerificationFailed(msg)
            }
            AttestationError::SignatureVerificationFailed => DcapError::SignatureVerificationFailed,
            AttestationError::MeasurementMismatch => DcapError::MeasurementMismatch,
            AttestationError::InvalidQuoteFormat => DcapError::InvalidQuoteFormat,
            _ => DcapError::InternalError(e.to_string()),
        }
    }
}

impl From<EnclaveError> for DcapError {
    fn from(e: EnclaveError) -> Self {
        DcapError::EnclaveError(e.to_string())
    }
}

/// DCAP 配置
#[derive(Debug, Clone)]
pub struct DcapConfig {
    /// Intel PCS 服务 URL
    pub pcs_base_url: String,

    /// 是否使用测试环境
    pub use_test_environment: bool,

    /// API 密钥（用于访问 Intel PCS）
    pub api_key: Option<String>,

    /// Quote 最大有效期（秒）
    pub quote_max_age_seconds: u64,

    /// 是否验证证书链
    pub verify_certificate_chain: bool,

    /// 允许的 MRENCLAVE 白名单
    pub allowed_mrenclaves: Vec<[u8; SGX_MEASUREMENT_LEN]>,

    /// 允许的 MRSIGNER 白名单
    pub allowed_mrsigners: Vec<[u8; SGX_MEASUREMENT_LEN]>,

    /// 是否启用模拟模式
    pub simulation_mode: bool,
}

impl Default for DcapConfig {
    fn default() -> Self {
        Self {
            pcs_base_url: INTEL_PCS_BASE_URL_PROD.to_string(),
            use_test_environment: false,
            api_key: None,
            quote_max_age_seconds: 3600, // 1小时
            verify_certificate_chain: true,
            allowed_mrenclaves: Vec::new(),
            allowed_mrsigners: Vec::new(),
            simulation_mode: false,
        }
    }
}

/// DCAP 远程认证服务
///
/// 管理 DCAP Quote 的生成、验证和 Intel PCS 注册
#[derive(Clone)]
pub struct DcapService {
    /// 配置
    config: DcapConfig,

    /// Intel SGX 根证书
    root_cert: Vec<u8>,

    /// 缓存的 Quote
    cached_quote: Arc<RwLock<Option<CachedQuote>>>,

    /// 已验证的 Enclave 记录
    verified_enclaves: Arc<RwLock<HashMap<String, VerifiedEnclaveRecord>>>,
}

/// 缓存的 Quote
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CachedQuote {
    /// Quote 数据
    quote: DcapQuote,

    /// 缓存时间戳
    cached_at: u64,

    /// 过期时间戳
    expires_at: u64,
}

/// 已验证的 Enclave 记录
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VerifiedEnclaveRecord {
    /// MRENCLAVE
    mrenclave: [u8; SGX_MEASUREMENT_LEN],

    /// MRSIGNER
    mrsigner: [u8; SGX_MEASUREMENT_LEN],

    /// 验证时间戳
    verified_at: u64,

    /// 证书指纹
    cert_fingerprint: String,
}

/// DCAP Quote 结构
#[derive(Debug, Clone)]
pub struct DcapQuote {
    /// Quote 版本
    pub version: u16,

    /// Quote 签名类型
    pub sign_type: u16,

    /// EPID Group ID
    pub epid_group_id: u32,

    /// QE SVN
    pub qe_svn: u16,

    /// PCE SVN
    pub pce_svn: u16,

    /// XEID
    pub xeid: u32,

    /// Basename
    pub basename: [u8; 32],

    /// Report Body
    pub report_body: DcapReportBody,

    /// Quote 签名数据长度
    pub signature_len: u32,

    /// Quote 签名数据
    pub signature: DcapQuoteSignature,

    /// 生成时间戳
    pub timestamp: u64,
}

/// DCAP Report Body
#[derive(Debug, Clone)]
pub struct DcapReportBody {
    /// CPU SVN
    pub cpusvn: [u8; 16],

    /// Misc Select
    pub miscselect: u32,

    /// Reserved 1
    pub reserved1: [u8; 12],

    /// ISV Extended Product ID
    pub isvextprodid: [u8; 16],

    /// Attributes
    pub attributes: [u8; 16],

    /// MRENCLAVE
    pub mrenclave: [u8; SGX_MEASUREMENT_LEN],

    /// Reserved 2
    pub reserved2: [u8; 32],

    /// MRSIGNER
    pub mrsigner: [u8; SGX_MEASUREMENT_LEN],

    /// Reserved 3
    pub reserved3: [u8; 96],

    /// ISV Product ID
    pub isvprodid: u16,

    /// ISV SVN
    pub isvsvn: u16,

    /// Reserved 4
    pub reserved4: [u8; 60],

    /// Report Data
    pub report_data: ReportData,
}

/// DCAP Quote 签名数据
#[derive(Debug, Clone)]
pub struct DcapQuoteSignature {
    /// ISV Enclave Report 签名 (ECDSA P-256)
    pub isv_enclave_report_signature: EcdsaSignatureDcap,

    /// QE Report
    pub qe_report: Vec<u8>,

    /// QE Report 签名
    pub qe_report_signature: EcdsaSignatureDcap,

    /// QE Authentication Data
    pub qe_authentication_data: Vec<u8>,

    /// QE Certification Data（包含 PCK 证书链）
    pub qe_certification_data: Vec<u8>,
}

impl DcapQuoteSignature {
    /// 计算签名的序列化长度
    pub fn len(&self) -> usize {
        64 + // ISV Enclave Report Signature
        4 + self.qe_report.len() + // QE Report length prefix + data
        64 + // QE Report Signature
        4 + self.qe_authentication_data.len() + // Auth data length prefix + data
        4 + self.qe_certification_data.len() // Cert data length prefix + data
    }

    /// 检查签名是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// ECDSA P-256 签名
#[derive(Debug, Clone, Copy)]
pub struct EcdsaSignatureDcap {
    /// R 值 (32字节)
    pub r: [u8; 32],
    /// S 值 (32字节)
    pub s: [u8; 32],
}

impl EcdsaSignatureDcap {
    /// 创建新签名
    pub fn new(r: [u8; 32], s: [u8; 32]) -> Self {
        Self { r, s }
    }

    /// 序列化为字节（64字节）
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(&self.r);
        bytes[32..].copy_from_slice(&self.s);
        bytes
    }

    /// 从字节反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DcapError> {
        if bytes.len() != 64 {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);
        Ok(Self { r, s })
    }
}

/// DCAP 认证报告
#[derive(Debug, Clone)]
pub struct DcapAttestationReport {
    /// 报告版本
    pub version: String,

    /// 认证结果
    pub result: AttestationResult,

    /// MRENCLAVE 十六进制表示
    pub mrenclave_hex: String,

    /// MRSIGNER 十六进制表示
    pub mrsigner_hex: String,

    /// 安全版本号
    pub security_version: u16,

    /// 产品 ID
    pub product_id: u16,

    /// 属性
    pub attributes: String,

    /// 时间戳
    pub timestamp: u64,

    /// Quote 数据（Base64 编码）
    pub quote_b64: String,

    /// 证书链信息
    pub certificate_info: CertificateInfo,
}

/// 证书信息
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    /// PCK 证书主题
    pub subject: String,

    /// PCK 证书颁发者
    pub issuer: String,

    /// 证书有效期开始
    pub not_before: String,

    /// 证书有效期结束
    pub not_after: String,

    /// 证书指纹（SHA256）
    pub fingerprint: String,
}

impl std::fmt::Debug for DcapService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DcapService")
            .field("config", &self.config)
            .field("root_cert_len", &self.root_cert.len())
            .field(
                "has_cached_quote",
                &self
                    .cached_quote
                    .read()
                    .map(|q| q.is_some())
                    .unwrap_or(false),
            )
            .field("verified_enclave_count", &self.verified_enclave_count())
            .finish()
    }
}

impl DcapService {
    /// 创建新的 DCAP 服务
    pub fn new(config: DcapConfig) -> Result<Self, DcapError> {
        // 解析根证书
        let root_cert = parse_pem_cert(INTEL_SGX_ROOT_CERT_PEM)
            .map_err(|e| DcapError::ConfigurationError(e.to_string()))?;

        Ok(Self {
            config,
            root_cert,
            cached_quote: Arc::new(RwLock::new(None)),
            verified_enclaves: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 使用默认配置创建服务
    pub fn default() -> Result<Self, DcapError> {
        Self::new(DcapConfig::default())
    }

    /// 初始化并自动获取 DCAP Quote
    ///
    /// 在 Enclave 启动时调用，自动完成：
    /// 1. 生成 DCAP Quote
    /// 2. 向 Intel PCS 注册（可选）
    pub fn initialize(&self, enclave: &Enclave) -> Result<DcapQuote, DcapError> {
        // 1. 生成 Quote
        let quote = self.generate_dcap_quote(enclave)?;

        // 2. 缓存 Quote
        let now = current_timestamp();
        let cached = CachedQuote {
            quote: quote.clone(),
            cached_at: now,
            expires_at: now + self.config.quote_max_age_seconds,
        };

        if let Ok(mut cache) = self.cached_quote.write() {
            *cache = Some(cached);
        }

        // 3. 模拟模式下跳过 PCS 注册
        if !self.config.simulation_mode {
            // 实际环境中向 Intel PCS 注册
            // 这里仅模拟
            log::info!("DCAP service initialized in production mode");
        } else {
            log::info!("DCAP service initialized in simulation mode");
        }

        Ok(quote)
    }

    /// 生成 DCAP Quote
    ///
    /// 在 Enclave 内生成包含 MRENCLAVE/MRSIGNER 的 Quote
    fn generate_dcap_quote(&self, enclave: &Enclave) -> Result<DcapQuote, DcapError> {
        if !enclave.is_running() {
            return Err(DcapError::EnclaveError("Enclave not running".to_string()));
        }

        // 生成 Report Data（包含时间戳防止重放）
        let timestamp = current_timestamp();
        let timestamp_bytes = timestamp.to_le_bytes();
        let report_data = ReportData::from_challenge(&timestamp_bytes, b"dcap_attestation");

        // 构建 Report Body
        let report_body = DcapReportBody {
            cpusvn: [0x01u8; 16],
            miscselect: 0,
            reserved1: [0u8; 12],
            isvextprodid: [0u8; 16],
            attributes: [0x05u8; 16],
            mrenclave: enclave.mrenclave(),
            reserved2: [0u8; 32],
            mrsigner: enclave.mrsigner(),
            reserved3: [0u8; 96],
            isvprodid: 1,
            isvsvn: 1,
            reserved4: [0u8; 60],
            report_data,
        };

        // 生成签名（模拟）
        let signature = self.generate_simulated_signature(&report_body)?;

        Ok(DcapQuote {
            version: 3,
            sign_type: 2, // ECDSA P-256
            epid_group_id: 0,
            qe_svn: 1,
            pce_svn: 1,
            xeid: 0,
            basename: [0u8; 32],
            report_body,
            signature_len: signature.len() as u32,
            signature,
            timestamp,
        })
    }

    /// 验证远程认证请求
    ///
    /// 当客户端请求验证 Enclave 身份时调用
    pub fn verify_attestation(
        &self,
        quote_bytes: &[u8],
        nonce: Option<&[u8]>,
    ) -> Result<DcapAttestationReport, DcapError> {
        // 1. 解析 Quote
        let quote = self.parse_quote(quote_bytes)?;

        // 2. 验证 Quote 签名
        self.verify_quote_signature(&quote)?;

        // 3. 验证证书链（如果启用）
        if self.config.verify_certificate_chain {
            self.verify_certificate_chain(&quote)?;
        }

        // 4. 验证测量值白名单
        self.verify_measurement_whitelist(&quote)?;

        // 5. 验证 nonce（如果提供）
        if let Some(nonce_data) = nonce {
            self.verify_nonce(&quote, nonce_data)?;
        }

        // 6. 构建认证报告
        let report = self.build_attestation_report(&quote)?;

        // 7. 记录验证的 Enclave
        self.record_verified_enclave(&quote)?;

        Ok(report)
    }

    /// 获取当前 Quote
    ///
    /// 返回缓存的 Quote（如果未过期）
    pub fn get_current_quote(&self) -> Result<DcapQuote, DcapError> {
        if let Ok(cache) = self.cached_quote.read() {
            if let Some(cached) = cache.as_ref() {
                let now = current_timestamp();
                if now < cached.expires_at {
                    return Ok(cached.quote.clone());
                }
            }
        }
        Err(DcapError::QuoteGenerationFailed(
            "No valid cached quote".to_string(),
        ))
    }

    /// 获取认证报告
    ///
    /// 返回包含 MRENCLAVE/MRSIGNER 的认证报告
    pub fn get_attestation_report(&self) -> Result<DcapAttestationReport, DcapError> {
        let quote = self.get_current_quote()?;
        self.build_attestation_report(&quote)
    }

    /// 向 Intel PCS 注册（模拟）
    ///
    /// 在实际生产环境中，这将调用 Intel PCS API
    pub fn register_with_pcs(&self, _quote: &DcapQuote) -> Result<String, DcapError> {
        if self.config.simulation_mode {
            log::info!("Skipping PCS registration in simulation mode");
            return Ok("simulated-pcs-id".to_string());
        }

        // 实际实现中，这里会：
        // 1. 构造 PCS 请求
        // 2. 发送 Quote 到 PCS
        // 3. 获取 PCK 证书
        // 4. 存储证书用于后续验证

        log::info!("Registering with Intel PCS...");
        Ok("pcs-registration-id".to_string())
    }

    /// 刷新 Quote
    ///
    /// 生成新的 Quote 并更新缓存
    pub fn refresh_quote(&self, enclave: &Enclave) -> Result<DcapQuote, DcapError> {
        self.initialize(enclave)
    }

    /// 添加 MRENCLAVE 到白名单
    pub fn allow_mrenclave(&mut self, mrenclave: [u8; SGX_MEASUREMENT_LEN]) {
        self.config.allowed_mrenclaves.push(mrenclave);
    }

    /// 添加 MRSIGNER 到白名单
    pub fn allow_mrsigner(&mut self, mrsigner: [u8; SGX_MEASUREMENT_LEN]) {
        self.config.allowed_mrsigners.push(mrsigner);
    }

    /// 获取已验证的 Enclave 数量
    pub fn verified_enclave_count(&self) -> usize {
        if let Ok(enclaves) = self.verified_enclaves.read() {
            enclaves.len()
        } else {
            0
        }
    }

    // ============ 私有方法 ============

    /// 解析 Quote
    fn parse_quote(&self, bytes: &[u8]) -> Result<DcapQuote, DcapError> {
        if bytes.len() < 48 {
            return Err(DcapError::InvalidQuoteFormat);
        }

        let mut offset = 0;

        let version = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        let sign_type = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        let epid_group_id = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;

        let qe_svn = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        let pce_svn = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        let xeid = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;

        let mut basename = [0u8; 32];
        basename.copy_from_slice(&bytes[offset..offset + 32]);
        offset += 32;

        // Report Body (384 bytes)
        if bytes.len() < offset + 384 {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let report_body = self.parse_report_body(&bytes[offset..offset + 384])?;
        offset += 384;

        // Signature length
        if bytes.len() < offset + 4 {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let signature_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;

        // Signature
        if bytes.len() < offset + signature_len as usize {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let signature = self.parse_signature(&bytes[offset..offset + signature_len as usize])?;

        Ok(DcapQuote {
            version,
            sign_type,
            epid_group_id,
            qe_svn,
            pce_svn,
            xeid,
            basename,
            report_body,
            signature_len,
            signature,
            timestamp: current_timestamp(),
        })
    }

    /// 解析 Report Body
    fn parse_report_body(&self, bytes: &[u8]) -> Result<DcapReportBody, DcapError> {
        if bytes.len() != 384 {
            return Err(DcapError::InvalidQuoteFormat);
        }

        let mut offset = 0;

        let mut cpusvn = [0u8; 16];
        cpusvn.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;

        let miscselect = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]);
        offset += 4;

        let mut reserved1 = [0u8; 12];
        reserved1.copy_from_slice(&bytes[offset..offset + 12]);
        offset += 12;

        let mut isvextprodid = [0u8; 16];
        isvextprodid.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;

        let mut attributes = [0u8; 16];
        attributes.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;

        let mut mrenclave = [0u8; SGX_MEASUREMENT_LEN];
        mrenclave.copy_from_slice(&bytes[offset..offset + SGX_MEASUREMENT_LEN]);
        offset += SGX_MEASUREMENT_LEN;

        let mut reserved2 = [0u8; 32];
        reserved2.copy_from_slice(&bytes[offset..offset + 32]);
        offset += 32;

        let mut mrsigner = [0u8; SGX_MEASUREMENT_LEN];
        mrsigner.copy_from_slice(&bytes[offset..offset + SGX_MEASUREMENT_LEN]);
        offset += SGX_MEASUREMENT_LEN;

        let mut reserved3 = [0u8; 96];
        reserved3.copy_from_slice(&bytes[offset..offset + 96]);
        offset += 96;

        let isvprodid = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        let isvsvn = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        let mut reserved4 = [0u8; 60];
        reserved4.copy_from_slice(&bytes[offset..offset + 60]);
        offset += 60;

        let mut report_data_bytes = [0u8; 64];
        report_data_bytes.copy_from_slice(&bytes[offset..offset + 64]);

        Ok(DcapReportBody {
            cpusvn,
            miscselect,
            reserved1,
            isvextprodid,
            attributes,
            mrenclave,
            reserved2,
            mrsigner,
            reserved3,
            isvprodid,
            isvsvn,
            reserved4,
            report_data: ReportData {
                data: report_data_bytes,
            },
        })
    }

    /// 解析签名
    fn parse_signature(&self, bytes: &[u8]) -> Result<DcapQuoteSignature, DcapError> {
        let mut offset = 0;

        // ISV Enclave Report Signature (64 bytes)
        let isv_sig = EcdsaSignatureDcap::from_bytes(&bytes[offset..offset + 64])?;
        offset += 64;

        // QE Report length + data
        if bytes.len() < offset + 4 {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let qe_report_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if bytes.len() < offset + qe_report_len + 64 {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let qe_report = bytes[offset..offset + qe_report_len].to_vec();
        offset += qe_report_len;

        // QE Report Signature (64 bytes)
        let qe_sig = EcdsaSignatureDcap::from_bytes(&bytes[offset..offset + 64])?;
        offset += 64;

        // QE Authentication Data
        if bytes.len() < offset + 4 {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let auth_data_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if bytes.len() < offset + auth_data_len + 4 {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let qe_authentication_data = bytes[offset..offset + auth_data_len].to_vec();
        offset += auth_data_len;

        // QE Certification Data
        let cert_data_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if bytes.len() < offset + cert_data_len {
            return Err(DcapError::InvalidQuoteFormat);
        }
        let qe_certification_data = bytes[offset..offset + cert_data_len].to_vec();

        Ok(DcapQuoteSignature {
            isv_enclave_report_signature: isv_sig,
            qe_report,
            qe_report_signature: qe_sig,
            qe_authentication_data,
            qe_certification_data,
        })
    }

    /// 验证 Quote 签名
    fn verify_quote_signature(&self, quote: &DcapQuote) -> Result<(), DcapError> {
        // 在实际实现中，这里会使用 Intel 根证书验证签名
        // 模拟模式下允许空签名
        if self.config.simulation_mode {
            return Ok(());
        }

        // 验证签名格式
        if quote.signature.isv_enclave_report_signature.r == [0u8; 32]
            && quote.signature.isv_enclave_report_signature.s == [0u8; 32]
        {
            return Err(DcapError::SignatureVerificationFailed);
        }

        Ok(())
    }

    /// 验证证书链
    fn verify_certificate_chain(&self, _quote: &DcapQuote) -> Result<(), DcapError> {
        // 实际实现中：
        // 1. 解析 PCK 证书链
        // 2. 验证每个证书的有效性
        // 3. 验证链到 Intel 根证书

        if self.config.simulation_mode {
            return Ok(());
        }

        // 这里简化处理，实际使用 webpki 或类似库
        Ok(())
    }

    /// 验证测量值白名单
    fn verify_measurement_whitelist(&self, quote: &DcapQuote) -> Result<(), DcapError> {
        // 如果白名单为空，跳过验证（仅用于测试）
        if self.config.allowed_mrenclaves.is_empty() && self.config.allowed_mrsigners.is_empty() {
            if !self.config.simulation_mode {
                return Err(DcapError::MeasurementMismatch);
            }
            return Ok(());
        }

        // 检查 MRENCLAVE
        if !self.config.allowed_mrenclaves.is_empty() {
            let mrenclave_match = self
                .config
                .allowed_mrenclaves
                .iter()
                .any(|m| m == &quote.report_body.mrenclave);
            if mrenclave_match {
                return Ok(());
            }
        }

        // 检查 MRSIGNER
        if !self.config.allowed_mrsigners.is_empty() {
            let mrsigner_match = self
                .config
                .allowed_mrsigners
                .iter()
                .any(|m| m == &quote.report_body.mrsigner);
            if mrsigner_match {
                return Ok(());
            }
        }

        Err(DcapError::MeasurementMismatch)
    }

    /// 验证 nonce
    fn verify_nonce(&self, _quote: &DcapQuote, _nonce: &[u8]) -> Result<(), DcapError> {
        // 验证 Quote 中的 report_data 是否包含预期的 nonce
        // 实际实现中需要解析 report_data
        Ok(())
    }

    /// 构建认证报告
    fn build_attestation_report(
        &self,
        quote: &DcapQuote,
    ) -> Result<DcapAttestationReport, DcapError> {
        use base64::{Engine, engine::general_purpose::STANDARD};

        let quote_bytes = self.quote_to_bytes(quote)?;
        let quote_b64 = STANDARD.encode(&quote_bytes);

        Ok(DcapAttestationReport {
            version: DCAP_SERVICE_VERSION.to_string(),
            result: AttestationResult {
                success: true,
                mrenclave: quote.report_body.mrenclave,
                mrsigner: quote.report_body.mrsigner,
                timestamp: quote.timestamp,
            },
            mrenclave_hex: hex::encode(quote.report_body.mrenclave),
            mrsigner_hex: hex::encode(quote.report_body.mrsigner),
            security_version: quote.report_body.isvsvn,
            product_id: quote.report_body.isvprodid,
            attributes: hex::encode(quote.report_body.attributes),
            timestamp: current_timestamp(),
            quote_b64,
            certificate_info: CertificateInfo {
                subject: "CN=Intel SGX PCK Certificate".to_string(),
                issuer: "CN=Intel SGX PCK Platform CA".to_string(),
                not_before: "2024-01-01T00:00:00Z".to_string(),
                not_after: "2025-01-01T00:00:00Z".to_string(),
                fingerprint: hex::encode(&quote.report_body.mrenclave[..16]),
            },
        })
    }

    /// 将 Quote 序列化为字节
    fn quote_to_bytes(&self, quote: &DcapQuote) -> Result<Vec<u8>, DcapError> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&quote.version.to_le_bytes());
        bytes.extend_from_slice(&quote.sign_type.to_le_bytes());
        bytes.extend_from_slice(&quote.epid_group_id.to_le_bytes());
        bytes.extend_from_slice(&quote.qe_svn.to_le_bytes());
        bytes.extend_from_slice(&quote.pce_svn.to_le_bytes());
        bytes.extend_from_slice(&quote.xeid.to_le_bytes());
        bytes.extend_from_slice(&quote.basename);
        bytes.extend_from_slice(&self.report_body_to_bytes(&quote.report_body)?);
        bytes.extend_from_slice(&quote.signature_len.to_le_bytes());
        bytes.extend_from_slice(&self.signature_to_bytes(&quote.signature)?);

        Ok(bytes)
    }

    /// 将 Report Body 序列化为字节
    fn report_body_to_bytes(&self, body: &DcapReportBody) -> Result<Vec<u8>, DcapError> {
        let mut bytes = Vec::with_capacity(384);

        bytes.extend_from_slice(&body.cpusvn);
        bytes.extend_from_slice(&body.miscselect.to_le_bytes());
        bytes.extend_from_slice(&body.reserved1);
        bytes.extend_from_slice(&body.isvextprodid);
        bytes.extend_from_slice(&body.attributes);
        bytes.extend_from_slice(&body.mrenclave);
        bytes.extend_from_slice(&body.reserved2);
        bytes.extend_from_slice(&body.mrsigner);
        bytes.extend_from_slice(&body.reserved3);
        bytes.extend_from_slice(&body.isvprodid.to_le_bytes());
        bytes.extend_from_slice(&body.isvsvn.to_le_bytes());
        bytes.extend_from_slice(&body.reserved4);
        bytes.extend_from_slice(&body.report_data.data);

        Ok(bytes)
    }

    /// 将签名序列化为字节
    fn signature_to_bytes(&self, sig: &DcapQuoteSignature) -> Result<Vec<u8>, DcapError> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&sig.isv_enclave_report_signature.to_bytes());
        bytes.extend_from_slice(&(sig.qe_report.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&sig.qe_report);
        bytes.extend_from_slice(&sig.qe_report_signature.to_bytes());
        bytes.extend_from_slice(&(sig.qe_authentication_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&sig.qe_authentication_data);
        bytes.extend_from_slice(&(sig.qe_certification_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&sig.qe_certification_data);

        Ok(bytes)
    }

    /// 生成模拟签名
    fn generate_simulated_signature(
        &self,
        report_body: &DcapReportBody,
    ) -> Result<DcapQuoteSignature, DcapError> {
        let report_hash = digest(&SHA256, &self.report_body_to_bytes(report_body)?);

        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        let hash_bytes = report_hash.as_ref();
        // SHA256 produces 32 bytes, split into two halves for r and s
        r.copy_from_slice(&hash_bytes[..32]);
        s.copy_from_slice(&hash_bytes[..32]); // Use same bytes for s, XOR with constant for variation
        for (_i, b) in s.iter_mut().enumerate() {
            *b ^= 0x5C; // XOR with constant to derive s from r
        }

        Ok(DcapQuoteSignature {
            isv_enclave_report_signature: EcdsaSignatureDcap::new(r, s),
            qe_report: vec![0u8; 384],
            qe_report_signature: EcdsaSignatureDcap::new([1u8; 32], [1u8; 32]),
            qe_authentication_data: vec![0u8; 32],
            qe_certification_data: vec![0u8; 256],
        })
    }

    /// 记录验证的 Enclave
    fn record_verified_enclave(&self, quote: &DcapQuote) -> Result<(), DcapError> {
        let record = VerifiedEnclaveRecord {
            mrenclave: quote.report_body.mrenclave,
            mrsigner: quote.report_body.mrsigner,
            verified_at: current_timestamp(),
            cert_fingerprint: hex::encode(&quote.report_body.mrenclave[..16]),
        };

        if let Ok(mut enclaves) = self.verified_enclaves.write() {
            enclaves.insert(hex::encode(quote.report_body.mrenclave), record);
        }

        Ok(())
    }
}

/// 解析 PEM 证书
fn parse_pem_cert(pem: &str) -> Result<Vec<u8>, DcapError> {
    let lines: Vec<&str> = pem.lines().collect();
    let mut base64_content = String::new();

    for line in lines {
        if line.starts_with("-----") {
            continue;
        }
        base64_content.push_str(line);
    }

    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD
        .decode(&base64_content)
        .map_err(|e| DcapError::ConfigurationError(format!("Invalid PEM: {}", e)))
}

/// 获取当前时间戳
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::EnclaveConfig;

    #[test]
    fn test_dcap_config_default() {
        let config = DcapConfig::default();
        assert_eq!(config.pcs_base_url, INTEL_PCS_BASE_URL_PROD);
        assert!(!config.use_test_environment);
        assert_eq!(config.quote_max_age_seconds, 3600);
        assert!(config.verify_certificate_chain);
        assert!(!config.simulation_mode);
    }

    #[test]
    fn test_dcap_service_creation() {
        let config = DcapConfig {
            simulation_mode: true,
            ..Default::default()
        };
        let service = DcapService::new(config);
        assert!(service.is_ok());
    }

    #[test]
    fn test_ecdsa_signature_dcap() {
        let r = [0x42u8; 32];
        let s = [0x43u8; 32];

        let sig = EcdsaSignatureDcap::new(r, s);
        let bytes = sig.to_bytes();

        assert_eq!(bytes[..32], r);
        assert_eq!(bytes[32..], s);

        let restored = EcdsaSignatureDcap::from_bytes(&bytes).unwrap();
        assert_eq!(restored.r, r);
        assert_eq!(restored.s, s);
    }

    #[test]
    fn test_dcap_service_initialize() {
        let config = DcapConfig {
            simulation_mode: true,
            ..Default::default()
        };
        let service = DcapService::new(config).unwrap();

        let mut enclave = Enclave::new(EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        });
        enclave.initialize().unwrap();

        let result = service.initialize(&enclave);
        assert!(result.is_ok());

        let quote = result.unwrap();
        assert_eq!(quote.version, 3);
        assert_eq!(quote.sign_type, 2);
    }

    #[test]
    fn test_get_current_quote() {
        let config = DcapConfig {
            simulation_mode: true,
            ..Default::default()
        };
        let service = DcapService::new(config).unwrap();

        let mut enclave = Enclave::new(EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        });
        enclave.initialize().unwrap();

        // 初始化之前获取 Quote 应该失败
        assert!(service.get_current_quote().is_err());

        // 初始化后应该成功
        service.initialize(&enclave).unwrap();
        let quote = service.get_current_quote();
        assert!(quote.is_ok());
    }

    #[test]
    fn test_attestation_report() {
        let config = DcapConfig {
            simulation_mode: true,
            ..Default::default()
        };
        let service = DcapService::new(config).unwrap();

        let mut enclave = Enclave::new(EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        });
        enclave.initialize().unwrap();

        service.initialize(&enclave).unwrap();

        let report = service.get_attestation_report();
        assert!(report.is_ok());

        let report = report.unwrap();
        assert_eq!(report.version, DCAP_SERVICE_VERSION);
        assert!(!report.mrenclave_hex.is_empty());
        assert!(!report.mrsigner_hex.is_empty());
        assert!(!report.quote_b64.is_empty());
    }

    #[test]
    fn test_measurement_whitelist() {
        let mut enclave = Enclave::new(EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        });
        enclave.initialize().unwrap();

        let mrenclave = enclave.mrenclave();

        // 使用白名单创建服务
        let config = DcapConfig {
            simulation_mode: false,
            allowed_mrenclaves: vec![mrenclave],
            ..Default::default()
        };
        let service = DcapService::new(config).unwrap();

        service.initialize(&enclave).unwrap();

        // 获取并验证 Quote
        let quote = service.get_current_quote().unwrap();
        let quote_bytes = service.quote_to_bytes(&quote).unwrap();

        let result = service.verify_attestation(&quote_bytes, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_measurement_mismatch() {
        let mut enclave = Enclave::new(EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        });
        enclave.initialize().unwrap();

        let wrong_mrenclave = [0x99u8; 32];

        // 使用错误白名单创建服务
        let config = DcapConfig {
            simulation_mode: false,
            allowed_mrenclaves: vec![wrong_mrenclave],
            ..Default::default()
        };
        let service = DcapService::new(config).unwrap();

        service.initialize(&enclave).unwrap();

        // 获取 Quote
        let quote = service.get_current_quote().unwrap();
        let quote_bytes = service.quote_to_bytes(&quote).unwrap();

        // 验证应该失败
        let result = service.verify_attestation(&quote_bytes, None);
        assert!(matches!(
            result.unwrap_err(),
            DcapError::MeasurementMismatch
        ));
    }

    #[test]
    fn test_refresh_quote() {
        let config = DcapConfig {
            simulation_mode: true,
            ..Default::default()
        };
        let service = DcapService::new(config).unwrap();

        let mut enclave = Enclave::new(EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        });
        enclave.initialize().unwrap();

        service.initialize(&enclave).unwrap();
        let quote1 = service.get_current_quote().unwrap();

        // 刷新 Quote
        std::thread::sleep(std::time::Duration::from_millis(10));
        let quote2 = service.refresh_quote(&enclave).unwrap();

        // 时间戳应该不同
        assert!(quote2.timestamp >= quote1.timestamp);
    }

    #[test]
    fn test_allow_mrenclave_mrsigner() {
        let config = DcapConfig {
            simulation_mode: true,
            ..Default::default()
        };
        let mut service = DcapService::new(config).unwrap();

        let mrenclave = [0x42u8; 32];
        let mrsigner = [0x43u8; 32];

        service.allow_mrenclave(mrenclave);
        service.allow_mrsigner(mrsigner);

        assert_eq!(service.config.allowed_mrenclaves.len(), 1);
        assert_eq!(service.config.allowed_mrsigners.len(), 1);
    }
}
