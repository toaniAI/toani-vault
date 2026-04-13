#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sgx_error.h"
#include "sgx_urts.h"
#include "Enclave_u.h"

#define ECALL_SUCCESS 0
#define ECALL_UNSUPPORTED 1
#define ECALL_INVALID_INPUT 2
#define ECALL_BUFFER_TOO_SMALL 3
#define ECALL_INTERNAL_ERROR 4

#define LAST_ERROR_CAPACITY 1024

struct credbridge_sgx_runtime {
    sgx_enclave_id_t enclave_id;
    int debug;
};

static __thread char g_last_error[LAST_ERROR_CAPACITY];

static void set_last_error(const char* fmt, ...) {
    va_list args;
    va_start(args, fmt);
    vsnprintf(g_last_error, sizeof(g_last_error), fmt, args);
    va_end(args);
}

static int map_sgx_status(const char* operation, sgx_status_t status) {
    if (status == SGX_SUCCESS) {
        return ECALL_SUCCESS;
    }

    set_last_error("%s failed with SGX status 0x%08x", operation, (unsigned int)status);
    return ECALL_INTERNAL_ERROR;
}

static int map_ecall_status(const char* operation, int32_t ecall_status) {
    if (ecall_status == ECALL_SUCCESS) {
        return ECALL_SUCCESS;
    }

    set_last_error("%s returned enclave status %d", operation, ecall_status);
    return ecall_status;
}

const char* credbridge_sgx_runtime_last_error(void) {
    return g_last_error;
}

int32_t credbridge_sgx_runtime_open(const char* enclave_path, int debug, void** out_runtime) {
    struct credbridge_sgx_runtime* runtime = NULL;
    sgx_enclave_id_t enclave_id = 0;
    sgx_misc_attribute_t misc_attr;
    sgx_status_t status;

    memset(&misc_attr, 0, sizeof(misc_attr));
    g_last_error[0] = '\0';

    if (enclave_path == NULL || out_runtime == NULL) {
        set_last_error("credbridge_sgx_runtime_open received invalid input");
        return ECALL_INVALID_INPUT;
    }

    runtime = (struct credbridge_sgx_runtime*)calloc(1, sizeof(*runtime));
    if (runtime == NULL) {
        set_last_error("failed to allocate runtime state");
        return ECALL_INTERNAL_ERROR;
    }

    status = sgx_create_enclave(enclave_path, debug, NULL, NULL, &enclave_id, &misc_attr);
    if (status != SGX_SUCCESS) {
        free(runtime);
        return map_sgx_status("sgx_create_enclave", status);
    }

    runtime->enclave_id = enclave_id;
    runtime->debug = debug;
    *out_runtime = runtime;
    return ECALL_SUCCESS;
}

int32_t credbridge_sgx_runtime_close(void* runtime_handle) {
    struct credbridge_sgx_runtime* runtime = (struct credbridge_sgx_runtime*)runtime_handle;
    sgx_status_t status;

    g_last_error[0] = '\0';
    if (runtime == NULL) {
        return ECALL_SUCCESS;
    }

    status = sgx_destroy_enclave(runtime->enclave_id);
    free(runtime);

    if (status != SGX_SUCCESS) {
        return map_sgx_status("sgx_destroy_enclave", status);
    }
    return ECALL_SUCCESS;
}

int32_t credbridge_sgx_runtime_get_identity(
    void* runtime_handle,
    uint8_t* mrenclave,
    size_t mrenclave_len,
    uint8_t* mrsigner,
    size_t mrsigner_len) {
    struct credbridge_sgx_runtime* runtime = (struct credbridge_sgx_runtime*)runtime_handle;
    int32_t ecall_status = ECALL_INTERNAL_ERROR;
    sgx_status_t status;

    g_last_error[0] = '\0';
    if (runtime == NULL) {
        set_last_error("credbridge_sgx_runtime_get_identity requires an open runtime");
        return ECALL_INVALID_INPUT;
    }

    status = credbridge_enclave_get_identity(runtime->enclave_id, &ecall_status, mrenclave, mrenclave_len, mrsigner, mrsigner_len);
    if (status != SGX_SUCCESS) {
        return map_sgx_status("credbridge_enclave_get_identity", status);
    }
    return map_ecall_status("credbridge_enclave_get_identity", ecall_status);
}

