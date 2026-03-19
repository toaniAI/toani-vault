//! SGX DCAP 远程认证协议实现
//!
//! 实现基于 Intel SGX Data Center Attestation Primitives (DCAP) 的远程认证协议。
//! 支持 ECDSA P-256 Quote 生成和验证，建立安全可信通道。
//!
//! # 架构概述
//!
//! ```text
//! 远程认证流程：
//! 1. Verifier (挑战者) 生成随机挑战
//! 2. Enclave 生成 Quote (包含 REPORT_DATA = hash(challenge + enclave_identity))
//! 3. Verifier 验证 Quote (通过 Intel PCS 或本地 QVE)
//! 4. 验证成功后建立安全通道
//! ```
//!
//! # 安全属性
//!
//! - **身份验证**: 验证 Enclave MRENCLAVE/MRSIGNER
//! - **完整性**: REPORT_DATA 绑定挑战和身份
//! - ** freshness**: 随机挑战防止重放攻击
//! - **不可否认**: ECDSA 签名提供不可否认性

use crate::crypto::CryptoError;
use crate::tee::enclave::{Enclave, EnclaveError};
use ring::digest::{SHA256, digest};
use std::time::{SystemTime, UNIX_EPOCH};

/// SGX Quote 版本
pub const SGX_QUOTE_VERSION: u16 = 3;

/// SGX Quote 类型 (ECDSA P-256 with P-256 curve)
pub const SGX_QUOTE_TYPE_ECDSA_256: u16 = 2;

/// Report Data 长度 (64字节)
pub const SGX_REPORT_DATA_LEN: usize = 64;

/// SGX Measurement (MRENCLAVE/MRSIGNER) 长度
pub const SGX_MEASUREMENT_LEN: usize = 32;

/// ECDSA P-256 签名长度 (r + s)
pub const ECDSA_P256_SIGNATURE_LEN: usize = 64;

/// ECDSA P-256 公钥长度 (uncompressed)
pub const ECDSA_P256_PUBLIC_KEY_LEN: usize = 64;

/// 认证协议版本
pub const ATTESTATION_PROTOCOL_VERSION: u8 = 1;

/// Quote 最大长度
pub const MAX_QUOTE_LEN: usize = 8192;

/// 挑战默认有效期（秒）
pub const CHALLENGE_DEFAULT_TTL: u64 = 300; // 5分钟

/// DCAP 远程认证错误类型
#[derive(Debug)]
pub enum AttestationError {
    /// Quote 生成失败
    QuoteGenerationFailed(String),

    /// Quote 验证失败
    QuoteVerificationFailed(String),

    /// 签名验证失败
    SignatureVerificationFailed,

    /// 挑战验证失败
    ChallengeVerificationFailed,

    /// 测量值不匹配
    MeasurementMismatch,

    /// Quote 已过期
    QuoteExpired,

    /// 无效的 Quote 格式
    InvalidQuoteFormat,

    /// 无效的公钥
    InvalidPublicKey,

    /// 认证超时
    AttestationTimeout,

    /// 内部错误
    InternalError(String),
}

impl std::fmt::Display for AttestationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttestationError::QuoteGenerationFailed(msg) => {
                write!(f, "Quote generation failed: {}", msg)
            }
            AttestationError::QuoteVerificationFailed(msg) => {
                write!(f, "Quote verification failed: {}", msg)
            }
            AttestationError::SignatureVerificationFailed => {
                write!(f, "Signature verification failed")
            }
            AttestationError::ChallengeVerificationFailed => {
                write!(f, "Challenge verification failed")
            }
            AttestationError::MeasurementMismatch => write!(f, "Measurement mismatch"),
            AttestationError::QuoteExpired => write!(f, "Quote expired"),
            AttestationError::InvalidQuoteFormat => write!(f, "Invalid quote format"),
            AttestationError::InvalidPublicKey => write!(f, "Invalid public key"),
            AttestationError::AttestationTimeout => write!(f, "Attestation timeout"),
            AttestationError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for AttestationError {}

impl From<CryptoError> for AttestationError {
    fn from(e: CryptoError) -> Self {
        AttestationError::InternalError(e.to_string())
    }
}

impl From<EnclaveError> for AttestationError {
    fn from(e: EnclaveError) -> Self {
        AttestationError::InternalError(e.to_string())
    }
}

/// SGX Report Data
///
/// 包含用于绑定的用户数据和身份验证信息
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReportData {
    /// 64字节用户数据（通常包含挑战哈希）
    pub data: [u8; SGX_REPORT_DATA_LEN],
}

