#[cfg(feature = "tee-hardware")]
use crate::tee::ffi_types::{SGX_REPORT_DATA_LEN, SGX_TARGET_INFO_LEN};
#[cfg(feature = "tee-hardware")]
use crate::tee::host_runtime::{EnclaveRuntime, SgxHostRuntime};
#[cfg(feature = "tee-hardware")]
use crate::tee::quote::QuoteParser;
#[cfg(feature = "tee-hardware")]
use crate::tee::sealing::SealPolicy;
use base64::{Engine as _, engine::general_purpose::STANDARD};
#[cfg(feature = "tee-hardware")]
use libc::{RTLD_LAZY, dlclose, dlerror, dlopen, dlsym, time_t};
use serde::Deserialize;
use std::env;
#[cfg(feature = "tee-hardware")]
use std::ffi::{CStr, CString, c_char, c_void};
#[cfg(feature = "tee-hardware")]
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_ROOT_KEY_HEX_ENV: &str = "TEE_SGX_ROOT_KEY_HEX";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_ROOT_KEY_PATH_ENV: &str = "TEE_SGX_ROOT_KEY_PATH";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_MRENCLAVE_ENV: &str = "TEE_SGX_MRENCLAVE";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_MRSIGNER_ENV: &str = "TEE_SGX_MRSIGNER";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_QUOTE_B64_ENV: &str = "TEE_SGX_QUOTE_B64";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_QUOTE_HEX_ENV: &str = "TEE_SGX_QUOTE_HEX";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_QUOTE_PATH_ENV: &str = "TEE_SGX_QUOTE_PATH";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_PCS_REGISTRATION_ID_ENV: &str = "TEE_SGX_PCS_REGISTRATION_ID";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_QUOTE_GENERATOR_CMD_ENV: &str = "TEE_SGX_QUOTE_GENERATOR_CMD";
pub(crate) const TEE_SGX_QUOTE_VERIFY_CMD_ENV: &str = "TEE_SGX_QUOTE_VERIFY_CMD";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_PCS_REGISTER_CMD_ENV: &str = "TEE_SGX_PCS_REGISTER_CMD";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_REPORT_DATA_HEX_ENV: &str = "TEE_SGX_REPORT_DATA_HEX";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_REPORT_B64_ENV: &str = "TEE_SGX_REPORT_B64";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_REPORT_HEX_ENV: &str = "TEE_SGX_REPORT_HEX";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_REPORT_PATH_ENV: &str = "TEE_SGX_REPORT_PATH";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_REPORT_GENERATOR_CMD_ENV: &str = "TEE_SGX_REPORT_GENERATOR_CMD";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_DCAP_QL_LIB_PATH_ENV: &str = "TEE_SGX_DCAP_QL_LIB_PATH";
#[cfg(feature = "tee-hardware")]
pub(crate) const TEE_SGX_DCAP_QV_LIB_PATH_ENV: &str = "TEE_SGX_DCAP_QV_LIB_PATH";
pub(crate) const TEE_SGX_QUOTE_INPUT_B64_ENV: &str = "TEE_SGX_QUOTE_INPUT_B64";
pub(crate) const TEE_SGX_VERIFY_NONCE_HEX_ENV: &str = "TEE_SGX_VERIFY_NONCE_HEX";

#[cfg(feature = "tee-hardware")]
const SGX_REPORT_SIZE: usize = 432;
#[cfg(feature = "tee-hardware")]
const SGX_QL_QV_RESULT_OK: u32 = 0;
#[cfg(feature = "tee-hardware")]
const SGX_QL_QV_RESULT_CONFIG_NEEDED: u32 = 1;
#[cfg(feature = "tee-hardware")]
const SGX_QL_QV_RESULT_OUT_OF_DATE: u32 = 2;
#[cfg(feature = "tee-hardware")]
const SGX_QL_QV_RESULT_OUT_OF_DATE_CONFIG_NEEDED: u32 = 3;
#[cfg(feature = "tee-hardware")]
const SGX_QL_QV_RESULT_SW_HARDENING_NEEDED: u32 = 4;
#[cfg(feature = "tee-hardware")]
const SGX_QL_QV_RESULT_CONFIG_AND_SW_HARDENING_NEEDED: u32 = 5;

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub(crate) struct HardwareVerificationEvidence {
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub not_before: Option<String>,
    pub not_after: Option<String>,
    pub fingerprint: Option<String>,
    pub verifier: Option<String>,
    pub collateral_status: Option<String>,
}

