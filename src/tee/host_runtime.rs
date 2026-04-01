//! Host-side SGX enclave runtime bridge.
//!
//! Hardware mode now loads a normal URTS host bridge shared library, then asks that bridge to
//! create and manage the signed enclave via `sgx_create_enclave`/ECALLs. The signed enclave is
//! never `dlopen`'d directly because that bypasses SGX execution entirely.

use crate::config::TEE_ENCLAVE_PATH_ENV;
use crate::crypto::EncryptedBlob;
#[cfg(target_os = "linux")]
use crate::tee::ffi_types::{
    ENCLAVE_BLOB_BUFFER_LEN, ENCLAVE_PLAINTEXT_BUFFER_LEN, EnclaveReport, EnclaveSealingKey,
    SGX_MEASUREMENT_LEN, SGX_REPORT_LEN,
};
use crate::tee::ffi_types::{
    EcallStatus, EnclaveIdentity, SGX_REPORT_DATA_LEN, SGX_SEALING_KEY_LEN, SGX_TARGET_INFO_LEN,
};
use crate::tee::sealing::SealPolicy;
use std::env;
#[cfg(target_os = "linux")]
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(target_os = "linux")]
use libc::{RTLD_LAZY, dlclose, dlerror, dlopen, dlsym};

const TEE_SGX_HOST_BRIDGE_LIB_PATH_ENV: &str = "TEE_SGX_HOST_BRIDGE_LIB_PATH";
const DEFAULT_HOST_BRIDGE_LIB_NAME: &str = "libcredbridge_sgx_urts_bridge.so";

/// Host/enclave runtime bridge abstraction.
pub trait EnclaveRuntime: Send + Sync {
    fn get_identity(&self) -> Result<EnclaveIdentity, HostRuntimeError>;

