FROM ubuntu:22.04 AS sgxsdk

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    mkdir -p /usr/share/keyrings /etc/apt/sources.list.d; \
    curl -fsSL https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key \
      | gpg --dearmor -o /usr/share/keyrings/intel-sgx-keyring.gpg; \
    echo 'deb [arch=amd64 signed-by=/usr/share/keyrings/intel-sgx-keyring.gpg] https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main' \
      > /etc/apt/sources.list.d/intel-sgx.list; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
      libsgx-sdk \
      libsgx-sdk-dev; \
    rm -rf /var/lib/apt/lists/*

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
