#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENCLAVE_DIR="${ROOT_DIR}/sgx-enclave"
BUILD_DIR="${ROOT_DIR}/target/sgx-enclave"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "SGX enclave build must run on Linux" >&2
  exit 1
fi

mkdir -p "${BUILD_DIR}"

if [[ -f /opt/intel/sgxsdk/environment ]]; then
  # shellcheck disable=SC1091
  source /opt/intel/sgxsdk/environment
fi

if [[ -x "${ROOT_DIR}/scripts/check_sgx_environment.sh" ]]; then
  "${ROOT_DIR}/scripts/check_sgx_environment.sh"
fi

echo "Building Phase A SGX enclave skeleton..."
cargo build --manifest-path "${ENCLAVE_DIR}/Cargo.toml" --release

ARTIFACT_CANDIDATE="${ENCLAVE_DIR}/target/release/libcredbridge_enclave.so"
if [[ -f "${ARTIFACT_CANDIDATE}" ]]; then
  cp "${ARTIFACT_CANDIDATE}" "${BUILD_DIR}/libcredbridge_enclave.so"
  echo "Copied enclave artifact to ${BUILD_DIR}/libcredbridge_enclave.so"
else
  echo "Expected artifact ${ARTIFACT_CANDIDATE} was not produced" >&2
  exit 1
fi
