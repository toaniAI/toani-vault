# CredBridge Vault Service - Docker 镜像
# 多阶段构建：编译阶段 + 运行时阶段

# =============================================================================
# 阶段 1: 编译器 (Builder)
# =============================================================================
FROM rust:1.88-slim-bookworm AS builder

# 安装编译依赖
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    make \
    autoconf \
    automake \
    libtool \
    && rm -rf /var/lib/apt/lists/*

# 创建构建目录
WORKDIR /build

# 先复制 Cargo 文件以利用缓存层
COPY Cargo.toml Cargo.lock ./

# 复制 src 目录
COPY src/ src/

# 创建虚拟 main.rs 来缓存依赖编译（空的模块文件用于缓存依赖编译）
RUN touch src/{api,audit,connector,crypto,mcp,models,services,tee,token,vault}/*.rs 2>/dev/null || true

# 编译依赖（缓存层）
RUN cargo build --release && \
    rm -rf src target/release/deps/vault_service* target/release/vault_service*

# 复制实际源码
COPY src ./src

# 重新编译应用程序
RUN cargo build --release

# 验证二进制文件
RUN ls -la /build/target/release/vault-service && \
    file /build/target/release/vault-service

# =============================================================================
# 阶段 1b: 编译 nsjail
# =============================================================================
FROM rust:1.88-slim-bookworm AS nsjail-builder

# 安装 nsjail 编译依赖
RUN apt-get update && apt-get install -y \
    git \
    pkg-config \
    libprotobuf-dev \
    libprotobuf32 \
    protobuf-compiler \
    libnl-route-3-dev \
    make \
    g++ \
    autoconf \
    automake \
    libtool \
    flex \
    bison \
    && rm -rf /var/lib/apt/lists/*

# 克隆并编译 nsjail
RUN git clone https://github.com/google/nsjail.git /nsjail && \
    cd /nsjail && \
    git submodule update --init && \
    make -j$(nproc) && \
    strip nsjail

# =============================================================================
# 阶段 2: 运行时 (Runtime)
# =============================================================================
FROM debian:bookworm-slim AS runtime

# 安装运行时依赖（包括 nsjail 需要的库）
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    libssl3 \
    procps \
    libprotobuf32 \
    libnl-route-3-200 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get clean

# 复制 nsjail 二进制文件
COPY --from=nsjail-builder /nsjail/nsjail /usr/local/bin/nsjail
RUN chmod +x /usr/local/bin/nsjail && nsjail -V || true

# 创建非特权用户
RUN groupadd -r credbridge --gid=1000 && \
    useradd -r -g credbridge --uid=1000 -s /bin/false -d /app credbridge

# 创建应用目录
WORKDIR /app

# 从构建阶段复制二进制文件
COPY --from=builder /build/target/release/vault-service /app/vault-service

# 复制健康检查脚本
COPY --from=builder /build/target/release/vault-service /app/healthcheck
COPY docker/scripts/healthcheck.sh /app/healthcheck.sh
RUN chmod +x /app/healthcheck.sh

# 创建必要的目录并设置权限
RUN mkdir -p /app/data /app/logs /app/config && \
    chown -R credbridge:credbridge /app

# 注意：nsjail 需要 root 权限创建命名空间
# 生产环境应使用 capabilities 限制权限
USER root

# 暴露服务端口
EXPOSE 8080

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD ["/app/healthcheck.sh"]

# 环境变量默认值
ENV RUST_LOG=info \
    VAULT_SERVICE_HOST=0.0.0.0 \
    VAULT_SERVICE_PORT=8080 \
    VAULT_SERVICE_ENV=production

# 启动命令
ENTRYPOINT ["/app/vault-service"]