impl ReportData {
    /// 从挑战和额外数据创建 Report Data
    pub fn from_challenge(challenge: &[u8], enclave_identity: &[u8]) -> Self {
        let mut hasher_input = Vec::with_capacity(challenge.len() + enclave_identity.len());
        hasher_input.extend_from_slice(challenge);
        hasher_input.extend_from_slice(enclave_identity);

        let hash = digest(&SHA256, &hasher_input);

        let mut data = [0u8; SGX_REPORT_DATA_LEN];
        data[..32].copy_from_slice(hash.as_ref());
        // 剩余32字节可用于扩展

        Self { data }
    }

    /// 创建空的 Report Data
    pub fn empty() -> Self {
        Self {
            data: [0u8; SGX_REPORT_DATA_LEN],
        }
    }

    /// 验证 Report Data 是否包含预期的挑战绑定
    pub fn verify_binding(&self, challenge: &[u8], enclave_identity: &[u8]) -> bool {
        let expected = Self::from_challenge(challenge, enclave_identity);
        self.data[..32] == expected.data[..32]
    }
}

/// SGX Quote 结构
///
/// 基于 Intel SGX DCAP Quote 格式 (Version 3)
#[derive(Debug, Clone)]
pub struct Quote {
    /// Quote 版本
    pub version: u16,

    /// Quote 签名类型
    pub sign_type: u16,

    /// EPID Group ID (保留字段，DCAP 不使用)
    pub epid_group_id: u32,

    /// QE SVN (Quoting Enclave Security Version Number)
    pub qe_svn: u16,

    /// PCE SVN (Provisioning Certification Enclave SVN)
    pub pce_svn: u16,

    /// XEID (保留)
    pub xeid: u32,

    /// Basename (保留字段，DCAP 不使用)
    pub basename: [u8; 32],

    /// Report Body (包含 MRENCLAVE, MRSIGNER, REPORT_DATA 等)
    pub report_body: ReportBody,

    /// Quote 签名数据长度
    pub signature_len: u32,

    /// Quote 签名数据
    pub signature: QuoteSignature,

    /// Quote 生成时间戳
    pub timestamp: u64,
}

impl Quote {
    /// 创建新的 Quote (用于模拟/测试)
    pub fn new(report_body: ReportBody, signature: QuoteSignature) -> Self {
        Self {
            version: SGX_QUOTE_VERSION,
            sign_type: SGX_QUOTE_TYPE_ECDSA_256,
            epid_group_id: 0,
            qe_svn: 1,
            pce_svn: 1,
            xeid: 0,
            basename: [0u8; 32],
            report_body,
            signature_len: signature.len() as u32,
            signature,
            timestamp: current_timestamp(),
        }
    }

    /// 序列化 Quote 为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(MAX_QUOTE_LEN);

        // Header
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.sign_type.to_le_bytes());
        bytes.extend_from_slice(&self.epid_group_id.to_le_bytes());
        bytes.extend_from_slice(&self.qe_svn.to_le_bytes());
        bytes.extend_from_slice(&self.pce_svn.to_le_bytes());
        bytes.extend_from_slice(&self.xeid.to_le_bytes());
        bytes.extend_from_slice(&self.basename);

        // Report Body
        bytes.extend_from_slice(&self.report_body.to_bytes());

        // Signature
        bytes.extend_from_slice(&self.signature_len.to_le_bytes());
        bytes.extend_from_slice(&self.signature.to_bytes());

        bytes
    }

    /// 从字节反序列化 Quote
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AttestationError> {
        if bytes.len() < 48 {
            return Err(AttestationError::InvalidQuoteFormat);
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

        // 检查是否有足够的字节读取 Report Body
        if bytes.len() < offset + 384 {
            return Err(AttestationError::InvalidQuoteFormat);
        }

        // Report Body (固定大小 384 字节)
        let report_body = ReportBody::from_bytes(&bytes[offset..offset + 384])
            .map_err(|_| AttestationError::InvalidQuoteFormat)?;
        offset += 384;

        if bytes.len() < offset + 4 {
            return Err(AttestationError::InvalidQuoteFormat);
        }

        let signature_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if bytes.len() < offset + signature_len {
            return Err(AttestationError::InvalidQuoteFormat);
        }

        let signature = QuoteSignature::from_bytes(&bytes[offset..offset + signature_len])
            .map_err(|_| AttestationError::InvalidQuoteFormat)?;

        Ok(Self {
            version,
            sign_type,
            epid_group_id,
            qe_svn,
            pce_svn,
            xeid,
            basename,
            report_body,
            signature_len: signature_len as u32,
            signature,
            timestamp: current_timestamp(),
        })
    }

    /// 获取 MRENCLAVE
    pub fn mrenclave(&self) -> &[u8; SGX_MEASUREMENT_LEN] {
        &self.report_body.mrenclave
    }

    /// 获取 MRSIGNER
    pub fn mrsigner(&self) -> &[u8; SGX_MEASUREMENT_LEN] {
        &self.report_body.mrsigner
    }

    /// 获取 Report Data
    pub fn report_data(&self) -> &ReportData {
        &self.report_body.report_data
    }

    /// 验证 Quote 基本格式
    pub fn validate_format(&self) -> Result<(), AttestationError> {
        if self.version != SGX_QUOTE_VERSION {
            return Err(AttestationError::InvalidQuoteFormat);
        }

        if self.sign_type != SGX_QUOTE_TYPE_ECDSA_256 {
            return Err(AttestationError::InvalidQuoteFormat);
        }

        if self.signature_len == 0 {
            return Err(AttestationError::InvalidQuoteFormat);
        }

        Ok(())
    }
}

