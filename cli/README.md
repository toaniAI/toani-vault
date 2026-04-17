# @toani/vault-cli

Toani Vault npm CLI package.

- Package: `@toani/vault-cli`
- Executable: `toani`

## Scope

The current published CLI is intentionally narrow:

- `config`
- `sandbox`
- `--help`
- `--version`

Do not assume the package exposes `auth`, `credentials`, `tokens`, `service-accounts`, or `audit`
commands unless you have verified a newer build.

## Install

```bash
npm install -g @toani/vault-cli@0.0.11
```

## Configure

Create credentials and issue bearer tokens in the Dashboard UI first. The CLI only consumes an
existing `credential_id` and bearer token; it does not mint either one.

Use the Dashboard UI to:

- create the credential
- copy the resulting `credential_id`
- issue or copy a restricted bearer token

Then configure the CLI with flags, environment variables, or local config:

```bash
export TOANI_BASE_URL="https://api.example.com"
export TOANI_VAULT_TOKEN="<BEARER_TOKEN>"

toani config init --url https://api.example.com --token <BEARER_TOKEN>
toani config show
```

When `--base-url`, `--token`, or `--output` are passed, the CLI persists those values to
`~/.toani/config.json` for the active profile.

Config file fields include:

- `baseUrl`
- `token`
- `currentTenantId`
- `currentProfile`
- `profiles`
- `output` (`table` or `json`)
- `timeout`

Token resolution priority:

1. explicit `--token`
2. `TOANI_VAULT_TOKEN`
3. `CREDBRIDGE_TOKEN`
4. saved profile `token` in `~/.toani/config.json`

Base URL resolution priority:

1. explicit `--base-url`
2. `TOANI_BASE_URL`
3. `CREDBRIDGE_BASE_URL`
4. saved profile `baseUrl` in `~/.toani/config.json`
5. default `https://api.credbridge.example/`

## Commands

```bash
toani config init --url <service-url> [--token <BEARER_TOKEN>]
toani config show
toani sandbox create-session --service-id <service> --original-intent <intent> [--credential-id <id>] [--start-url <url>]
toani sandbox list-sessions
toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
toani sandbox pause <sessionId>
toani sandbox resume <sessionId>
toani sandbox bootstrap-page <sessionId> --mode rocket_loader [--script-selectors '<json-array>'] [--include-plain-scripts true|false] [--replay-lifecycle-events true|false] [--wait-selector <selector>] [--wait-timeout-ms <ms>]
toani sandbox execute <sessionId> --operation-type <type> [--params '{"selector":"#btn"}']
toani sandbox export-dom <sessionId> [--format html|text|json] [--root-selector body]
toani sandbox export-data <sessionId> --selectors '[".row"]' [--format json|csv|pdf]
toani sandbox get-operation <operationId>
toani sandbox stats
toani --version
toani --help
```

## Sandbox Workflow

The CLI controls remote TEE sandbox sessions. Browser-backed operations run through the backend
Lightpanda + puppeteer-core runtime; `http_request` is the direct HTTP operation and does not start
Lightpanda. The CLI is not a local browser runner and not an abstract "sandbox node" system.

Use this sequence:

1. Identify or create the credential in the Dashboard UI.
2. Copy the `credential_id`.
3. `toani sandbox create-session --service-id <service> --credential-id <credential_id> --original-intent <intent> [--start-url <url>]`
4. `toani sandbox execute <sessionId> --operation-type navigate ...`
5. `toani sandbox bootstrap-page <sessionId> --mode rocket_loader ...` when the page needs controlled bundle replay; add `--replay-lifecycle-events true` for late-mounted login forms
6. `toani sandbox execute <sessionId> --operation-type wait ...`
7. `toani sandbox execute <sessionId> --operation-type fill|click ...`
8. `toani sandbox get-operation` when the server returns an operation id
9. `toani sandbox get-session` when you need current state
10. `toani sandbox terminate`

For secret-backed login flows, always pass `--credential-id` when creating the session. The
backend can also resolve by `service_id`, but explicit credential binding is the reliable path for
sessions that need secret consumption.

### Recommended `test-web.zk.me` chain

Use this chain when validating a secret-backed login flow against `test-web.zk.me`:

```bash
toani sandbox create-session \
  --service-id <service> \
  --credential-id <credential_id> \
  --original-intent "Sign in to test-web.zk.me" \
  --start-url https://test-web.zk.me/login

toani sandbox execute <sessionId> \
  --operation-type navigate \
  --params '{"url":"https://test-web.zk.me/login"}'

toani sandbox bootstrap-page <sessionId> \
  --mode rocket_loader \
  --replay-lifecycle-events true \
  --wait-selector 'input[name=email]' \
  --wait-timeout-ms 15000

toani sandbox execute <sessionId> \
  --operation-type wait \
  --params '{"selector":"input[name=email]","timeout_ms":15000}'

toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":{"$credential":"username"}}'

toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'

toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'

toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
```

### Operation types

