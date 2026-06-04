# @toani/vault-cli

Toani Vault npm CLI package.

- Package: `@toani/vault-cli`
- Executable: `toani-vault`

## Scope

The CLI now supports interactive onboarding in addition to the existing read-only and sandbox flows:

- `login`
- `doctor`
- `config`
- `credentials` (`list`, `get`)
- `approvals` (`generate-request-id`, `create`, `status`, `wait`)
- `sandbox`
- `--help`
- `--version`

Do not assume the package exposes `auth`, mutating `credentials` commands, `tokens`,
`service-accounts`, or `audit` commands unless you have verified a newer build.

## Install

```bash
npm install -g @toani/vault-cli@latest
```

## Configure

Recommended first-run flow:

```bash
toani-vault login
toani-vault doctor
```

`toani-vault login` opens the Dashboard in your browser, walks you through credential + token creation,
then lets you choose between clipboard auto-detect, manual paste, or `.env` before validation and
OS Keychain storage (macOS Keychain / libsecret / Windows Credential Manager).

Manual configuration remains available for compatibility. Create credentials and issue bearer tokens
in the Dashboard UI first. The CLI only consumes an existing `credential_id` and bearer token; it
does not mint either one.

Use the Dashboard UI to:

- create the credential
- copy the resulting `credential_id`
- issue or copy a restricted bearer token

Then configure the CLI with flags, environment variables, or local config:

```bash
export TOANI_BASE_URL="https://api.example.com"
export TOANI_VAULT_TOKEN="<BEARER_TOKEN>"

toani-vault config init --url https://api.example.com --token <BEARER_TOKEN>
toani-vault config show
```

When `--base-url` or `--output` are passed, the CLI persists those values to
`~/.toani/config.json` for the active profile. When `--token` is passed to `toani-vault config init`,
the token is stored in the OS Keychain instead of being written to disk.

Config file fields include:

- `baseUrl`
- `currentTenantId`
- `currentProfile`
- `profiles`
- `output` (`table` or `json`)
- `timeout`

Token resolution priority:

1. explicit `--token`
2. `TOANI_VAULT_TOKEN`
3. `CREDBRIDGE_TOKEN`
4. OS Keychain entry `toani-vault-cli:default`
5. legacy saved profile `token` in `~/.toani/config.json`

Base URL resolution priority:

1. explicit `--base-url`
2. `TOANI_BASE_URL`
3. `CREDBRIDGE_BASE_URL`
4. saved profile `baseUrl` in `~/.toani/config.json`
5. default `TOANI_VAULT_DASHBOARD_BASE_URL` or `https://dashboard.toani.ai`

## Commands

```bash
toani-vault login [--base-url <service-url>] [--skip-validate]
toani-vault doctor [--base-url <service-url>]
toani-vault config init --url <service-url> [--token <BEARER_TOKEN>]
toani-vault config show
toani-vault credentials list [--service-id <id>] [--credential-type <type>] [--only-valid true|false]
toani-vault credentials get <credentialId>
toani-vault approvals generate-request-id
toani-vault approvals create --business-type <type> --business-id <id>
toani-vault approvals status <approvalId>
toani-vault approvals wait <approvalId> [--timeout-ms <ms>] [--poll-interval-ms <ms>]
toani-vault sandbox request --operation-type <type> --params '{"key":"value"}'
toani-vault sandbox get-request <operationId>
toani-vault --version
toani-vault --help
```

Approval wait notes:

- `toani-vault approvals generate-request-id` returns a canonical runtime approval `request_id`
- `toani-vault approvals create` remains async by default and still returns immediately with `status=pending`
- add `--wait` to `toani-vault approvals create` to create and then synchronously poll for a terminal state
- `toani-vault approvals wait` polls `status` until the approval becomes `approved`, `rejected`, or `cancelled`
- local wait timeout is client-side only; timeout output keeps the last `status=pending` snapshot and adds `wait_result=timeout`
- exit codes are scriptable:
  - `0` for `approved`
  - `20` for `rejected`
  - `21` for `cancelled`
  - `124` for client-side timeout