#[cfg(feature = "tee-hardware")]
pub(crate) fn load_hardware_root_key_bytes(policy: SealPolicy) -> Result<[u8; 32], String> {
    if let Some(runtime) = load_hardware_runtime_if_configured()? {
        return runtime
            .get_sealing_key(policy)
            .map_err(|error| error.to_string());
    }

    let bytes = if let Ok(value) = env::var(TEE_SGX_ROOT_KEY_HEX_ENV) {
        decode_text_bytes(TEE_SGX_ROOT_KEY_HEX_ENV, &value)?
    } else if let Ok(path) = env::var(TEE_SGX_ROOT_KEY_PATH_ENV) {
        load_binary_or_text_file(TEE_SGX_ROOT_KEY_PATH_ENV, &path)?
    } else {
        return Err(format!(
            "TEE_MODE=hardware requested, but no hardware root key material was configured; set `{TEE_SGX_ROOT_KEY_HEX_ENV}` or `{TEE_SGX_ROOT_KEY_PATH_ENV}`"
        ));
    };

    if bytes.len() != 32 {
        return Err(format!(
            "hardware root key material must be exactly 32 bytes, got {}",
            bytes.len()
        ));
    }

    let mut root_key = [0u8; 32];
    root_key.copy_from_slice(&bytes);
    Ok(root_key)
}

#[cfg(feature = "tee-hardware")]
pub(crate) fn load_hardware_quote_bytes() -> Result<Vec<u8>, String> {
    if let Ok(value) = env::var(TEE_SGX_QUOTE_B64_ENV) {
        return decode_text_bytes(TEE_SGX_QUOTE_B64_ENV, &value);
    }

    if let Ok(value) = env::var(TEE_SGX_QUOTE_HEX_ENV) {
        return decode_text_bytes(TEE_SGX_QUOTE_HEX_ENV, &value);
    }

    if let Ok(path) = env::var(TEE_SGX_QUOTE_PATH_ENV) {
        return load_binary_or_text_file(TEE_SGX_QUOTE_PATH_ENV, &path);
    }

    Err(format!(
        "TEE_MODE=hardware requested, but no SGX quote was configured; set `{TEE_SGX_QUOTE_B64_ENV}`, `{TEE_SGX_QUOTE_HEX_ENV}`, or `{TEE_SGX_QUOTE_PATH_ENV}`"
    ))
}

#[cfg(feature = "tee-hardware")]
pub(crate) fn load_or_generate_hardware_quote(
    report_data: Option<&[u8; 64]>,
) -> Result<Vec<u8>, String> {
    let report_data = report_data.copied().unwrap_or([0u8; SGX_REPORT_DATA_LEN]);

    if let Some(runtime) = load_hardware_runtime_if_configured()? {
        return generate_quote_via_targeted_dcap_ffi(&runtime, report_data);
    }

    if has_hardware_report_source_config() {
        tracing::warn!(
            "using legacy SGX report source fallback for quote generation; this path is for debugging only and bypasses QE target-info driven report generation"
        );
        return generate_quote_via_dcap_ffi_from_report(&report_data);
    }

    if let Ok(command) = env::var(TEE_SGX_QUOTE_GENERATOR_CMD_ENV) {
        let mut command = shell_command(command.trim())?;
        command.env(TEE_SGX_REPORT_DATA_HEX_ENV, hex::encode(report_data));

        let output = command.output().map_err(|error| {
            format!("failed to execute `{TEE_SGX_QUOTE_GENERATOR_CMD_ENV}`: {error}")
        })?;

        if !output.status.success() {
            return Err(format_command_failure(
                TEE_SGX_QUOTE_GENERATOR_CMD_ENV,
                &output,
            ));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|error| {
            format!("`{TEE_SGX_QUOTE_GENERATOR_CMD_ENV}` returned non UTF-8 quote output: {error}")
        })?;

        return decode_text_bytes(TEE_SGX_QUOTE_GENERATOR_CMD_ENV, &stdout);
    }

    load_hardware_quote_bytes()
}

pub(crate) fn verify_quote_with_backend(
    quote_bytes: &[u8],
    nonce: Option<&[u8]>,
) -> Result<HardwareVerificationEvidence, String> {
    if let Ok(command) = env::var(TEE_SGX_QUOTE_VERIFY_CMD_ENV) {
        return verify_quote_with_command_backend(command.trim(), quote_bytes, nonce);
    }

    #[cfg(feature = "tee-hardware")]
    {
        return verify_quote_via_dcap_ffi(quote_bytes);
    }

    #[allow(unreachable_code)]
    Err(format!(
        "TEE_MODE=hardware verification requires a real quote verifier; set `{TEE_SGX_QUOTE_VERIFY_CMD_ENV}` to a backend command"
    ))
}

