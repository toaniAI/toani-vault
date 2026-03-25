#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${ROOT_DIR}/target/sgx-enclave"
UNSIGNED_SO="${BUILD_DIR}/libcredbridge_enclave.so"
SIGNED_SO="${BUILD_DIR}/credbridge_enclave.signed.so"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SGX enclave signing must run on Linux" >&2
  exit 1
fi

if [[ ! -f "${UNSIGNED_SO}" ]]; then
  echo "Unsigned enclave artifact not found at ${UNSIGNED_SO}" >&2
  exit 1
fi

if command -v sgx_sign >/dev/null 2>&1; then
  if [[ -n "${SGX_SIGNING_KEY:-}" && -f "${SGX_SIGNING_KEY}" ]]; then
    sgx_sign sign \
      -enclave "${UNSIGNED_SO}" \
      -key "${SGX_SIGNING_KEY}" \
      -out "${SIGNED_SO}" \
      >/dev/null
    echo "Signed enclave written to ${SIGNED_SO}"
    exit 0
  fi
  echo "sgx_sign is available but SGX_SIGNING_KEY is missing; copying unsigned artifact as a placeholder" >&2
fi

cp "${UNSIGNED_SO}" "${SIGNED_SO}"
echo "Generated placeholder signed artifact at ${SIGNED_SO}"
