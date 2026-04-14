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
npm install -g @toani/vault-cli@0.0.5
```

## Configure

Issue a restricted bearer token in the Dashboard first, then configure the CLI with flags,
environment variables, or local config:

```bash
export TOANI_BASE_URL="https://dev-credbridge.bitkinetic.com"
export TOANI_VAULT_TOKEN="<BEARER_TOKEN>"

toani config init --url https://dev-credbridge.bitkinetic.com --token <BEARER_TOKEN>
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
2. active profile `token`
3. `TOANI_VAULT_TOKEN`
4. `CREDBRIDGE_TOKEN`

Base URL resolution priority:

1. explicit `--base-url`
2. `TOANI_BASE_URL`
3. `CREDBRIDGE_BASE_URL`
4. `config.baseUrl`
5. default `https://dev-credbridge.bitkinetic.com/`

## Commands

```bash
toani config init --url <service-url> [--token <BEARER_TOKEN>]
toani config show
toani sandbox create-session --service-id <service> --original-intent <intent> [--credential-id <id>] [--start-url <url>]
toani sandbox list-sessions
toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
toani sandbox execute <sessionId> --operation-type <type> [--params '{"selector":"#btn"}']
toani sandbox get-operation <operationId>
toani sandbox stats
toani --version
toani --help
```

## Sandbox Workflow

The CLI controls remote TEE sandbox sessions. It is not a local browser runner and not an abstract
"sandbox node" system.

Use this sequence:

1. `toani sandbox create-session`
2. `toani sandbox execute`
3. `toani sandbox get-operation` when the server returns an operation id
4. `toani sandbox get-session` when you need current state
5. `toani sandbox terminate`

### Operation types

- `navigate`
- `click`
- `fill`
- `get_text`
- `get_attribute`
- `execute_script`
- `wait`
- `screenshot`
- `export`

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

# Click
toani sandbox execute <sessionId> \
  --operation-type click \
  --params '{"selector":"button[type=submit]"}'

# Fill
toani sandbox execute <sessionId> \
  --operation-type fill \
  --params '{"selector":"input[name=email]","value":"user@example.com"}'

# Inspect operation result
toani sandbox get-operation <operationId>

# End the session
toani sandbox terminate <sessionId>
```

## Notes

- Dashboard is the supported public token issuance surface for CLI usage.
- CLI integrations only use bearer tokens. Browser-side Privy/session flows are not exposed as CLI
  commands.
- `sandbox terminate` maps to the backend close-session route (`DELETE /api/v1/sandbox/sessions/:id`).
