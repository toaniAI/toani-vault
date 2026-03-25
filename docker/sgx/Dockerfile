FROM rust:1.88.0-slim-bookworm

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
