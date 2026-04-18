mod ecalls;

use ecalls::{
    ECALL_INVALID_INPUT, ECALL_SUCCESS, ECALL_UNSUPPORTED, ENCLAVE_BLOB_BUFFER_LEN,
    SGX_MEASUREMENT_LEN, SGX_REPORT_DATA_LEN, SGX_REPORT_LEN, SGX_SEALING_KEY_LEN,
    SGX_TARGET_INFO_LEN,
};
use std::ptr;

#[no_mangle]
pub extern "C" fn credbridge_enclave_get_identity(
    mrenclave: *mut u8,
    mrenclave_len: usize,
    mrsigner: *mut u8,
    mrsigner_len: usize,
) -> i32 {
    if mrenclave.is_null()
        || mrsigner.is_null()
        || mrenclave_len != SGX_MEASUREMENT_LEN
        || mrsigner_len != SGX_MEASUREMENT_LEN
    {
        return ECALL_INVALID_INPUT;
    }

    let (identity_mrenclave, identity_mrsigner) = ecalls::get_identity();
    unsafe {
        ptr::copy_nonoverlapping(identity_mrenclave.as_ptr(), mrenclave, SGX_MEASUREMENT_LEN);
        ptr::copy_nonoverlapping(identity_mrsigner.as_ptr(), mrsigner, SGX_MEASUREMENT_LEN);
    }
    ECALL_SUCCESS
}

#[no_mangle]
pub extern "C" fn credbridge_enclave_get_report(
    report_data: *const u8,
    report_data_len: usize,
    report_bytes: *mut u8,
    report_bytes_len: usize,
    written_len: *mut usize,
) -> i32 {
    if report_data.is_null()
        || report_bytes.is_null()
        || written_len.is_null()
        || report_data_len != SGX_REPORT_DATA_LEN
        || report_bytes_len != SGX_REPORT_LEN
    {
        return ECALL_INVALID_INPUT;
    }

    let report_data = unsafe { std::slice::from_raw_parts(report_data, report_data_len) };
    let Ok(report) = ecalls::get_report(report_data) else {
        return ECALL_UNSUPPORTED;
    };

    unsafe {
        ptr::copy_nonoverlapping(report.as_ptr(), report_bytes, SGX_REPORT_LEN);
        *written_len = SGX_REPORT_LEN;
    }
    ECALL_SUCCESS
}

#[no_mangle]
pub extern "C" fn credbridge_enclave_get_targeted_report(
    target_info: *const u8,
    target_info_len: usize,
    report_data: *const u8,
    report_data_len: usize,
    report_bytes: *mut u8,
    report_bytes_len: usize,
    written_len: *mut usize,
) -> i32 {
    if target_info.is_null()
        || report_data.is_null()
        || report_bytes.is_null()
        || written_len.is_null()
        || target_info_len != SGX_TARGET_INFO_LEN
        || report_data_len != SGX_REPORT_DATA_LEN
        || report_bytes_len != SGX_REPORT_LEN
    {
        return ECALL_INVALID_INPUT;
    }

    let target_info = unsafe { std::slice::from_raw_parts(target_info, target_info_len) };
    let report_data = unsafe { std::slice::from_raw_parts(report_data, report_data_len) };
    let Ok(report) = ecalls::get_targeted_report(target_info, report_data) else {
        return ECALL_UNSUPPORTED;
    };

    unsafe {
        ptr::copy_nonoverlapping(report.as_ptr(), report_bytes, SGX_REPORT_LEN);
        *written_len = SGX_REPORT_LEN;
    }
    ECALL_SUCCESS
}

