# C09 Notes

- Outcome: `failed`
- Failure class: `env`
- Auth bootstrap succeeded by minting a fresh backend session from preserved Privy state.
- `POST /api/v1/sandbox/sessions`, `GET /api/v1/sandbox/sessions/:id`, `GET /api/v1/sandbox/operations/:id`, `DELETE /api/v1/sandbox/sessions/:id`, and `GET /api/v1/sandbox/stats` all returned `200/201` and persisted the expected runtime objects.
- The failing step is `POST /api/v1/sandbox/sessions/:id/execute`: the API returned `200`, but `data.success=false` and the operation was recorded as `failed`.
- The failure detail is consistent across API response, PostgreSQL persistence, and Loki logs: nsjail could not prepare the sandbox seccomp policy and exited `255`.
- `logmcp` evidence was attached from `zkme-dev` / `credbridge`; it shows the deployed pod building `--seccomp_string` values for nsjail around the same timeframe as this run.

Main mismatches:

- Live execute semantics are transport-successful but runtime-failed; the API surface does not promote this to an HTTP error.
- `stats.before` / `stats.after` stayed `healthy=true` while also reporting `Insufficient warm instances`.
- Runtime evidence points to a deployment/runtime policy issue in the sandbox nsjail configuration rather than an auth or harness issue.
