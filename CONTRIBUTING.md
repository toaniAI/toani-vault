# Contributing to CredBridge

Thank you for your interest in contributing to CredBridge! This document outlines the process for contributing to the project and how to get your development environment set up.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Development Setup](#development-setup)
- [Coding Standards](#coding-standards)
- [Pull Request Process](#pull-request-process)
- [Issue Reporting](#issue-reporting)

## Code of Conduct

By participating in this project, you agree to abide by our [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before contributing.

## Development Setup

### Prerequisites

- **Rust** 1.75+ (install via [rustup](https://rustup.rs/))
- **Node.js** 20+ with npm (for CLI and TypeScript tooling)
- **Docker** and Docker Compose (for running external services)
- **PostgreSQL** 15+ (or use the Docker Compose stack)
- **Redis** 7+ (or use the Docker Compose stack)

### 1. Fork and Clone

```bash
git clone https://github.com/your-fork/credbridge.git
cd credbridge
```

### 2. Start External Services

The easiest way to get all required services running is via Docker Compose:

```bash
docker compose -f docker/docker-compose.yml up -d
```

This starts PostgreSQL, Redis, immudb, and HashiCorp Vault with default development credentials.

### 3. Configure Environment

Copy the example environment file and adjust as needed:

```bash
cp .env.example .env
```

Key variables for local development:

```bash
DATABASE_URL=postgres://credbridge:credbridge@localhost:5432/credbridge
REDIS_URL=redis://localhost:6379
TEE_MODE=simulation        # Use simulation mode for local dev (no SGX hardware needed)
CREDBRIDGE_ENV=development
RUST_LOG=debug
```

### 4. Run Database Migrations

```bash
cargo run --bin db-verify  # verify connection
sqlx migrate run           # apply migrations
```

### 5. Build and Run

```bash
cargo build
RUST_LOG=debug cargo run
```

The API server starts on `http://localhost:8080`.

### 6. Verify Setup

```bash
# Run all backend tests
cargo test

# Run linter
cargo clippy --tests -- -D warnings

```

## Coding Standards

### Rust Backend

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `thiserror` for structured error types
- Use `zeroize` to securely erase sensitive data from memory
- Never use `.unwrap()` or `.expect()` in production code — propagate errors with `?`
- All database queries must use SQLx parameterized bindings (no string interpolation)
- Constant-time comparison for any secret material (use `constant_time_eq`)

Before every commit, all three gates must pass with **zero errors and zero warnings**:

```bash
cargo fmt                          # format
cargo clippy --tests -- -D warnings  # lint (including test code)
cargo test                         # all tests
```

### Commit Messages

Use the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
<type>(<scope>): <short summary>

<optional body>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, `ci`

Examples:

```
feat(api): add credential rotation endpoint
fix(crypto): use constant-time comparison for token validation
docs: update deployment guide for SGX hardware mode
```

### Security Requirements

CredBridge handles sensitive credential data. All contributions must:

- Never log secrets, tokens, or plaintext credential values
- Use the existing four-layer key hierarchy for any new encryption operations
- Include negative test cases for any security-sensitive code paths
- Pass a security review for changes to `src/crypto/`, `src/tee/`, or `src/token/`

## Pull Request Process

1. **Branch from `master`** — create a feature branch with a descriptive name:

   ```bash
   git checkout -b feat/credential-rotation
   ```

2. **Make your changes** — keep commits focused and atomic.

3. **Ensure all gates pass locally** before pushing:

   ```bash
   cargo fmt && cargo clippy --tests -- -D warnings && cargo test
   ```

4. **Open a Pull Request** against `master`. Fill out the PR template completely.

5. **Address review feedback** — maintainers may request changes. Push additional commits to the same branch; do not force-push after review has started.

6. **Squash policy** — maintainers may squash commits on merge to keep history clean.

### PR Requirements

- [ ] All CI checks pass (lint, test, build)
- [ ] New functionality includes tests
- [ ] Public API changes include documentation updates
- [ ] Security-sensitive changes have been reviewed for vulnerabilities
- [ ] No secrets or credentials committed

## Issue Reporting

### Bug Reports

Use the [Bug Report](.github/ISSUE_TEMPLATE/bug_report.md) issue template. Please include:

- A clear description of the problem
- Exact steps to reproduce
- Expected vs. actual behavior
- Environment details (OS, Rust version, TEE mode, etc.)

**Security vulnerabilities** must not be reported via public GitHub issues. Please email **opensource@zkme.com** directly.

### Feature Requests

Use the [Feature Request](.github/ISSUE_TEMPLATE/feature_request.md) issue template. Describe the problem you are trying to solve, not just the solution you have in mind.

### Questions

For general questions and discussion, open a GitHub Discussion rather than an issue.

---

Thank you for contributing to CredBridge!
