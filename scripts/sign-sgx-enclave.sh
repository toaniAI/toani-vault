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

if [[ ! -f "${UNSIGNED_SO}" ]]; then
  echo "Unsigned enclave artifact not found at ${UNSIGNED_SO}" >&2
  exit 1
fi

if ! command -v sgx_sign >/dev/null 2>&1; then
  echo "sgx_sign is required; install Intel SGX SDK and ensure it is on PATH" >&2
  exit 1
fi

SIGNING_KEY="${SGX_SIGNING_KEY:-}"
if [[ -z "${SIGNING_KEY}" && -f /opt/intel/sgxsdk/SampleCode/SampleEnclave/Enclave_private.pem ]]; then
  SIGNING_KEY="/opt/intel/sgxsdk/SampleCode/SampleEnclave/Enclave_private.pem"
fi

if [[ -z "${SIGNING_KEY}" ]]; then
  echo "SGX_SIGNING_KEY is required (or the Intel sample key must exist under /opt/intel/sgxsdk/SampleCode/SampleEnclave/Enclave_private.pem)" >&2
  exit 1
fi

if [[ ! -f "${SIGNING_KEY}" ]]; then
  echo "Signing key not found at ${SIGNING_KEY}" >&2
  exit 1
fi

sgx_sign sign \
  -enclave "${UNSIGNED_SO}" \
  -key "${SIGNING_KEY}" \
  -config "${ENCLAVE_CONFIG}" \
  -out "${SIGNED_SO}" \
  >/dev/null

echo "Signed enclave written to ${SIGNED_SO}"
