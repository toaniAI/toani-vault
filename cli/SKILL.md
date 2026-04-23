# Toani Vault CLI Skill

## Goal

Provide agents with an immediately executable `toani` usage guide, with special focus on:

- `login` / `doctor` onboarding and health checks
- the current real command surface
- the read-only limits of `credentials list/get`
- the standard `sandbox` execution chain
- the controlled page bootstrap semantics of `bootstrap_page`
- the actual semantics of `execute_script.bindings` and secret consumption
- minimal executable examples for common tasks

## Core Mental Model

1. `toani` is a CLI, not SDK pseudocode.
2. `sandbox` is a remote TEE browser session provided by the CredBridge backend. It is not a local browser and not the agent's own runtime node.
3. Page interactions are composed with `toani sandbox create-session`, `toani sandbox bootstrap-page`, and `toani sandbox execute`.
4. `http_request` is a backend-side direct HTTP call. It does not launch the remote browser.
5. For Rocket Loader-style pages, explicitly run `bootstrap-page` before `wait` / `fill` / `click`.
6. When you need page state, prefer `get-session`, `get-operation`, `export-dom`, and `execute_script`.
7. Always `terminate` when finished. Do not leave long-lived active sessions behind.
8. The Dashboard / UI remains the creation surface for credentials and tokens. The CLI currently exposes credential metadata reads only and does not create, update, delete, or decrypt credentials.
9. `login` is now the preferred entry path. `config init --token` remains only as a compatibility path.
10. Tokens should now be stored in the OS Keychain first instead of being written to `~/.toani/config.json` by default.

## Current Supported Surface

Trust the current CLI implementation and `toani --help` before trusting older documentation.

The currently exposed command groups are:

- `login`
- `doctor`
- `config`
- `credentials`
- `sandbox`
- `--help`
- `--version`

Do not assume these command groups exist unless you verify them first:

- `auth`
- `tokens`
- `service-accounts`
- `audit`

`credentials` currently exposes only two read-only subcommands:

- `toani credentials list [--service-id <id>] [--credential-type <type>] [--only-valid true|false]`
- `toani credentials get <credentialId>`

Do not assume these not-yet-exposed `credentials` subcommands exist:

- `create`
- `update`
- `delete`
- `decrypt`

## Dashboard / UI Responsibility Boundary

- Credentials are created on the Dashboard Credentials page.
- Bearer tokens are issued on the Dashboard Tokens page.
- The CLI reads those artifacts. It can currently read credential metadata directly, but it does not create credentials or tokens.
- The recommended flow is `toani login`, which opens the Dashboard and guides the user through obtaining credentials and tokens.

## Global Flags and Precedence

Global flags:

- `--output json|table`
- `--base-url <URL>`
- `--token <TOKEN>`

Important: these global flags must appear before the command group, for example:

- `toani --output json credentials list`
- `toani --base-url https://api.example.com sandbox stats`

Do not write them like this:

- `toani credentials list --output json`
- `toani sandbox stats --base-url https://api.example.com`

Base URL precedence:

1. `--base-url`
2. `TOANI_BASE_URL`
3. `CREDBRIDGE_BASE_URL`
4. `config.baseUrl`
5. Default `TOANI_VAULT_DASHBOARD_BASE_URL` or `https://dashboard.toani.ai`

Token precedence:

1. `--token`
2. `TOANI_VAULT_TOKEN`
3. `CREDBRIDGE_TOKEN`
4. OS Keychain `toani-vault-cli:default`
5. Legacy `config.token`

Notes:

- `--base-url` and `--output` are written back to `~/.toani/config.json`
- `config init --token` now writes the token to the OS Keychain instead of defaulting to plaintext config storage
- The CLI still reads a historical `token` in `~/.toani/config.json`, but that is now a legacy path
- Default to `--output json` for automation
- Never log a token or commit it to the repository

## Standard Initialization

```bash
toani --help
toani login
toani doctor
toani --output json config show
toani --output json credentials list
```

If the user does not have a token yet, prefer `toani login`. Do not start by asking them to type `config init --token` unless they explicitly need the legacy-compatible flow.

