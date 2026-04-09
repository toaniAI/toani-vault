# Install Toani CLI Skill

## Prerequisites

- Node.js >= 22
- npm login (for publishing or private install if needed)

## Local Dev Install

```bash
cd /Users/yvan/AIWorkspace/credbridge/cli
npm install
npm run build
npm pack
npm install -g ./toani-vault-cli-0.1.0.tgz
```

## Registry Install

```bash
npm install -g @toani/vault-cli
```

## Smoke Checks

```bash
toani --help
toani --version
toani config show
```