fn verify_quote_with_command_backend(
    command: &str,
    quote_bytes: &[u8],
    nonce: Option<&[u8]>,
) -> Result<HardwareVerificationEvidence, String> {
    let mut command = shell_command(command)?;
    command
        .env(TEE_SGX_QUOTE_INPUT_B64_ENV, STANDARD.encode(quote_bytes))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(nonce) = nonce {
        command.env(TEE_SGX_VERIFY_NONCE_HEX_ENV, hex::encode(nonce));
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to spawn `{TEE_SGX_QUOTE_VERIFY_CMD_ENV}`: {error}"))?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| "hardware verify backend stdin is unavailable".to_string())?
        .write_all(quote_bytes)
        .map_err(|error| format!("failed to send quote to verifier stdin: {error}"))?;
    let _ = child.stdin.take();

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed waiting for quote verifier: {error}"))?;

    if !output.status.success() {
        return Err(format_command_failure(
            TEE_SGX_QUOTE_VERIFY_CMD_ENV,
            &output,
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("hardware quote verifier returned invalid UTF-8: {error}"))?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(HardwareVerificationEvidence {
            verifier: Some("external-command".to_string()),
            ..HardwareVerificationEvidence::default()
        });
    }

    serde_json::from_str(trimmed).map_err(|error| {
        format!("hardware verifier output must be empty or JSON metadata, got parse error: {error}")
    })
}

#[cfg(feature = "tee-hardware")]
pub(crate) fn register_pcs_with_backend(quote_bytes: &[u8]) -> Result<Option<String>, String> {
    let Ok(command) = env::var(TEE_SGX_PCS_REGISTER_CMD_ENV) else {
        return Ok(None);
    };

    let output = shell_command(command.trim())?
        .env(TEE_SGX_QUOTE_INPUT_B64_ENV, STANDARD.encode(quote_bytes))
        .output()
        .map_err(|error| format!("failed to execute `{TEE_SGX_PCS_REGISTER_CMD_ENV}`: {error}"))?;

    if !output.status.success() {
        return Err(format_command_failure(
            TEE_SGX_PCS_REGISTER_CMD_ENV,
            &output,
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("PCS register backend returned invalid UTF-8: {error}"))?;
    let registration_id = stdout.trim();
    if registration_id.is_empty() {
        return Err(format!(
            "`{TEE_SGX_PCS_REGISTER_CMD_ENV}` completed successfully but returned an empty registration id"
        ));
    }

    Ok(Some(registration_id.to_string()))
}

#[cfg(feature = "tee-hardware")]
pub(crate) fn load_hardware_measurements() -> Result<([u8; 32], [u8; 32]), String> {
    if let Some(runtime) = load_hardware_runtime_if_configured()? {
        let identity = runtime.get_identity().map_err(|error| error.to_string())?;
        return Ok((identity.mrenclave, identity.mrsigner));
    }

    if let Ok(bytes) = load_hardware_quote_bytes() {
        let mrenclave = QuoteParser::extract_mrenclave(&bytes).map_err(|error| {
            format!("failed to extract MRENCLAVE from configured SGX quote: {error}")
        })?;
        let mrsigner = QuoteParser::extract_mrsigner(&bytes).map_err(|error| {
            format!("failed to extract MRSIGNER from configured SGX quote: {error}")
        })?;
        return Ok((mrenclave, mrsigner));
    }

    let mrenclave = env::var(TEE_SGX_MRENCLAVE_ENV).map_err(|_| {
        format!("TEE_MODE=hardware requested, but neither a quote nor `{TEE_SGX_MRENCLAVE_ENV}` was configured")
    })?;
    let mrsigner = env::var(TEE_SGX_MRSIGNER_ENV).map_err(|_| {
        format!("TEE_MODE=hardware requested, but neither a quote nor `{TEE_SGX_MRSIGNER_ENV}` was configured")
    })?;

    Ok((
        parse_measurement_hex(TEE_SGX_MRENCLAVE_ENV, &mrenclave)?,
        parse_measurement_hex(TEE_SGX_MRSIGNER_ENV, &mrsigner)?,
    ))
}

#[cfg(feature = "tee-hardware")]
pub(crate) fn parse_measurement_hex(label: &str, value: &str) -> Result<[u8; 32], String> {
    let decoded =
        hex::decode(value.trim()).map_err(|error| format!("invalid hex in `{label}`: {error}"))?;
    if decoded.len() != 32 {
        return Err(format!(
            "`{label}` must decode to exactly 32 bytes, got {}",
            decoded.len()
        ));
    }

    let mut measurement = [0u8; 32];
    measurement.copy_from_slice(&decoded);
    Ok(measurement)
}

#[cfg(feature = "tee-hardware")]
fn load_binary_or_text_file(label: &str, path: &str) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read `{label}` from `{path}`: {error}"))?;

    if bytes.len() == 32 || bytes.len() > 384 {
        return Ok(bytes);
    }

    if let Ok(text) = std::str::from_utf8(&bytes) {
        return decode_text_bytes(label, text);
    }

    Ok(bytes)
}

#[cfg(feature = "tee-hardware")]
fn decode_text_bytes(label: &str, value: &str) -> Result<Vec<u8>, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(format!("`{label}` must not be empty"));
    }

    if let Ok(bytes) = hex::decode(normalized) {
        return Ok(bytes);
    }

    STANDARD
        .decode(normalized)
        .map_err(|error| format!("`{label}` is neither valid hex nor base64: {error}"))
}

fn shell_command(command: &str) -> Result<Command, String> {
    if command.is_empty() {
        return Err("hardware backend command must not be empty".to_string());
    }

    let mut shell = Command::new("/bin/sh");
    shell.arg("-lc").arg(command);
    Ok(shell)
}

fn format_command_failure(label: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if !stderr.trim().is_empty() {
        stderr.trim().to_string()
    } else if !stdout.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        format!("exit status {}", output.status)
    };

    format!("`{label}` failed: {detail}")
}