int32_t credbridge_sgx_runtime_get_targeted_report(
    void* runtime_handle,
    const uint8_t* target_info,
    size_t target_info_len,
    const uint8_t* report_data,
    size_t report_data_len,
    uint8_t* report_bytes,
    size_t report_bytes_len,
    size_t* written_len) {
    struct credbridge_sgx_runtime* runtime = (struct credbridge_sgx_runtime*)runtime_handle;
    int32_t ecall_status = ECALL_INTERNAL_ERROR;
    sgx_status_t status;

    g_last_error[0] = '\0';
    if (runtime == NULL) {
        set_last_error("credbridge_sgx_runtime_get_targeted_report requires an open runtime");
        return ECALL_INVALID_INPUT;
    }

    status = credbridge_enclave_get_targeted_report(
        runtime->enclave_id,
        &ecall_status,
        target_info,
        target_info_len,
        report_data,
        report_data_len,
        report_bytes,
        report_bytes_len,
        written_len);
    if (status != SGX_SUCCESS) {
        return map_sgx_status("credbridge_enclave_get_targeted_report", status);
    }
    return map_ecall_status("credbridge_enclave_get_targeted_report", ecall_status);
}

int32_t credbridge_sgx_runtime_get_sealing_key(
    void* runtime_handle,
    uint32_t policy,
    uint8_t* key_bytes,
    size_t key_bytes_len,
    size_t* written_len) {
    struct credbridge_sgx_runtime* runtime = (struct credbridge_sgx_runtime*)runtime_handle;
    int32_t ecall_status = ECALL_INTERNAL_ERROR;
    sgx_status_t status;

    g_last_error[0] = '\0';
    if (runtime == NULL) {
        set_last_error("credbridge_sgx_runtime_get_sealing_key requires an open runtime");
        return ECALL_INVALID_INPUT;
    }

    status = credbridge_enclave_get_sealing_key(runtime->enclave_id, &ecall_status, policy, key_bytes, key_bytes_len, written_len);
    if (status != SGX_SUCCESS) {
        return map_sgx_status("credbridge_enclave_get_sealing_key", status);
    }
    return map_ecall_status("credbridge_enclave_get_sealing_key", ecall_status);
}

int32_t credbridge_sgx_runtime_encrypt_credential(
    void* runtime_handle,
    const char* tenant_id,
    const char* user_id_hash,
    const char* credential_id,
    const uint8_t* plaintext,
    size_t plaintext_len,
    uint8_t* blob_json,
    size_t blob_json_len,
    size_t* written_len) {
    (void)runtime_handle;
    (void)tenant_id;
    (void)user_id_hash;
    (void)credential_id;
    (void)plaintext;
    (void)plaintext_len;
    (void)blob_json;
    (void)blob_json_len;
    (void)written_len;
    set_last_error("credential encryption remains host-side in phase A; no enclave ECALL is exposed for this operation");
    return ECALL_UNSUPPORTED;
}

int32_t credbridge_sgx_runtime_decrypt_credential(
    void* runtime_handle,
    const char* tenant_id,
    const char* user_id_hash,
    const char* credential_id,
    const char* blob_json,
    uint8_t* plaintext,
    size_t plaintext_len,
    size_t* written_len) {
    (void)runtime_handle;
    (void)tenant_id;
    (void)user_id_hash;
    (void)credential_id;
    (void)blob_json;
    (void)plaintext;
    (void)plaintext_len;
    (void)written_len;
    set_last_error("credential decryption remains host-side in phase A; no enclave ECALL is exposed for this operation");
    return ECALL_UNSUPPORTED;
}
