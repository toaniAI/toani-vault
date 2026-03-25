//! Host-side SGX enclave runtime bridge.
//!
//! Phase A keeps this bridge intentionally small: validate that a signed enclave artifact
//! exists, load it on Linux SGX runners, and provide typed ECALL wrappers for identity,
//! report generation, and sealing-key retrieval.

use crate::config::TEE_ENCLAVE_PATH_ENV;
use crate::crypto::EncryptedBlob;
#[cfg(target_os = "linux")]
use crate::tee::ffi_types::{
    ENCLAVE_BLOB_BUFFER_LEN, ENCLAVE_PLAINTEXT_BUFFER_LEN, EnclaveReport, EnclaveSealingKey,
    SGX_MEASUREMENT_LEN, SGX_REPORT_LEN,
};
use crate::tee::ffi_types::{
    EcallStatus, EnclaveIdentity, SGX_REPORT_DATA_LEN, SGX_SEALING_KEY_LEN,
};
use crate::tee::sealing::SealPolicy;
use std::env;
#[cfg(target_os = "linux")]
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(target_os = "linux")]
use libc::{RTLD_LAZY, dlclose, dlerror, dlopen, dlsym};

/// Host/enclave runtime bridge abstraction.
///
/// The concrete SGX loader implements this trait, and tests can inject a mock runtime.
pub trait EnclaveRuntime: Send + Sync {
    fn get_identity(&self) -> Result<EnclaveIdentity, HostRuntimeError>;

    fn get_report(
        &self,
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError>;

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

/// Shared runtime handle used by enclave/provisioning/sealing code.
pub type SharedEnclaveRuntime = Arc<dyn EnclaveRuntime>;

/// Load-time and call-time failures from the host runtime bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRuntimeError {
    MissingEnclavePath,
    ArtifactMissing {
        path: PathBuf,
    },
    ArtifactNotAFile {
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
    EcallReturned {
        operation: &'static str,
        status: EcallStatus,
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
            HostRuntimeError::ArtifactNotAFile { path } => write!(
                f,
                "enclave artifact path `{}` is not a file",
                path.display()
            ),
            HostRuntimeError::LoadFailed { path, detail } => write!(
                f,
                "failed to load enclave artifact `{}`: {detail}",
                path.display()
            ),
            HostRuntimeError::UnsupportedPlatform { operation } => write!(
                f,
                "host runtime operation `{operation}` is only available on Linux SGX runners"
            ),
            HostRuntimeError::SymbolMissing { symbol, detail } => {
                write!(f, "missing enclave symbol `{symbol}`: {detail}")
            }
            HostRuntimeError::EcallReturned { operation, status } => {
                write!(f, "enclave ECALL `{operation}` returned `{status}`")
            }
            HostRuntimeError::InvalidBufferSize {
                operation,
                expected,
                actual,
            } => write!(
                f,
                "enclave ECALL `{operation}` returned {actual} bytes, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for HostRuntimeError {}

/// Runtime handle for a loaded SGX enclave artifact.
pub struct SgxHostRuntime {
    enclave_path: PathBuf,
    debug_mode: bool,
    #[cfg(target_os = "linux")]
    handle: *mut c_void,
}

unsafe impl Send for SgxHostRuntime {}
unsafe impl Sync for SgxHostRuntime {}

impl std::fmt::Debug for SgxHostRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SgxHostRuntime")
            .field("enclave_path", &self.enclave_path)
            .field("debug_mode", &self.debug_mode)
            .finish_non_exhaustive()
    }
}

impl Drop for SgxHostRuntime {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if !self.handle.is_null() {
            // SAFETY: handle was returned by dlopen in load().
            unsafe {
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

            let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                HostRuntimeError::LoadFailed {
                    path: path.to_path_buf(),
                    detail: "enclave artifact path contains an interior NUL byte".to_string(),
                }
            })?;
            // SAFETY: dlopen/dlerror are used with a validated path and the returned handle
            // is owned by the runtime until Drop.
            let handle = unsafe { dlopen(c_path.as_ptr(), RTLD_LAZY) };
            if handle.is_null() {
                return Err(HostRuntimeError::LoadFailed {
                    path: path.to_path_buf(),
                    detail: last_dlerror(),
                });
            }

            if debug_mode {
                tracing::debug!(enclave_path = %path.display(), "loaded SGX enclave runtime");
            }

            Ok(Self {
                enclave_path: path.to_path_buf(),
                debug_mode,
                handle,
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
                unsafe extern "C" fn(*mut u8, usize, *mut u8, usize) -> EcallStatus;

            let mut mrenclave = [0u8; SGX_MEASUREMENT_LEN];
            let mut mrsigner = [0u8; SGX_MEASUREMENT_LEN];
            let status = unsafe {
                let func: GetIdentityFn = self.symbol("credbridge_enclave_get_identity")?;
                func(
                    mrenclave.as_mut_ptr(),
                    mrenclave.len(),
                    mrsigner.as_mut_ptr(),
                    mrsigner.len(),
                )
            };

            self.ensure_success("get_identity", status)?;

            Ok(EnclaveIdentity::new(mrenclave, mrsigner))
        }
    }

    pub fn get_report(
        &self,
        report_data: [u8; SGX_REPORT_DATA_LEN],
    ) -> Result<Vec<u8>, HostRuntimeError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = report_data;
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "get_report",
            })
        }

