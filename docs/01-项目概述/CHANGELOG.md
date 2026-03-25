# Changelog

All notable changes to CredBridge will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0-phase2] - 2026-03-12

### Added

#### EP5: Credential Sync
- TypeScript SDK with full API support (`sdk-typescript/`)
- Rust SDK with async support (`sdk-rust/`)
- Credential CRUD operations
- Token-based authentication
- Error handling and retry logic

#### EP6: MCP Server
- SSE Transport endpoint (`/sse`)
- Message handling endpoint (`/message`)
- MCP Tools: `credential_list`, `credential_get`, `credential_decrypt`, `audit_query`
- Bearer Token authentication middleware
- Message queue implementation

#### EP7: Security & Compliance
- Multi-tenant isolation with Schema-per-Tenant pattern
- Row Level Security (RLS) policies
- Tenant context middleware
- Cross-tenant access prevention

#### EP9: Operations & Deployment
- Docker Compose configuration for development
- Production Docker Compose with security hardening
- Prometheus metrics endpoint (`/metrics`)
- Grafana dashboards (pre-configured)
- Health check endpoints (`/health`, `/health/detail`)
- Backup and restore scripts

### Fixed

#### P0 Critical Fixes
- **P0-01**: Implemented Connector trait framework (`src/connector/`)
  - Connector trait with init/execute/cleanup lifecycle
  - HTTPConnector implementation
  - Timeout control mechanism
  - Error type definitions
  - Registry pattern

- **P0-02**: Implemented credential versioning
  - `VaultEntry.version` field
  - `PUT /api/v1/credentials/:id` API
  - `GET /api/v1/credentials/:id/versions` API
  - `POST /api/v1/credentials/:id/rollback` API
  - Version history storage

- **P0-03**: Implemented MCP SSE endpoints
  - SSE Transport protocol
  - Message queue
  - Authentication middleware
  - MCP Tools registration

- **P0-04**: Implemented RLS policies
  - `init-rls.sql` migration script
  - Tenant isolation policies
  - Context middleware

#### Security Fixes (HIGH Priority)
- **H-001**: Token blacklist moved from memory HashSet to Redis with TTL
- **H-002**: Added compile-time DEBUG flag checks, production builds disable debug by default
- **H-003**: Replaced `std::sync::RwLock` with `tokio::sync::RwLock` and timeout mechanism
- **H-004**: Added key versioning and rotation support (`KeyVersion` struct)

### Changed
- VaultEntry model now includes `version`, `updated_at`, `previous_version_id` fields
- Token blacklist now requires Redis configuration for production deployments
- Enclave debug mode requires explicit `allow-debug` feature flag

### Security
- Implemented RLS for tenant isolation
- Added token blacklist persistence
- Enhanced key management with versioning
- Added compile-time security checks

## [1.0.0-phase1] - 2026-03-11

### Added
- Initial MVP release
- TEE Enclave core module with SGX support
- Four-layer key hierarchy (L0-L3)
- AES-256-GCM credential encryption
- PASETO v4.local token authentication
- Audit logging with immudb
- Remote attestation (SGX DCAP)
- Basic multi-tenant support
- Web console (React frontend)
- API documentation
- User manual
- Deployment documentation

### Core Components
- Enclave lifecycle management (`src/tee/enclave.rs`)
- Key management with TTL cache (`src/tee/keys.rs`)
- SGX sealing and attestation (`src/tee/sealing.rs`, `src/tee/attestation.rs`)
- Credential vault service (`src/vault/`)
- Audit system (`src/audit/`)
- Crypto utilities (`src/crypto/`)

### API Endpoints
- `GET /health` - Health check
- `GET /health/detail` - Detailed health check
- `POST /api/v1/credentials` - Create credential
- `GET /api/v1/credentials` - List credentials
- `GET /api/v1/credentials/:id` - Get credential
- `POST /api/v1/credentials/:id/decrypt` - Decrypt credential
- `DELETE /api/v1/credentials/:id` - Delete credential
- `GET /api/v1/audit/logs` - Query audit logs
- `GET /api/v1/audit/logs/:id` - Get audit entry
- `POST /api/v1/audit/export` - Export audit logs
- `POST /api/v1/audit/verify` - Verify audit entry

[1.0.0-phase2]: https://github.com/credbridge/vault-service/compare/v1.0.0-phase1...v1.0.0-phase2
[1.0.0-phase1]: https://github.com/credbridge/vault-service/releases/tag/v1.0.0-phase1