- `navigate`
- `click`
- `fill`
- `get_text`
- `bootstrap_page` via the dedicated `sandbox bootstrap-page` subcommand
- `execute_script`
- `wait`
- `http_request`
- `export`
- `dom_export`

### Secret handling contract

- `bootstrap-page` only replays approved page bundles. It does not accept raw script text, does not accept bindings, does not consume credentials, and rejects credential references in its request body.
- When `--script-selectors` is omitted, the backend uses its built-in generic external-script discovery set and still keeps `include_plain_scripts` as the gate for replaying non-Rocket-Loader scripts.
- `bootstrap-page` optionally supports `--replay-lifecycle-events true` to replay `DOMContentLoaded` / `load` / `pageshow` after bundle reinjection for compatibility-sensitive pages.
- When `bootstrap-page` times out on `--wait-selector`, the backend error includes URL, title, script counts, matched selectors, sample script descriptors, and pre/post-injection `readyState` diagnostics to help isolate whether discovery, reinjection, or page mount failed.
- `fill` remains the controlled secret sink. Its top-level `value` field may be either a plain
  string or a credential reference such as `{"$credential":"password"}`.
- `execute_script` may still receive `bindings`, but every binding value must be a plain string.
  Do not pass credential references in `execute_script.bindings`.
- When `execute_script.bindings` contains `{"$credential":"..."}`, the backend rejects the request
  instead of resolving the secret into script-visible data.

### Examples

```bash
# Create a sandbox session
toani sandbox create-session \
  --service-id svc_example \
  --original-intent "Open the login page in TEE sandbox" \
  --start-url https://target-site.com/login

# Navigate
toani sandbox execute <sessionId> \
  --operation-type navigate \
  --params '{"url":"https://target-site.com/login"}'

# Bootstrap a Rocket Loader page before waiting/filling
toani sandbox bootstrap-page <sessionId> \
  --mode rocket_loader \
  --replay-lifecycle-events true \
  --wait-selector 'input[name=email]' \
  --wait-timeout-ms 15000

# Wait for the login form after bundle replay
toani sandbox execute <sessionId> \
  --operation-type wait \
  --params '{"selector":"input[name=email]","timeout_ms":15000}'

# Click
toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'

# Fill
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":"user@example.com"}'

# Fill from a stored credential reference
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'

# Login flow for Rocket Loader pages
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":{"$credential":"username"}}'
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'
toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'

# Execute script with plain-string bindings only
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return document.querySelector(bindings.selector)?.textContent?.trim() ?? null","bindings":{"selector":"h1"}}'

# Invalid: execute_script bindings cannot resolve credentials
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return bindings.password","bindings":{"password":{"$credential":"password"}}}'
# Expected result: backend rejects the request because execute_script bindings only support plain strings.

# Export redacted DOM
toani sandbox export-dom <sessionId> \
  --format html \
  --root-selector body \
  --include-text true \
  --include-metadata true \
  --extra-sensitive-selectors '["#token",".secret"]'

# Export selected text data
toani sandbox export-data <sessionId> \
  --format json \
  --selectors '[".balance",".status"]'

# Inspect operation result
toani sandbox get-operation <operationId>

# End the session
toani sandbox terminate <sessionId>
```

## Notes

- Dashboard UI is the supported creation surface for credentials and bearer tokens.
- CLI integrations only use bearer tokens. Browser-side Privy/session flows are not exposed as CLI
  commands.
- Prefer top-level controlled operations such as `fill` for credential consumption; do not design
  flows that require secrets to become script-visible values.
- `sandbox terminate` maps to the backend close-session route (`DELETE /api/v1/sandbox/sessions/:id`).

## Common Failures

- `未检测到 CLI 可用的 API Token`
  - Create or copy the token in the Dashboard UI, then run `toani config init --url <api-url> --token <BEARER_TOKEN>` or set `TOANI_VAULT_TOKEN`.

- `missing required field: credential_id or service_id`
  - Provide `--credential-id` for secret-backed login, or provide `--service-id` when the backend
    should resolve the credential for you.

- `credential_id does not match service_id '<service>'`
  - The session is bound to a credential from a different service. Recheck the Dashboard UI and use
    the credential that belongs to the target service.

- `bootstrap_failed: selector_not_found: ...`
  - Inspect `discovered_scripts`, `reinjected_scripts`, `ready_state_before_scan`,
    `ready_state_after_injection`, `matched_selectors`, `sample_script_descriptors`, and
    `selector_exists_at_failure`. If the page mounts late, retry with `--replay-lifecycle-events true`.

- `bootstrap-page only accepts fixed bootstrap flags; raw scripts, bindings, and --params are not supported`
  - Use only the dedicated bootstrap flags. Do not send raw script text, bindings, or `--params` to
    `bootstrap-page`.

- `execute_script.bindings` rejects a credential reference
  - Move the secret to `fill.value` or another controlled host operation. `execute_script.bindings`
    only accepts plain strings.

- 401 / 403 responses from the API
  - Confirm the bearer token came from the Dashboard UI and that the CLI is reading the intended
    `~/.toani/config.json`, environment variables, and global flags.
