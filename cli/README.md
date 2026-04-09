# @toani/vault-cli

Toani Vault npm CLI package.

- Package: `@toani/vault-cli`
- Executable: `toani`

## Install

```bash
npm install -g @toani/vault-cli
```

## Configure

```bash
toani config init --url https://dev-credbridge.bitkinetic.com/ --token <TOKEN>
```

Config is stored at `~/.toani/config.json` with fields:
- `baseUrl`
- `token` (API Access Token)
- `sessionToken` (Session Token)
- `output` (`table` or `json`)
- `timeout`

Token resolution priority:
1. explicit `--token`
2. `config.token`
3. `config.sessionToken`

Base URL resolution priority:
1. explicit `--base-url`
2. env `TOANI_BASE_URL`
3. env `CREDBRIDGE_BASE_URL`
4. `config.baseUrl`
5. default `https://dev-credbridge.bitkinetic.com/`

## Commands

```bash
toani auth login --url <URL> --token <TOKEN>
toani auth status
toani auth session --privy-access-token <PRIVY_TOKEN>
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
```

## Notes

- `auth logout` only clears `sessionToken`; it does not clear `token`.
- `auth access-token create --store` writes the created access token into `config.token` (default behavior).
- `Privy Access Token` is only for exchanging `Session Token`.
- `API Access Token` and `Service Account Token` are for API/CLI automation calls.
