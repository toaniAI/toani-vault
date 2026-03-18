//! DCAP Quote 结构解析与序列化
//!
//! 提供 Intel SGX DCAP Quote 格式的完整解析和序列化支持。
//! 支持 Quote Version 3 和 ECDSA P-256 签名类型。
//!
//! # Quote 格式结构
//!
//! ```text
//! Quote Header (48 bytes)
//! ├── version           (2 bytes)    - Quote 版本 (3)
//! ├── sign_type         (2 bytes)    - 签名类型 (2 = ECDSA P-256)
//! ├── epid_group_id     (4 bytes)    - EPID Group ID
//! ├── qe_svn            (2 bytes)    - QE Security Version
//! ├── pce_svn           (2 bytes)    - PCE Security Version
//! ├── xeid              (4 bytes)    - XEID
//! └── basename          (32 bytes)   - Basename
//!
//! Report Body (384 bytes)
//! ├── cpusvn            (16 bytes)   - CPU Security Version
//! ├── miscselect        (4 bytes)    - Misc Select
//! ├── reserved1         (12 bytes)   - Reserved
//! ├── isvextprodid      (16 bytes)   - ISV Extended Product ID
//! ├── attributes        (16 bytes)   - Attributes
//! ├── mrenclave         (32 bytes)   - MRENCLAVE
//! ├── reserved2         (32 bytes)   - Reserved
//! ├── mrsigner          (32 bytes)   - MRSIGNER
//! ├── reserved3         (96 bytes)   - Reserved
//! ├── isvprodid         (2 bytes)    - ISV Product ID
//! ├── isvsvn            (2 bytes)    - ISV SVN
//! ├── reserved4         (60 bytes)   - Reserved
//! └── report_data       (64 bytes)   - Report Data
//!
//! Quote Signature (variable)
//! ├── signature_len     (4 bytes)    - 签名数据长度
//! └── signature_data    (variable)   - 签名数据
//! ```
//!
//! # 使用示例
//!
//! ```ignore
//! use vault_service::tee::quote::{QuoteParser, QuoteSerializer, QuoteValidator};
//!
//! // 解析 Quote
//! let quote_bytes = vec![0u8; 1024]; // 从网络或文件读取的 Quote
//! let quote = QuoteParser::parse(&quote_bytes)?;
//!
//! // 访问 Quote 字段
//! println!("MRENCLAVE: {}", hex::encode(quote.report_body.mrenclave));
//! println!("MRSIGNER: {}", hex::encode(quote.report_body.mrsigner));
//!
//! // 验证 Quote
//! let validator = QuoteValidator::new();
//! validator.validate(&quote)?;
//!
//! // 序列化 Quote
//! let serialized = QuoteSerializer::serialize(&quote)?;
//! ```

use crate::tee::attestation::{ReportData, SGX_MEASUREMENT_LEN, SGX_REPORT_DATA_LEN};
use crate::tee::dcap::{DcapError, DcapQuote, DcapQuoteSignature, DcapReportBody};

/// Quote 解析器
pub struct QuoteParser;

/// Quote 序列化器
pub struct QuoteSerializer;

/// Quote 验证器
pub struct QuoteValidator {
    /// 期望的 Quote 版本
    expected_version: u16,

    /// 期望的签名类型
    expected_sign_type: u16,
}

/// Quote 解析结果
#[derive(Debug, Clone)]
pub struct ParsedQuote {
    /// Quote 版本
    pub version: u16,

    /// 签名类型
    pub sign_type: u16,

    /// Report Body
    pub report_body: ParsedReportBody,

    /// 原始字节（可选）
    pub raw_bytes: Option<Vec<u8>>,
}

/// 解析的 Report Body
#[derive(Debug, Clone)]
pub struct ParsedReportBody {
    /// CPU SVN
    pub cpusvn: [u8; 16],

    /// MRENCLAVE
    pub mrenclave: [u8; SGX_MEASUREMENT_LEN],

    /// MRSIGNER
    pub mrsigner: [u8; SGX_MEASUREMENT_LEN],

    /// ISV Product ID
    pub isvprodid: u16,

    /// ISV SVN
    pub isvsvn: u16,

    /// Attributes
    pub attributes: [u8; 16],

    /// Report Data
    pub report_data: ReportData,
}

/// Quote 元数据
#[derive(Debug, Clone)]
pub struct QuoteMetadata {
    /// Quote 大小（字节）
    pub size: usize,

    /// 版本
    pub version: u16,

    /// 签名类型
    pub sign_type: u16,

