---
name: toani-vault-cli
description: Use the Toani Vault CLI accurately for login, doctor, credentials metadata, and sandbox broker workflows.
metadata:
  short-description: Guidance for the Toani Vault CLI and sandbox broker flows
---

# Toani CLI Skill

## Scope

This skill documents the current public CLI contract for approval-aware credential usage.

- Credential metadata: `toani-vault credentials list`, `toani-vault credentials get`
- Approval flow: `toani-vault approvals generate-request-id`, `create`, `status`, `wait`
- Sandbox broker flow: `toani-vault sandbox request`, `toani-vault sandbox get-request`
- Legacy sandbox session lifecycle commands are retired from public usage.

## Approval-Aware Flow

1. `toani-vault login`
2. `toani-vault doctor`
3. Find the target credential:

```bash
toani-vault credentials list --service-id <service-id>
toani-vault credentials get <credentialId>
```

4. Inspect `requiresApproval` / `requires_approval` in the credential metadata.

- If `false`: execute the sandbox request directly.
- If `true`: approval is mandatory before execution.

## Required Approval Contract

When a credential has `requires_approval=true`, the execution plan MUST include approval setup before `toani-vault sandbox request`.

- `business_type` is always `credential_runtime_access`
- `business_id` is always the `request_id`
- the same `request_id` must be used for:
  - `toani-vault sandbox request` (auto-creates approval on first call)
  - the final `toani-vault sandbox request` (re-run after approval granted)
- each approved `request_id` is single-use after a successful execution
- do not reuse a consumed `request_id`; generate a new one for the next approved run

## Runtime Approval Chain

### Standard Flow (auto-approval via sandbox)

This is the recommended path for tokens without tenant admin scope:

```bash
# 1. Generate request_id
toani-vault approvals generate-request-id

# 2. First sandbox call — auto-creates the approval server-side.
#    Expected to fail with "waiting for approval" on first attempt.
toani-vault sandbox request \
  --operation-type http_request \
  --credential-id <credentialId> \
  --request-id <request_id> \
  --params '{...}'

# 3. User approves via Web Dashboard (https://dashboard.example.com/)
#    The approval ID is shown in the error message from step 2.

# 4. Re-run the same sandbox request after approval.
#    Must use the exact same --request-id.
toani-vault sandbox request \
  --operation-type http_request \
  --credential-id <credentialId> \
  --request-id <request_id> \
  --params '{...}'
```

### Admin Flow (manual approval via CLI)

Requires a token with tenant admin scope:

```bash
toani-vault approvals create --business-type credential_runtime_access --business-id <request_id> [--wait]
```

Note: `approvals create`, `approvals status`, and `approvals wait` all require tenant admin scope.
If you see `"Tenant admin scope is required"`, use the standard auto-approval flow above.

## Sandbox Broker Examples

### Generic HTTP Request

```bash
toani-vault sandbox request --operation-type http_request \
  --credential-id <credentialId> \
  --request-id <request_id> \
  --params '{"method":"GET","url":"https://api.example.com/health"}'
```

### OKX Production (Real Trading)

```bash
toani-vault sandbox request --operation-type http_request \
  --credential-id <credentialId> \
  --request-id <request_id> \
  --params '{
    "method": "GET",
    "url": "https://www.okx.com/api/v5/account/balance",
    "headers": {
      "OK-ACCESS-KEY": "${credential.api_key}",
      "OK-ACCESS-PASSPHRASE": "${credential.passphrase}",
      "OK-ACCESS-TIMESTAMP": "${functions.okx_timestamp()}",
      "OK-ACCESS-SIGN": "${functions.okx_sign()}"
    }
  }'
```

### OKX Demo/Simulated Trading

Must include `x-simulated-trading: 1` header. Without it, OKX returns `50101: APIKey does not match current environment`.

```bash
toani-vault sandbox request --operation-type http_request \
  --credential-id <credentialId> \
  --request-id <request_id> \
  --params '{
    "method": "GET",
    "url": "https://www.okx.com/api/v5/account/balance",
    "headers": {
      "OK-ACCESS-KEY": "${credential.api_key}",
      "OK-ACCESS-PASSPHRASE": "${credential.passphrase}",
      "OK-ACCESS-TIMESTAMP": "${functions.okx_timestamp()}",
      "OK-ACCESS-SIGN": "${functions.okx_sign()}",
      "x-simulated-trading": "1"
    }
  }'
```

Inspect the operation detail:

```bash
toani-vault sandbox get-request <operationId>
```

## request_id Guidance

- `toani-vault sandbox request` can auto-generate a UUID-based `request_id` when `--request-id` is omitted.
- For approval-gated credentials, do not rely on auto-generation. Generate the `request_id` first and explicitly reuse it across the first sandbox call (auto-creates approval) and the re-run after approval.
- Recommended format: `req_<timestamp_ms>_<uuid_v7>`
- Use `toani-vault approvals generate-request-id` instead of inventing your own format
- `request_id` must not encode credential IDs, service IDs, usernames, or secret-bearing business context
- concurrent waiting requests may converge on the same pending approval, but a successfully consumed approval result requires a fresh `request_id`
- If a sandbox request fails for non-approval reasons (e.g., wrong headers, bad params), the `request_id` may still be consumed. Generate a new one for the corrected request.

## Planning Rule

Any plan that uses a credential with `requires_approval=true` must explicitly include:

1. request id generation
2. first sandbox request (auto-creates approval, expect "waiting for approval" error)
3. user approves via Web Dashboard
4. sandbox execution with the same request id after approval
5. the single-use reminder for the next run

Do not produce a sandbox-only plan when approval is required.

## Troubleshooting

| Error | Cause | Fix |
|---|---|---|
| `Tenant admin scope is required` | Token lacks admin scope for `approvals create/status/wait` | Use standard auto-approval flow: sandbox request auto-creates approval |
| `APIKey does not match current environment` (OKX 50101) | Missing or wrong demo/production header | Add `x-simulated-trading: 1` for demo, remove it for production |
| `Runtime access is waiting for approval` | Expected on first sandbox call — approval not yet granted | User must approve via Web Dashboard, then re-run with same `--request-id` |

## Token Scope

The token must have appropriate scopes for each operation:

| Scope | Required for |
|---|---|
| `credential:read` | `credentials list`, `credentials get` |
| `credential:decrypt` | Sandbox requests with credential-backed headers |
| `credential:write` | Creating/deleting credentials |
| `audit:read` | Querying audit logs |
| tenant admin | `approvals create`, `approvals status`, `approvals wait` (not needed for auto-approval flow) |

Run `toani-vault doctor` to check current token validity and scope.
