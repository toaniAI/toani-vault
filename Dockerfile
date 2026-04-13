# CredBridge 后端 Dockerfile

FROM ubuntu:22.04 AS sgxsdk

ARG SGX_SDK_VERSION=2.28.100.1
ARG SGX_SDK_URL=https://download.01.org/intel-sgx/sgx-linux/2.28/distro/ubuntu22.04-server/sgx_linux_x64_sdk_2.28.100.1.bin

RUN apt-get update && apt-get install -y --no-install-recommends \
    binutils \
    ca-certificates \
    curl \
    gnupg \
    make \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    curl -fsSL "${SGX_SDK_URL}" -o /tmp/sgx_linux_x64_sdk.bin; \
    chmod +x /tmp/sgx_linux_x64_sdk.bin; \
    /tmp/sgx_linux_x64_sdk.bin --prefix=/opt/intel; \
    rm -f /tmp/sgx_linux_x64_sdk.bin

FROM rust:1.88.0-slim-bookworm AS builder

WORKDIR /app

ARG SGX_SIGNING_KEY

COPY --from=sgxsdk /opt/intel/sgxsdk /opt/intel/sgxsdk

ENV SGX_SDK=/opt/intel/sgxsdk
ENV PATH=/opt/intel/sgxsdk/bin/x64:${PATH}

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libsodium-dev \
    file \
    cmake \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# 先复制 manifest，尽量复用依赖缓存。
COPY Cargo.toml Cargo.lock ./
COPY sdk-rust/Cargo.toml ./sdk-rust/
COPY vault-service/Cargo.toml ./vault-service/
COPY examples/rust/Cargo.toml ./examples/rust/

RUN mkdir -p src sdk-rust/src vault-service/src examples/rust/src
RUN printf 'fn main() {}\n' > src/main.rs
RUN printf 'fn main() {}\n' > examples/rust/src/main.rs
RUN printf 'pub fn placeholder() {}\n' > sdk-rust/src/lib.rs
RUN printf 'pub fn placeholder() {}\n' > vault-service/src/lib.rs
RUN cargo build --release || true

COPY src ./src
COPY sdk-rust ./sdk-rust
COPY vault-service ./vault-service
COPY examples ./examples
COPY migrations ./migrations
COPY sgx-enclave ./sgx-enclave
COPY scripts ./scripts
RUN set -eu; \
    if [ -n "${SGX_SIGNING_KEY:-}" ]; then \
      umask 077; \
      printf '%s\n' "${SGX_SIGNING_KEY}" > /tmp/sgx-signing-key.pem; \
      export SGX_SIGNING_KEY=/tmp/sgx-signing-key.pem; \
    fi; \
    SKIP_SGX_CHECK=1 bash scripts/build-sgx-enclave.sh; \
    bash scripts/sign-sgx-enclave.sh; \
    rm -f /tmp/sgx-signing-key.pem
RUN cargo build --release --features tee-hardware

ARG RUNTIME_BASE_IMAGE=hub.bitkinetic.com/zkme/credbridge-runtime-sandbox:jammy-sgx2.28.100.1-nsjail3.6-node20-playwright1.58.2

FROM ${RUNTIME_BASE_IMAGE}

WORKDIR /app

ENV SEALED_STORAGE_PATH=/app/data/sealed
ENV SGX_AESM_SOCKET_PATH=/var/run/aesmd/aesm.socket

COPY --from=builder /app/target/release/vault-service /app/vault-service
COPY --from=builder /app/target/sgx-enclave/credbridge_enclave.signed.so /app/credbridge_enclave.signed.so
COPY --from=builder /app/target/sgx-enclave/libcredbridge_sgx_urts_bridge.so /app/libcredbridge_sgx_urts_bridge.so
COPY --from=builder /app/migrations /app/migrations
COPY frontend/scripts /app/frontend/scripts
COPY docker/scripts/healthcheck.sh /app/healthcheck.sh
COPY docker/scripts/runtime-preflight.sh /app/runtime-preflight.sh

RUN mkdir -p /app/data/sealed /app/config \
    && useradd -m -u 1000 appuser \
    && cp /etc/sgx_default_qcnl.conf /app/config/sgx_default_qcnl.conf \
    && ln -sf /app/config/sgx_default_qcnl.conf /etc/sgx_default_qcnl.conf \
    && chmod +x /app/healthcheck.sh /app/runtime-preflight.sh \
    && chown -R appuser:appuser /app

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD sh -c 'curl -fsS "http://127.0.0.1:${CREDBRIDGE_PORT:-8080}/health" >/dev/null || exit 1'

ENTRYPOINT ["/app/runtime-preflight.sh"]
CMD ["/app/vault-service"]
