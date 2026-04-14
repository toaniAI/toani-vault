# C12 Notes

- Outcome: `failed`
- Method: raw RESP probe against Redis DB `3`; local `redis-cli` was not installed on this runner, so the probe used `rtk python3` socket requests with password kept out of summaries.
- Code reference for expected key shapes is in `raw/source-key-shapes.txt`.
- Source still advertises Redis token-store keys under `credbridge:tokens:{tenant}:active`, `credbridge:tokens:{tenant}:revoked`, and `credbridge:token:{jti}`.
- For tenant `019d8414-bf83-7641-a3fb-ad93d58715bd`, both tenant-scoped keys were absent with `EXISTS=0`, `TYPE=none`, and `TTL=-2`.
- All four recorded token IDs had no `credbridge:token:{id}` hash, no active zset score, and no revoked-set membership.
- `SCAN` returned no keys for `credbridge:*`, `credbridge:token:*`, `credbridge:tokens:*`, `token:blacklist:*`, or any recorded token/session/sandbox/operation ID.
- `DBSIZE=1`, but the only key in DB 3 was unrelated: `CHAIN_ERROR_WHITE_LIST` (set, persistent, no TTL).
- The PostgreSQL reconciliation already showed live tokens persisted in `credbridge_vault.api_tokens`, not `scope_tokens`; Redis did not contain parallel runtime token state for those same IDs.
- Session-key absence is secondary evidence only: source inspection indicates auth sessions are PostgreSQL-backed in the current live path.

Main mismatches:

- No Redis runtime token material was present for freshly issued or revoked tokens created during this run.
- No tenant-level active-token zset or revoked-token set existed for the live tenant used in the run.
- The only observed Redis key was unrelated chain-error whitelist state, not auth/token/session runtime state.
- The observed DB 3 state does not match the Redis token-store contract implied by `src/token/redis_store.rs` and older Redis-backed token expectations.
