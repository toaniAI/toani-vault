# CredBridge 后端 Dockerfile
# Rust 多阶段构建

# 阶段1：构建
FROM hub.bitkinetic.com/public/rust:1.75-bullseye AS builder

WORKDIR /app

# 安装依赖
RUN apt-get update && apt-get install -y pkg-config libssl-dev

# 复制 Cargo 文件并缓存依赖
COPY Cargo.toml Cargo.lock ./
COPY cli/Cargo.toml ./cli/
COPY mcp-server/Cargo.toml ./mcp-server/
COPY sdk-rust/Cargo.toml ./sdk-rust/
COPY vault-service/Cargo.toml ./vault-service/
COPY examples/rust/Cargo.toml ./examples/rust/

# 创建虚拟 main.rs 来缓存依赖层
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true

# 复制源码并构建
COPY src ./src
COPY cli ./cli
COPY mcp-server ./mcp-server
COPY sdk-rust ./sdk-rust
COPY vault-service ./vault-service
COPY examples ./examples
COPY migrations ./migrations

# 重新构建（只编译变化的部分）
RUN cargo build --release --bin credbridge

# 阶段2：运行
FROM debian:bullseye-slim

WORKDIR /app

# 安装运行时依赖
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

# 复制二进制文件
COPY --from=builder /app/target/release/credbridge /app/credbridge

# 复制 migrations（如果需要）
COPY --from=builder /app/migrations /app/migrations

# 非 root 用户运行
RUN useradd -m -u 1000 appuser && chown -R appuser:appuser /app
USER appuser

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD /app/credbridge healthcheck || exit 1

CMD ["/app/credbridge"]