    fn get_targeted_report(
        &self,
        target_info: [u8; SGX_TARGET_INFO_LEN],
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError>;

    fn get_report(
        &self,
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError> {
        self.get_targeted_report([0u8; SGX_TARGET_INFO_LEN], report_data)
    }

    fn get_sealing_key(
        &self,
        policy: SealPolicy,
    ) -> Result<[u8; SGX_SEALING_KEY_LEN], HostRuntimeError>;

    fn encrypt_credential(
        &self,
        tenant_id: &str,
        user_id_hash: &str,
        credential_id: &str,
        plaintext: &[u8],
    ) -> Result<EncryptedBlob, HostRuntimeError>;

    fn decrypt_credential(
        &self,
        tenant_id: &str,
        user_id_hash: &str,
        credential_id: &str,
        blob: &EncryptedBlob,
    ) -> Result<Vec<u8>, HostRuntimeError>;
}

pub type SharedEnclaveRuntime = Arc<dyn EnclaveRuntime>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRuntimeError {
    MissingEnclavePath,
    ArtifactMissing {
        path: PathBuf,
    },
    ArtifactNotAFile {
        path: PathBuf,
    },
    BridgeLibraryMissing {
        path: PathBuf,
    },
    LoadFailed {
        path: PathBuf,
        detail: String,
    },
    UnsupportedPlatform {
        operation: &'static str,
    },
    SymbolMissing {
        symbol: String,
        detail: String,
    },
    BridgeCallFailed {
        operation: &'static str,
        detail: String,
    },
    InvalidBufferSize {
        operation: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl HostRuntimeError {
    pub fn is_missing_enclave_path(&self) -> bool {
        matches!(self, Self::MissingEnclavePath)
    }
}

impl std::fmt::Display for HostRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostRuntimeError::MissingEnclavePath => write!(
                f,
                "no real SGX hardware provider configured; set `{TEE_ENCLAVE_PATH_ENV}`"
            ),
            HostRuntimeError::ArtifactMissing { path } => write!(
                f,
                "no real SGX hardware provider configured; enclave artifact not found at `{}`",
                path.display()
            ),
            HostRuntimeError::ArtifactNotAFile { path } => {
                write!(
                    f,
                    "enclave artifact path `{}` is not a file",
                    path.display()
                )
            }
            HostRuntimeError::BridgeLibraryMissing { path } => write!(
                f,
                "URTS host bridge library not found at `{}`; build `target/sgx-enclave/{DEFAULT_HOST_BRIDGE_LIB_NAME}` or set `{TEE_SGX_HOST_BRIDGE_LIB_PATH_ENV}`",
                path.display()
            ),
            HostRuntimeError::LoadFailed { path, detail } => {
                write!(
                    f,
                    "failed to load SGX runtime asset `{}`: {detail}",
                    path.display()
                )
            }
            HostRuntimeError::UnsupportedPlatform { operation } => write!(
                f,
                "host runtime operation `{operation}` is only available on Linux SGX runners"
            ),
            HostRuntimeError::SymbolMissing { symbol, detail } => {
                write!(f, "missing SGX runtime symbol `{symbol}`: {detail}")
            }
            HostRuntimeError::BridgeCallFailed { operation, detail } => {
                write!(f, "SGX runtime operation `{operation}` failed: {detail}")
            }
            HostRuntimeError::InvalidBufferSize {
                operation,
                expected,
                actual,
            } => write!(
                f,
                "SGX runtime operation `{operation}` returned {actual} bytes, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for HostRuntimeError {}

pub struct SgxHostRuntime {
    enclave_path: PathBuf,
    bridge_path: PathBuf,
    debug_mode: bool,
    #[cfg(target_os = "linux")]
    handle: *mut c_void,
    #[cfg(target_os = "linux")]
    runtime_handle: *mut c_void,
}

unsafe impl Send for SgxHostRuntime {}
unsafe impl Sync for SgxHostRuntime {}

impl std::fmt::Debug for SgxHostRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SgxHostRuntime")
            .field("enclave_path", &self.enclave_path)
            .field("bridge_path", &self.bridge_path)
            .field("debug_mode", &self.debug_mode)
            .finish_non_exhaustive()
    }
}

impl Drop for SgxHostRuntime {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        unsafe {
            if !self.runtime_handle.is_null() {
                let close: Result<unsafe extern "C" fn(*mut c_void) -> i32, _> =
                    load_symbol_from(self.handle, "credbridge_sgx_runtime_close");
                if let Ok(close) = close {
                    let _ = close(self.runtime_handle);
                }
                self.runtime_handle = std::ptr::null_mut();
            }

            if !self.handle.is_null() {
                dlclose(self.handle);
            }
        }
    }
}

impl SgxHostRuntime {
    pub fn load_from_env() -> Result<Self, HostRuntimeError> {
        let enclave_path =
            env::var(TEE_ENCLAVE_PATH_ENV).map_err(|_| HostRuntimeError::MissingEnclavePath)?;
        let debug_mode = parse_bool_env("TEE_DEBUG");
        Self::load(enclave_path, debug_mode)
    }

    pub fn load<P: AsRef<Path>>(
        enclave_path: P,
        debug_mode: bool,
    ) -> Result<Self, HostRuntimeError> {
        let path = enclave_path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(HostRuntimeError::MissingEnclavePath);
        }
        if !path.exists() {
            return Err(HostRuntimeError::ArtifactMissing {
                path: path.to_path_buf(),
            });
        }
        if !path.is_file() {
            return Err(HostRuntimeError::ArtifactNotAFile {
                path: path.to_path_buf(),
            });
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = debug_mode;
            Err(HostRuntimeError::UnsupportedPlatform { operation: "load" })
        }

        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStrExt;

            let bridge_path = resolve_bridge_path(path)?;
            if !bridge_path.exists() {
                return Err(HostRuntimeError::BridgeLibraryMissing {
                    path: bridge_path.clone(),
                });
            }
            if !bridge_path.is_file() {
                return Err(HostRuntimeError::LoadFailed {
                    path: bridge_path.clone(),
                    detail: "URTS host bridge path is not a file".to_string(),
                });
            }

            let c_bridge_path = CString::new(bridge_path.as_os_str().as_bytes()).map_err(|_| {
                HostRuntimeError::LoadFailed {
                    path: bridge_path.clone(),
                    detail: "bridge library path contains an interior NUL byte".to_string(),
                }
            })?;
            let handle = unsafe { dlopen(c_bridge_path.as_ptr(), RTLD_LAZY) };
            if handle.is_null() {
                return Err(HostRuntimeError::LoadFailed {
                    path: bridge_path.clone(),
                    detail: last_dlerror(),
                });
            }

            type OpenFn = unsafe extern "C" fn(*const c_char, i32, *mut *mut c_void) -> i32;
            let open: OpenFn = unsafe { load_symbol_from(handle, "credbridge_sgx_runtime_open")? };

            let c_enclave_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                HostRuntimeError::LoadFailed {
                    path: path.to_path_buf(),
                    detail: "enclave artifact path contains an interior NUL byte".to_string(),
                }
            })?;
            let mut runtime_handle = std::ptr::null_mut();
            let status = unsafe {
                open(
                    c_enclave_path.as_ptr(),
                    if debug_mode { 1 } else { 0 },
                    &mut runtime_handle,
                )
            };
            let decoded = EcallStatus::from_raw(status).unwrap_or(EcallStatus::InternalError);
            if !decoded.is_success() || runtime_handle.is_null() {
                let detail = unsafe { bridge_last_error_from(handle) };
                unsafe { dlclose(handle) };
                return Err(HostRuntimeError::LoadFailed {
                    path: path.to_path_buf(),
                    detail: if detail.is_empty() {
                        format!("runtime open returned status {}", decoded)
                    } else {
                        detail
                    },
                });
            }

