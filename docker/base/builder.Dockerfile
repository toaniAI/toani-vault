ARG SGX_SDK_VERSION=2.28.100.1
ARG SGX_SDK_URL=https://download.01.org/intel-sgx/sgx-linux/2.28/distro/ubuntu22.04-server/sgx_linux_x64_sdk_2.28.100.1.bin
ARG RUST_IMAGE=rust:1.88.0-slim-bookworm
ARG UBUNTU_IMAGE=ubuntu:22.04
ARG APT_MIRROR_SCHEME=https
ARG APT_MIRROR_HOST=mirrors.aliyun.com

FROM ${UBUNTU_IMAGE} AS sgxsdk

ARG SGX_SDK_VERSION
ARG SGX_SDK_URL
ARG APT_MIRROR_SCHEME
ARG APT_MIRROR_HOST

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates; \
    rm -rf /var/lib/apt/lists/*; \
    sed -i "s|http://archive.ubuntu.com/ubuntu/|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/ubuntu/|g; s|http://security.ubuntu.com/ubuntu/|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/ubuntu/|g; s|https://archive.ubuntu.com/ubuntu/|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/ubuntu/|g; s|https://security.ubuntu.com/ubuntu/|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/ubuntu/|g" /etc/apt/sources.list; \
    apt-get update && apt-get install -y --no-install-recommends \
    binutils \
    curl \
    gnupg \
    make \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    curl -fsSL "${SGX_SDK_URL}" -o /tmp/sgx_linux_x64_sdk.bin; \
    chmod +x /tmp/sgx_linux_x64_sdk.bin; \
    /tmp/sgx_linux_x64_sdk.bin --prefix=/opt/intel; \
    rm -f /tmp/sgx_linux_x64_sdk.bin

FROM ${RUST_IMAGE}

WORKDIR /app

ARG APT_MIRROR_SCHEME
ARG APT_MIRROR_HOST

COPY --from=sgxsdk /opt/intel/sgxsdk /opt/intel/sgxsdk

ENV SGX_SDK=/opt/intel/sgxsdk
ENV PATH=/opt/intel/sgxsdk/bin/x64:${PATH}
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_PROFILE_RELEASE_LTO=off
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates; \
    rm -rf /var/lib/apt/lists/*; \
    if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
      sed -i "s|http://deb.debian.org/debian|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian|g; s|http://security.debian.org/debian-security|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian-security|g; s|https://deb.debian.org/debian|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian|g; s|https://security.debian.org/debian-security|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian-security|g" /etc/apt/sources.list.d/debian.sources; \
    fi; \
    if [ -f /etc/apt/sources.list ]; then \
      sed -i "s|http://deb.debian.org/debian|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian|g; s|http://security.debian.org/debian-security|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian-security|g; s|https://deb.debian.org/debian|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian|g; s|https://security.debian.org/debian-security|${APT_MIRROR_SCHEME}://${APT_MIRROR_HOST}/debian-security|g" /etc/apt/sources.list; \
    fi; \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    libsodium-dev \
    file \
    cmake \
    build-essential \
    && rm -rf /var/lib/apt/lists/*