        #[cfg(target_os = "linux")]
        {
            type GetReportFn =
                unsafe extern "C" fn(*const u8, usize, *mut u8, usize, *mut usize) -> EcallStatus;

            let mut report = EnclaveReport::new([0u8; SGX_REPORT_LEN], 0);
            let status = unsafe {
                let func: GetReportFn = self.symbol("credbridge_enclave_get_report")?;
                func(
                    report_data.as_ptr(),
                    report_data.len(),
                    report.bytes.as_mut_ptr(),
                    report.bytes.len(),
                    &mut report.written_len,
                )
            };

            self.ensure_success("get_report", status)?;
            if report.written_len != SGX_REPORT_LEN {
                return Err(HostRuntimeError::InvalidBufferSize {
                    operation: "get_report",
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
                unsafe extern "C" fn(u32, *mut u8, usize, *mut usize) -> EcallStatus;

            let mut key = EnclaveSealingKey::new([0u8; SGX_SEALING_KEY_LEN], 0);
            let status = unsafe {
                let func: GetSealingKeyFn = self.symbol("credbridge_enclave_get_sealing_key")?;
                func(
                    seal_policy_to_raw(policy),
                    key.bytes.as_mut_ptr(),
                    key.bytes.len(),
                    &mut key.written_len,
                )
            };

            self.ensure_success("get_sealing_key", status)?;
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
                *const c_char,
                *const c_char,
                *const c_char,
                *const u8,
                usize,
                *mut u8,
                usize,
                *mut usize,
            ) -> EcallStatus;

            let tenant_id = c_string(tenant_id, "tenant_id", "encrypt_credential")?;
            let user_id_hash = c_string(user_id_hash, "user_id_hash", "encrypt_credential")?;
            let credential_id = c_string(credential_id, "credential_id", "encrypt_credential")?;
            let mut blob_json = vec![0u8; ENCLAVE_BLOB_BUFFER_LEN];
            let mut written_len = 0usize;

            let status = unsafe {
                let func: EncryptCredentialFn =
                    self.symbol("credbridge_enclave_encrypt_credential")?;
                func(
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

            self.ensure_success("encrypt_credential", status)?;
            let blob_json = utf8_output(
                "encrypt_credential",
                &blob_json,
                written_len,
                ENCLAVE_BLOB_BUFFER_LEN,
            )?;
            EncryptedBlob::from_json(blob_json).map_err(|error| HostRuntimeError::LoadFailed {
                path: self.enclave_path.clone(),
                detail: format!("invalid encrypted blob payload from enclave: {error}"),
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
                *const c_char,
                *const c_char,
                *const c_char,
                *const c_char,
                *mut u8,
                usize,
                *mut usize,
            ) -> EcallStatus;

            let tenant_id = c_string(tenant_id, "tenant_id", "decrypt_credential")?;
            let user_id_hash = c_string(user_id_hash, "user_id_hash", "decrypt_credential")?;
            let credential_id = c_string(credential_id, "credential_id", "decrypt_credential")?;
            let blob_json = blob
                .to_json()
                .map_err(|error| HostRuntimeError::LoadFailed {
                    path: self.enclave_path.clone(),
                    detail: format!("failed to serialize encrypted blob for enclave: {error}"),
                })?;
            let blob_json = c_string(&blob_json, "blob_json", "decrypt_credential")?;
            let mut plaintext = vec![0u8; ENCLAVE_PLAINTEXT_BUFFER_LEN];
            let mut written_len = 0usize;

            let status = unsafe {
                let func: DecryptCredentialFn =
                    self.symbol("credbridge_enclave_decrypt_credential")?;
                func(
                    tenant_id.as_ptr(),
                    user_id_hash.as_ptr(),
                    credential_id.as_ptr(),
                    blob_json.as_ptr(),
                    plaintext.as_mut_ptr(),
                    plaintext.len(),
                    &mut written_len,
                )
            };

            self.ensure_success("decrypt_credential", status)?;
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
        let c_name = CString::new(name).map_err(|_| HostRuntimeError::SymbolMissing {
            symbol: name.to_string(),
            detail: "symbol name contains an interior NUL byte".to_string(),
        })?;
        let symbol = unsafe { dlsym(self.handle, c_name.as_ptr()) };
        if symbol.is_null() {
            return Err(HostRuntimeError::SymbolMissing {
                symbol: name.to_string(),
                detail: last_dlerror(),
            });
        }

        Ok(unsafe { std::mem::transmute_copy(&symbol) })
    }

    #[cfg(target_os = "linux")]
    fn ensure_success(
        &self,
        operation: &'static str,
        status: EcallStatus,
    ) -> Result<(), HostRuntimeError> {
        if status.is_success() {
            return Ok(());
        }

        Err(HostRuntimeError::EcallReturned { operation, status })
    }
}

impl EnclaveRuntime for SgxHostRuntime {
    fn get_identity(&self) -> Result<EnclaveIdentity, HostRuntimeError> {
        SgxHostRuntime::get_identity(self)
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
fn c_string(
    value: &str,
    label: &str,
    operation: &'static str,
) -> Result<CString, HostRuntimeError> {
    CString::new(value).map_err(|_| HostRuntimeError::LoadFailed {
        path: PathBuf::new(),
        detail: format!("`{label}` for `{operation}` contains an interior NUL byte"),
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

    std::str::from_utf8(&bytes[..written_len]).map_err(|_| HostRuntimeError::LoadFailed {
        path: PathBuf::new(),
        detail: format!("`{operation}` returned invalid UTF-8"),
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
    // SAFETY: dlerror returns a thread-local error string when dlopen/dlsym fails.
    let ptr = unsafe { dlerror() };
    if ptr.is_null() {
        return "unknown dlopen/dlsym error".to_string();
    }

    // SAFETY: the pointer is a valid NUL-terminated C string managed by libc.
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