            if debug_mode {
                tracing::debug!(
                    enclave_path = %path.display(),
                    bridge_path = %bridge_path.display(),
                    "loaded SGX enclave runtime via URTS bridge"
                );
            }

            Ok(Self {
                enclave_path: path.to_path_buf(),
                bridge_path,
                debug_mode,
                handle,
                runtime_handle,
            })
        }
    }

    pub fn enclave_path(&self) -> &Path {
        &self.enclave_path
    }

    pub fn debug_mode(&self) -> bool {
        self.debug_mode
    }

    pub fn get_identity(&self) -> Result<EnclaveIdentity, HostRuntimeError> {
        #[cfg(not(target_os = "linux"))]
        {
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "get_identity",
            })
        }

        #[cfg(target_os = "linux")]
        {
            type GetIdentityFn =
                unsafe extern "C" fn(*mut c_void, *mut u8, usize, *mut u8, usize) -> i32;

            let mut mrenclave = [0u8; SGX_MEASUREMENT_LEN];
            let mut mrsigner = [0u8; SGX_MEASUREMENT_LEN];
            let func: GetIdentityFn =
                unsafe { self.symbol("credbridge_sgx_runtime_get_identity")? };
            let status = unsafe {
                func(
                    self.runtime_handle,
                    mrenclave.as_mut_ptr(),
                    mrenclave.len(),
                    mrsigner.as_mut_ptr(),
                    mrsigner.len(),
                )
            };
            self.ensure_bridge_success("get_identity", status)?;
            Ok(EnclaveIdentity::new(mrenclave, mrsigner))
        }
    }

    pub fn get_report(
        &self,
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError> {
        self.get_targeted_report([0u8; SGX_TARGET_INFO_LEN], report_data)
    }

    pub fn get_targeted_report(
        &self,
        target_info: [u8; SGX_TARGET_INFO_LEN],
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (target_info, report_data);
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "get_targeted_report",
            })
        }

        #[cfg(target_os = "linux")]
        {
            type GetTargetedReportFn = unsafe extern "C" fn(
                *mut c_void,
                *const u8,
                usize,
                *const u8,
                usize,
                *mut u8,
                usize,
                *mut usize,
            ) -> i32;

            let mut report = EnclaveReport::new([0u8; SGX_REPORT_LEN], 0);
            let func: GetTargetedReportFn =
                unsafe { self.symbol("credbridge_sgx_runtime_get_targeted_report")? };
            let status = unsafe {
                func(
                    self.runtime_handle,
                    target_info.as_ptr(),
                    target_info.len(),
                    report_data.as_ptr(),
                    report_data.len(),
                    report.bytes.as_mut_ptr(),
                    report.bytes.len(),
                    &mut report.written_len,
                )
            };

            self.ensure_bridge_success("get_targeted_report", status)?;
            if report.written_len != SGX_REPORT_LEN {
                return Err(HostRuntimeError::InvalidBufferSize {
                    operation: "get_targeted_report",
                    expected: SGX_REPORT_LEN,
                    actual: report.written_len,
                });
            }
            Ok(report.as_slice().to_vec())
        }
    }

    pub fn get_sealing_key(
        &self,
        policy: SealPolicy,
    ) -> Result<[u8; SGX_SEALING_KEY_LEN], HostRuntimeError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = policy;
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "get_sealing_key",
            })
        }

        #[cfg(target_os = "linux")]
        {
            type GetSealingKeyFn =
                unsafe extern "C" fn(*mut c_void, u32, *mut u8, usize, *mut usize) -> i32;

            let mut key = EnclaveSealingKey::new([0u8; SGX_SEALING_KEY_LEN], 0);
            let func: GetSealingKeyFn =
                unsafe { self.symbol("credbridge_sgx_runtime_get_sealing_key")? };
            let status = unsafe {
                func(
                    self.runtime_handle,
                    seal_policy_to_raw(policy),
                    key.bytes.as_mut_ptr(),
                    key.bytes.len(),
                    &mut key.written_len,
                )
            };

            self.ensure_bridge_success("get_sealing_key", status)?;
            if key.written_len != SGX_SEALING_KEY_LEN {
                return Err(HostRuntimeError::InvalidBufferSize {
                    operation: "get_sealing_key",
                    expected: SGX_SEALING_KEY_LEN,
                    actual: key.written_len,
                });
            }
            Ok(key.bytes)
        }
    }

    pub fn encrypt_credential(
        &self,
        tenant_id: &str,
        user_id_hash: &str,
        credential_id: &str,
        plaintext: &[u8],
    ) -> Result<EncryptedBlob, HostRuntimeError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (tenant_id, user_id_hash, credential_id, plaintext);
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "encrypt_credential",
            })
        }

        #[cfg(target_os = "linux")]
        {
            type EncryptCredentialFn = unsafe extern "C" fn(
                *mut c_void,
                *const c_char,
                *const c_char,
                *const c_char,
                *const u8,
                usize,
                *mut u8,
                usize,
                *mut usize,
            ) -> i32;

            let tenant_id = c_string(tenant_id, "tenant_id", "encrypt_credential")?;
            let user_id_hash = c_string(user_id_hash, "user_id_hash", "encrypt_credential")?;
            let credential_id = c_string(credential_id, "credential_id", "encrypt_credential")?;
            let mut blob_json = vec![0u8; ENCLAVE_BLOB_BUFFER_LEN];
            let mut written_len = 0usize;
            let func: EncryptCredentialFn =
                unsafe { self.symbol("credbridge_sgx_runtime_encrypt_credential")? };
            let status = unsafe {
                func(
                    self.runtime_handle,
                    tenant_id.as_ptr(),
                    user_id_hash.as_ptr(),
                    credential_id.as_ptr(),
                    plaintext.as_ptr(),
                    plaintext.len(),
                    blob_json.as_mut_ptr(),
                    blob_json.len(),
                    &mut written_len,
                )
            };
            self.ensure_bridge_success("encrypt_credential", status)?;
            let blob_json = utf8_output(
                "encrypt_credential",
                &blob_json,
                written_len,
                ENCLAVE_BLOB_BUFFER_LEN,
            )?;
            EncryptedBlob::from_json(blob_json).map_err(|error| {
                HostRuntimeError::BridgeCallFailed {
                    operation: "encrypt_credential",
                    detail: format!("invalid encrypted blob payload from runtime: {error}"),
                }
            })
        }
    }

    pub fn decrypt_credential(
        &self,
        tenant_id: &str,
        user_id_hash: &str,
        credential_id: &str,
        blob: &EncryptedBlob,
    ) -> Result<Vec<u8>, HostRuntimeError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (tenant_id, user_id_hash, credential_id, blob);
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "decrypt_credential",
            })
        }

        #[cfg(target_os = "linux")]
        {
            type DecryptCredentialFn = unsafe extern "C" fn(
                *mut c_void,
                *const c_char,
                *const c_char,
                *const c_char,
                *const c_char,
                *mut u8,
                usize,
                *mut usize,
            ) -> i32;

            let tenant_id = c_string(tenant_id, "tenant_id", "decrypt_credential")?;
            let user_id_hash = c_string(user_id_hash, "user_id_hash", "decrypt_credential")?;
            let credential_id = c_string(credential_id, "credential_id", "decrypt_credential")?;
            let blob_json = blob
                .to_json()
                .map_err(|error| HostRuntimeError::BridgeCallFailed {
                    operation: "decrypt_credential",
                    detail: format!("failed to serialize encrypted blob for runtime: {error}"),
                })?;
            let blob_json = c_string(&blob_json, "blob_json", "decrypt_credential")?;
            let mut plaintext = vec![0u8; ENCLAVE_PLAINTEXT_BUFFER_LEN];
            let mut written_len = 0usize;
            let func: DecryptCredentialFn =
                unsafe { self.symbol("credbridge_sgx_runtime_decrypt_credential")? };
            let status = unsafe {
                func(
                    self.runtime_handle,
                    tenant_id.as_ptr(),
                    user_id_hash.as_ptr(),
                    credential_id.as_ptr(),
                    blob_json.as_ptr(),
                    plaintext.as_mut_ptr(),
                    plaintext.len(),
                    &mut written_len,
                )
            };
            self.ensure_bridge_success("decrypt_credential", status)?;
            byte_output(
                "decrypt_credential",
                &plaintext,
                written_len,
                ENCLAVE_PLAINTEXT_BUFFER_LEN,
            )
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn symbol<T: Copy>(&self, name: &str) -> Result<T, HostRuntimeError> {
        unsafe { load_symbol_from(self.handle, name) }
    }

    #[cfg(target_os = "linux")]
    fn ensure_bridge_success(
        &self,
        operation: &'static str,
        status: i32,
    ) -> Result<(), HostRuntimeError> {
        let decoded = EcallStatus::from_raw(status).unwrap_or(EcallStatus::InternalError);
        if decoded.is_success() {
            return Ok(());
        }

        let detail = self.bridge_last_error();
        Err(HostRuntimeError::BridgeCallFailed {
            operation,
            detail: if detail.is_empty() {
                format!("runtime returned status {decoded}")
            } else {
                detail
            },
        })
    }

    #[cfg(target_os = "linux")]
    fn bridge_last_error(&self) -> String {
        unsafe { bridge_last_error_from(self.handle) }
    }
}

