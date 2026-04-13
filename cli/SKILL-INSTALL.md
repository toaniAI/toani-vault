# Install Toani CLI Skill

## Prerequisites

- Node.js >= 22
- npm login (for publish/private installs)

## Local Dev Install

```bash
cd /Users/yvan/AIWorkspace/credbridge/cli
npm install
npm run build
npm pack
npm install -g ./toani-vault-cli-0.0.2.tgz
```

## Registry Install

```bash
npm install -g @toani/vault-cli@0.0.2
```

## Smoke Checks

```bash
toani --help
toani --version
toani config show
```

## Recommended Base URL Setup

```bash
export TOANI_BASE_URL=https://dev-credbridge.bitkinetic.com/
toani auth status
```
