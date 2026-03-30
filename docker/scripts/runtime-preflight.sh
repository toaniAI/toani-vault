#!/bin/sh

set -eu

log() {
    echo "[runtime-preflight] $*" >&2
}

fail() {
    log "ERROR: $*"
    exit 1
}

find_existing_path() {
    for path in "$@"; do
        if [ -n "$path" ] && [ -e "$path" ]; then
            echo "$path"
            return 0
        fi
    done
    return 1
}

normalize_json_bool() {
    value="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"

    case "$value" in
        1|true|yes|on)
            echo "true"
            ;;
        0|false|no|off)
            echo "false"
            ;;
        *)
            fail "invalid boolean value for SGX/DCAP config: $1"
            ;;
    esac
}

library_exists() {
    env_path="$1"
    shift

    if [ -n "$env_path" ]; then
        configured_path="$(printenv "$env_path" 2>/dev/null || true)"
        if [ -n "$configured_path" ]; then
            [ -e "$configured_path" ] && return 0
            fail "configured library path $env_path=$configured_path does not exist"
        fi
    fi

    for candidate in "$@"; do
        if [ -e "$candidate" ]; then
            return 0
        fi
    done

    if command -v ldconfig >/dev/null 2>&1; then
        for candidate in "$@"; do
            base="$(basename "$candidate")"
            if ldconfig -p 2>/dev/null | grep -q "$base"; then
                return 0
            fi
        done
    fi

    return 1
}

write_qcnl_config() {
    config_path="${SGX_QCNL_CONFIG_PATH:-/etc/sgx_default_qcnl.conf}"
    pccs_url="${DCAP_PCCS_URL:-}"
    use_secure_cert="${DCAP_USE_SECURE_CERT:-true}"
    collateral_service="${DCAP_COLLATERAL_SERVICE:-}"
    pccs_api_version="${DCAP_PCCS_API_VERSION:-}"

    if [ -z "$pccs_url" ]; then
        if [ -f "$config_path" ]; then
            log "DCAP_PCCS_URL not set, keeping existing QCNL config at $config_path"
            return 0
        fi
        fail "DCAP_PCCS_URL is required when no QCNL config exists at $config_path"
    fi

    use_secure_cert="$(normalize_json_bool "$use_secure_cert")"

    cat > "$config_path" <<EOF
{
  "pccs_url": "$pccs_url",
  "use_secure_cert": $use_secure_cert$(if [ -n "$collateral_service" ]; then printf ',\n  "collateral_service": "%s"' "$collateral_service"; fi)$(if [ -n "$pccs_api_version" ]; then printf ',\n  "pccs_api_version": "%s"' "$pccs_api_version"; fi)
}
EOF

    log "wrote SGX QCNL config to $config_path"
}

TEE_MODE_VALUE="${TEE_MODE:-simulation}"

if [ "$TEE_MODE_VALUE" != "hardware" ]; then
    exec "$@"
fi

log "TEE_MODE=hardware detected, running SGX/DCAP preflight checks"

if [ -n "${TEE_ENCLAVE_PATH:-}" ]; then
    [ -f "$TEE_ENCLAVE_PATH" ] || fail "TEE_ENCLAVE_PATH does not point to a file: $TEE_ENCLAVE_PATH"
elif [ -f /app/credbridge_enclave.signed.so ]; then
    export TEE_ENCLAVE_PATH=/app/credbridge_enclave.signed.so
else
    fail "TEE_ENCLAVE_PATH is not set and /app/credbridge_enclave.signed.so is missing"
fi

SGX_DEVICE_PATH="$(find_existing_path \
    /dev/sgx_enclave \
    /dev/sgx/enclave \
    /dev/sgx \
    /dev/isgx || true)"
[ -n "$SGX_DEVICE_PATH" ] || fail "no SGX device found; mount /dev/sgx_enclave or equivalent into the container"
log "found SGX device at $SGX_DEVICE_PATH"

SGX_PROVISION_PATH="$(find_existing_path \
    /dev/sgx_provision \
    /dev/sgx/provision || true)"
AESM_SOCKET_PATH="${SGX_AESM_SOCKET_PATH:-/var/run/aesmd/aesm.socket}"
AESM_SOCKET_PRESENT=""
[ -S "$AESM_SOCKET_PATH" ] && AESM_SOCKET_PRESENT="$AESM_SOCKET_PATH"

if [ -z "$SGX_PROVISION_PATH" ] && [ -z "$AESM_SOCKET_PRESENT" ]; then
    fail "neither SGX provisioning device nor AESM socket is available; mount /dev/sgx_provision or $AESM_SOCKET_PATH"
fi

[ -n "$SGX_PROVISION_PATH" ] && log "found SGX provisioning device at $SGX_PROVISION_PATH"
[ -n "$AESM_SOCKET_PRESENT" ] && log "found AESM socket at $AESM_SOCKET_PRESENT"

library_exists \
    TEE_SGX_DCAP_QL_LIB_PATH \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_ql.so.1 \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_ql.so \
    /usr/lib/libsgx_dcap_ql.so.1 \
    /usr/lib/libsgx_dcap_ql.so \
    /lib/x86_64-linux-gnu/libsgx_dcap_ql.so.1 \
    /lib/x86_64-linux-gnu/libsgx_dcap_ql.so \
    || fail "Intel DCAP quote library is missing; install libsgx-dcap-ql"
log "Intel DCAP quote library is available"

library_exists \
    TEE_SGX_DCAP_QV_LIB_PATH \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so.1 \
    /usr/lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so \
    /usr/lib/libsgx_dcap_quoteverify.so.1 \
    /usr/lib/libsgx_dcap_quoteverify.so \
    /lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so.1 \
    /lib/x86_64-linux-gnu/libsgx_dcap_quoteverify.so \
    || fail "Intel DCAP quote verification library is missing; install libsgx-dcap-quote-verify"
log "Intel DCAP quote verification library is available"

library_exists \
    "" \
    /usr/lib/x86_64-linux-gnu/libdcap_quoteprov.so.1 \
    /usr/lib/x86_64-linux-gnu/libdcap_quoteprov.so \
    /usr/lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so.1 \
    /usr/lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so \
    /usr/lib/libdcap_quoteprov.so.1 \
    /usr/lib/libdcap_quoteprov.so \
    /usr/lib/libsgx_default_qcnl_wrapper.so.1 \
    /usr/lib/libsgx_default_qcnl_wrapper.so \
    /lib/x86_64-linux-gnu/libdcap_quoteprov.so.1 \
    /lib/x86_64-linux-gnu/libdcap_quoteprov.so \
    /lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so.1 \
    /lib/x86_64-linux-gnu/libsgx_default_qcnl_wrapper.so \
    || fail "Intel DCAP default QPL library is missing; install libsgx-dcap-default-qpl"
log "Intel DCAP default QPL library is available"

write_qcnl_config

exec "$@"