#[cfg(feature = "tee-hardware")]
fn has_hardware_report_source_config() -> bool {
    [
        TEE_SGX_REPORT_B64_ENV,
        TEE_SGX_REPORT_HEX_ENV,
        TEE_SGX_REPORT_PATH_ENV,
        TEE_SGX_REPORT_GENERATOR_CMD_ENV,
    ]
    .iter()
    .any(|name| {
        env::var(name)
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    })
}

#[cfg(feature = "tee-hardware")]
fn generate_quote_via_dcap_ffi_from_report(report_data: &[u8; 64]) -> Result<Vec<u8>, String> {
    let report_bytes = load_or_generate_hardware_report(report_data)?;
    generate_quote_via_dcap_ffi(&report_bytes)
}

#[cfg(feature = "tee-hardware")]
trait QuoteGenerationBackend {
    fn get_target_info(&self, target_info: &mut [u8; SGX_TARGET_INFO_LEN]) -> u32;
    fn get_quote_size(&self, quote_size: &mut u32) -> u32;
    fn get_quote(&self, report_bytes: &[u8], quote_size: u32, quote: &mut [u8]) -> u32;
}

#[cfg(feature = "tee-hardware")]
struct DcapQlQuoteGenerationBackend {
    _library: DynamicLibrary,
    get_target_info_fn: unsafe extern "C" fn(*mut c_void) -> u32,
    get_quote_size_fn: unsafe extern "C" fn(*mut u32) -> u32,
    get_quote_fn: unsafe extern "C" fn(*const c_void, u32, *mut u8) -> u32,
}

#[cfg(feature = "tee-hardware")]
impl DcapQlQuoteGenerationBackend {
    fn open() -> Result<Self, String> {
        let library = DynamicLibrary::open(
            TEE_SGX_DCAP_QL_LIB_PATH_ENV,
            &["libsgx_dcap_ql.so.1", "libsgx_dcap_ql.so"],
        )?;
        let get_target_info_fn = unsafe {
            library
                .symbol("sgx_qe_get_target_info")
                .map_err(|error| format!("failed to resolve sgx_qe_get_target_info: {error}"))?
        };
        let get_quote_size_fn = unsafe {
            library
                .symbol("sgx_qe_get_quote_size")
                .map_err(|error| format!("failed to resolve sgx_qe_get_quote_size: {error}"))?
        };
        let get_quote_fn = unsafe {
            library
                .symbol("sgx_qe_get_quote")
                .map_err(|error| format!("failed to resolve sgx_qe_get_quote: {error}"))?
        };

        Ok(Self {
            _library: library,
            get_target_info_fn,
            get_quote_size_fn,
            get_quote_fn,
        })
    }
}

#[cfg(feature = "tee-hardware")]
impl QuoteGenerationBackend for DcapQlQuoteGenerationBackend {
    fn get_target_info(&self, target_info: &mut [u8; SGX_TARGET_INFO_LEN]) -> u32 {
        unsafe { (self.get_target_info_fn)(target_info.as_mut_ptr().cast()) }
    }

    fn get_quote_size(&self, quote_size: &mut u32) -> u32 {
        unsafe { (self.get_quote_size_fn)(quote_size) }
    }

    fn get_quote(&self, report_bytes: &[u8], quote_size: u32, quote: &mut [u8]) -> u32 {
        unsafe { (self.get_quote_fn)(report_bytes.as_ptr().cast(), quote_size, quote.as_mut_ptr()) }
    }
}

#[cfg(feature = "tee-hardware")]
fn generate_quote_via_targeted_dcap_ffi(
    runtime: &dyn EnclaveRuntime,
    report_data: [u8; SGX_REPORT_DATA_LEN],
) -> Result<Vec<u8>, String> {
    let backend = DcapQlQuoteGenerationBackend::open()?;
    generate_quote_via_targeted_backend(runtime, &backend, report_data)
}

#[cfg(feature = "tee-hardware")]
fn generate_quote_via_targeted_backend(
    runtime: &dyn EnclaveRuntime,
    backend: &impl QuoteGenerationBackend,
    report_data: [u8; SGX_REPORT_DATA_LEN],
) -> Result<Vec<u8>, String> {
    let mut target_info = [0u8; SGX_TARGET_INFO_LEN];
    let rc = backend.get_target_info(&mut target_info);
    if rc != 0 {
        return Err(format!(
            "sgx_qe_get_target_info failed with code 0x{rc:08x}"
        ));
    }

    let report_bytes = runtime
        .get_targeted_report(target_info, report_data)
        .map_err(|error| format!("targeted report generation failed: {error}"))?;
    generate_quote_via_backend_from_report(backend, &report_bytes)
}

