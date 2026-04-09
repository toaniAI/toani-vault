# Toani Vault CLI Skill

This skill documents how to use `@toani/vault-cli` (`toani`) for service-account and session operations.

## Purpose

Operate Toani Vault from terminal with a consistent command surface:
- auth
- config
- credentials
- tokens
- sandbox
- audit

## Quick Start

1. Install CLI

```bash
npm install -g @toani/vault-cli
```

2. Initialize config

```bash
toani config init --url http://localhost:8080 --token <TOKEN>
```

3. Check status

```bash
toani auth status
```

## Output Modes

- `table` (default)
- `json`

Set globally with `--output json` or in config.

## Safety

- Token/session token is stored locally in `~/.toani/config.json`.
- Do not commit local config files.
