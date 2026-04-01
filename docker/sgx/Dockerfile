FROM ubuntu:22.04 AS sgxsdk

ARG SGX_SDK_VERSION=2.28.100.1
ARG SGX_SDK_URL=https://download.01.org/intel-sgx/sgx-linux/2.28/distro/ubuntu22.04-server/sgx_linux_x64_sdk_2.28.100.1.bin

RUN apt-get update && apt-get install -y --no-install-recommends \
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

FROM rust:1.88.0-slim-bookworm

COPY --from=sgxsdk /opt/intel/sgxsdk /opt/intel/sgxsdk

ENV SGX_SDK=/opt/intel/sgxsdk
ENV PATH=/opt/intel/sgxsdk/bin/x64:${PATH}

RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    cmake \
    libssl-dev \
    pkg-config \
    protobuf-compiler \
    libsodium-dev \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

COPY . /workspace

CMD ["bash", "-lc", "scripts/build-sgx-enclave.sh && scripts/sign-sgx-enclave.sh && cargo test --features tee-hardware --lib --tests --no-run"]