## Onboarding

`toani-vault login` supports three paths:

- account exists: open Dashboard and guide you through credential + token setup
- needs signup: open the sign-in page, then return to the guided flow
- already has token: use the shared token entry flow

Guided onboarding now shows a token menu before clipboard watching:

- auto-detect from clipboard
- paste it here now
- set `TOANI_VAULT_TOKEN` in `.env`

All manual token-entry paths share the same fallback menu:

- `Paste it here now`
- `Set TOANI_VAULT_TOKEN in .env`
- `Cancel`

Validation failures are classified with concrete next steps for:

- invalid or expired token (`401`)
- insufficient scope (`403`)
- DNS failure
- connection refused
- timeout
- generic network failure

Run `toani-vault doctor` after setup to verify CLI version, Node.js, token storage, token format, base
URL reachability, and token validity.

## Credential Metadata Workflow

The CLI now exposes a read-only `credentials` group for metadata retrieval.

```bash
# List all readable credentials
toani-vault credentials list

# Filter by service id
toani-vault credentials list --service-id schwab

# Filter by type and validity
toani-vault credentials list --credential-type api_key --only-valid true

# Fetch one credential metadata record
toani-vault credentials get 018f1b4e-7e9e-7f3a-8b5c-2d4e6f8a0b2c
```

These commands read metadata only. They do not expose plaintext secrets, do not decrypt credentials,
and still require a bearer token with `credential:read`.

When a credential was created through the REST API with transport config, the metadata returned by
`credentials list` / `get` includes `provider`, `allowed_domains`, and `custom_functions`. The CLI
still does not create or update those fields; use the Dashboard or REST API for mutation.

When `credentials get` or `credentials list` shows `requiresApproval` / `requires_approval` as
`true`, approval is mandatory before sandbox execution. The CLI table output prints the required
follow-up chain automatically.

## Runtime Approval Flow

For a credential with `requires_approval=true`, always use this sequence:

```bash
# 1. Confirm the credential requires approval
toani-vault credentials get <credentialId>

# 2. Generate a canonical request_id
toani-vault approvals generate-request-id

# 3. Create or wait on the approval
toani-vault approvals create \
  --business-type credential_runtime_access \
  --business-id <request_id> \
  --wait

# 4. Execute the sandbox request with the same request_id
toani-vault sandbox request \
  --operation-type http_request \
  --credential-id <credentialId> \
  --request-id <request_id> \
  --params '{"method":"GET","url":"https://api.example.com/health"}'
```

Runtime approval contract:

- `business_type` is always `credential_runtime_access`
- `business_id` is always the generated `request_id`
- the same `request_id` must be reused across approval creation and sandbox execution
- the recommended format is `req_<timestamp_ms>_<uuid_v7>`
- do not encode service IDs, credential IDs, or usernames into `request_id`
- once an approved execution succeeds, that `request_id` is consumed and must not be reused

## Sandbox Workflow

The CLI controls remote TEE sandbox broker requests (session lifecycle APIs are retired). Browser-backed operations run through the backend
Lightpanda + puppeteer-core runtime; `http_request` is the direct HTTP operation and does not start
Lightpanda. The CLI is not a local browser runner and not an abstract "sandbox node" system.

Use this sequence:

1. Identify or create the credential in the Dashboard UI.
2. If `requires_approval=true`, generate `request_id` and complete the runtime approval flow first.
3. Build a broker request payload (`operation_type`, `parameters`).
4. `toani-vault sandbox request --operation-type <type> --params '{...}'`
5. Capture `operationId` from the response.
6. `toani-vault sandbox get-request <operationId>` to inspect final status/data.

If `--request-id` is not provided, the CLI automatically generates one with UUID for approval-required
requests. If you need deterministic retries or to correlate an approval flow manually, pass
`--request-id <uuid>` yourself.

