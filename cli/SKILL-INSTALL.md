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
npm install -g ./toani-vault-cli-*.tgz
```

## Registry Install

```bash
npm install -g @toani/vault-cli@latest
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
export TOANI_VAULT_DASHBOARD_BASE_URL=https://dashboard.example.com
export TOANI_BASE_URL=https://api.example.com/
export TOANI_VAULT_TOKEN=<BEARER_TOKEN>
toani config init --url https://api.example.com --token <BEARER_TOKEN>
```

After `toani login`, the CLI can also copy the bundled `SKILL.md` into:

- `~/.claude/skills/toani-vault-cli/SKILL.md`
- `~/.codex/skills/toani-vault-cli/SKILL.md`

You can choose Claude Code, Codex, both, or skip the install.

## Important Scope Note

The currently published CLI exposes:

- `login`
- `doctor`
- `config`
- `credentials`
- `sandbox`

Do not expect `auth`, `tokens`, `service-accounts`, or `audit` groups unless you have verified a
newer build.
