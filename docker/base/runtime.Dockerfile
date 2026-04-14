ARG UBUNTU_IMAGE=ubuntu:22.04
ARG NSJAIL_ARCHIVE=nsjail-3.6.tar.gz
ARG KAFEL_GIT_URL=https://github.com/google/kafel.git

FROM ${UBUNTU_IMAGE} AS nsjail-builder

ARG NSJAIL_ARCHIVE
ARG KAFEL_GIT_URL

RUN apt-get update && apt-get install -y --no-install-recommends \
    autoconf \
    automake \
    bison \
    ca-certificates \
    flex \
    g++ \
    git \
    libnl-route-3-dev \
    libprotobuf-dev \
    make \
    pkg-config \
    protobuf-compiler \
    tar \
    && update-ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY ${NSJAIL_ARCHIVE} /tmp/nsjail.tar.gz

RUN mkdir -p /tmp/nsjail-src \
    && tar -xzf /tmp/nsjail.tar.gz -C /tmp/nsjail-src --strip-components=1 \
    && if [ ! -f /tmp/nsjail-src/kafel/Makefile ]; then \
         rm -rf /tmp/nsjail-src/kafel; \
         git clone --depth=1 "${KAFEL_GIT_URL}" /tmp/nsjail-src/kafel; \
       fi \
    && cd /tmp/nsjail-src \
    && make -j"$(nproc)" \
    && strip nsjail

FROM ${UBUNTU_IMAGE}

WORKDIR /app

ENV SEALED_STORAGE_PATH=/app/data/sealed
ENV SGX_AESM_SOCKET_PATH=/var/run/aesmd/aesm.socket
ENV NODE_PATH=/opt/credbridge-browser-runtime/node_modules
ENV PLAYWRIGHT_SKIP_BROWSER_GC=1

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    gnupg \
    libnl-route-3-200 \
    libprotobuf23 \
    libssl3 \
    libc-bin \
    procps \
    uidmap \
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

RUN set -eux; \
    mkdir -p /etc/apt/keyrings; \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
      | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg; \
    echo 'deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_20.x nodistro main' \
      > /etc/apt/sources.list.d/nodesource.list; \
    apt-get update; \
    apt-get install -y --no-install-recommends nodejs; \
    rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    mkdir -p /opt/credbridge-browser-runtime; \
    cd /opt/credbridge-browser-runtime; \
    printf '%s\n' '{' \
      '  "name": "credbridge-browser-runtime",' \
      '  "private": true,' \
      '  "dependencies": {' \
      '    "playwright": "1.58.2"' \
      '  }' \
      '}' > package.json; \
    npm install --omit=dev --no-fund --no-audit; \
    npx playwright install --with-deps chromium; \
    npm cache clean --force

COPY --from=nsjail-builder /tmp/nsjail-src/nsjail /usr/local/bin/nsjail

RUN chmod +x /usr/local/bin/nsjail \
    && ln -sf /usr/local/bin/nsjail /usr/bin/nsjail \
    && command -v newuidmap \
    && command -v newgidmap \
    && test -u /usr/bin/newuidmap \
    && test -u /usr/bin/newgidmap \
    && printf '%s\n' 'root:100000:65536' >> /etc/subuid \
    && printf '%s\n' 'root:100000:65536' >> /etc/subgid \
    && printf '%s\n' 'appuser:100000:65536' >> /etc/subuid \
    && printf '%s\n' 'appuser:100000:65536' >> /etc/subgid