/// SGX Report Body
///
/// 包含 Enclave 的身份信息和属性
#[derive(Debug, Clone)]
pub struct ReportBody {
    /// CPU SVN (Security Version Number)
    pub cpusvn: [u8; 16],

    /// Misc Select
    pub miscselect: u32,

    /// 保留字段 (CET attributes + reserved)
    pub reserved1: [u8; 12],

    /// ISV Extended Product ID
    pub isvextprodid: [u8; 16],

    /// ISV Attributes
    pub attributes: [u8; 16],

    /// MRENCLAVE (Enclave 测量值)
    pub mrenclave: [u8; SGX_MEASUREMENT_LEN],

    /// 保留字段
    pub reserved2: [u8; 32],

    /// MRSIGNER (Enclave 签名者测量值)
    pub mrsigner: [u8; SGX_MEASUREMENT_LEN],

    /// 保留字段
    pub reserved3: [u8; 96],

    /// ISV Product ID
    pub isvprodid: u16,

    /// ISV SVN
    pub isvsvn: u16,

    /// 保留字段
    pub reserved4: [u8; 60],

    /// Report Data (用户自定义数据)
    pub report_data: ReportData,
}

impl ReportBody {
    /// 序列化为字节（固定384字节）
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(384);

        bytes.extend_from_slice(&self.cpusvn);
        bytes.extend_from_slice(&self.miscselect.to_le_bytes());
        bytes.extend_from_slice(&self.reserved1);
        bytes.extend_from_slice(&self.isvextprodid);
        bytes.extend_from_slice(&self.attributes);
        bytes.extend_from_slice(&self.mrenclave);
        bytes.extend_from_slice(&self.reserved2);
        bytes.extend_from_slice(&self.mrsigner);
        bytes.extend_from_slice(&self.reserved3);
        bytes.extend_from_slice(&self.isvprodid.to_le_bytes());
        bytes.extend_from_slice(&self.isvsvn.to_le_bytes());
        bytes.extend_from_slice(&self.reserved4);
        bytes.extend_from_slice(&self.report_data.data);

