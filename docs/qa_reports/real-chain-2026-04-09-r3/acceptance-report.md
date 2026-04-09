# Real-Chain Acceptance Report (R3)

Date: 2026-04-09
Scope: service account model + token metadata + CLI/SDK/documentation gaps from PLAN-2.md

## 1) Runtime Startup

Dependencies:

```bash
docker compose -f docker/docker-compose.yml up -d postgres redis immudb vault
```

App startup (local binary, real DB/Redis/immudb/Vault, mock Privy):

```bash
PRIVY_MOCK_ENABLED=true \
TEE_MODE=simulation \
CREDBRIDGE_STORAGE_BACKEND=postgres \
DATABASE_URL='postgresql://credbridge:credbridge_secret@127.0.0.1:5432/credbridge' \
REDIS_URL='redis://:credbridge_redis_pass@127.0.0.1:6379/0' \
VAULT_ADDR='http://127.0.0.1:8200' \
VAULT_TOKEN='credbridge-root-token' \
IMMUDB_HOST='127.0.0.1' \
IMMUDB_PORT='3322' \
IMMUDB_DATABASE='credbridge_audit' \
IMMUDB_USERNAME='credbridge' \
IMMUDB_PASSWORD='credbridge_immudb_pass' \
cargo run --bin vault-service
```

Health check: `GET /health` returned `{"status":"alive","ready":true,...}`.

## 2) API Real-Chain Execution

Evidence directory: `docs/qa_reports/real-chain-2026-04-09-r3/`

User chain:
- `01_auth_session.status` = `200`
- `04_create_access_token.status` = `200` (`subject_type=user`, `issued_from=session`)
- `05_tokens_list.status` = `200`
- `06_token_get.status` = `200`

Service account chain:
- `07_service_account_create.status` = `200`
- `08_service_account_token_create.status` = `200` (`subject_type=service_account`, `issued_from=service_account`)
- `09_service_account_tokens_list.status` = `200`
- `10_sa_auth_me_forbidden.status` = `403` (service account blocked on human-session endpoint)
- `11_revoke_sa_token.status` = `200` (`revoked=true`)
- `12_verify_revoked_sa_token.status` = `200` (`valid=false`)
- `13_sa_token_over_scope.status` = `403` (scope ceiling enforced)

## 3) CLI Real-Chain Execution

CLI evidence:
- `cli_01_config_init.json`
- `cli_02_auth_session.json`
- `cli_03_access_token_create_store.json`
- `cli_04_service_account_create.json`
- `cli_05_service_account_token_create.json`
- `cli_06_service_account_token_list.json`
- `cli_07_tokens_list.json`
- `cli_08_tokens_get.json`
- `cli_09_auth_logout.json`
- `cli_10_config_show_after_logout.json`

Key checks:
- `auth access-token create --store` wrote API token to config
- `auth logout` cleared `sessionToken` and preserved `token`
- `service-accounts` command group is executable end-to-end on real backend

## 4) Database and Audit Evidence

- DB snapshot: `r3_db_state.txt`
  - contains created `service_accounts` record
  - contains `api_tokens` rows for user access token + service account token with revoke timestamp
- Audit snapshot: `r3_audit_state.txt`
  - contains `token_issue`/`token_revoke` rows with token-related event payloads

## 5) Gate Decision

Decision: **PASS (for this change scope)**

Covered and passed:
- independent service account subject model (API behavior)
- token metadata create/list/get/revoke flow
- service account scope ceiling enforcement
- service account prohibition on `/auth/me`
- CLI command coverage for new token/service-account flows

Not covered in this run:
- old token compatibility path where token exists but `api_tokens` metadata row is missing
- full frontend regression coverage
- SGX hardware mode runtime acceptance (this run is `TEE_MODE=simulation`)
