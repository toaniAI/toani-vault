# Public Branch Sanitization Rule

This rule governs how `master` is prepared for `public`, and how `public` must be reviewed before it is pushed to GitHub.

## Scope

- `master` is the source of truth for product development, internal operations, and QA history.
- `public` is a sanitized mirror intended for GitHub sync.
- `opensource` is reference-only. Do not merge from it directly.

## Master Baseline Rules

- Keep this rule file on `master` and update it whenever the sanitization boundary changes.
- Do not reintroduce the `frontend` gitlink or root-level workflow references to `frontend/`.
- The nested `frontend/` repository may exist locally for developer convenience, but it must stay untracked by this repository.
- Root documentation and current developer entrypoints must not instruct users to run frontend commands from this repository.

## Public Branch Content Rules

### Always sanitize in `public`

- CI/CD files that contain private branch names, registries, deploy images, secret backends, or internal domains.
- Container and chart defaults that contain internal image registries, private hostnames, private namespaces, or real credentials.
- Example env files that contain internal service URLs, production-like secrets, or real third-party credentials.

### Remove entirely from `public`

- `zentao/`
- `docs/requirements/`
- `docs/qa_reports/`
- Internal-only agent and DevOps references that are tied to private infrastructure, including:
  - `.agents/skills/devops-config-gen/`
  - `.agents/skills/credbridge-k8s-exec/`
  - `.agents/skills/toani-vault-cli/`

### Keep only as public-safe templates in `public`

- `.drone.yml`
- `Dockerfile`
- `.values.yaml`
- `.test.values.yaml`
- `.env.example`
- `docker/`
- `config/chart/`

Replace private values with explicit placeholders such as:

- `ghcr.io/example-org/example-image`
- `registry.example.com/example/project`
- `https://pccs.example.com/sgx/certification/v4/`
- `postgresql://username:password@db.example.com:5432/credbridge`
- `redis://:password@redis.example.com:6379/0`
- `replace-with-your-secret`

## Review Checklist Before Updating `public`

### Git topology

- `git ls-files --stage` must not contain a `160000` entry for `frontend`.
- If other orphaned gitlinks block clean clone or push, record them explicitly before changing them.

### Required scans

- `rtk rg -n 'frontend/' README.md README_zh.md AGENTS.md docs/07-developer-guide/README.md docs/07-developer-guide/test-strategy/README.md`
- `rtk rg -n 'bitkinetic|aliyuncs|branch_dev|branch_test|deploy-auth-key|deploy-base-url|docker-hub-username|docker-hub-password|git@git\\.bitkinetic' . --hidden --glob '!.git'`
- `rtk rg -n 'PRIVY_APP_SECRET|postgresql://|redis://|VAULT_TOKEN=' .env.example .values.yaml .test.values.yaml`

All hits in `public` must either be removed or reduced to public-safe placeholders.

### Structural validation

- `rtk docker compose -f docker/docker-compose.yml config`
- `rtk cargo fmt`
- `rtk cargo clippy --tests -- -D warnings`
- `rtk cargo test`

If Helm is available in the environment, also run:

- `rtk helm template credbridge ./config/chart -f config/chart/values.yaml`

## Merge Maintenance

- Make backend and SDK changes on `master` first.
- When promoting to `public`, replay only public-safe changes and re-run the scans above.
- Do not cherry-pick internal QA evidence, internal runbooks, or private deployment automation into `public`.