If the user gives only a "credential name" but not a `credential_id`, first verify the current environment and token, then prefer `toani credentials list --service-id <service>` to inspect metadata instead of guessing.
If the credential is intended for secret-backed login, first confirm the real `credential_id` with `toani credentials list` or `toani credentials get <id>`, then pass that ID to `create-session`. If the CLI token cannot read metadata, fall back to confirming it in the Dashboard UI.

## Onboarding / Doctor

### `toani login`

```bash
toani login [--base-url <URL>] [--skip-validate]
```

Actual semantics:

- interactive onboarding
- supports three paths:
  - existing account, with browser guidance into the Dashboard
  - sign up first, then return to the main flow
  - already have a token, using `.env` / clipboard / manual paste
- watches the clipboard and auto-detects a PASETO token
- while clipboard watching is active:
  - `P` switches to manual paste
  - `Q` cancels
- validates the token through the API by default
- `--skip-validate` skips only the API validation step, not the interactive flow
- after validation succeeds, the token is written to the OS Keychain when possible
- after login succeeds, the bundled `SKILL.md` can optionally be installed into `~/.claude/skills/toani-vault-cli/` or `~/.codex/skills/toani-vault-cli/`
- if the Keychain write fails, the CLI clearly reports "not persisted" and does not silently write the token back to plaintext config

### `toani doctor`

```bash
toani doctor [--base-url <URL>]
```

Checks:

1. CLI version
2. Node.js version
3. Token storage
4. Token format
5. Base URL
6. Server reachable
7. Token valid

It prefers the Keychain token. If only the legacy `config.json.token` path is found, it warns about plaintext storage.

## Credentials Command Table

```bash
toani credentials list [--service-id <id>] [--credential-type <type>] [--only-valid true|false]
toani credentials get <credentialId>
```

Capability boundary:

- returns credential metadata only
- does not return plaintext secrets
- does not perform decryption
- still requires the bearer token to include `credential:read`
- if a request fails, first check token scope, base URL, and target environment

## Sandbox Command Table

```bash
toani sandbox create-session --service-id <serviceId> --original-intent <intent> [--credential-id <id>]
toani sandbox list-sessions
toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
toani sandbox pause <sessionId>
toani sandbox resume <sessionId>
toani sandbox bootstrap-page <sessionId> --mode rocket_loader [--script-selectors '<json-array>'] [--include-plain-scripts true|false] [--replay-lifecycle-events true|false] [--wait-selector <selector>] [--wait-timeout-ms <ms>]
toani sandbox execute <sessionId> --operation-type <type> [--params '<json>']
toani sandbox export-dom <sessionId> [--format html|text|json] [--root-selector body]
toani sandbox export-data <sessionId> --selectors '<json-array>' [--format json|csv|pdf]
toani sandbox get-operation <operationId>
toani sandbox stats
```

## Supported `execute` Operation Types

Browser operations:

- `navigate`
- `click`
- `fill`
- `bootstrap_page` (through the `sandbox bootstrap-page` subcommand)
- `get_text`
- `execute_script`
- `wait`
- `export`
- `dom_export`

Non-browser operations:

- `http_request`

Do not use these legacy operation names:

- `get_attribute`
- `screenshot`

## Common `--params` Fields

Combine these as needed by operation type:

- `url`
- `selector`
- `value`
- `mode`
- `script_selectors`
- `include_plain_scripts`
- `replay_lifecycle_events`
- `wait_selector`
- `wait_timeout_ms`
- `script`
- `bindings`
- `timeout_ms`
- `milliseconds`
- `duration_ms`
- `method`
- `headers`
- `body`
- `selectors`
- `sensitive`

When calling `execute --operation-type dom_export`, use the backend field names:

- `root_selector`
- `format`
- `include_text`
- `include_metadata`
- `extra_sensitive_selectors`
- `max_bytes`

## Standard Call Flow

When the user wants to "open a page in the TEE browser and operate on it", use this order:

1. `toani --help`
2. `toani --output json config show`
3. If you need a credential ID, first query and confirm it with `toani credentials list` / `get`
4. `toani sandbox create-session ...`
5. `toani sandbox execute <sessionId> --operation-type navigate ...`
6. If the page uses Rocket Loader or the bundle has not started, explicitly run `toani sandbox bootstrap-page <sessionId> --mode rocket_loader ...`
   For pages with weaker compatibility or late-mounted lifecycle hooks, prefer adding `--replay-lifecycle-events true`