#[no_mangle]
pub extern "C" fn credbridge_enclave_get_sealing_key(
    policy: u32,
    key_bytes: *mut u8,
    key_bytes_len: usize,
    written_len: *mut usize,
) -> i32 {
    if key_bytes.is_null() || written_len.is_null() || key_bytes_len != SGX_SEALING_KEY_LEN {
        return ECALL_INVALID_INPUT;
    }

    let Ok(key) = ecalls::get_sealing_key(policy) else {
        return ECALL_UNSUPPORTED;
    };

    unsafe {
        ptr::copy_nonoverlapping(key.as_ptr(), key_bytes, SGX_SEALING_KEY_LEN);
        *written_len = SGX_SEALING_KEY_LEN;
    }
    ECALL_SUCCESS
}

#[no_mangle]
pub extern "C" fn credbridge_enclave_encrypt_credential(
    tenant_id: *const std::os::raw::c_char,
    user_id_hash: *const std::os::raw::c_char,
    credential_id: *const std::os::raw::c_char,
    plaintext: *const u8,
    plaintext_len: usize,
    blob_json: *mut u8,
    blob_json_len: usize,
    written_len: *mut usize,
) -> i32 {
    if tenant_id.is_null()
        || user_id_hash.is_null()
        || credential_id.is_null()
        || plaintext.is_null()
        || blob_json.is_null()
        || written_len.is_null()
        || blob_json_len == 0
    {
        return ECALL_INVALID_INPUT;
    }

    let tenant_id = unsafe { std::ffi::CStr::from_ptr(tenant_id) };
    let user_id_hash = unsafe { std::ffi::CStr::from_ptr(user_id_hash) };
    let credential_id = unsafe { std::ffi::CStr::from_ptr(credential_id) };
    let plaintext = unsafe { std::slice::from_raw_parts(plaintext, plaintext_len) };

    let Ok(blob_json_value) = ecalls::encrypt_credential(
        &tenant_id.to_string_lossy(),
        &user_id_hash.to_string_lossy(),
        &credential_id.to_string_lossy(),
        plaintext,
    ) else {
        return ECALL_UNSUPPORTED;
    };

    let blob_json_bytes = blob_json_value.as_bytes();
    if blob_json_bytes.len() > blob_json_len || blob_json_bytes.len() > ENCLAVE_BLOB_BUFFER_LEN {
        return ECALL_INVALID_INPUT;
    }

    unsafe {
        ptr::copy_nonoverlapping(blob_json_bytes.as_ptr(), blob_json, blob_json_bytes.len());
        *written_len = blob_json_bytes.len();
    }
    ECALL_SUCCESS
}

#[no_mangle]
pub extern "C" fn credbridge_enclave_decrypt_credential(
    tenant_id: *const std::os::raw::c_char,
    user_id_hash: *const std::os::raw::c_char,
    credential_id: *const std::os::raw::c_char,
    blob_json: *const std::os::raw::c_char,
    plaintext: *mut u8,
    plaintext_len: usize,
    written_len: *mut usize,
) -> i32 {
    if tenant_id.is_null()
        || user_id_hash.is_null()
        || credential_id.is_null()
        || blob_json.is_null()
        || plaintext.is_null()
        || written_len.is_null()
    {
        return ECALL_INVALID_INPUT;
    }

    let tenant_id = unsafe { std::ffi::CStr::from_ptr(tenant_id) };
    let user_id_hash = unsafe { std::ffi::CStr::from_ptr(user_id_hash) };
    let credential_id = unsafe { std::ffi::CStr::from_ptr(credential_id) };
    let blob_json = unsafe { std::ffi::CStr::from_ptr(blob_json) };

    let Ok(plaintext_bytes) = ecalls::decrypt_credential(
        &tenant_id.to_string_lossy(),
        &user_id_hash.to_string_lossy(),
        &credential_id.to_string_lossy(),
        &blob_json.to_string_lossy(),
    ) else {
        return ECALL_UNSUPPORTED;
    };

    if plaintext_bytes.len() > plaintext_len {
        return ECALL_INVALID_INPUT;
    }

    unsafe {
        ptr::copy_nonoverlapping(plaintext_bytes.as_ptr(), plaintext, plaintext_bytes.len());
        *written_len = plaintext_bytes.len();
    }
    ECALL_SUCCESS
}