For secret-backed login or API flows, pass credential references in request parameters (for example `{"$credential":"password"}` or template values like `${credential.api_key}`).

### Recommended `test-web.zk.me` chain

Use this chain when validating a secret-backed login flow against `test-web.zk.me`:

```bash
toani-vault sandbox request \
  --operation-type navigate \
  --params '{"url":"https://test-web.zk.me/login"}'

toani-vault sandbox request \
  --operation-type http_request \
  --params '{
    "method":"POST",
    "url":"https://test-web.zk.me/login",
    "headers":{"Content-Type":"application/json"},
    "body":{
      "email":{"$credential":"username"},
      "password":{"$credential":"password"}
    }
  }'

toani-vault sandbox get-request <operationId>
```

### Operation types

- `navigate`
- `click`
- `fill`
- `get_text`
- `execute_script`
- `wait`
- `http_request`

### Secret handling contract

- `fill` remains the controlled secret sink. Its top-level `value` field may be either a plain
  string or a credential reference such as `{"$credential":"password"}`.
- `http_request` may resolve credential references inside nested headers/body values. When a remote
  API expects fixed framing, use `prefix` / `suffix`, for example
  `{"$credential":"api_key","prefix":"Bearer "}`.
- `http_request` also supports string templates for exchange REST flows:
  `${credential.api_key}`, `${credential.secret_key}`, `${credential.passphrase}`,
  `${functions.okx_timestamp()}`, `${functions.okx_sign()}`,
  `${functions.binance_timestamp()}`, `${functions.binance_sign()}`.
- Before execution, the backend enforces the credential-level `allowed_domains` whitelist. Matching
  ignores scheme, supports explicit ports, and supports leading subdomain wildcards such as
  `*.okx.com:443`.
- `http_request` also supports a `query` object inside `--params`. Template rendering runs before
  the final `allowed_domains` check, so the rendered URL must still land on an allowed `host:port`.
- `allowed_domains` accepts exact hosts such as `api.binance.com:443` and leading subdomain
  wildcards such as `*.okx.com:443`. It does not allow paths, and `*.okx.com:443` does not match
  `okx.com:443`, `evil-okx.com:443`, or `www.okx.com.evil.com:443`.
- `execute_script` may still receive `bindings`, but every binding value must be a plain string.
  Do not pass credential references in `execute_script.bindings`.
- When `execute_script.bindings` contains `{"$credential":"..."}`, the backend rejects the request
  instead of resolving the secret into script-visible data.

### Exchange `http_request` templates

Use this pattern when a request uses an exchange API-key credential and the credential
metadata already contains the correct `provider` and `allowed_domains`.

```bash
# OKX private REST request
toani-vault sandbox request \
  --operation-type http_request \
  --params '{
    "method":"GET",
    "url":"https://www.okx.com/api/v5/account/balance",
    "headers":{
      "OK-ACCESS-KEY":"${credential.api_key}",
      "OK-ACCESS-TIMESTAMP":"${functions.okx_timestamp()}",
      "OK-ACCESS-PASSPHRASE":"${credential.passphrase}",
      "OK-ACCESS-SIGN":"${functions.okx_sign()}"
    }
  }'

# Binance signed REST request
toani-vault sandbox request \
  --operation-type http_request \
  --params '{
    "method":"GET",
    "url":"https://api.binance.com/api/v3/account",
    "query":{
      "recvWindow":"${functions.recv_window()}",
      "timestamp":"${functions.binance_timestamp()}",
      "signature":"${functions.binance_sign()}"
    },
    "headers":{
      "X-MBX-APIKEY":"${credential.api_key}"
    }
  }'
```

`custom_functions` must return strings. At runtime they can read `credential`, `provider`,
`method`, `url`, `query`, `headers`, and `body`, but network access and `process.env` are blocked.

### Examples