7. `toani sandbox execute <sessionId> --operation-type wait ...`
8. `toani sandbox execute <sessionId> --operation-type fill|click ...`
9. If you get an async response, call `toani sandbox get-operation <operationId>`
10. When you need to confirm state, call `toani sandbox get-session <sessionId>`
11. Finish with `toani sandbox terminate <sessionId>`

Do not skip `create-session`.

## Secret Usage Rules

### First separate "getting a credential ID" from "consuming a secret in the sandbox"

These are different actions:

1. First obtain the `credential_id`
   The preferred path is `toani credentials list` / `toani credentials get`
2. Then bind that credential to the TEE session with `toani sandbox create-session --credential-id ...`
3. Later, when `fill` runs, the backend resolves and consumes the relevant field inside the sandbox on demand

Important:

- `credential_id` is only a credential reference, not plaintext
- the correct path is not "decrypt locally and pass it into the browser"; it is "bind the session first, then consume it inside the TEE via controlled host operations"
- `bootstrap-page` only performs controlled bundle replay; it does not consume credentials and does not expose secrets to scripts
- when `--script-selectors` is omitted, the backend uses built-in generic external-script discovery rules; `include_plain_scripts` still only controls whether non-Rocket-Loader executable scripts may be replayed
- session creation for secret-backed login should explicitly bind `credential_id`; do not guess credentials from `service_id` alone

### Referencing credentials in `fill`

This is the allowed and recommended secret consumption pattern:

```bash
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'
```

### Controlled injection semantics of `bootstrap-page`

This subcommand exists specifically to explicitly replay the controlled bundle startup after Lightpanda opens a Rocket Loader page:

- only fixed `mode=rocket_loader` is allowed
- only controlled fields are accepted; `--params` is not accepted
- raw script is not accepted
- `bindings` are not accepted
- optional `replay_lifecycle_events` re-emits `DOMContentLoaded` / `load` / `pageshow` after bundle replay
- does not consume credentials
- later secret consumption must still happen through controlled host operations such as `fill`

If `wait_selector` times out, the error now includes diagnostics such as URL, title, script discovery / replay counts, and `readyState` so you can distinguish "scripts did not match" from "the page never mounted".

Recommended login chain:

1. `navigate`
2. `bootstrap-page`
3. `wait`
4. `fill`
5. `click`

Example:

```bash
toani sandbox bootstrap-page <sessionId> \
  --mode rocket_loader \
  --replay-lifecycle-events true \
  --wait-selector 'input[name=email]' \
  --wait-timeout-ms 15000
```

### `bindings` in `execute_script`

This is the most important current semantic rule:

- `bindings` only accept plain strings
- `bindings` do not support `{"$credential":"..."}`
- secrets are not allowed into the `execute_script` context

Correct example:

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return document.querySelector(bindings.selector)?.getAttribute(\"href\") ?? null","bindings":{"selector":"a.download"}}'
```

Incorrect example:

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return bindings.password","bindings":{"password":{"$credential":"password"}}}'
```

Expected result:

- the backend rejects the request directly
- the error text explains that `execute_script.bindings` only accepts plain strings
- no plaintext secret should appear in the result or in the error

## Common Example List

### Example 1: Initialize configuration

```bash
toani login
toani doctor
```

### Example 1A: Initialize configuration via the compatibility path

```bash
toani config init --url https://api.example.com --token <BEARER_TOKEN>
toani --output json config show
```

### Example 2: Create a sandbox session

```bash
toani sandbox create-session \
  --service-id svc_example \
  --original-intent "Open target page in TEE sandbox"
```

### Example 3: Create a sandbox session with credential binding

```bash
toani sandbox create-session \
  --service-id svc_example \
  --credential-id <credentialId> \
  --original-intent "Login with credential-backed session"
```

### Example 3A: List readable credential metadata

```bash
toani --output json credentials list
```

### Example 3B: Filter credential metadata by service

```bash
toani --output json credentials list \
  --service-id svc_example \
  --only-valid true
```

### Example 3C: Read one credential metadata record

```bash
toani --output json credentials get <credentialId>
```

### Example 4: Navigate

```bash
toani sandbox execute <sessionId> \
  --operation-type navigate \
  --params '{"url":"https://target-site.com/login"}'
```