impl EnclaveRuntime for SgxHostRuntime {
    fn get_identity(&self) -> Result<EnclaveIdentity, HostRuntimeError> {
        SgxHostRuntime::get_identity(self)
    }

    fn get_targeted_report(
        &self,
        target_info: [u8; SGX_TARGET_INFO_LEN],
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError> {
        SgxHostRuntime::get_targeted_report(self, target_info, report_data)
    }

    fn get_report(
        &self,
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError> {
        SgxHostRuntime::get_report(self, report_data)
    }

    fn get_sealing_key(
        &self,
        policy: SealPolicy,
    ) -> Result<[u8; SGX_SEALING_KEY_LEN], HostRuntimeError> {
        SgxHostRuntime::get_sealing_key(self, policy)
    }

    fn encrypt_credential(
        &self,
        tenant_id: &str,
        user_id_hash: &str,
        credential_id: &str,
        plaintext: &[u8],
    ) -> Result<EncryptedBlob, HostRuntimeError> {
        SgxHostRuntime::encrypt_credential(self, tenant_id, user_id_hash, credential_id, plaintext)
    }

    fn decrypt_credential(
        &self,
        tenant_id: &str,
        user_id_hash: &str,
        credential_id: &str,
        blob: &EncryptedBlob,
    ) -> Result<Vec<u8>, HostRuntimeError> {
        SgxHostRuntime::decrypt_credential(self, tenant_id, user_id_hash, credential_id, blob)
    }
}

#[cfg(target_os = "linux")]
fn resolve_bridge_path(enclave_path: &Path) -> Result<PathBuf, HostRuntimeError> {
    if let Ok(path) = env::var(TEE_SGX_HOST_BRIDGE_LIB_PATH_ENV) {
        let path = PathBuf::from(path);
        if path.as_os_str().is_empty() {
            return Err(HostRuntimeError::LoadFailed {
                path,
                detail: "bridge library path is empty".to_string(),
            });
        }
        return Ok(path);
    }

    let parent = enclave_path
        .parent()
        .ok_or_else(|| HostRuntimeError::LoadFailed {
            path: enclave_path.to_path_buf(),
            detail: "enclave path has no parent directory".to_string(),
        })?;
    Ok(parent.join(DEFAULT_HOST_BRIDGE_LIB_NAME))
}

#[cfg(target_os = "linux")]
unsafe fn load_symbol_from<T: Copy>(
    handle: *mut c_void,
    name: &str,
) -> Result<T, HostRuntimeError> {
    let c_name = CString::new(name).map_err(|_| HostRuntimeError::SymbolMissing {
        symbol: name.to_string(),
        detail: "symbol name contains an interior NUL byte".to_string(),
    })?;
    let symbol = unsafe { dlsym(handle, c_name.as_ptr()) };
    if symbol.is_null() {
        return Err(HostRuntimeError::SymbolMissing {
            symbol: name.to_string(),
            detail: last_dlerror(),
        });
    }

    Ok(unsafe { std::mem::transmute_copy(&symbol) })
}

#[cfg(target_os = "linux")]
unsafe fn bridge_last_error_from(handle: *mut c_void) -> String {
    type LastErrorFn = unsafe extern "C" fn() -> *const c_char;
    let func: Result<LastErrorFn, _> =
        unsafe { load_symbol_from(handle, "credbridge_sgx_runtime_last_error") };
    let Ok(func) = func else {
        return String::new();
    };
    let ptr = unsafe { func() };
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string()
}

#[cfg(target_os = "linux")]
fn c_string(
    value: &str,
    label: &str,
    operation: &'static str,
) -> Result<CString, HostRuntimeError> {
    CString::new(value).map_err(|_| HostRuntimeError::BridgeCallFailed {
        operation,
        detail: format!("`{label}` contains an interior NUL byte"),
    })
}

#[cfg(target_os = "linux")]
fn utf8_output<'a>(
    operation: &'static str,
    bytes: &'a [u8],
    written_len: usize,
    expected_max: usize,
) -> Result<&'a str, HostRuntimeError> {
    if written_len == 0 || written_len > expected_max {
        return Err(HostRuntimeError::InvalidBufferSize {
            operation,
            expected: expected_max,
            actual: written_len,
        });
    }

