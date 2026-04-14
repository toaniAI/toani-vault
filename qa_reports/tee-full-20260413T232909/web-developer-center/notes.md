## Case

- Claim: `C05`
- Target: `https://dev-credbridge.bitkinetic.com/developer`
- Outcome: reachable after auth, but visible Developer Center guidance is not contract-consistent

## What I validated

- Opening `/developer` without session redirected to `/login`, which is consistent with protected routing in [router.tsx](/Users/yvan/AIWorkspace/credbridge/frontend/src/app/router.tsx:40) and the live site behavior.
- Privy email OTP login with `test-7226@privy.io` and code `450192` succeeded.
- After login, `/developer` rendered and exposed four tabs: `系统说明`, `API 文档`, `SDK 示例`, `测试工具`.
- I captured the visible API Docs and SDK Examples content from the live page and compared it against:
  - [cli/README.md](/Users/yvan/AIWorkspace/credbridge/cli/README.md:14)
  - [USER_MANUAL.md](/Users/yvan/AIWorkspace/credbridge/docs/08-%E7%94%A8%E6%88%B7%E6%8C%87%E5%8D%97/USER_MANUAL.md:44)
  - [router.tsx](/Users/yvan/AIWorkspace/credbridge/frontend/src/app/router.tsx:94)

## Findings

1. Developer Center is live and reachable only after authentication. No public access path was available.
2. The page content is stale/incomplete for CLI bootstrap:
   - visible guidance says CLI cannot do Privy login and should use a dashboard-issued sandbox token directly with `toani ... sandbox stats`
   - current CLI README says the supported bootstrap is `Profile -> Automation Access` followed by `toani config init --url ... --token <AUTOMATION_TOKEN>`
3. The page content is stale/incomplete for CLI scope:
   - visible CLI guidance frames CLI as sandbox-only
   - current CLI README exposes `auth`, `credentials`, `tokens`, `service-accounts`, `sandbox`, `audit`, and `config` command families
4. The SDK examples are stale for base URL guidance:
   - multiple visible examples hard-code `http://localhost:8082`
   - current contract baseline uses `https://dev-credbridge.bitkinetic.com/` as the live default/example base URL
5. The page did **not** expose direct decrypt instructions in the visible content I captured. That is consistent with the current metadata-only direction and I did not record a decrypt mismatch from the live page itself.

## Baseline inconsistency noticed

- The user manual still documents CLI commands under `credbridge ...`, while the current CLI README and live Developer Center both use `toani ...`. I treated the README as the more current CLI contract, but this baseline drift should be cleaned up separately.