### Example 5: Explicitly bootstrap a Rocket Loader page

```bash
toani sandbox bootstrap-page <sessionId> \
  --mode rocket_loader \
  --replay-lifecycle-events true \
  --wait-selector 'input[name=email]' \
  --wait-timeout-ms 15000
```

### Example 6: Click a button

```bash
toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'
```

### Example 7: Fill plain text

```bash
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":"user@example.com"}'
```

### Example 8: Fill a credential-backed field

```bash
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'
```

### Example 9: Wait for an element

```bash
toani sandbox execute <sessionId> \
  --operation-type wait \
  --params '{"selector":"#dashboard","timeout_ms":10000}'
```

### Example 10: Read text

```bash
toani sandbox execute <sessionId> \
  --operation-type get_text \
  --params '{"selector":"h1"}'
```

### Example 11: Execute a script to read an attribute or text value

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return document.querySelector(bindings.selector)?.textContent?.trim() ?? null","bindings":{"selector":"h1"}}'
```

### Example 12: Execute a script to read page state

```bash
toani sandbox execute <sessionId> \
  --operation-type execute_script \
  --params '{"script":"return { href: location.href, title: document.title }"}'
```

### Example 13: Export a redacted DOM

```bash
toani sandbox export-dom <sessionId> \
  --format html \
  --root-selector body \
  --include-text true \
  --include-metadata true \
  --extra-sensitive-selectors '["#token",".secret"]'
```

### Example 14: Backend-side direct HTTP request

```bash
toani sandbox execute <sessionId> \
  --operation-type http_request \
  --params '{"url":"https://api.example.com/status","method":"GET","timeout_ms":10000}'
```

### Example 15: Export text from multiple selectors

```bash
toani sandbox execute <sessionId> \
  --operation-type export \
  --params '{"selectors":["h1",".status"]}'
```

### Example 16: Export structured data

```bash
toani sandbox export-data <sessionId> \
  --selectors '["h1",".status"]' \
  --format json