    std::str::from_utf8(&bytes[..written_len]).map_err(|_| HostRuntimeError::BridgeCallFailed {
        operation,
        detail: "runtime returned invalid UTF-8".to_string(),
    })
}

#[cfg(target_os = "linux")]
fn byte_output(
    operation: &'static str,
    bytes: &[u8],
    written_len: usize,
    expected_max: usize,
) -> Result<Vec<u8>, HostRuntimeError> {
    if written_len > expected_max {
        return Err(HostRuntimeError::InvalidBufferSize {
            operation,
            expected: expected_max,
            actual: written_len,
        });
    }

    Ok(bytes[..written_len].to_vec())
}

fn parse_bool_env(name: &str) -> bool {
    env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

#[cfg(target_os = "linux")]
fn seal_policy_to_raw(policy: SealPolicy) -> u32 {
    match policy {
        SealPolicy::Mrenclave => 0,
        SealPolicy::Mrsigner => 1,
    }
}

#[cfg(target_os = "linux")]
fn last_dlerror() -> String {
    let ptr = unsafe { dlerror() };
    if ptr.is_null() {
        return "unknown dlopen/dlsym error".to_string();
    }

    unsafe { CStr::from_ptr(ptr.cast::<c_char>()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn hardware_runtime_rejects_missing_enclave_path() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            env::remove_var(TEE_ENCLAVE_PATH_ENV);
        }

        let error = SgxHostRuntime::load_from_env().unwrap_err();
        assert!(matches!(error, HostRuntimeError::MissingEnclavePath));
        assert!(error.to_string().contains("TEE_ENCLAVE_PATH"));
    }

    #[test]
    fn hardware_runtime_rejects_nonexistent_enclave_artifact() {
        let _guard = env_lock().lock().unwrap();
        let missing_path = std::env::temp_dir().join(format!(
            "credbridge-missing-enclave-{}",
            uuid::Uuid::new_v4()
        ));

        unsafe {
            env::set_var(TEE_ENCLAVE_PATH_ENV, &missing_path);
        }

        let error = SgxHostRuntime::load_from_env().unwrap_err();
        assert!(matches!(error, HostRuntimeError::ArtifactMissing { .. }));
        assert!(
            error
                .to_string()
                .contains("no real SGX hardware provider configured")
        );

        unsafe {
            env::remove_var(TEE_ENCLAVE_PATH_ENV);
        }
    }
}
