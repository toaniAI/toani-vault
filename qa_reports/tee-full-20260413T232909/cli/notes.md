C10 outcome: failed (`failure_class: product`)

Summary:
- The published CLI build can persist a real bearer token into `~/.toani/config.json` when invoked with global `--base-url` and `--token` flags.
- The README-promised `config`, `auth`, `credentials`, `tokens`, and `audit` command groups are not exposed by the current CLI entrypoint. Each invocation fails before any API call with `Unknown command group: ...`.
- The only exposed group is `sandbox`. `sandbox create-session` correctly enforces local argument validation. `sandbox stats` and `sandbox list-sessions` exit non-zero with no surfaced error text when run against the live service.
- Direct HTTP diagnostics show the live access token from this run is valid bearer material but only carries `credential:read`, so `/api/v1/sandbox/stats` returns `403 insufficient_scope`. The opaque web session token is also unsuitable for the CLI and was already revoked by the prior auth test, yielding `401 invalid_token`.

Auth source used:
- Primary CLI bearer: `api-auth/raw/access_token.txt` (PASETO access token, created in C06)
- Secondary negative source: `api-auth/raw/session_token.txt` (opaque session token, rejected/revoked)

Top contract drift:
- `cli/README.md` tells users to bootstrap with `toani config init ...`, but the shipped CLI has no `config` group.
- `cli/README.md` documents `auth`, `credentials`, `tokens`, `audit`, and `service-accounts`, but `cli/src/index.ts` routes only `sandbox`.
- The live acceptance flow currently produces `credential:read` tokens, which are insufficient for the only exposed CLI group (`sandbox:*`).

Why claim failed:
- C10 requires the CLI to drive the documented command surface across multiple groups. The current published build cannot do that because the entrypoint does not expose those groups at all.

Runtime/log notes:
- `logmcp` was not available in this session, but it was not required to classify the primary failure because the route mismatch is local to the CLI build and the live sandbox scope failure was confirmed directly via HTTP `403 insufficient_scope`.

Leader recheck:
- Reconfirmed `cli/dist/index.js --help` only advertises the `sandbox` group.
- Reconfirmed `toani auth status` fails immediately with `Unknown command group: auth`.
- Reconfirmed live `GET /api/v1/sandbox/stats` with the current access token returns `403 insufficient_scope` with `required_scope=sandbox:read` and `current_scopes=credential:read`.
- Reconfirmed the prior web session token is unusable for sandbox CLI calls because the live API returns `401 invalid_token` after revocation.
