ARG UBUNTU_IMAGE=ubuntu:22.04

FROM ${UBUNTU_IMAGE}

WORKDIR /app

ENV SEALED_STORAGE_PATH=/app/data/sealed
ENV SGX_AESM_SOCKET_PATH=/var/run/aesmd/aesm.socket

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gnupg \
    libssl3 \
    libc-bin \
    procps \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    mkdir -p /usr/share/keyrings /etc/apt/sources.list.d; \
    curl -fsSL https://download.01.org/intel-sgx/sgx_repo/ubuntu/intel-sgx-deb.key \
      | gpg --dearmor -o /usr/share/keyrings/intel-sgx-keyring.gpg; \
    echo 'deb [arch=amd64 signed-by=/usr/share/keyrings/intel-sgx-keyring.gpg] https://download.01.org/intel-sgx/sgx_repo/ubuntu jammy main' \
      > /etc/apt/sources.list.d/intel-sgx.list; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
      libsgx-enclave-common \
      libsgx-urts \
      libsgx-dcap-ql \
      libsgx-dcap-quote-verify \
      libsgx-quote-ex \
      libsgx-dcap-default-qpl \
      sgx-aesm-service; \
    rm -rf /var/lib/apt/lists/*
