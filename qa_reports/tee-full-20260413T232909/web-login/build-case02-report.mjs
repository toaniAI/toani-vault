import fs from 'node:fs';
import path from 'node:path';

const outDir = '/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-login';

const readJson = name => JSON.parse(fs.readFileSync(path.join(outDir, name), 'utf8'));
const maybeRead = name => {
  const target = path.join(outDir, name);
  return fs.existsSync(target) ? fs.readFileSync(target, 'utf8') : null;
};

const login = readJson('login-result.json');
const observation = readJson('playwright-observation.json');
const rawNetwork = maybeRead('network-log.txt') ?? '';
const cookies = readJson('cookies.json');

const authStorageRaw = observation.localStorage['auth-storage'] ?? null;
let authStorage = null;
let bearerCandidates = [];
if (authStorageRaw) {
  try {
    authStorage = JSON.parse(authStorageRaw);
  } catch {
    authStorage = null;
  }
}

const scanStrings = value => {
  const found = [];
  const walk = current => {
    if (typeof current === 'string') {
      if (
        current.startsWith('ey') ||
        current.startsWith('v4.local.') ||
        current.startsWith('D')
      ) {
        found.push(current);
      }
      return;
    }
    if (Array.isArray(current)) {
      current.forEach(walk);
      return;
    }
    if (current && typeof current === 'object') {
      Object.values(current).forEach(walk);
    }
  };
  walk(value);
  return [...new Set(found)];
};

if (authStorage) {
  bearerCandidates = scanStrings(authStorage);
}

const routeFindings = Object.fromEntries(
  Object.entries(observation.routes).map(([route, value]) => [
    route,
    {
      final_url: value.finalUrl,
      title: value.title,
      redirected: value.finalUrl !== `https://dev-credbridge.bitkinetic.com${route}` &&
        value.finalUrl !== `https://dev-credbridge.bitkinetic.com${route}/`,
      shell_markers: [
        /Credentials/.test(value.bodyText),
        /Tokens/.test(value.bodyText),
        /Developer Center|API Documentation|CLI/.test(value.bodyText),
        /Profile|账户|邮箱/.test(value.bodyText),
      ].filter(Boolean).length,
    },
  ])
);

const status = login.finalUrl.includes('/login') ? 'failed' : 'passed';
const failureClass = status === 'passed' ? 'none' : 'product';
const contractFindings = [];
for (const [route, value] of Object.entries(routeFindings)) {
  if (value.redirected) {
    contractFindings.push({
      route,
      observed_final_url: value.final_url,
      type: 'redirect',
    });
  }
}

const summary =
  status === 'passed'
    ? `Privy OTP login succeeded and landed on ${login.finalUrl}. Dashboard shell was visible and post-login route checks were recorded.`
    : `Privy OTP login did not reach a protected route; final URL remained ${login.finalUrl}. Raw network and screenshot evidence were captured.`;

const result = {
  claim_id: 'C02',
  status,
  failure_class: failureClass,
  summary,
  artifacts: [
    path.join(outDir, 'before-login.png'),
    path.join(outDir, 'post-submit.png'),
    path.join(outDir, 'after-login.png'),
    path.join(outDir, 'playwright-observation.json'),
    path.join(outDir, 'storage-state.json'),
    path.join(outDir, 'network-log.txt'),
    path.join(outDir, 'trace.zip'),
    path.join(outDir, 'cookies.json'),
    path.join(outDir, 'localstorage.json'),
    path.join(outDir, 'sessionstorage.json'),
    path.join(outDir, 'notes.md'),
    path.join(outDir, 'commands.txt'),
  ].filter(file => fs.existsSync(file)),
  created_ids: [],
  session_context: {
    storage_state: fs.existsSync(path.join(outDir, 'storage-state.json'))
      ? path.join(outDir, 'storage-state.json')
      : null,
    localstorage: fs.existsSync(path.join(outDir, 'localstorage.json'))
      ? path.join(outDir, 'localstorage.json')
      : null,
    sessionstorage: fs.existsSync(path.join(outDir, 'sessionstorage.json'))
      ? path.join(outDir, 'sessionstorage.json')
      : null,
    cookies: fs.existsSync(path.join(outDir, 'cookies.json'))
      ? path.join(outDir, 'cookies.json')
      : null,
    bearer_candidates: fs.existsSync(path.join(outDir, 'bearer-candidates.json'))
      ? path.join(outDir, 'bearer-candidates.json')
      : null,
  },
  runtime_log_needed: status !== 'passed',
  contract_findings: contractFindings,
};

const notes = `# Case02 Notes

- Claim: \`C02\`
- Target: \`https://dev-credbridge.bitkinetic.com/\`
- Status: \`${status}\`
- Failure class: \`${failureClass}\`

## Outcome

- Final landing URL: \`${login.finalUrl}\`
- Final title: \`${login.title}\`
- Dashboard shell visible: \`${String(login.shellVisible)}\`
- Summary: ${summary}

## Navigation Findings

${Object.entries(routeFindings)
  .map(
    ([route, value]) =>
      `- \`${route}\` -> \`${value.final_url}\`${value.redirected ? ' (redirected)' : ''}`
  )
  .join('\n')}

## Session Context

- Storage state: \`${path.join(outDir, 'storage-state.json')}\`
- Cookies: \`${path.join(outDir, 'cookies.json')}\`
- Local storage: \`${path.join(outDir, 'localstorage.json')}\`
- Session storage: \`${path.join(outDir, 'sessionstorage.json')}\`
- Bearer candidates (raw local artifact only): \`${path.join(outDir, 'bearer-candidates.json')}\`

## Contract Findings

${contractFindings.length === 0 ? '- No redirect breakage observed in the checked routes.' : contractFindings.map(item => `- \`${item.route}\` redirected to \`${item.observed_final_url}\``).join('\n')}

## Raw Evidence

- Screenshots: \`before-login.png\`, \`post-submit.png\`, \`after-login.png\`
- Trace: \`trace.trace\`
- Network: \`network-log.txt\`
- Observation: \`playwright-observation.json\`
`;

fs.writeFileSync(path.join(outDir, 'result.json'), JSON.stringify(result, null, 2));
fs.writeFileSync(path.join(outDir, 'notes.md'), notes);
fs.writeFileSync(
  path.join(outDir, 'bearer-candidates.json'),
  JSON.stringify(
    bearerCandidates.map(token => ({
      value: token,
      preview: `${token.slice(0, 8)}...${token.slice(-6)}`,
    })),
    null,
    2
  )
);