    /// 是否包含签名
    pub has_signature: bool,

    /// 签名数据大小
    pub signature_size: usize,
}

impl QuoteParser {
    /// 创建新的解析器
    pub fn new() -> Self {
        Self
    }

    /// 解析 Quote 字节
    ///
    /// # 参数
    /// - `bytes`: Quote 原始字节
    ///
    /// # 返回
    /// 解析后的 Quote 结构
    pub fn parse(bytes: &[u8]) -> Result<ParsedQuote, QuoteParseError> {
        if bytes.len() < 432 {
            return Err(QuoteParseError::InsufficientData {
                expected: 432,
                actual: bytes.len(),
            });
        }

        let mut offset = 0;

        // 解析 Header
        let version = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        let sign_type = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        // 跳过 EPID Group ID, QE SVN, PCE SVN, XEID
        offset += 12;

        // 跳过 Basename
        offset += 32;

        // 解析 Report Body (384 bytes)
        let report_body = Self::parse_report_body(&bytes[offset..offset + 384])?;

        Ok(ParsedQuote {
            version,
            sign_type,
            report_body,
            raw_bytes: Some(bytes.to_vec()),
        })
    }

    /// 快速解析 MRENCLAVE
    ///
    /// 不解析整个 Quote，仅提取 MRENCLAVE
    pub fn extract_mrenclave(bytes: &[u8]) -> Result<[u8; SGX_MEASUREMENT_LEN], QuoteParseError> {
        if bytes.len() < 112 {
            return Err(QuoteParseError::InsufficientData {
                expected: 112,
                actual: bytes.len(),
            });
        }

        // MRENCLAVE 在 Report Body 的偏移 64 位置
        // Report Body 起始于 Quote 偏移 48 位置
        // 所以 MRENCLAVE 起始于 Quote 偏移 112
        let mut mrenclave = [0u8; SGX_MEASUREMENT_LEN];
        mrenclave.copy_from_slice(&bytes[112..144]);

        Ok(mrenclave)
    }

    /// 快速解析 MRSIGNER
    ///
    /// 不解析整个 Quote，仅提取 MRSIGNER
    pub fn extract_mrsigner(bytes: &[u8]) -> Result<[u8; SGX_MEASUREMENT_LEN], QuoteParseError> {
        if bytes.len() < 176 {
            return Err(QuoteParseError::InsufficientData {
                expected: 176,
                actual: bytes.len(),
            });
        }

        // MRSIGNER 在 Report Body 的偏移 128 位置
        // Report Body 起始于 Quote 偏移 48 位置
        // 所以 MRSIGNER 起始于 Quote 偏移 176
        let mut mrsigner = [0u8; SGX_MEASUREMENT_LEN];
        mrsigner.copy_from_slice(&bytes[176..208]);

        Ok(mrsigner)
    }

    /// 解析 Report Body
    fn parse_report_body(bytes: &[u8]) -> Result<ParsedReportBody, QuoteParseError> {
        if bytes.len() != 384 {
            return Err(QuoteParseError::InvalidReportBodySize(bytes.len()));
        }

        let mut offset = 0;

        let mut cpusvn = [0u8; 16];
        cpusvn.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;

        // 跳过 miscselect (4 bytes)
        offset += 4;

        // 跳过 reserved1 (12 bytes)
        offset += 12;

        // 跳过 isvextprodid (16 bytes)
        offset += 16;

        // 跳过 attributes (16 bytes)
        let mut attributes = [0u8; 16];
        attributes.copy_from_slice(&bytes[offset..offset + 16]);
        offset += 16;

        // MRENCLAVE (32 bytes)
        let mut mrenclave = [0u8; SGX_MEASUREMENT_LEN];
        mrenclave.copy_from_slice(&bytes[offset..offset + SGX_MEASUREMENT_LEN]);
        offset += SGX_MEASUREMENT_LEN;

        // 跳过 reserved2 (32 bytes)
        offset += 32;

        // MRSIGNER (32 bytes)
        let mut mrsigner = [0u8; SGX_MEASUREMENT_LEN];
        mrsigner.copy_from_slice(&bytes[offset..offset + SGX_MEASUREMENT_LEN]);
        offset += SGX_MEASUREMENT_LEN;

        // 跳过 reserved3 (96 bytes)
        offset += 96;

        // ISV Product ID (2 bytes)
        let isvprodid = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        // ISV SVN (2 bytes)
        let isvsvn = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        // 跳过 reserved4 (60 bytes)
        offset += 60;

        // Report Data (64 bytes)
        let mut report_data_bytes = [0u8; SGX_REPORT_DATA_LEN];
        report_data_bytes.copy_from_slice(&bytes[offset..offset + SGX_REPORT_DATA_LEN]);

        Ok(ParsedReportBody {
            cpusvn,
            mrenclave,
            mrsigner,
            isvprodid,
            isvsvn,
            attributes,
            report_data: ReportData {
                data: report_data_bytes,
            },
        })
    }

