#include <stdint.h>
#include <stddef.h>
#include <string.h>

#include "sgx_report.h"
#include "sgx_tcrypto.h"
#include "sgx_tseal.h"
#include "sgx_trts.h"
#include "sgx_utils.h"

#define ECALL_SUCCESS 0
#define ECALL_UNSUPPORTED 1
#define ECALL_INVALID_INPUT 2
#define ECALL_BUFFER_TOO_SMALL 3
#define ECALL_INTERNAL_ERROR 4

#define CREDBRIDGE_KEY_POLICY_MRENCLAVE 0u
#define CREDBRIDGE_KEY_POLICY_MRSIGNER 1u

static int copy_report_bytes(const sgx_report_t* report, uint8_t* report_bytes, size_t report_bytes_len, size_t* written_len) {
    if (report == NULL || report_bytes == NULL || written_len == NULL) {
        return ECALL_INVALID_INPUT;
    }
    if (report_bytes_len < sizeof(sgx_report_t)) {
        return ECALL_BUFFER_TOO_SMALL;
    }

    memcpy(report_bytes, report, sizeof(sgx_report_t));
    *written_len = sizeof(sgx_report_t);
    return ECALL_SUCCESS;
}

int32_t credbridge_enclave_get_identity(
    uint8_t* mrenclave,
    size_t mrenclave_len,
    uint8_t* mrsigner,
    size_t mrsigner_len) {
    const sgx_report_t* self_report = NULL;

    if (mrenclave == NULL || mrsigner == NULL || mrenclave_len != SGX_HASH_SIZE || mrsigner_len != SGX_HASH_SIZE) {
        return ECALL_INVALID_INPUT;
    }

    self_report = sgx_self_report();
    if (self_report == NULL) {
        return ECALL_INTERNAL_ERROR;
    }

    memcpy(mrenclave, self_report->body.mr_enclave.m, SGX_HASH_SIZE);
    memcpy(mrsigner, self_report->body.mr_signer.m, SGX_HASH_SIZE);
    return ECALL_SUCCESS;
}

int32_t credbridge_enclave_get_report(
    const uint8_t* report_data,
    size_t report_data_len,
    uint8_t* report_bytes,
    size_t report_bytes_len,
    size_t* written_len) {
    sgx_target_info_t self_target;
    sgx_report_t report;
    sgx_report_data_t report_data_value;
    sgx_status_t status;

    if (report_data == NULL || report_bytes == NULL || written_len == NULL || report_data_len != SGX_REPORT_DATA_SIZE) {
        return ECALL_INVALID_INPUT;
    }

    memset(&self_target, 0, sizeof(self_target));
    memset(&report, 0, sizeof(report));
    memset(&report_data_value, 0, sizeof(report_data_value));
    memcpy(report_data_value.d, report_data, SGX_REPORT_DATA_SIZE);

    status = sgx_self_target(&self_target);
    if (status != SGX_SUCCESS) {
        return ECALL_INTERNAL_ERROR;
    }

    status = sgx_create_report(&self_target, &report_data_value, &report);
    if (status != SGX_SUCCESS) {
        return ECALL_INTERNAL_ERROR;
    }

    return copy_report_bytes(&report, report_bytes, report_bytes_len, written_len);
}

int32_t credbridge_enclave_get_targeted_report(
    const uint8_t* target_info,
    size_t target_info_len,
    const uint8_t* report_data,
    size_t report_data_len,
    uint8_t* report_bytes,
    size_t report_bytes_len,
    size_t* written_len) {
    sgx_report_t report;
    sgx_report_data_t report_data_value;
    sgx_status_t status;

    if (target_info == NULL || report_data == NULL || report_bytes == NULL || written_len == NULL) {
        return ECALL_INVALID_INPUT;
    }
    if (target_info_len != sizeof(sgx_target_info_t) || report_data_len != SGX_REPORT_DATA_SIZE) {
        return ECALL_INVALID_INPUT;
    }

    memset(&report, 0, sizeof(report));
    memset(&report_data_value, 0, sizeof(report_data_value));
    memcpy(report_data_value.d, report_data, SGX_REPORT_DATA_SIZE);

    status = sgx_create_report((const sgx_target_info_t*)target_info, &report_data_value, &report);
    if (status != SGX_SUCCESS) {
        return ECALL_INTERNAL_ERROR;
    }

    return copy_report_bytes(&report, report_bytes, report_bytes_len, written_len);
}

int32_t credbridge_enclave_get_sealing_key(
    uint32_t policy,
    uint8_t* key_bytes,
    size_t key_bytes_len,
    size_t* written_len) {
    sgx_key_request_t request;
    sgx_key_128bit_t sgx_key;
    sgx_sha256_hash_t digest;
    uint8_t hash_input[sizeof(sgx_key) + sizeof(uint32_t)];
    sgx_status_t status;

    if (key_bytes == NULL || written_len == NULL) {
        return ECALL_INVALID_INPUT;
    }
    if (key_bytes_len < SGX_HASH_SIZE) {
        return ECALL_BUFFER_TOO_SMALL;
    }

    memset(&request, 0, sizeof(request));
    memset(&sgx_key, 0, sizeof(sgx_key));
    memset(&digest, 0, sizeof(digest));
    memset(hash_input, 0, sizeof(hash_input));

    request.key_name = SGX_KEYSELECT_SEAL;
    switch (policy) {
        case CREDBRIDGE_KEY_POLICY_MRENCLAVE:
            request.key_policy = SGX_KEYPOLICY_MRENCLAVE;
            break;
        case CREDBRIDGE_KEY_POLICY_MRSIGNER:
            request.key_policy = SGX_KEYPOLICY_MRSIGNER;
            break;
        default:
            return ECALL_INVALID_INPUT;
    }

    status = sgx_get_key(&request, &sgx_key);
    if (status != SGX_SUCCESS) {
        return ECALL_INTERNAL_ERROR;
    }

    memcpy(hash_input, &sgx_key, sizeof(sgx_key));
    memcpy(hash_input + sizeof(sgx_key), &policy, sizeof(policy));
    status = sgx_sha256_msg(hash_input, (uint32_t)sizeof(hash_input), &digest);
    if (status != SGX_SUCCESS) {
        return ECALL_INTERNAL_ERROR;
    }

    memcpy(key_bytes, digest, SGX_HASH_SIZE);
    *written_len = SGX_HASH_SIZE;

    memset(&sgx_key, 0, sizeof(sgx_key));
    memset(hash_input, 0, sizeof(hash_input));
    return ECALL_SUCCESS;
}

int32_t credbridge_enclave_encrypt_credential(
    const char* tenant_id,
    const char* user_id_hash,
    const char* credential_id,
    const uint8_t* plaintext,
    size_t plaintext_len,
    uint8_t* blob_json,
    size_t blob_json_len,
    size_t* written_len) {
    (void)tenant_id;
    (void)user_id_hash;
    (void)credential_id;
    (void)plaintext;
    (void)plaintext_len;
    (void)blob_json;
    (void)blob_json_len;
    (void)written_len;
    return ECALL_UNSUPPORTED;
}

int32_t credbridge_enclave_decrypt_credential(
    const char* tenant_id,
    const char* user_id_hash,
    const char* credential_id,
    const char* blob_json,
    uint8_t* plaintext,
    size_t plaintext_len,
    size_t* written_len) {
    (void)tenant_id;
    (void)user_id_hash;
    (void)credential_id;
    (void)blob_json;
    (void)plaintext;
    (void)plaintext_len;
    (void)written_len;
    return ECALL_UNSUPPORTED;
}
