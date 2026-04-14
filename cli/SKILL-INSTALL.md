# Install Toani CLI Skill

## Prerequisites

- Node.js >= 22
- npm login (only needed for publish/private installs)

## Local Dev Install

```bash
cd /Users/yvan/AIWorkspace/credbridge/cli
npm install
npm run build
npm pack
npm install -g ./toani-vault-cli-0.0.5.tgz
```

## Registry Install

```bash
npm install -g @toani/vault-cli@0.0.5
```

## Smoke Checks

```bash
toani --help
toani --version
toani config show
toani sandbox stats
```

## Recommended Setup

```bash
export TOANI_BASE_URL=https://dev-credbridge.bitkinetic.com/
export TOANI_VAULT_TOKEN=<BEARER_TOKEN>
toani config init --url https://dev-credbridge.bitkinetic.com --token <BEARER_TOKEN>
```

## Important Scope Note

The currently published CLI exposes:

- `config`
- `sandbox`

Do not expect `auth`, `credentials`, `tokens`, `service-accounts`, or `audit` groups unless you
have verified a newer build.
