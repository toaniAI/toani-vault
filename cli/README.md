# @toani/vault-cli

Toani Vault npm CLI package.

- Package: `@toani/vault-cli`
- Executable: `toani`

## Install

```bash
npm install -g @toani/vault-cli
```

## Configure

Current CLI supports `config` for local setup and `sandbox` for execution.
Issue a restricted token in the Dashboard first, then either persist the service URL with
`config init` or pass values directly via flags and environment variables:

```bash
export TOANI_BASE_URL="https://dev-credbridge.bitkinetic.com"
export TOANI_VAULT_TOKEN="<BEARER_TOKEN>"

toani config init --url https://dev-credbridge.bitkinetic.com
toani sandbox stats
toani --base-url https://dev-credbridge.bitkinetic.com --token <BEARER_TOKEN> sandbox list-sessions
```

When `--base-url`, `--token`, or `--output` are passed, the CLI persists those values to `~/.toani/config.json` for the active profile.

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
3. env `TOANI_VAULT_TOKEN`

Base URL resolution priority:

1. explicit `--base-url`
2. env `TOANI_BASE_URL`
3. env `CREDBRIDGE_BASE_URL`
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
```

## Notes

- Dashboard is the only supported manual token issuance surface for public CLI usage.
- CLI integrations only use bearer tokens. Browser-side Privy/session flows are not exposed as CLI commands.
- The published CLI exposes `config` plus the `sandbox` command group.
- `sandbox terminate` maps to the backend close-session route (`DELETE /api/v1/sandbox/sessions/:id`).
