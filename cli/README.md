# @toani/vault-cli

Toani Vault npm CLI package.

- Package: `@toani/vault-cli`
- Executable: `toani`

## Install

```bash
npm install -g @toani/vault-cli
```

## Configure

Recommended CLI bootstrap:

1. Sign in to the web app.
2. Open `Profile -> Automation Access`.
3. Create an automation token for your tenant scopes.
4. Configure the CLI with that token:

```bash
toani config init --url https://dev-credbridge.bitkinetic.com/ --token <AUTOMATION_TOKEN>
```

`Privy Access Token` exchange is browser-oriented and is not expected to be available directly from a terminal workflow.

Legacy/manual session import is still supported when you already have a session token:

```bash
toani auth login --url https://dev-credbridge.bitkinetic.com/ --session-token <SESSION_TOKEN>
```

Config is stored at `~/.toani/config.json` with fields:

- `baseUrl`
- `automationToken` (preferred API token for automation commands)
- `sessionToken` (Session Token)
- `currentTenantId`
- `currentProfile`
- `profiles`
- `output` (`table` or `json`)
- `timeout`

Token resolution priority:

1. explicit `--token`
2. active profile `automationToken`
3. env `TOANI_VAULT_TOKEN`
4. `sessionToken` only for session-only auth commands

Base URL resolution priority:

1. explicit `--base-url`
2. env `TOANI_BASE_URL`
3. env `CREDBRIDGE_BASE_URL`
4. `config.baseUrl`
5. default `https://dev-credbridge.bitkinetic.com/`

## Commands

```bash
toani auth login --url <URL> --session-token <TOKEN>
toani auth status
toani auth session --privy-access-token <PRIVY_TOKEN>
toani auth use-tenant <tenant-id>
toani auth token create --name <name> --scope <scope1,scope2> [--ttl-seconds 900] [--save]
toani auth token list
toani auth token get <token-id>
toani auth token revoke <token-id>
toani auth access-token create --scope <scope1,scope2> [--ttl-seconds 900] [--store]
toani auth access-token revoke --token-id <id>
toani auth me
toani auth memberships
toani auth logout

toani credentials list [--service-id <id>] [--credential-type <type>]
toani credentials get <credentialId>
toani credentials create --service-id <id> --credential-type <type> --data '{"k":"v"}'
toani credentials delete <credentialId>
toani credentials decrypt <credentialId> [--reason <text>]

toani tokens create [--expires-in 3600] [--scope credential:read]
toani tokens list
toani tokens get <token-id>
toani tokens verify [--token <token>]
toani tokens stats
toani tokens revoke [--token-id <id>]

toani service-accounts create --name <name> --scope <scope1,scope2> [--description <text>]
toani service-accounts list
toani service-accounts get <id>
toani service-accounts update <id> [--name <name>] [--description <text>] [--status <active|disabled|deleted>] [--scope <scope1,scope2>]
toani service-accounts token create <service-account-id> --scope <scope1,scope2> [--ttl-seconds 3600] [--display-name <name>]
toani service-accounts token list <service-account-id>

toani sandbox create-session --service-id <service> --original-intent <intent> [--credential-id <id>] [--start-url <url>]
toani sandbox list-sessions
toani sandbox get-session <sessionId>
toani sandbox terminate <sessionId>
toani sandbox execute <sessionId> --operation-type <type> [--params '{"selector":"#btn"}']
toani sandbox get-operation <operationId>
toani sandbox stats

toani audit logs [--from <iso>] [--to <iso>] [--action <name>] [--service <name>] [--outcome <ok|error>] [--limit 50]
toani audit export [--format json|csv] [--from <iso>] [--to <iso>]
toani audit verify [--payload '{"log_id":"..."}']

toani config show
toani config set <key> <value>
toani config get <key>
toani config profile create <name>
toani config profile use <name>
toani config profile show
```

## Notes

- `auth logout` only clears `sessionToken`; it does not clear `automationToken`.
- `auth token create --save` writes the created automation token into the active profile.
- `Privy Access Token` is only for exchanging `Session Token` and is primarily a browser-side flow.
- `API Access Token` and `Service Account Token` are for API/CLI automation calls.
