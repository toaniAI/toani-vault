# CredBridge 后端 Dockerfile

FROM hub.bitkinetic.com/public/rust:1.75-bullseye AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# 先复制 manifest，尽量复用依赖缓存。
COPY Cargo.toml Cargo.lock ./
COPY cli/Cargo.toml ./cli/
COPY mcp-server/Cargo.toml ./mcp-server/
COPY sdk-rust/Cargo.toml ./sdk-rust/
COPY vault-service/Cargo.toml ./vault-service/
COPY examples/rust/Cargo.toml ./examples/rust/

RUN mkdir -p src cli/src sdk-rust/src mcp-server/src vault-service/src examples/rust/src
RUN printf 'fn main() {}\n' > src/main.rs
RUN printf 'fn main() {}\n' > cli/src/main.rs
RUN printf 'fn main() {}\n' > mcp-server/src/main.rs
RUN printf 'fn main() {}\n' > examples/rust/src/main.rs
RUN printf 'pub fn placeholder() {}\n' > sdk-rust/src/lib.rs
RUN printf 'pub fn placeholder() {}\n' > vault-service/src/lib.rs
RUN cargo build --release || true

COPY src ./src
COPY cli ./cli
COPY mcp-server ./mcp-server
COPY sdk-rust ./sdk-rust
COPY vault-service ./vault-service
COPY examples ./examples
COPY migrations ./migrations

RUN cargo build --release

FROM debian:bullseye-slim

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/vault-service /app/vault-service
COPY --from=builder /app/migrations /app/migrations

RUN useradd -m -u 1000 appuser && chown -R appuser:appuser /app
USER appuser

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD sh -c 'curl -fsS "http://127.0.0.1:${CREDBRIDGE_PORT:-8080}/health" >/dev/null || exit 1'

CMD ["/app/vault-service"]
