# Static Implementation Notes

## Scope

Plan target: `http_request` credential wrapper + alias support and rejection semantics.

## Findings

`C1` static evidence is present in the local repository.

- Backend wrapper parsing and contract checks are present in [src/api/sandbox.rs](/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs:1514).
  Relevant behaviors observed:
  - `prefix` / `suffix` fields are part of the credential reference struct.
  - wrapper object only allows `$credential`, `prefix`, `suffix`.
  - non-string `prefix` / `suffix` produce explicit validation errors.
- Backend alias support is present in [src/api/sandbox.rs](/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs:1665) and [src/tee/sandbox/session.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/session.rs:1593).
  Relevant behaviors observed:
  - API key payload accepts `api_key`.
  - legacy aliases `key` and `apiKey` are normalized alongside `api_key`.
- Backend negative-path guards are present in [src/api/sandbox.rs](/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs:1431) and [src/tee/sandbox/session.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/session.rs:772).
  Relevant behaviors observed:
  - `bootstrap_page` rejects credential references.
  - `execute_script` rejects credential references.
- Static test coverage exists for wrapper parsing, alias handling, and rejection semantics in [src/api/sandbox.rs](/Users/yvan/AIWorkspace/credbridge/src/api/sandbox.rs:2707) and [src/tee/sandbox/session.rs](/Users/yvan/AIWorkspace/credbridge/src/tee/sandbox/session.rs:1928).
- TS SDK canonical API key serialization is present in [sdk-typescript/src/credentials.ts](/Users/yvan/AIWorkspace/credbridge/sdk-typescript/src/credentials.ts:126).
  `createApiKey(...)` builds `plaintextData` as `{ api_key: apiKey }`.
- CLI public docs mention `http_request` credential reference wrapping in [cli/README.md](/Users/yvan/AIWorkspace/credbridge/cli/README.md:253) and [cli/SKILL.md](/Users/yvan/AIWorkspace/credbridge/cli/SKILL.md:20).
  Example shown: `{"$credential":"api_key","prefix":"Bearer "}`.

## Static conclusion

Local code matches the implementation surface described by the plan for `C1`.
