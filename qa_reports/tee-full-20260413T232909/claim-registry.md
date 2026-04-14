# Claim Registry

| Claim ID | Priority | Surface | Statement | Required evidence |
| --- | --- | --- | --- | --- |
| C01 | P0 | Baseline | Target site, homepage, `/health`, attestation health, and readiness endpoints are reachable and coherent. | raw responses, timestamps, route list, blocker classification |
| C02 | P0 | Web Auth | Privy OTP login establishes backend session and lands in the dashboard without broken navigation. | screenshots, HAR/network excerpts, final URL/title, session response |
| C03 | P0 | Web Credentials | Dashboard credential flow can create and inspect metadata safely without exposing plaintext; delete behavior is verified or explicitly absent. | screenshots, create/list/detail/delete responses |
| C04 | P0 | Web Tokens | Dashboard token flow can create a token under current contract and revoke/delete if available. | screenshots, form observations, create/revoke/delete responses |
| C05 | P1 | Developer Center | Developer Center content matches current contract and does not expose stale token/decrypt guidance. | screenshots, text excerpts, mismatch notes |
| C06 | P0 | API Auth | Auth session, me, memberships, and access-token paths behave correctly for valid and invalid inputs. | request/response samples, status codes, token usage notes |
| C07 | P0 | API Credentials | Credential API create/list/get behaves as metadata-only, decrypt direct-call is rejected, and delete path is verified if supported. | request/response samples, created IDs, failure samples |
| C08 | P0 | API Tokens | Token API create/get/list/verify/revoke works under valid scope rules and rejects invalid scope/credential combinations. | request/response samples, invalid cases, created IDs |
| C09 | P0 | API Sandbox | Sandbox session lifecycle, execute path, operation lookup, and termination work or fail with traceable evidence. | request/response samples, session/operation IDs, failure details |
| C10 | P0 | CLI | CLI can authenticate with real bearer token and run current auth/sandbox commands, plus at least one negative path. | command list, exit codes, output summaries, correlated API evidence |
| C11 | P0 | DB Reconciliation | PostgreSQL state matches business outcomes for created credentials, tokens, sessions, operations, and audit records. | SQL results, matched object table, mismatches |
| C12 | P0 | Redis Reconciliation | Redis DB 3 keys and TTL/state changes match observed auth/session/token behavior. | redis queries, TTL/state summary, mismatches |

## Decision Policy

- Allowed claim result: `passed`, `conditional_pass`, `failed`, `inconclusive`
- Allowed failure class: `product`, `env`, `test_harness`, `data`, `flake`, `unknown`
- No overall pass if any P0 claim remains `unknown` or lacks runtime evidence.