#[cfg(feature = "tee-hardware")]
fn generate_quote_via_dcap_ffi(report_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let backend = DcapQlQuoteGenerationBackend::open()?;
    generate_quote_via_backend_from_report(&backend, report_bytes)
}

#[cfg(feature = "tee-hardware")]
fn generate_quote_via_backend_from_report(
    backend: &impl QuoteGenerationBackend,
    report_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    ensure_valid_sgx_report(report_bytes)
        .map_err(|error| format!("targeted report generation failed: {error}"))?;

    let mut quote_size = 0u32;
    let rc = backend.get_quote_size(&mut quote_size);
    if rc != 0 {
        return Err(format!("sgx_qe_get_quote_size failed with code 0x{rc:08x}"));
    }
    if quote_size == 0 {
        return Err("sgx_qe_get_quote_size returned 0".to_string());
    }

    let mut quote = vec![0u8; quote_size as usize];
    let rc = backend.get_quote(report_bytes, quote_size, &mut quote);
    if rc != 0 {
        return Err(format!("sgx_qe_get_quote failed with code 0x{rc:08x}"));
    }

    Ok(quote)
}

#[cfg(feature = "tee-hardware")]
fn ensure_valid_sgx_report(report_bytes: &[u8]) -> Result<(), String> {
    if report_bytes.len() != SGX_REPORT_SIZE {
        return Err(format!(
            "SGX report must be exactly {SGX_REPORT_SIZE} bytes, got {}",
            report_bytes.len()
        ));
    }
    Ok(())
}