```

### Example 17: Query an asynchronous operation result

```bash
toani sandbox get-operation <operationId>
```

### Example 18: Query session state

```bash
toani sandbox get-session <sessionId>
```

### Example 19: `test-web.zk.me` login flow

```bash
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
```

### Recommended `test-web.zk.me` command chain

When validating secret-backed login, prefer this order:

1. Create the credential in the Dashboard UI and confirm the `credential_id`
2. Generate a bearer token in the Dashboard UI
3. Reconfirm the target credential with `toani --output json credentials list --service-id <service>` or `toani --output json credentials get <credentialId>`
4. `toani sandbox create-session --service-id <service> --credential-id <credentialId> --original-intent "Sign in to test-web.zk.me"`
5. `toani sandbox execute <sessionId> --operation-type navigate --params '{"url":"https://test-web.zk.me/login"}'`
6. `toani sandbox bootstrap-page <sessionId> --mode rocket_loader --replay-lifecycle-events true --wait-selector 'input[name=email]' --wait-timeout-ms 15000`
7. `toani sandbox execute <sessionId> --operation-type wait --params '{"selector":"input[name=email]","timeout_ms":15000}'`
8. `toani sandbox execute <sessionId> --operation-type fill --params '{"selector":"input[name=email]","value":{"$credential":"username"}}'`
9. `toani sandbox execute <sessionId> --operation-type fill --params '{"selector":"input[name=password]","value":{"$credential":"password"}}'`
10. `toani sandbox execute <sessionId> --operation-type click --params '{"selector":"button[type=submit]"}'`
11. `toani sandbox get-session <sessionId>`
12. `toani sandbox terminate <sessionId>`

### Example 20: End the session

```bash
toani sandbox terminate <sessionId>
```

## Explicitly Forbidden Misuse

These are all incorrect:

- treating the TEE sandbox as the agent's own nodes, graph, or workflow nodes
- running `sandbox execute` without a `sessionId`
- inventing command groups that the CLI does not implement
- treating `sandbox` like local Playwright, browser devtools, or an OpenClaw built-in browser operator
- using legacy operations such as `get_attribute` or `screenshot`
- passing raw script, `bindings`, or `--params` into `bootstrap-page`
- letting secrets enter `execute_script.bindings`
- replacing concrete commands and parameters with natural-language descriptions

## Common Errors and Fixes

- Error: `Unknown command group`
  - Fix: run `toani --help` first and do not use unpublished command groups from old docs

- Error: `Usage: toani credentials get <credentialId>`
  - Fix: `get` requires the positional `<credentialId>` argument explicitly

- Error: `Invalid boolean for --only-valid: <value>`
  - Fix: `--only-valid` only accepts parseable booleans such as `true` / `false`

- Error: `No usable API token was found for the CLI`
  - Fix: prefer `toani login`; the compatibility path is to create or copy a token in the Dashboard UI, then run `toani config init --url <api-url> --token <BEARER_TOKEN>`, or set `TOANI_VAULT_TOKEN`

- Error: `Token invalid`, `Insufficient scope`, `DNS error`, `Connection refused`, `Timeout`
  - Fix: these are categorized validation results from `toani login`. Regenerate the token, widen scope, correct `--base-url`, verify network / VPN access, or start the target backend as needed

- Error: `Usage: toani sandbox create-session ...` or missing `--service-id`, `--original-intent`
  - Fix: those two parameters are required for `create-session`

- Error: `missing required field: credential_id or service_id`
  - Fix: for secret-backed login, prefer explicitly passing `--credential-id`; if you want backend-side service resolution, pass `--service-id`

- Error: `credential_id does not match service_id '<service>'`
  - Fix: the bound credential belongs to a different service. Recheck the Dashboard UI and retry with a credential from the target service

- Error: `Usage: toani sandbox execute <sessionId> --operation-type <type>`
  - Fix: `execute` requires both `sessionId` and `--operation-type`

- Error: `Usage: toani sandbox bootstrap-page ...` or `bootstrap-page currently only supports --mode rocket_loader`
  - Fix: explicitly pass `sessionId` and `--mode rocket_loader`; the first release supports only the controlled Rocket Loader bootstrap flow

- Error: `Invalid JSON for --params`
  - Fix: `--params` must be a valid JSON string. Wrapping the whole JSON object in single quotes is recommended

- Error: connected to the wrong environment
  - Fix: explicitly pass `--base-url`, then confirm it with `toani config show`

- Error: backend returned an unsupported `operation_type`
  - Fix: compare it against the operation type list in this document and do not use `get_attribute` or `screenshot`

- Error: `execute_script.bindings` received `{"$credential":"..."}`
  - Fix: move secret consumption to a controlled top-level field such as `fill.value`; script bindings must contain only plain strings

- Error: `bootstrap-page only accepts fixed bootstrap flags; raw scripts, bindings, and --params are not supported`
  - Fix: use only the controlled fields: `--script-selectors`, `--include-plain-scripts`, `--replay-lifecycle-events`, `--wait-selector`, and `--wait-timeout-ms`

- Error: `bootstrap_failed: selector_not_found: ...`
  - Fix: inspect `discovered_scripts`, `reinjected_scripts`, `ready_state_before_scan`, `ready_state_after_injection`, `ready_state`, `body_present`, `matched_selectors`, `sample_script_descriptors`, and `selector_exists_at_failure`. For partially compatible pages, try `--replay-lifecycle-events true` first. If `discovered_scripts=0`, use `matched_selectors` / `sample_script_descriptors` to determine whether selectors missed or `include_plain_scripts` blocked normal scripts

- Error: browser runtime errors mentioning `browser runtime closed without response`, `lightpanda`, `puppeteer-core`, `CDP`, or `nsjail`
  - Fix: this is a remote Lightpanda runtime or isolation-policy issue, not a local CLI browser problem. Preserve the `operationId`, run `toani sandbox get-operation <operationId>`, and hand the session, operation, base URL, and error details to the backend team

- Error: 401 / 403
  - Fix: first confirm the token came from the Dashboard UI, then check the precedence order between `--token`, environment variables, Keychain, and legacy `~/.toani/config.json`. If the wrong environment is selected, verify `--base-url`

## Security Notes

- Local config path: `~/.toani/config.json`
- New tokens are no longer written to the config file by default, but historical files may still contain legacy tokens
- OS Keychain service name: `toani-vault-cli`, account name: `default`
- Prefer environment-variable token injection for automation
- Never write bearer tokens into long-lived scripts, screenshots, or logs