    /// 解析 Quote 元数据
    pub fn parse_metadata(bytes: &[u8]) -> Result<QuoteMetadata, QuoteParseError> {
        if bytes.len() < 48 {
            return Err(QuoteParseError::InsufficientData {
                expected: 48,
                actual: bytes.len(),
            });
        }

        let version = u16::from_le_bytes([bytes[0], bytes[1]]);
        let sign_type = u16::from_le_bytes([bytes[2], bytes[3]]);

        let has_signature = bytes.len() > 432;
        let signature_size = if has_signature {
            bytes.len().saturating_sub(432)
        } else {
            0
        };

        Ok(QuoteMetadata {
            size: bytes.len(),
            version,
            sign_type,
            has_signature,
            signature_size,
        })
    }
}

impl Default for QuoteParser {
    fn default() -> Self {
        Self::new()
    }
}

impl QuoteSerializer {
    /// 创建新的序列化器
    pub fn new() -> Self {
        Self
    }

    /// 序列化 Quote 为字节
    pub fn serialize(quote: &DcapQuote) -> Result<Vec<u8>, QuoteSerializeError> {
        let mut bytes = Vec::with_capacity(1024);

        // Header (48 bytes)
        bytes.extend_from_slice(&quote.version.to_le_bytes());
        bytes.extend_from_slice(&quote.sign_type.to_le_bytes());
        bytes.extend_from_slice(&quote.epid_group_id.to_le_bytes());
        bytes.extend_from_slice(&quote.qe_svn.to_le_bytes());
        bytes.extend_from_slice(&quote.pce_svn.to_le_bytes());
        bytes.extend_from_slice(&quote.xeid.to_le_bytes());
        bytes.extend_from_slice(&quote.basename);

        // Report Body (384 bytes)
        bytes.extend_from_slice(&Self::serialize_report_body(&quote.report_body)?);

        // Signature
        bytes.extend_from_slice(&quote.signature_len.to_le_bytes());
        bytes.extend_from_slice(&Self::serialize_signature(&quote.signature)?);

        Ok(bytes)
    }