#[cfg(feature = "tee-hardware")]
fn load_hardware_runtime_if_configured() -> Result<Option<SgxHostRuntime>, String> {
    match SgxHostRuntime::load_from_env() {
        Ok(runtime) => Ok(Some(runtime)),
        Err(error) if error.is_missing_enclave_path() => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(feature = "tee-hardware")]
fn load_or_generate_hardware_report(report_data: &[u8; 64]) -> Result<Vec<u8>, String> {
    if let Ok(command) = env::var(TEE_SGX_REPORT_GENERATOR_CMD_ENV) {
        let output = shell_command(command.trim())?
            .env(TEE_SGX_REPORT_DATA_HEX_ENV, hex::encode(report_data))
            .output()
            .map_err(|error| {
                format!("failed to execute `{TEE_SGX_REPORT_GENERATOR_CMD_ENV}`: {error}")
            })?;

        if !output.status.success() {
            return Err(format_command_failure(
                TEE_SGX_REPORT_GENERATOR_CMD_ENV,
                &output,
            ));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|error| {
            format!(
                "`{TEE_SGX_REPORT_GENERATOR_CMD_ENV}` returned non UTF-8 report output: {error}"
            )
        })?;
        return decode_text_bytes(TEE_SGX_REPORT_GENERATOR_CMD_ENV, &stdout);
    }

    if let Ok(value) = env::var(TEE_SGX_REPORT_B64_ENV) {
        return decode_text_bytes(TEE_SGX_REPORT_B64_ENV, &value);
    }
    if let Ok(value) = env::var(TEE_SGX_REPORT_HEX_ENV) {
        return decode_text_bytes(TEE_SGX_REPORT_HEX_ENV, &value);
    }
    if let Ok(path) = env::var(TEE_SGX_REPORT_PATH_ENV) {
        return load_binary_or_text_file(TEE_SGX_REPORT_PATH_ENV, &path);
    }

    Err(format!(
        "no SGX report source configured; set `{TEE_SGX_REPORT_GENERATOR_CMD_ENV}`, `{TEE_SGX_REPORT_B64_ENV}`, `{TEE_SGX_REPORT_HEX_ENV}`, or `{TEE_SGX_REPORT_PATH_ENV}`"
    ))
}

#[cfg(feature = "tee-hardware")]
fn verify_quote_via_dcap_ffi(quote_bytes: &[u8]) -> Result<HardwareVerificationEvidence, String> {
    let library = DynamicLibrary::open(
        TEE_SGX_DCAP_QV_LIB_PATH_ENV,
        &["libsgx_dcap_quoteverify.so.1", "libsgx_dcap_quoteverify.so"],
    )?;
    let get_supplemental_size: unsafe extern "C" fn(*mut u32) -> u32 =
        unsafe { library.symbol("sgx_qv_get_quote_supplemental_data_size")? };
    let get_collateral: unsafe extern "C" fn(*const u8, u32, *mut *mut u8, *mut u32) -> u32 =
        unsafe { library.symbol("tee_qv_get_collateral")? };
    let free_collateral: unsafe extern "C" fn(*mut u8) =
        unsafe { library.symbol("tee_qv_free_collateral")? };
    let verify_quote: unsafe extern "C" fn(
        *const u8,
        u32,
        *const c_void,
        time_t,
        *mut u32,
        *mut u32,
        *mut c_void,
        u32,
        *mut u8,
    ) -> u32 = unsafe { library.symbol("sgx_qv_verify_quote")? };

    let mut supplemental_size = 0u32;
    let rc = unsafe { get_supplemental_size(&mut supplemental_size) };
    if rc != 0 {
        return Err(format!(
            "sgx_qv_get_quote_supplemental_data_size failed with code 0x{rc:08x}"
        ));
    }

    let mut collateral_ptr = std::ptr::null_mut();
    let mut collateral_size = 0u32;
    let rc = unsafe {
        get_collateral(
            quote_bytes.as_ptr(),
            quote_bytes.len() as u32,
            &mut collateral_ptr,
            &mut collateral_size,
        )
    };
    if rc != 0 {
        return Err(format!("tee_qv_get_collateral failed with code 0x{rc:08x}"));
    }
    let collateral = CollateralGuard {
        ptr: collateral_ptr,
        free_fn: free_collateral,
    };
    if collateral.ptr.is_null() || collateral_size == 0 {
        return Err("tee_qv_get_collateral returned empty collateral".to_string());
    }

    let mut collateral_expiration_status = 0u32;
    let mut verification_result = u32::MAX;
    let mut supplemental_data = vec![0u8; supplemental_size as usize];
    let now = current_unix_time_t()?;
    let rc = unsafe {
        verify_quote(
            quote_bytes.as_ptr(),
            quote_bytes.len() as u32,
            collateral.ptr.cast(),
            now,
            &mut collateral_expiration_status,
            &mut verification_result,
            std::ptr::null_mut(),
            supplemental_size,
            supplemental_data.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return Err(format!("sgx_qv_verify_quote failed with code 0x{rc:08x}"));
    }
    if collateral_expiration_status != 0 {
        return Err(format!(
            "quote collateral is expired or invalid (status={collateral_expiration_status})"
        ));
    }
    if !is_accepted_qv_result(verification_result) {
        return Err(format!(
            "quote verification result was not trusted: {} ({verification_result})",
            qv_result_label(verification_result)
        ));
    }

    Ok(HardwareVerificationEvidence {
        verifier: Some("dcap-ffi".to_string()),
        collateral_status: Some(qv_result_label(verification_result).to_string()),
        ..HardwareVerificationEvidence::default()
    })
}

#[cfg(feature = "tee-hardware")]
fn current_unix_time_t() -> Result<time_t, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system clock is before unix epoch: {error}"))?
        .as_secs();
    seconds
        .try_into()
        .map_err(|_| "current unix timestamp does not fit into time_t".to_string())
}

#[cfg(feature = "tee-hardware")]
fn is_accepted_qv_result(result: u32) -> bool {
    matches!(
        result,
        SGX_QL_QV_RESULT_OK
            | SGX_QL_QV_RESULT_CONFIG_NEEDED
            | SGX_QL_QV_RESULT_OUT_OF_DATE
            | SGX_QL_QV_RESULT_OUT_OF_DATE_CONFIG_NEEDED
            | SGX_QL_QV_RESULT_SW_HARDENING_NEEDED
            | SGX_QL_QV_RESULT_CONFIG_AND_SW_HARDENING_NEEDED
    )
}

#[cfg(feature = "tee-hardware")]
fn qv_result_label(result: u32) -> &'static str {
    match result {
        SGX_QL_QV_RESULT_OK => "ok",
        SGX_QL_QV_RESULT_CONFIG_NEEDED => "config-needed",
        SGX_QL_QV_RESULT_OUT_OF_DATE => "out-of-date",
        SGX_QL_QV_RESULT_OUT_OF_DATE_CONFIG_NEEDED => "out-of-date-config-needed",
        SGX_QL_QV_RESULT_SW_HARDENING_NEEDED => "sw-hardening-needed",
        SGX_QL_QV_RESULT_CONFIG_AND_SW_HARDENING_NEEDED => "config-and-sw-hardening-needed",
        _ => "untrusted",
    }
}

#[cfg(feature = "tee-hardware")]
struct CollateralGuard {
    ptr: *mut u8,
    free_fn: unsafe extern "C" fn(*mut u8),
}

#[cfg(feature = "tee-hardware")]
impl Drop for CollateralGuard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { (self.free_fn)(self.ptr) };
        }
    }
}

#[cfg(feature = "tee-hardware")]
struct DynamicLibrary(*mut c_void);

#[cfg(feature = "tee-hardware")]
impl DynamicLibrary {
    fn open(env_name: &str, default_names: &[&str]) -> Result<Self, String> {
        if let Ok(path) = env::var(env_name) {
            if !path.trim().is_empty() {
                return Self::open_single(path.trim());
            }
        }

        let mut errors = Vec::new();
        for name in default_names {
            match Self::open_single(name) {
                Ok(library) => return Ok(library),
                Err(error) => errors.push(format!("{name}: {error}")),
            }
        }

        Err(format!(
            "failed to load SGX/DCAP library; tried {}",
            errors.join(", ")
        ))
    }