        bytes
    }

    /// 从字节反序列化（固定384字节）
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AttestationError> {
        if bytes.len() != 384 {
            return Err(AttestationError::InvalidQuoteFormat);
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

        let mut report_data_bytes = [0u8; SGX_REPORT_DATA_LEN];
        report_data_bytes.copy_from_slice(&bytes[offset..offset + SGX_REPORT_DATA_LEN]);

        Ok(Self {
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
}

/// Quote 签名数据结构
#[derive(Debug, Clone)]
pub struct QuoteSignature {
    /// ISV Enclave Report 签名
    pub isv_enclave_report_signature: EcdsaSignature,

    /// QE Report
    pub qe_report: Vec<u8>,

    /// QE Report 签名
    pub qe_report_signature: EcdsaSignature,

    /// QE Authentication Data
    pub qe_authentication_data: Vec<u8>,

    /// QE Cert Data (包含证书链或公钥)
    pub qe_certification_data: Vec<u8>,
}

impl QuoteSignature {
    /// 获取签名数据长度
    pub fn len(&self) -> usize {
        64 + // isv_enclave_report_signature
        4 + self.qe_report.len() +
        64 + // qe_report_signature
        4 + self.qe_authentication_data.len() +
        4 + self.qe_certification_data.len()
    }

    /// 检查签名数据是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 序列化为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // ISV Enclave Report Signature
        bytes.extend_from_slice(&self.isv_enclave_report_signature.to_bytes());

        // QE Report length + data
        bytes.extend_from_slice(&(self.qe_report.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.qe_report);

        // QE Report Signature
        bytes.extend_from_slice(&self.qe_report_signature.to_bytes());

        // QE Authentication Data length + data
        bytes.extend_from_slice(&(self.qe_authentication_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.qe_authentication_data);

        // QE Certification Data length + data
        bytes.extend_from_slice(&(self.qe_certification_data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.qe_certification_data);

        bytes
    }

    /// 从字节反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AttestationError> {
        let mut offset = 0;

        // ISV Enclave Report Signature (64 bytes)
        let isv_sig = EcdsaSignature::from_bytes(&bytes[offset..offset + 64])
            .map_err(|_| AttestationError::InvalidQuoteFormat)?;
        offset += 64;

        // QE Report
        if bytes.len() < offset + 4 {
            return Err(AttestationError::InvalidQuoteFormat);
        }
        let qe_report_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if bytes.len() < offset + qe_report_len + 64 {
            return Err(AttestationError::InvalidQuoteFormat);
        }
        let qe_report = bytes[offset..offset + qe_report_len].to_vec();
        offset += qe_report_len;

        // QE Report Signature (64 bytes)
        let qe_sig = EcdsaSignature::from_bytes(&bytes[offset..offset + 64])
            .map_err(|_| AttestationError::InvalidQuoteFormat)?;
        offset += 64;

        // QE Authentication Data
        if bytes.len() < offset + 4 {
            return Err(AttestationError::InvalidQuoteFormat);
        }
        let auth_data_len = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;

        if bytes.len() < offset + auth_data_len + 4 {
            return Err(AttestationError::InvalidQuoteFormat);
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
            return Err(AttestationError::InvalidQuoteFormat);
        }
        let qe_certification_data = bytes[offset..offset + cert_data_len].to_vec();

        Ok(Self {
            isv_enclave_report_signature: isv_sig,
            qe_report,
            qe_report_signature: qe_sig,
            qe_authentication_data,
            qe_certification_data,
        })
    }
}

/// ECDSA P-256 签名
#[derive(Debug, Clone, Copy)]
pub struct EcdsaSignature {
    /// R 值 (32字节)
    pub r: [u8; 32],
    /// S 值 (32字节)
    pub s: [u8; 32],
}

impl EcdsaSignature {
    /// 创建新的签名
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

    /// 从字节反序列化（64字节）
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AttestationError> {
        if bytes.len() != 64 {
            return Err(AttestationError::InvalidQuoteFormat);
        }
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[..32]);
        s.copy_from_slice(&bytes[32..]);
        Ok(Self { r, s })
    }
}

/// ECDSA P-256 公钥
#[derive(Debug, Clone, Copy)]
pub struct EcdsaPublicKey {
    /// X 坐标 (32字节)
    pub x: [u8; 32],
    /// Y 坐标 (32字节)
    pub y: [u8; 32],
}

impl EcdsaPublicKey {
    /// 创建新的公钥
    pub fn new(x: [u8; 32], y: [u8; 32]) -> Self {
        Self { x, y }
    }

    /// 转换为未压缩格式（65字节，包含0x04前缀）
    pub fn to_uncompressed(&self) -> [u8; 65] {
        let mut bytes = [0u8; 65];
        bytes[0] = 0x04; // 未压缩点格式
        bytes[1..33].copy_from_slice(&self.x);
        bytes[33..65].copy_from_slice(&self.y);
        bytes
    }

    /// 从未压缩格式解析
    pub fn from_uncompressed(bytes: &[u8]) -> Result<Self, AttestationError> {
        if bytes.len() != 65 || bytes[0] != 0x04 {
            return Err(AttestationError::InvalidPublicKey);
        }
        let mut x = [0u8; 32];
        let mut y = [0u8; 32];
        x.copy_from_slice(&bytes[1..33]);
        y.copy_from_slice(&bytes[33..65]);
        Ok(Self { x, y })
    }
}

/// DCAP 远程认证服务
///
/// 管理 Quote 的生成和验证
#[derive(Clone)]
pub struct AttestationService {
    /// 验证者公钥（用于验证 Quote 签名）
    verifier_public_key: Option<EcdsaPublicKey>,

    /// 允许的 MRENCLAVE 列表（白名单）
    allowed_mrenclaves: Vec<[u8; SGX_MEASUREMENT_LEN]>,

    /// 允许的 MRSIGNER 列表（白名单）
    allowed_mrsigners: Vec<[u8; SGX_MEASUREMENT_LEN]>,

    /// Quote 最大年龄（秒）
    max_quote_age: u64,

    /// 是否允许模拟模式
    allow_simulation: bool,
}

impl AttestationService {
    /// 创建新的认证服务
    pub fn new() -> Self {
        Self {
            verifier_public_key: None,
            allowed_mrenclaves: Vec::new(),
            allowed_mrsigners: Vec::new(),
            max_quote_age: 3600, // 1小时
            allow_simulation: false,
        }
    }

    /// 设置验证者公钥
    pub fn with_verifier_key(mut self, key: EcdsaPublicKey) -> Self {
        self.verifier_public_key = Some(key);
        self
    }

    /// 添加允许的 MRENCLAVE
    pub fn allow_mrenclave(mut self, mrenclave: [u8; SGX_MEASUREMENT_LEN]) -> Self {
        self.allowed_mrenclaves.push(mrenclave);
        self
    }

    /// 添加允许的 MRSIGNER
    pub fn allow_mrsigner(mut self, mrsigner: [u8; SGX_MEASUREMENT_LEN]) -> Self {
        self.allowed_mrsigners.push(mrsigner);
        self
    }

    /// 设置 Quote 最大年龄
    pub fn with_max_quote_age(mut self, seconds: u64) -> Self {
        self.max_quote_age = seconds;
        self
    }

    /// 允许模拟模式（仅用于开发测试）
    pub fn allow_simulation(mut self, allow: bool) -> Self {
        self.allow_simulation = allow;
        self
    }

    /// 生成 Quote
    ///
    /// # 参数
    /// - `enclave`: Enclave 实例
    /// - `challenge`: 来自验证者的挑战
    ///
    /// # 返回
    /// 包含 Quote 的远程认证结果
    pub fn generate_quote(
        &self,
        enclave: &Enclave,
        challenge: &[u8],
    ) -> Result<Quote, AttestationError> {
        // 确保 Enclave 正在运行
        if !enclave.is_running() {
            return Err(AttestationError::InternalError(
                "Enclave not running".to_string(),
            ));
        }

        // 生成 Report Data
        let enclave_identity = generate_enclave_identity(enclave);
        let report_data = ReportData::from_challenge(challenge, &enclave_identity);

        // 生成 Report Body
        let report_body = generate_report_body(enclave, report_data)?;

        // 生成签名（模拟）
        let signature = generate_simulated_signature(&report_body)?;

        // 构建 Quote
        let quote = Quote::new(report_body, signature);

        Ok(quote)
    }

    /// 验证 Quote
    ///
    /// # 验证步骤
    /// 1. 验证 Quote 格式
    /// 2. 验证签名
    /// 3. 验证测量值在白名单中
    /// 4. 验证挑战绑定
    /// 5. 验证 Quote 未过期
    pub fn verify_quote(
        &self,
        quote: &Quote,
        challenge: &[u8],
        enclave_identity: &[u8],
    ) -> Result<AttestationResult, AttestationError> {
        // 1. 验证格式
        quote.validate_format()?;

        // 2. 验证时间戳
        let now = current_timestamp();
        if now.saturating_sub(quote.timestamp) > self.max_quote_age {
            return Err(AttestationError::QuoteExpired);
        }

        // 3. 验证测量值
        self.verify_measurement(quote)?;

        // 4. 验证挑战绑定
        if !quote
            .report_data()
            .verify_binding(challenge, enclave_identity)
        {
            return Err(AttestationError::ChallengeVerificationFailed);
        }

        // 5. 验证签名（实际实现中验证 ECDSA 签名）
        self.verify_signature(quote)?;

        Ok(AttestationResult {
            success: true,
            mrenclave: *quote.mrenclave(),
            mrsigner: *quote.mrsigner(),
            timestamp: quote.timestamp,
        })
    }

    /// 验证测量值
    fn verify_measurement(&self, quote: &Quote) -> Result<(), AttestationError> {
        // 如果白名单为空，跳过验证（仅用于测试）
        if self.allowed_mrenclaves.is_empty() && self.allowed_mrsigners.is_empty() {
            if !self.allow_simulation {
                return Err(AttestationError::MeasurementMismatch);
            }
            return Ok(());
        }

        // 检查 MRENCLAVE
        if !self.allowed_mrenclaves.is_empty() {
            let mrenclave_match = self
                .allowed_mrenclaves
                .iter()
                .any(|m| m == quote.mrenclave());
            if mrenclave_match {
                return Ok(());
            }
        }

        // 检查 MRSIGNER
        if !self.allowed_mrsigners.is_empty() {
            let mrsigner_match = self.allowed_mrsigners.iter().any(|m| m == quote.mrsigner());
            if mrsigner_match {
                return Ok(());
            }
        }

        Err(AttestationError::MeasurementMismatch)
    }

    /// 验证签名
    ///
    /// 验证逻辑分三层：
    /// 1. 若签名全零：模拟模式允许，非模拟模式拒绝
    /// 2. 若已配置 `verifier_public_key`：执行真实 ECDSA P-256 验证（固定长度 r||s 格式）
    /// 3. 若未配置公钥且签名非零：接受（用于内部模拟 Quote，其中签名由哈希填充）
    ///
    /// 注意：生产环境应始终调用 `with_verifier_key()` 配置 Intel Attestation Key 公钥。
    fn verify_signature(&self, quote: &Quote) -> Result<(), AttestationError> {
        let sig = &quote.signature.isv_enclave_report_signature;

        // 全零签名检查
        let sig_is_zero = sig.r == [0u8; 32] && sig.s == [0u8; 32];

        if sig_is_zero {
            // 全零签名在模拟模式下允许（生成时未设置有效签名）
            if self.allow_simulation {
                return Ok(());
            }
            return Err(AttestationError::SignatureVerificationFailed);
        }

        // 若已配置验证者公钥，执行真实 ECDSA P-256 验证
        if let Some(ref vk) = self.verifier_public_key {
            use ring::signature::{self, UnparsedPublicKey};

            // 构建未压缩公钥格式（0x04 || x || y，共 65 字节）
            let pub_key_bytes = vk.to_uncompressed();

            // 消息为 Report Body 的序列化字节（与签名时相同）
            let message = quote.report_body.to_bytes();

            // 签名为 r || s（64 字节 raw 固定长度格式）
            let sig_bytes = sig.to_bytes();

            let key = UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, &pub_key_bytes);

            key.verify(&message, &sig_bytes)
                .map_err(|_| AttestationError::SignatureVerificationFailed)?;

            return Ok(());
        }

        // 未配置验证者公钥且签名非零：
        // - 模拟模式下允许（内部生成的模拟 Quote 使用 SHA-256 哈希填充签名）
        // - 非模拟模式下拒绝（fail-closed），生产部署必须通过 with_verifier_key() 配置公钥
        if self.allow_simulation {
            return Ok(());
        }
        Err(AttestationError::SignatureVerificationFailed)
    }
}

impl Default for AttestationService {
    fn default() -> Self {
        Self::new()
    }
}

/// 远程认证结果
#[derive(Debug, Clone)]
pub struct AttestationResult {
    /// 认证是否成功
    pub success: bool,

    /// MRENCLAVE 测量值
    pub mrenclave: [u8; SGX_MEASUREMENT_LEN],

    /// MRSIGNER 测量值
    pub mrsigner: [u8; SGX_MEASUREMENT_LEN],

    /// 认证时间戳
    pub timestamp: u64,
}

/// 远程认证会话
///
/// 管理一次完整的远程认证流程
#[derive(Debug)]
pub struct AttestationSession {
    /// 会话 ID
    pub session_id: String,

    /// 挑战数据
    pub challenge: [u8; 32],

    /// 会话状态
    pub state: AttestationState,

    /// 创建时间戳
    pub created_at: u64,

    /// 超时时间戳
    pub expires_at: u64,
}

/// 认证会话状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttestationState {
    /// 已创建挑战
    ChallengeCreated,
    /// 已收到 Quote
    QuoteReceived,
    /// 认证成功
    Verified,
    /// 认证失败
    Failed,
    /// 已过期
    Expired,
}

impl AttestationSession {
    /// 创建新的认证会话
    pub fn new(session_id: String, ttl_seconds: u64) -> Result<Self, AttestationError> {
        let challenge = generate_challenge()?;
        let now = current_timestamp();

        Ok(Self {
            session_id,
            challenge,
            state: AttestationState::ChallengeCreated,
            created_at: now,
            expires_at: now + ttl_seconds,
        })
    }

    /// 检查是否过期
    pub fn is_expired(&self) -> bool {
        current_timestamp() > self.expires_at
    }

    /// 更新状态
    pub fn transition_to(&mut self, new_state: AttestationState) -> Result<(), AttestationError> {
        // 状态机转换验证
        let valid_transition = matches!(
            (self.state, new_state),
            (
                AttestationState::ChallengeCreated,
                AttestationState::QuoteReceived
            ) | (
                AttestationState::ChallengeCreated,
                AttestationState::Expired
            ) | (AttestationState::QuoteReceived, AttestationState::Verified)
                | (AttestationState::QuoteReceived, AttestationState::Failed)
                | (AttestationState::QuoteReceived, AttestationState::Expired)
        );

        if !valid_transition {
            return Err(AttestationError::InternalError(
                "Invalid state transition".to_string(),
            ));
        }

        self.state = new_state;
        Ok(())
    }
}

/// 生成随机挑战
fn generate_challenge() -> Result<[u8; 32], AttestationError> {
    use rand::RngCore;

    let mut challenge = [0u8; 32];
    let mut rng = rand::thread_rng();
    rng.try_fill_bytes(&mut challenge)
        .map_err(|_| AttestationError::InternalError("RNG failed".to_string()))?;

    Ok(challenge)
}

/// 生成 Enclave 身份标识
fn generate_enclave_identity(enclave: &Enclave) -> Vec<u8> {
    let mut identity = Vec::with_capacity(64);
    identity.extend_from_slice(&enclave.mrenclave());
    identity.extend_from_slice(&enclave.mrsigner());
    identity
}

/// Report Body 属性标志
///
/// SGX 属性位定义（位掩码）
/// - 位 0: INIT (1) - Enclave 已初始化
/// - 位 1: DEBUG (2) - 调试模式启用
/// - 位 2-63: 保留
const SGX_ATTRIBUTE_INIT: u64 = 0x01;
const SGX_ATTRIBUTE_DEBUG: u64 = 0x02;

/// 编译时安全检查
///
/// 如果启用了 `strict-production` 特性，在调试构建中会触发编译错误
#[cfg(all(feature = "strict-production", debug_assertions))]
compile_error!("严格生产模式不能在调试构建中启用");

/// 检查是否允许调试模式
///
/// 在发布构建中，只有显式启用 `allow-debug` 特性才允许调试模式
#[inline]
fn is_debug_mode_allowed(enclave_debug_mode: bool) -> bool {
    // 调试构建总是允许调试模式
    if cfg!(debug_assertions) {
        return true;
    }

    // 发布构建需要显式启用 allow-debug 特性
    if cfg!(feature = "allow-debug") {
        return enclave_debug_mode;
    }

    // 默认：生产构建不允许调试模式
    false
}

/// 生成 Report Body（模拟实现）
fn generate_report_body(
    enclave: &Enclave,
    report_data: ReportData,
) -> Result<ReportBody, AttestationError> {
    // 安全修复：DEBUG 标志只能在调试构建或显式允许时启用
    // 生产构建（--release）默认不会设置 DEBUG 位
    let allow_debug = is_debug_mode_allowed(enclave.config().debug_mode);

    let attributes = if allow_debug {
        // 调试模式：允许设置 DEBUG 标志
        // 警告：生产环境必须禁用此功能
        let attr_val = SGX_ATTRIBUTE_INIT | SGX_ATTRIBUTE_DEBUG;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&attr_val.to_le_bytes());
        bytes
    } else {
        // 生产模式：仅设置 INIT 标志
        let attr_val = SGX_ATTRIBUTE_INIT;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&attr_val.to_le_bytes());
        bytes
    };

    Ok(ReportBody {
        cpusvn: [0x01u8; 16],
        miscselect: 0,
        reserved1: [0u8; 12],
        isvextprodid: [0u8; 16],
        attributes,
        mrenclave: enclave.mrenclave(),
        reserved2: [0u8; 32],
        mrsigner: enclave.mrsigner(),
        reserved3: [0u8; 96],
        isvprodid: 1,
        isvsvn: 1,
        reserved4: [0u8; 60],
        report_data,
    })
}