    /// 序列化 Report Body
    fn serialize_report_body(body: &DcapReportBody) -> Result<Vec<u8>, QuoteSerializeError> {
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

    /// 序列化签名
    fn serialize_signature(sig: &DcapQuoteSignature) -> Result<Vec<u8>, QuoteSerializeError> {
        let mut bytes = Vec::new();

        // ISV Enclave Report Signature (64 bytes)
        bytes.extend_from_slice(&sig.isv_enclave_report_signature.to_bytes());

        // QE Report
        bytes.extend_from_slice(&(sig.qe_report.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&sig.qe_report);

        // QE Report Signature (64 bytes)
        bytes.extend_from_slice(&sig.qe_report_signature.to_bytes());

        // QE Authentication Data
        bytes.extend_from_slice(&(sig.qe_authentication_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&sig.qe_authentication_data);

        // QE Certification Data
        bytes.extend_from_slice(&(sig.qe_certification_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&sig.qe_certification_data);

        Ok(bytes)
    }

    /// 序列化 Quote 为十六进制字符串
    pub fn serialize_to_hex(quote: &DcapQuote) -> Result<String, QuoteSerializeError> {
        let bytes = Self::serialize(quote)?;
        Ok(hex::encode(&bytes))
    }

    /// 序列化 Quote 为 Base64 字符串
    pub fn serialize_to_base64(quote: &DcapQuote) -> Result<String, QuoteSerializeError> {
        let bytes = Self::serialize(quote)?;
        use base64::{Engine, engine::general_purpose::STANDARD};
        Ok(STANDARD.encode(&bytes))
    }
}

impl Default for QuoteSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl QuoteValidator {
    /// 创建新的验证器
    pub fn new() -> Self {
        Self {
            expected_version: 3,
            expected_sign_type: 2, // ECDSA P-256
        }
    }

    /// 设置期望的版本
    pub fn with_expected_version(mut self, version: u16) -> Self {
        self.expected_version = version;
        self
    }

    /// 设置期望的签名类型
    pub fn with_expected_sign_type(mut self, sign_type: u16) -> Self {
        self.expected_sign_type = sign_type;
        self
    }

    /// 验证 Quote
    pub fn validate(&self, quote: &ParsedQuote) -> Result<(), QuoteValidationError> {
        // 验证版本
        if quote.version != self.expected_version {
            return Err(QuoteValidationError::VersionMismatch {
                expected: self.expected_version,
                actual: quote.version,
            });
        }

        // 验证签名类型
        if quote.sign_type != self.expected_sign_type {
            return Err(QuoteValidationError::SignTypeMismatch {
                expected: self.expected_sign_type,
                actual: quote.sign_type,
            });
        }

        // 验证 MRENCLAVE 不为空
        if quote.report_body.mrenclave == [0u8; SGX_MEASUREMENT_LEN] {
            return Err(QuoteValidationError::InvalidMrenclave);
        }

        // 验证 MRSIGNER 不为空
        if quote.report_body.mrsigner == [0u8; SGX_MEASUREMENT_LEN] {
            return Err(QuoteValidationError::InvalidMrsigner);
        }

        Ok(())
    }

    /// 验证 DCAP Quote
    pub fn validate_dcap(&self, quote: &DcapQuote) -> Result<(), QuoteValidationError> {
        // 验证版本
        if quote.version != self.expected_version {
            return Err(QuoteValidationError::VersionMismatch {
                expected: self.expected_version,
                actual: quote.version,
            });
        }

        // 验证签名类型
        if quote.sign_type != self.expected_sign_type {
            return Err(QuoteValidationError::SignTypeMismatch {
                expected: self.expected_sign_type,
                actual: quote.sign_type,
            });
        }

        // 验证 Report Body
        if quote.report_body.mrenclave == [0u8; SGX_MEASUREMENT_LEN] {
            return Err(QuoteValidationError::InvalidMrenclave);
        }

        if quote.report_body.mrsigner == [0u8; SGX_MEASUREMENT_LEN] {
            return Err(QuoteValidationError::InvalidMrsigner);
        }

        // 验证签名长度
        if quote.signature_len == 0 {
            return Err(QuoteValidationError::MissingSignature);
        }

        Ok(())
    }

    /// 验证 Quote 字节
    pub fn validate_bytes(&self, bytes: &[u8]) -> Result<(), QuoteValidationError> {
        let quote = QuoteParser::parse(bytes)
            .map_err(|e| QuoteValidationError::ParseError(e.to_string()))?;
        self.validate(&quote)
    }
}

impl Default for QuoteValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Quote 解析错误
#[derive(Debug)]
pub enum QuoteParseError {
    /// 数据不足
    InsufficientData { expected: usize, actual: usize },

    /// 无效的 Report Body 大小
    InvalidReportBodySize(usize),

    /// 无效的版本
    InvalidVersion(u16),

    /// 无效的签名类型
    InvalidSignType(u16),

    /// 不支持的格式
    UnsupportedFormat(String),
}

impl std::fmt::Display for QuoteParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuoteParseError::InsufficientData { expected, actual } => {
                write!(
                    f,
                    "Insufficient data: expected {} bytes, got {}",
                    expected, actual
                )
            }
            QuoteParseError::InvalidReportBodySize(size) => {
                write!(f, "Invalid report body size: {} bytes (expected 384)", size)
            }
            QuoteParseError::InvalidVersion(version) => {
                write!(f, "Invalid quote version: {}", version)
            }
            QuoteParseError::InvalidSignType(sign_type) => {
                write!(f, "Invalid sign type: {}", sign_type)
            }
            QuoteParseError::UnsupportedFormat(format) => {
                write!(f, "Unsupported quote format: {}", format)
            }
        }
    }
}

impl std::error::Error for QuoteParseError {}

impl From<QuoteParseError> for DcapError {
    fn from(_e: QuoteParseError) -> Self {
        DcapError::InvalidQuoteFormat
    }
}

/// Quote 序列化错误
#[derive(Debug)]
pub enum QuoteSerializeError {
    /// 序列化失败
    SerializationFailed(String),

    /// 缓冲区溢出
    BufferOverflow,

    /// 无效的 Quote 数据
    InvalidQuoteData(String),
}

impl std::fmt::Display for QuoteSerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuoteSerializeError::SerializationFailed(msg) => {
                write!(f, "Serialization failed: {}", msg)
            }
            QuoteSerializeError::BufferOverflow => {
                write!(f, "Buffer overflow during serialization")
            }
            QuoteSerializeError::InvalidQuoteData(msg) => {
                write!(f, "Invalid quote data: {}", msg)
            }
        }
    }
}