    fn open_single(path: &str) -> Result<Self, String> {
        let c_path = CString::new(path)
            .map_err(|_| format!("library path contains interior NUL byte: {path}"))?;
        let handle = unsafe { dlopen(c_path.as_ptr(), RTLD_LAZY) };
        if handle.is_null() {
            return Err(last_dlerror());
        }
        Ok(Self(handle))
    }

    unsafe fn symbol<T: Copy>(&self, name: &str) -> Result<T, String> {
        let c_name = CString::new(name)
            .map_err(|_| format!("symbol name contains interior NUL byte: {name}"))?;
        let symbol = unsafe { dlsym(self.0, c_name.as_ptr()) };
        if symbol.is_null() {
            return Err(last_dlerror());
        }
        Ok(unsafe { std::mem::transmute_copy(&symbol) })
    }
}

#[cfg(feature = "tee-hardware")]
impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                dlclose(self.0);
            }
        }
    }
}

#[cfg(feature = "tee-hardware")]
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
    #[cfg(feature = "tee-hardware")]
    use crate::tee::ffi_types::{EnclaveIdentity, SGX_SEALING_KEY_LEN};
    #[cfg(feature = "tee-hardware")]
    use crate::tee::host_runtime::HostRuntimeError;
    #[cfg(feature = "tee-hardware")]
    use crate::tee::sealing::SealPolicy;
    #[cfg(feature = "tee-hardware")]
    use std::sync::Arc;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[cfg(feature = "tee-hardware")]
    #[derive(Clone)]
    struct MockRuntime {
        log: Arc<Mutex<Vec<&'static str>>>,
        targeted_report_result: Result<Vec<u8>, HostRuntimeError>,
    }

    #[cfg(feature = "tee-hardware")]
    impl EnclaveRuntime for MockRuntime {
        fn get_identity(&self) -> Result<EnclaveIdentity, HostRuntimeError> {
            Ok(EnclaveIdentity::new([0u8; 32], [0u8; 32]))
        }

        fn get_targeted_report(
            &self,
            _target_info: [u8; SGX_TARGET_INFO_LEN],
            _report_data: [u8; SGX_REPORT_DATA_LEN],
        ) -> Result<Vec<u8>, HostRuntimeError> {
            self.log.lock().unwrap().push("get_targeted_report");
            self.targeted_report_result.clone()
        }

        fn get_sealing_key(
            &self,
            _policy: SealPolicy,
        ) -> Result<[u8; SGX_SEALING_KEY_LEN], HostRuntimeError> {
            Ok([0u8; SGX_SEALING_KEY_LEN])
        }

        fn encrypt_credential(
            &self,
            _tenant_id: &str,
            _user_id_hash: &str,
            _credential_id: &str,
            _plaintext: &[u8],
        ) -> Result<crate::crypto::EncryptedBlob, HostRuntimeError> {
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "encrypt_credential",
            })
        }

        fn decrypt_credential(
            &self,
            _tenant_id: &str,
            _user_id_hash: &str,
            _credential_id: &str,
            _blob: &crate::crypto::EncryptedBlob,
        ) -> Result<Vec<u8>, HostRuntimeError> {
            Err(HostRuntimeError::UnsupportedPlatform {
                operation: "decrypt_credential",
            })
        }
    }

    #[cfg(feature = "tee-hardware")]
    struct MockQuoteBackend {
        log: Arc<Mutex<Vec<&'static str>>>,
        get_target_info_rc: u32,
        get_quote_size_rc: u32,
        get_quote_rc: u32,
        quote_size: u32,
    }

    #[cfg(feature = "tee-hardware")]
    impl QuoteGenerationBackend for MockQuoteBackend {
        fn get_target_info(&self, target_info: &mut [u8; SGX_TARGET_INFO_LEN]) -> u32 {
            self.log.lock().unwrap().push("get_target_info");
            target_info.fill(0xAB);
            self.get_target_info_rc
        }

        fn get_quote_size(&self, quote_size: &mut u32) -> u32 {
            self.log.lock().unwrap().push("get_quote_size");
            *quote_size = self.quote_size;
            self.get_quote_size_rc
        }

        fn get_quote(&self, _report_bytes: &[u8], _quote_size: u32, quote: &mut [u8]) -> u32 {
            self.log.lock().unwrap().push("get_quote");
            quote.fill(0xCD);
            self.get_quote_rc
        }
    }

    #[test]
    fn verify_quote_backend_accepts_json_metadata() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            env::set_var(
                TEE_SGX_QUOTE_VERIFY_CMD_ENV,
                "printf '{\"subject\":\"CN=test\",\"issuer\":\"CN=issuer\",\"fingerprint\":\"abc123\"}'",
            );
        }

        let evidence = verify_quote_with_backend(b"quote-bytes", Some(b"nonce")).unwrap();
        assert_eq!(evidence.subject.as_deref(), Some("CN=test"));
        assert_eq!(evidence.issuer.as_deref(), Some("CN=issuer"));
        assert_eq!(evidence.fingerprint.as_deref(), Some("abc123"));

        unsafe {
            env::remove_var(TEE_SGX_QUOTE_VERIFY_CMD_ENV);
            env::remove_var(TEE_SGX_VERIFY_NONCE_HEX_ENV);
            env::remove_var(TEE_SGX_QUOTE_INPUT_B64_ENV);
        }
    }

    #[test]
    fn verify_quote_backend_requires_command() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            env::remove_var(TEE_SGX_QUOTE_VERIFY_CMD_ENV);
        }

        let error = verify_quote_with_backend(b"quote-bytes", None).unwrap_err();
        assert!(error.contains(TEE_SGX_QUOTE_VERIFY_CMD_ENV));
    }

    #[cfg(feature = "tee-hardware")]
    #[test]
    fn targeted_quote_path_requests_qe_target_info_before_quote_size() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let runtime = MockRuntime {
            log: log.clone(),
            targeted_report_result: Ok(vec![0u8; SGX_REPORT_SIZE]),
        };
        let backend = MockQuoteBackend {
            log: log.clone(),
            get_target_info_rc: 0,
            get_quote_size_rc: 0,
            get_quote_rc: 0,
            quote_size: 16,
        };

        let quote = generate_quote_via_targeted_backend(&runtime, &backend, [0u8; 64]).unwrap();
        assert_eq!(quote.len(), 16);
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "get_target_info",
                "get_targeted_report",
                "get_quote_size",
                "get_quote"
            ]
        );
    }

    #[cfg(feature = "tee-hardware")]
    #[test]
    fn targeted_quote_path_reports_target_info_stage_errors() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let runtime = MockRuntime {
            log: log.clone(),
            targeted_report_result: Ok(vec![0u8; SGX_REPORT_SIZE]),
        };
        let backend = MockQuoteBackend {
            log: log.clone(),
            get_target_info_rc: 0x0000_e00f,
            get_quote_size_rc: 0,
            get_quote_rc: 0,
            quote_size: 16,
        };

        let error = generate_quote_via_targeted_backend(&runtime, &backend, [0u8; 64]).unwrap_err();
        assert!(error.contains("sgx_qe_get_target_info failed"));
        assert_eq!(*log.lock().unwrap(), vec!["get_target_info"]);
    }

    #[cfg(feature = "tee-hardware")]
    #[test]
    fn targeted_quote_path_reports_targeted_report_stage_errors() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let runtime = MockRuntime {
            log: log.clone(),
            targeted_report_result: Err(HostRuntimeError::MissingEnclavePath),
        };
        let backend = MockQuoteBackend {
            log: log.clone(),
            get_target_info_rc: 0,
            get_quote_size_rc: 0,
            get_quote_rc: 0,
            quote_size: 16,
        };

        let error = generate_quote_via_targeted_backend(&runtime, &backend, [0u8; 64]).unwrap_err();
        assert!(error.contains("targeted report generation failed"));
        assert_eq!(
            *log.lock().unwrap(),
            vec!["get_target_info", "get_targeted_report"]
        );
    }

    #[cfg(feature = "tee-hardware")]
    #[test]
    fn targeted_quote_path_reports_quote_size_stage_errors() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let runtime = MockRuntime {
            log: log.clone(),
            targeted_report_result: Ok(vec![0u8; SGX_REPORT_SIZE]),
        };
        let backend = MockQuoteBackend {
            log: log.clone(),
            get_target_info_rc: 0,
            get_quote_size_rc: 0x10,
            get_quote_rc: 0,
            quote_size: 16,
        };

        let error = generate_quote_via_targeted_backend(&runtime, &backend, [0u8; 64]).unwrap_err();
        assert!(error.contains("sgx_qe_get_quote_size failed"));
        assert_eq!(
            *log.lock().unwrap(),
            vec!["get_target_info", "get_targeted_report", "get_quote_size"]
        );
    }

    #[cfg(feature = "tee-hardware")]
    #[test]
    fn targeted_quote_path_reports_quote_stage_errors() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let runtime = MockRuntime {
            log: log.clone(),
            targeted_report_result: Ok(vec![0u8; SGX_REPORT_SIZE]),
        };
        let backend = MockQuoteBackend {
            log: log.clone(),
            get_target_info_rc: 0,
            get_quote_size_rc: 0,
            get_quote_rc: 0x20,
            quote_size: 16,
        };

        let error = generate_quote_via_targeted_backend(&runtime, &backend, [0u8; 64]).unwrap_err();
        assert!(error.contains("sgx_qe_get_quote failed"));
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "get_target_info",
                "get_targeted_report",
                "get_quote_size",
                "get_quote"
            ]
        );
    }
}
