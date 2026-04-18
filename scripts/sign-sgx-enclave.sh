#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${ROOT_DIR}/target/sgx-enclave"
UNSIGNED_SO="${BUILD_DIR}/libcredbridge_enclave.so"
SIGNED_SO="${BUILD_DIR}/credbridge_enclave.signed.so"
ENCLAVE_CONFIG="${ROOT_DIR}/sgx-enclave/Enclave.config.xml"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SGX enclave signing must run on Linux" >&2
  exit 1
fi

if [[ -f /opt/intel/sgxsdk/environment ]]; then
  # shellcheck disable=SC1091
  source /opt/intel/sgxsdk/environment
fi

SGX_SDK="${SGX_SDK:-/opt/intel/sgxsdk}"
SGX_ARCH="${SGX_ARCH:-x64}"
SGX_BIN_DIR="${SGX_SDK}/bin/${SGX_ARCH}"
SGX_SIGN_BIN="${SGX_SIGN_BIN:-}"

if [[ ! -f "${UNSIGNED_SO}" ]]; then
  echo "Unsigned enclave artifact not found at ${UNSIGNED_SO}" >&2
  exit 1
fi

if [[ -z "${SGX_SIGN_BIN}" ]] && command -v sgx_sign >/dev/null 2>&1; then
  SGX_SIGN_BIN="$(command -v sgx_sign)"
elif [[ -z "${SGX_SIGN_BIN}" ]] && [[ -x "${SGX_BIN_DIR}/sgx_sign" ]]; then
  SGX_SIGN_BIN="${SGX_BIN_DIR}/sgx_sign"
fi

if [[ -z "${SGX_SIGN_BIN}" ]]; then
  echo "sgx_sign is required; install Intel SGX SDK and ensure it is on PATH" >&2
  exit 1
fi

SIGNING_KEY="${SGX_SIGNING_KEY:-}"
if [[ -z "${SIGNING_KEY}" && -f /opt/intel/sgxsdk/SampleCode/SampleEnclave/Enclave_private.pem ]]; then
  SIGNING_KEY="/opt/intel/sgxsdk/SampleCode/SampleEnclave/Enclave_private.pem"
fi

if [[ -z "${SIGNING_KEY}" ]]; then
  cp -f "${UNSIGNED_SO}" "${SIGNED_SO}"
  echo "Warning: SGX_SIGNING_KEY is not set and Intel sample key is unavailable; copied unsigned enclave as placeholder to ${SIGNED_SO}" >&2
  echo "Warning: this artifact is for compile checks only and must not be used in production" >&2
  exit 0
fi

if [[ ! -f "${SIGNING_KEY}" ]]; then
  cp -f "${UNSIGNED_SO}" "${SIGNED_SO}"
  echo "Warning: signing key not found at ${SIGNING_KEY}; copied unsigned enclave as placeholder to ${SIGNED_SO}" >&2
  echo "Warning: this artifact is for compile checks only and must not be used in production" >&2
  exit 0
fi

"${SGX_SIGN_BIN}" sign \
  -enclave "${UNSIGNED_SO}" \
  -key "${SIGNING_KEY}" \
  -config "${ENCLAVE_CONFIG}" \
  -out "${SIGNED_SO}" \
  >/dev/null

echo "Signed enclave written to ${SIGNED_SO}"
