#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENCLAVE_DIR="${ROOT_DIR}/sgx-enclave"
BUILD_DIR="${ROOT_DIR}/target/sgx-enclave"
GENERATED_DIR="${BUILD_DIR}/generated"
TRUSTED_OBJ_DIR="${BUILD_DIR}/trusted"
UNTRUSTED_OBJ_DIR="${BUILD_DIR}/untrusted"
UNSIGNED_ENCLAVE_SO="${BUILD_DIR}/libcredbridge_enclave.so"
HOST_BRIDGE_SO="${BUILD_DIR}/libcredbridge_sgx_urts_bridge.so"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SGX enclave build must run on Linux" >&2
  exit 1
fi

if [[ -f /opt/intel/sgxsdk/environment ]]; then
  # shellcheck disable=SC1091
  source /opt/intel/sgxsdk/environment
fi

SGX_SDK="${SGX_SDK:-/opt/intel/sgxsdk}"
SGX_ARCH="${SGX_ARCH:-x64}"
SGX_BIN_DIR="${SGX_SDK}/bin/${SGX_ARCH}"
SGX_EDGER8R="${SGX_EDGER8R:-}"
SGX_COMMON_CFLAGS=(-m64 -O2 -fPIC -Wall -Wextra)
SGX_TRUSTED_CFLAGS=("${SGX_COMMON_CFLAGS[@]}" -Wno-implicit-function-declaration -nostdinc -fvisibility=hidden -fpie -fstack-protector)
SGX_UNTRUSTED_CFLAGS=("${SGX_COMMON_CFLAGS[@]}")
SGX_TRUSTED_INCLUDES=(
  -I"${SGX_SDK}/include"
  -I"${SGX_SDK}/include/tlibc"
  -I"${SGX_SDK}/include/libcxx"
  -I"${GENERATED_DIR}"
)
SGX_UNTRUSTED_INCLUDES=(
  -I"${SGX_SDK}/include"
  -I"${GENERATED_DIR}"
)

if [[ "${SGX_ARCH}" != "x64" ]]; then
  echo "Only SGX_ARCH=x64 is supported by this build script" >&2
  exit 1
fi

if [[ -z "${SGX_EDGER8R}" ]] && command -v sgx_edger8r >/dev/null 2>&1; then
  SGX_EDGER8R="$(command -v sgx_edger8r)"
elif [[ -z "${SGX_EDGER8R}" ]] && [[ -x "${SGX_BIN_DIR}/sgx_edger8r" ]]; then
  SGX_EDGER8R="${SGX_BIN_DIR}/sgx_edger8r"
fi

if [[ -z "${SGX_EDGER8R}" ]]; then
  echo "sgx_edger8r is required; install Intel SGX SDK and ensure it is on PATH" >&2
  exit 1
fi

if [[ ! -d "${SGX_SDK}/include" ]]; then
  echo "Intel SGX SDK headers not found under ${SGX_SDK}; set SGX_SDK correctly" >&2
  exit 1
fi

mkdir -p "${BUILD_DIR}" "${GENERATED_DIR}" "${TRUSTED_OBJ_DIR}" "${UNTRUSTED_OBJ_DIR}"
rm -f "${GENERATED_DIR}/Enclave_t.c" "${GENERATED_DIR}/Enclave_t.h" "${GENERATED_DIR}/Enclave_u.c" "${GENERATED_DIR}/Enclave_u.h"

if [[ "${SKIP_SGX_CHECK:-}" != "1" ]] && [[ -x "${ROOT_DIR}/scripts/check_sgx_environment.sh" ]]; then
  "${ROOT_DIR}/scripts/check_sgx_environment.sh"
fi

echo "Generating SGX edge code..."
"${SGX_EDGER8R}" "${ENCLAVE_DIR}/Enclave.edl" \
  --trusted-dir "${GENERATED_DIR}" \
  --untrusted-dir "${GENERATED_DIR}" \
  --search-path "${SGX_SDK}/include" \
  --search-path "${SGX_SDK}/include/tlibc" \
  --search-path "${SGX_SDK}/include/libcxx" \
  --search-path "${ENCLAVE_DIR}"

echo "Compiling trusted enclave objects..."
gcc "${SGX_TRUSTED_CFLAGS[@]}" "${SGX_TRUSTED_INCLUDES[@]}" \
  -c "${ENCLAVE_DIR}/trusted/Enclave.c" -o "${TRUSTED_OBJ_DIR}/Enclave.o"
gcc "${SGX_TRUSTED_CFLAGS[@]}" "${SGX_TRUSTED_INCLUDES[@]}" \
  -c "${GENERATED_DIR}/Enclave_t.c" -o "${TRUSTED_OBJ_DIR}/Enclave_t.o"

echo "Linking unsigned enclave image..."
g++ -m64 \
  -Wl,--no-undefined -nostdlib -nodefaultlibs -nostartfiles \
  -L"${SGX_SDK}/lib64" \
  -Wl,--whole-archive -lsgx_trts -Wl,--no-whole-archive \
  -Wl,--start-group -lsgx_tstdc -lsgx_tcxx -lsgx_tcrypto -Wl,--end-group \
  -Wl,-Bstatic -Wl,-Bsymbolic -Wl,--export-dynamic \
  -Wl,--defsym,__ImageBase=0 -Wl,-pie,-eenclave_entry \
  "${TRUSTED_OBJ_DIR}/Enclave.o" "${TRUSTED_OBJ_DIR}/Enclave_t.o" \
  -o "${UNSIGNED_ENCLAVE_SO}"

echo "Compiling untrusted host bridge objects..."
gcc "${SGX_UNTRUSTED_CFLAGS[@]}" "${SGX_UNTRUSTED_INCLUDES[@]}" \
  -c "${ENCLAVE_DIR}/untrusted/host_runtime_bridge.c" -o "${UNTRUSTED_OBJ_DIR}/host_runtime_bridge.o"
gcc "${SGX_UNTRUSTED_CFLAGS[@]}" "${SGX_UNTRUSTED_INCLUDES[@]}" \
  -c "${GENERATED_DIR}/Enclave_u.c" -o "${UNTRUSTED_OBJ_DIR}/Enclave_u.o"

echo "Linking URTS host bridge..."
g++ -m64 -shared \
  "${UNTRUSTED_OBJ_DIR}/host_runtime_bridge.o" "${UNTRUSTED_OBJ_DIR}/Enclave_u.o" \
  -L"${SGX_SDK}/lib64" -lsgx_urts -lpthread -ldl \
  -o "${HOST_BRIDGE_SO}"

echo "Built unsigned enclave: ${UNSIGNED_ENCLAVE_SO}"
echo "Built URTS host bridge: ${HOST_BRIDGE_SO}"
