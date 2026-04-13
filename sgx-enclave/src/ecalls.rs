use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SGX_MEASUREMENT_LEN: usize = 32;
pub const SGX_REPORT_DATA_LEN: usize = 64;
pub const SGX_REPORT_LEN: usize = 432;
pub const SGX_TARGET_INFO_LEN: usize = 512;
pub const SGX_SEALING_KEY_LEN: usize = 32;
pub const ENCLAVE_BLOB_BUFFER_LEN: usize = 16 * 1024;

pub const ECALL_SUCCESS: i32 = 0;
pub const ECALL_UNSUPPORTED: i32 = 1;
pub const ECALL_INVALID_INPUT: i32 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct StubEncryptedBlob {
    pub version: u8,
    pub algorithm: String,
    pub kdf: String,
    pub nonce: String,
    pub auth_tag: String,
    pub ciphertext: String,
    pub aad_hash: Option<String>,
}

pub fn get_identity() -> ([u8; SGX_MEASUREMENT_LEN], [u8; SGX_MEASUREMENT_LEN]) {
    let mut enclave_measurement = [0u8; SGX_MEASUREMENT_LEN];
    let mut signer_measurement = [0u8; SGX_MEASUREMENT_LEN];

    let enclave_hash = Sha256::digest(b"credbridge-phase-a-enclave");
    let signer_hash = Sha256::digest(b"credbridge-phase-a-signer");
    enclave_measurement.copy_from_slice(&enclave_hash);
    signer_measurement.copy_from_slice(&signer_hash);

    (enclave_measurement, signer_measurement)
}

pub fn get_report(report_data: &[u8]) -> Result<[u8; SGX_REPORT_LEN], i32> {
    get_targeted_report(&[0u8; SGX_TARGET_INFO_LEN], report_data)
}

pub fn get_targeted_report(
    target_info: &[u8],
    report_data: &[u8],
) -> Result<[u8; SGX_REPORT_LEN], i32> {
    if target_info.len() != SGX_TARGET_INFO_LEN || report_data.len() != SGX_REPORT_DATA_LEN {
        return Err(ECALL_INVALID_INPUT);
    }

    let mut report = [0u8; SGX_REPORT_LEN];
    report[..SGX_REPORT_DATA_LEN].copy_from_slice(report_data);
    let digest = Sha256::digest([target_info, report_data].concat());
    report[SGX_REPORT_DATA_LEN..SGX_REPORT_DATA_LEN + digest.len()].copy_from_slice(&digest);
    report[128..160].copy_from_slice(&target_info[..32]);
    Ok(report)
}

pub fn get_sealing_key(policy: u32) -> Result<[u8; SGX_SEALING_KEY_LEN], i32> {
    let label = match policy {
        0 => b"MRENCLAVE".as_slice(),
        1 => b"MRSIGNER".as_slice(),
        _ => return Err(ECALL_INVALID_INPUT),
    };

    let mut key = [0u8; SGX_SEALING_KEY_LEN];
    let digest = Sha256::digest(label);
    key.copy_from_slice(&digest);
    Ok(key)
}

pub fn encrypt_credential(
    tenant_id: &str,
    user_id_hash: &str,
    credential_id: &str,
    plaintext: &[u8],
) -> Result<String, i32> {
    let aad = format!("{tenant_id}:{user_id_hash}");
    let nonce = Sha256::digest(format!("{tenant_id}:{credential_id}:nonce").as_bytes());
    let auth_tag = Sha256::digest(format!("{aad}:{credential_id}:tag").as_bytes());
    let aad_hash = Sha256::digest(aad.as_bytes());

    let blob = StubEncryptedBlob {
        version: 1,
        algorithm: "SGX-ENCLAVE-STUB".to_string(),
        kdf: "PHASE-C-STUB".to_string(),
        nonce: URL_SAFE_NO_PAD.encode(&nonce[..12]),
        auth_tag: URL_SAFE_NO_PAD.encode(&auth_tag[..16]),
        ciphertext: URL_SAFE_NO_PAD.encode(plaintext),
        aad_hash: Some(URL_SAFE_NO_PAD.encode(aad_hash)),
    };

    serde_json::to_string(&blob).map_err(|_| ECALL_UNSUPPORTED)
}

pub fn decrypt_credential(
    _tenant_id: &str,
    _user_id_hash: &str,
    _credential_id: &str,
    blob_json: &str,
) -> Result<Vec<u8>, i32> {
    let blob: StubEncryptedBlob =
        serde_json::from_str(blob_json).map_err(|_| ECALL_INVALID_INPUT)?;
    URL_SAFE_NO_PAD
        .decode(blob.ciphertext.as_bytes())
        .map_err(|_| ECALL_INVALID_INPUT)
}