/// 生成模拟签名
fn generate_simulated_signature(
    report_body: &ReportBody,
) -> Result<QuoteSignature, AttestationError> {
    // 在实际实现中，这会调用 SGX 硬件生成真实签名
    // 这里使用哈希值模拟签名
    let report_hash = digest(&SHA256, &report_body.to_bytes());

    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&report_hash.as_ref()[..32]);
    s.copy_from_slice(&report_hash.as_ref()[..32]);

    Ok(QuoteSignature {
        isv_enclave_report_signature: EcdsaSignature::new(r, s),
        qe_report: vec![0u8; 384],
        qe_report_signature: EcdsaSignature::new([1u8; 32], [1u8; 32]),
        qe_authentication_data: vec![0u8; 32],
        qe_certification_data: vec![0u8; 64],
    })
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
    use crate::tee::{Enclave, EnclaveConfig};

    #[test]
    fn test_report_data_binding() {
        let challenge = b"test_challenge_123";
        let identity = b"enclave_identity";

        let report_data = ReportData::from_challenge(challenge, identity);

        // 验证绑定
        assert!(report_data.verify_binding(challenge, identity));

        // 错误挑战应该失败
        assert!(!report_data.verify_binding(b"wrong_challenge", identity));
    }

    #[test]
    fn test_quote_serialization() {
        let report_body = ReportBody {
            cpusvn: [0x01u8; 16],
            miscselect: 0,
            reserved1: [0u8; 12],
            isvextprodid: [0u8; 16],
            attributes: [0u8; 16],
            mrenclave: [0x42u8; 32],
            reserved2: [0u8; 32],
            mrsigner: [0x43u8; 32],
            reserved3: [0u8; 96],
            isvprodid: 1,
            isvsvn: 1,
            reserved4: [0u8; 60],
            report_data: ReportData::empty(),
        };

        let signature = QuoteSignature {
            isv_enclave_report_signature: EcdsaSignature::new([0u8; 32], [0u8; 32]),
            qe_report: vec![0u8; 384],
            qe_report_signature: EcdsaSignature::new([1u8; 32], [1u8; 32]),
            qe_authentication_data: vec![0u8; 32],
            qe_certification_data: vec![0u8; 64],
        };

        let quote = Quote::new(report_body, signature);

        // 序列化和反序列化
        let bytes = quote.to_bytes();
        let report_body_bytes = quote.report_body.to_bytes();
        eprintln!("Serialized bytes length: {}", bytes.len());
        eprintln!("Report body to_bytes length: {}", report_body_bytes.len());
        eprintln!("Quote signature.len(): {}", quote.signature.len());
        eprintln!("Quote signature_len field: {}", quote.signature_len);

        // 验证 signature.len() 和 signature_len 一致
        assert_eq!(
            quote.signature.len() as u32,
            quote.signature_len,
            "signature.len() and signature_len mismatch!"
        );

        // 验证序列化后有足够的字节
        let header_size = 2 + 2 + 4 + 2 + 2 + 4 + 32; // 48 bytes
        let report_body_size = 384;
        let expected_min_size = header_size + report_body_size + 4 + quote.signature.len();
        eprintln!("Expected min size: {}", expected_min_size);
        assert!(
            bytes.len() >= expected_min_size,
            "Serialized bytes too short: {} < {}",
            bytes.len(),
            expected_min_size
        );

        let restored = Quote::from_bytes(&bytes).unwrap();

        assert_eq!(restored.version, quote.version);
        assert_eq!(restored.sign_type, quote.sign_type);
        assert_eq!(restored.report_body.mrenclave, quote.report_body.mrenclave);
        assert_eq!(restored.report_body.mrsigner, quote.report_body.mrsigner);
    }

    #[test]
    fn test_attestation_service_generate_quote() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        let service = AttestationService::new().allow_simulation(true);
        let challenge = b"test_challenge";

        let quote = service.generate_quote(&enclave, challenge).unwrap();

        assert_eq!(quote.version, SGX_QUOTE_VERSION);
        assert_eq!(quote.sign_type, SGX_QUOTE_TYPE_ECDSA_256);
    }

    #[test]
    fn test_attestation_service_verify_quote() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        let service = AttestationService::new().allow_simulation(true);
        let challenge = b"test_challenge";

        let quote = service.generate_quote(&enclave, challenge).unwrap();

        let identity = generate_enclave_identity(&enclave);
        let result = service.verify_quote(&quote, challenge, &identity).unwrap();

        assert!(result.success);
        assert_eq!(result.mrenclave, enclave.mrenclave());
        assert_eq!(result.mrsigner, enclave.mrsigner());
    }

    #[test]
    fn test_attestation_session() {
        let session = AttestationSession::new("session_123".to_string(), 300).unwrap();

        assert_eq!(session.session_id, "session_123");
        assert_eq!(session.state, AttestationState::ChallengeCreated);
        assert!(!session.is_expired());

        // 测试状态转换
        let mut session = session;
        session
            .transition_to(AttestationState::QuoteReceived)
            .unwrap();
        assert_eq!(session.state, AttestationState::QuoteReceived);

        session.transition_to(AttestationState::Verified).unwrap();
        assert_eq!(session.state, AttestationState::Verified);
    }

    #[test]
    fn test_attestation_session_invalid_transition() {
        let mut session = AttestationSession::new("session_123".to_string(), 300).unwrap();

        // 不能直接转到 Verified
        assert!(session.transition_to(AttestationState::Verified).is_err());

        // 不能从 Verified 转回
        session
            .transition_to(AttestationState::QuoteReceived)
            .unwrap();
        session.transition_to(AttestationState::Verified).unwrap();
        assert!(
            session
                .transition_to(AttestationState::QuoteReceived)
                .is_err()
        );
    }

    #[test]
    fn test_ecdsa_signature() {
        let r = [0x42u8; 32];
        let s = [0x43u8; 32];

        let sig = EcdsaSignature::new(r, s);
        let bytes = sig.to_bytes();

        assert_eq!(bytes[..32], r);
        assert_eq!(bytes[32..], s);

        let restored = EcdsaSignature::from_bytes(&bytes).unwrap();
        assert_eq!(restored.r, r);
        assert_eq!(restored.s, s);
    }

    #[test]
    fn test_measurement_whitelist() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        // 创建带白名单的服务（测试使用模拟签名，因此允许模拟模式）
        let service = AttestationService::new()
            .allow_mrenclave(enclave.mrenclave())
            .allow_simulation(true);

        let challenge = b"test_challenge";
        let quote = service.generate_quote(&enclave, challenge).unwrap();

        let identity = generate_enclave_identity(&enclave);
        let result = service.verify_quote(&quote, challenge, &identity);

        assert!(result.is_ok());
    }

    #[test]
    fn test_measurement_mismatch() {
        let config = EnclaveConfig {
            debug_mode: true,
            ..Default::default()
        };

        let mut enclave = Enclave::new(config);
        enclave.initialize().unwrap();

        // 创建带不同白名单的服务
        let wrong_mrenclave = [0x99u8; 32];
        let service = AttestationService::new()
            .allow_mrenclave(wrong_mrenclave)
            .allow_simulation(false);

        let challenge = b"test_challenge";
        let quote = service.generate_quote(&enclave, challenge).unwrap();

        let identity = generate_enclave_identity(&enclave);
        let result = service.verify_quote(&quote, challenge, &identity);

        assert!(matches!(
            result.unwrap_err(),
            AttestationError::MeasurementMismatch
        ));
    }
}
