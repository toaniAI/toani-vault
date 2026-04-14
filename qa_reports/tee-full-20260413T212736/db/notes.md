# case11 notes

- status: completed
- method: direct PostgreSQL queries against `credbridge_vault` schema using `.env` `DATABASE_URL` credentials
- interpretation: deleted test credentials are persisted with `is_deleted=true`; sandbox session and failed sandbox operation are present; auth sessions are present; issued token IDs from `case04` and `case08` were not found in `credbridge_vault.scope_tokens`