impl std::error::Error for QuoteSerializeError {}

/// Quote 验证错误
#[derive(Debug)]
pub enum QuoteValidationError {
    /// 版本不匹配
    VersionMismatch { expected: u16, actual: u16 },

    /// 签名类型不匹配
    SignTypeMismatch { expected: u16, actual: u16 },

    /// 无效的 MRENCLAVE
    InvalidMrenclave,

    /// 无效的 MRSIGNER
    InvalidMrsigner,

    /// 缺少签名
    MissingSignature,

    /// 解析错误
    ParseError(String),

    /// 验证失败
    ValidationFailed(String),
}

impl std::fmt::Display for QuoteValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuoteValidationError::VersionMismatch { expected, actual } => {
                write!(f, "Version mismatch: expected {}, got {}", expected, actual)
            }
            QuoteValidationError::SignTypeMismatch { expected, actual } => {
                write!(
                    f,
                    "Sign type mismatch: expected {}, got {}",
                    expected, actual
                )
            }
            QuoteValidationError::InvalidMrenclave => {
                write!(f, "Invalid MRENCLAVE (all zeros)")
            }
            QuoteValidationError::InvalidMrsigner => {
                write!(f, "Invalid MRSIGNER (all zeros)")
            }
            QuoteValidationError::MissingSignature => {
                write!(f, "Missing quote signature")
            }
            QuoteValidationError::ParseError(msg) => {
                write!(f, "Parse error: {}", msg)
            }
            QuoteValidationError::ValidationFailed(msg) => {
                write!(f, "Validation failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for QuoteValidationError {}

impl From<QuoteValidationError> for DcapError {
    fn from(e: QuoteValidationError) -> Self {
        DcapError::QuoteVerificationFailed(e.to_string())
    }
}

/// Quote 工具函数
pub mod utils {
    use super::*;

    /// 格式化 MRENCLAVE 为可读字符串
    pub fn format_mrenclave(mrenclave: &[u8; SGX_MEASUREMENT_LEN]) -> String {
        format!(
            "{}...{}",
            hex::encode(&mrenclave[..8]),
            hex::encode(&mrenclave[24..])
        )
    }

    /// 格式化 MRSIGNER 为可读字符串
    pub fn format_mrsigner(mrsigner: &[u8; SGX_MEASUREMENT_LEN]) -> String {
        format!(
            "{}...{}",
            hex::encode(&mrsigner[..8]),
            hex::encode(&mrsigner[24..])
        )
    }

    /// 计算 Quote 哈希
    pub fn compute_quote_hash(quote_bytes: &[u8]) -> [u8; 32] {
        use ring::digest::{SHA256, digest};
        let hash = digest(&SHA256, quote_bytes);
        let mut result = [0u8; 32];
        result.copy_from_slice(hash.as_ref());
        result
    }

    /// 比较两个 MRENCLAVE
    pub fn mrenclave_eq(a: &[u8; SGX_MEASUREMENT_LEN], b: &[u8; SGX_MEASUREMENT_LEN]) -> bool {
        a == b
    }

    /// 比较两个 MRSIGNER
    pub fn mrsigner_eq(a: &[u8; SGX_MEASUREMENT_LEN], b: &[u8; SGX_MEASUREMENT_LEN]) -> bool {
        a == b
    }

    /// 创建空的 MRENCLAVE
    pub fn empty_mrenclave() -> [u8; SGX_MEASUREMENT_LEN] {
        [0u8; SGX_MEASUREMENT_LEN]
    }

    /// 创建空的 MRSIGNER
    pub fn empty_mrsigner() -> [u8; SGX_MEASUREMENT_LEN] {
        [0u8; SGX_MEASUREMENT_LEN]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tee::dcap::{DcapConfig, DcapService};
    use crate::tee::{Enclave, EnclaveConfig};

    fn create_test_quote() -> DcapQuote {
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

        service.initialize(&enclave).unwrap()
    }

    #[test]
    fn test_quote_parser_parse() {
        let quote = create_test_quote();
        let bytes = QuoteSerializer::serialize(&quote).unwrap();

        let parsed = QuoteParser::parse(&bytes);
        assert!(parsed.is_ok());

        let parsed = parsed.unwrap();
        assert_eq!(parsed.version, 3);
        assert_eq!(parsed.sign_type, 2);
    }

    #[test]
    fn test_quote_parser_extract_mrenclave() {
        let quote = create_test_quote();
        let bytes = QuoteSerializer::serialize(&quote).unwrap();

        let mrenclave = QuoteParser::extract_mrenclave(&bytes).unwrap();
        assert_eq!(mrenclave, quote.report_body.mrenclave);
    }

    #[test]
    fn test_quote_parser_extract_mrsigner() {
        let quote = create_test_quote();
        let bytes = QuoteSerializer::serialize(&quote).unwrap();

        let mrsigner = QuoteParser::extract_mrsigner(&bytes).unwrap();
        assert_eq!(mrsigner, quote.report_body.mrsigner);
    }

    #[test]
    fn test_quote_parser_parse_metadata() {
        let quote = create_test_quote();
        let bytes = QuoteSerializer::serialize(&quote).unwrap();

        let metadata = QuoteParser::parse_metadata(&bytes).unwrap();
        assert_eq!(metadata.version, 3);
        assert_eq!(metadata.sign_type, 2);
        assert!(metadata.has_signature);
        assert!(metadata.signature_size > 0);
    }

    #[test]
    fn test_quote_parser_insufficient_data() {
        let result = QuoteParser::parse(&[0u8; 100]);
        assert!(matches!(
            result.unwrap_err(),
            QuoteParseError::InsufficientData { .. }
        ));
    }

    #[test]
    fn test_quote_validator_validate() {
        let quote = create_test_quote();
        let bytes = QuoteSerializer::serialize(&quote).unwrap();

        let parsed = QuoteParser::parse(&bytes).unwrap();

        let validator = QuoteValidator::new();
        let result = validator.validate(&parsed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quote_validator_validate_dcap() {
        let quote = create_test_quote();

        let validator = QuoteValidator::new();
        let result = validator.validate_dcap(&quote);
        assert!(result.is_ok());
    }

    #[test]
    fn test_quote_validator_version_mismatch() {
        let mut quote = create_test_quote();
        quote.version = 99; // 无效版本

        let validator = QuoteValidator::new();
        let result = validator.validate_dcap(&quote);
        assert!(matches!(
            result.unwrap_err(),
            QuoteValidationError::VersionMismatch { .. }
        ));
    }

    #[test]
    fn test_quote_validator_invalid_mrenclave() {
        let mut quote = create_test_quote();
        quote.report_body.mrenclave = [0u8; 32]; // 无效的 MRENCLAVE

        let validator = QuoteValidator::new();
        let result = validator.validate_dcap(&quote);
        assert!(matches!(
            result.unwrap_err(),
            QuoteValidationError::InvalidMrenclave
        ));
    }

    #[test]
    fn test_quote_serializer_roundtrip() {
        let quote = create_test_quote();

        let bytes = QuoteSerializer::serialize(&quote).unwrap();
        let parsed = QuoteParser::parse(&bytes).unwrap();

        assert_eq!(parsed.version, quote.version);
        assert_eq!(parsed.sign_type, quote.sign_type);
        assert_eq!(parsed.report_body.mrenclave, quote.report_body.mrenclave);
        assert_eq!(parsed.report_body.mrsigner, quote.report_body.mrsigner);
    }

    #[test]
    fn test_utils_format_mrenclave() {
        let mrenclave = [0x42u8; 32];
        let formatted = utils::format_mrenclave(&mrenclave);
        assert!(formatted.starts_with("42424242"));
        assert!(formatted.ends_with("42424242"));
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_utils_compute_quote_hash() {
        let quote = create_test_quote();
        let bytes = QuoteSerializer::serialize(&quote).unwrap();

        let hash1 = utils::compute_quote_hash(&bytes);
        let hash2 = utils::compute_quote_hash(&bytes);

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, [0u8; 32]);
    }

    #[test]
    fn test_utils_mrenclave_eq() {
        let m1 = [0x42u8; 32];
        let m2 = [0x42u8; 32];
        let m3 = [0x43u8; 32];

        assert!(utils::mrenclave_eq(&m1, &m2));
        assert!(!utils::mrenclave_eq(&m1, &m3));
    }

    #[test]
    fn test_empty_mrenclave_mrsigner() {
        let empty_mrenclave = utils::empty_mrenclave();
        let empty_mrsigner = utils::empty_mrsigner();

        assert_eq!(empty_mrenclave, [0u8; 32]);
        assert_eq!(empty_mrsigner, [0u8; 32]);
    }
}