```bash
# Submit a broker request
toani-vault sandbox request \
  --operation-type navigate \
  --params '{"url":"https://target-site.com/login"}'

# Wait for page to render target element
toani-vault sandbox request \
  --operation-type wait \
  --params '{"selector":"input[name=email]","timeout_ms":15000}'

# Click
toani-vault sandbox request \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'

# Fill
toani-vault sandbox request \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":"user@example.com"}'

# Fill from a stored credential reference
toani-vault sandbox request \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'

# Login flow for Rocket Loader pages
toani-vault sandbox request \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":{"$credential":"username"}}'
toani-vault sandbox request \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'
toani-vault sandbox request \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'

# Execute script with plain-string bindings only
toani-vault sandbox request \
  --operation-type execute_script \
  --params '{"script":"return document.querySelector(bindings.selector)?.textContent?.trim() ?? null","bindings":{"selector":"h1"}}'

# Backend-side direct HTTP request with credential-backed Authorization
toani-vault sandbox request \
  --operation-type http_request \
  --params '{
    "method":"POST",
    "url":"https://openrouter.ai/api/v1/chat/completions",
    "headers":{
      "Authorization":{"$credential":"api_key","prefix":"Bearer "},
      "Content-Type":"application/json"
    },
    "body":{
      "model":"openai/gpt-4o-mini",
      "messages":[{"role":"user","content":"ping"}]
    },
    "timeout_ms":10000
  }'

# OKX private REST request using string templates
toani-vault sandbox request \
  --operation-type http_request \
  --params '{
    "method":"GET",
    "url":"https://www.okx.com/api/v5/account/balance",
    "headers":{
      "OK-ACCESS-KEY":"${credential.api_key}",
      "OK-ACCESS-TIMESTAMP":"${functions.okx_timestamp()}",
      "OK-ACCESS-PASSPHRASE":"${credential.passphrase}",
      "OK-ACCESS-SIGN":"${functions.okx_sign()}",
      "Content-Type":"application/json"
    }
  }'

# Binance SIGNED REST request using string templates
toani-vault sandbox request \
  --operation-type http_request \
  --params '{
    "method":"GET",
    "url":"https://api.binance.com/api/v3/account?timestamp=${functions.binance_timestamp()}&signature=${functions.binance_sign()}",
    "headers":{
      "X-MBX-APIKEY":"${credential.api_key}"
    }
  }'

# Invalid: execute_script bindings cannot resolve credentials
toani-vault sandbox request \
  --operation-type execute_script \
  --params '{"script":"return bindings.password","bindings":{"password":{"$credential":"password"}}}'
# Expected result: backend rejects the request because execute_script bindings only support plain strings.

# Query final operation detail
toani-vault sandbox get-request <operationId>
```

## Notes

- Dashboard UI is the supported creation surface for credentials and bearer tokens.
- CLI integrations only use bearer tokens. Browser-side Privy/session flows are not exposed as CLI
  commands.
- Prefer top-level controlled operations such as `fill` for credential consumption; do not design
  flows that require secrets to become script-visible values.
- sandbox command set is broker-only: submit with `sandbox request`, then query status/detail with `sandbox get-request`.

## Common Failures

- `No usable API token was found for the CLI`
  - Create or copy the token in the Dashboard UI, then run `toani-vault config init --url <api-url> --token <BEARER_TOKEN>` or set `TOANI_VAULT_TOKEN`.

- `request_id is required when credential requires approval`
  - `toani-vault sandbox request` now auto-generates `request_id` (UUID) when omitted.
  - To force a stable value (for retry/audit linkage), provide `--request-id <uuid>` explicitly.

- `execute_script.bindings` rejects a credential reference
  - Move the secret to `fill.value` or another controlled host operation. `execute_script.bindings`
    only accepts plain strings.

- 401 / 403 responses from the API
  - Confirm the bearer token came from the Dashboard UI and that the CLI is reading the intended
    `~/.toani/config.json`, environment variables, and global flags.
