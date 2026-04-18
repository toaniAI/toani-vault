//! SGX host/enclave FFI types.
//!
//! These types stay intentionally small so the host can compile on macOS while the
//! real SGX implementation is built and signed on Linux/TEE runners.

use core::fmt;

pub const SGX_MEASUREMENT_LEN: usize = 32;
pub const SGX_REPORT_DATA_LEN: usize = 64;
pub const SGX_REPORT_LEN: usize = 432;
pub const SGX_TARGET_INFO_LEN: usize = 512;
pub const SGX_SEALING_KEY_LEN: usize = 32;
pub const ENCLAVE_BLOB_BUFFER_LEN: usize = 16 * 1024;
pub const ENCLAVE_PLAINTEXT_BUFFER_LEN: usize = 64 * 1024;

/// Shared ECALL status codes for the host runtime bridge.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EcallStatus {
    Success = 0,
    Unsupported = 1,
    InvalidInput = 2,
    BufferTooSmall = 3,
    InternalError = 4,
}

impl EcallStatus {
    pub fn from_raw(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Success),
            1 => Some(Self::Unsupported),
            2 => Some(Self::InvalidInput),
            3 => Some(Self::BufferTooSmall),
            4 => Some(Self::InternalError),
            _ => None,
        }
    }

    pub fn as_raw(self) -> i32 {
        self as i32
    }

    pub fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }
}

impl fmt::Display for EcallStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            EcallStatus::Success => "success",
            EcallStatus::Unsupported => "unsupported",
            EcallStatus::InvalidInput => "invalid-input",
            EcallStatus::BufferTooSmall => "buffer-too-small",
            EcallStatus::InternalError => "internal-error",
        };
        f.write_str(label)
    }
}

/// MRENCLAVE/MRSIGNER pair returned by the enclave identity ECALL.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnclaveIdentity {
    pub mrenclave: [u8; SGX_MEASUREMENT_LEN],
    pub mrsigner: [u8; SGX_MEASUREMENT_LEN],
}

impl EnclaveIdentity {
    pub fn new(mrenclave: [u8; SGX_MEASUREMENT_LEN], mrsigner: [u8; SGX_MEASUREMENT_LEN]) -> Self {
        Self {
            mrenclave,
            mrsigner,
        }
    }
}

/// SGX report bytes written by the enclave-side report ECALL.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclaveReport {
    pub bytes: [u8; SGX_REPORT_LEN],
    pub written_len: usize,
}

impl EnclaveReport {
    pub fn new(bytes: [u8; SGX_REPORT_LEN], written_len: usize) -> Self {
        Self { bytes, written_len }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.written_len.min(SGX_REPORT_LEN)]
    }
}

/// Raw QE target info bytes passed from the host into enclave report generation.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclaveTargetInfo {
    pub bytes: [u8; SGX_TARGET_INFO_LEN],
}

impl EnclaveTargetInfo {
    pub fn new(bytes: [u8; SGX_TARGET_INFO_LEN]) -> Self {
        Self { bytes }
    }

    pub fn as_bytes(&self) -> &[u8; SGX_TARGET_INFO_LEN] {
        &self.bytes
    }
}

/// SGX sealing key bytes returned by the enclave-side sealing ECALL.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclaveSealingKey {
    pub bytes: [u8; SGX_SEALING_KEY_LEN],
    pub written_len: usize,
}

impl EnclaveSealingKey {
    pub fn new(bytes: [u8; SGX_SEALING_KEY_LEN], written_len: usize) -> Self {
        Self { bytes, written_len }
    }

    pub fn as_bytes(&self) -> &[u8; SGX_SEALING_KEY_LEN] {
        &self.bytes
    }
}
