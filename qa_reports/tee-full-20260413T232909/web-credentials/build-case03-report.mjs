import fs from 'node:fs';
import path from 'node:path';

const outDir = '/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials';
const residualPath =
  '/Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/residual-test-data.md';

const exists = file => fs.existsSync(path.join(outDir, file));
const rawInteraction = exists('interaction.raw.txt')
  ? fs.readFileSync(path.join(outDir, 'interaction.raw.txt'), 'utf8')
  : '';

const extractJson = raw => {
  const resultMatch = raw.match(/### Result\s*([\s\S]*?)\n### /);
  if (resultMatch) return JSON.parse(resultMatch[1].trim());

  const blockMatch = raw.match(/```json\s*([\s\S]*?)```/);
  if (blockMatch) return JSON.parse(blockMatch[1]);

  const lines = raw.split('\n');
  const start = lines.findIndex(line => line.trim().startsWith('{'));
  const end = [...lines].reverse().findIndex(line => line.trim().endsWith('}'));
  if (start === -1 || end === -1) {
    throw new Error('Could not extract JSON payload from interaction.raw.txt');
  }
  const finish = lines.length - end;
  return JSON.parse(lines.slice(start, finish).join('\n'));
};

const interaction = extractJson(rawInteraction);
fs.writeFileSync(path.join(outDir, 'interaction.json'), JSON.stringify(interaction, null, 2));
fs.writeFileSync(
  path.join(outDir, 'network-responses.json'),
  JSON.stringify(interaction.response_log ?? [], null, 2),
);

const created = interaction.created_ids ?? [];
const createOk = interaction.create_response?.status === 201 && created.length > 0 && interaction.created_visible_in_ui;
const noPlaintext =
  interaction.plaintext_exposed_in_ui === false && interaction.plaintext_exposed_in_responses === false;
const deleteVerified = interaction.delete_surface?.surfaced
  ? interaction.delete_response?.status === 200 || interaction.delete_response?.status === 204 || interaction.delete_surface?.stillVisible === false
  : null;

let status = 'passed';
let failureClass = 'none';

if (!interaction.list_rendered || !createOk) {
  status = 'failed';
  failureClass = 'product';
} else if (!noPlaintext) {
  status = 'failed';
  failureClass = 'product';
}

const contractFindings = [];
if (interaction.detail_surface?.mode === 'not_available') {
  contractFindings.push('No dedicated credential detail route or dialog surfaced from the created credential card.');
}
if (interaction.delete_surface?.surfaced === false) {
  contractFindings.push('Delete action was not surfaced for the created credential card.');
}
if (interaction.delete_surface?.surfaced === true && deleteVerified === false) {
  contractFindings.push('Delete action surfaced but did not remove the created credential or produce a successful DELETE response.');
}
if (noPlaintext) {
  contractFindings.push('UI body text and captured `/api/v1/credentials` responses did not expose plaintext username/password or `plaintext_data`.');
}

const summaryParts = [
  createOk
    ? `Credential creation via UI succeeded for service_id ${interaction.service_id}.`
    : 'Credential creation via UI did not complete successfully.',
  interaction.detail_surface?.mode === 'not_available'
    ? 'No dedicated detail surface was exposed from the credential list card.'
    : `Credential detail surfaced via ${interaction.detail_surface?.mode}.`,
  interaction.delete_surface?.surfaced
    ? deleteVerified === true
      ? 'Delete action was surfaced and completed.'
      : 'Delete action was surfaced but completion could not be verified.'
    : 'Delete action was not surfaced for the created credential.',
  noPlaintext
    ? 'No plaintext secret was observed in UI text or captured credential API responses.'
    : 'Plaintext secret material was observed in UI text or captured credential API responses.',
];

const artifacts = [
  path.join(outDir, 'credentials-snapshot.txt'),
  path.join(outDir, 'credentials-before-create.png'),
  path.join(outDir, 'new-credential-modal.png'),
  path.join(outDir, 'credentials-after-create.png'),
  path.join(outDir, 'credentials-after-detail-attempt.png'),
  path.join(outDir, 'credentials-after-delete-attempt.png'),
  path.join(outDir, 'interaction.json'),
  path.join(outDir, 'network-responses.json'),
  path.join(outDir, 'network-log.txt'),
  path.join(outDir, 'state-after.json'),
  path.join(outDir, 'trace.trace'),
  path.join(outDir, 'trace.network'),
].filter(file => fs.existsSync(file));

const result = {
  claim_id: 'C03',
  status,
  failure_class: failureClass,
  summary: summaryParts.join(' '),
  artifacts,
  created_ids: created,
  runtime_log_needed: false,
  contract_findings: contractFindings,
};

const notes = `# Case03 Notes

- Claim: \`C03\`
- Target: \`${interaction.target}\`
- Status: \`${status}\`
- Failure class: \`${failureClass}\`

## Outcome

- Service ID: \`${interaction.service_id}\`
- Credential created in UI: \`${String(createOk)}\`
- Detail surface: \`${interaction.detail_surface?.mode ?? 'unknown'}\`
- Delete surfaced: \`${String(interaction.delete_surface?.surfaced ?? false)}\`
- Delete verified: \`${deleteVerified === null ? 'not-applicable' : String(deleteVerified)}\`
- Plaintext exposed in UI: \`${String(interaction.plaintext_exposed_in_ui)}\`
- Plaintext exposed in responses: \`${String(interaction.plaintext_exposed_in_responses)}\`

## Summary

- ${summaryParts.join('\n- ')}

## Created IDs

${created.length ? created.map(item => `- \`${item.credential_id}\` / \`${item.label}\``).join('\n') : '- None'}

## Contract Findings

${contractFindings.length ? contractFindings.map(item => `- ${item}`).join('\n') : '- None'}

## Raw Evidence

${artifacts.map(item => `- \`${path.basename(item)}\``).join('\n')}
`;

const commands = [
  'rtk playwright-cli -s=c03 open https://dev-credbridge.bitkinetic.com/credentials',
  'rtk playwright-cli -s=c03 state-load /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-login/storage-state.json',
  'rtk playwright-cli -s=c03 goto https://dev-credbridge.bitkinetic.com/credentials',
  'rtk playwright-cli -s=c03 snapshot > /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials/credentials-snapshot.txt',
  'rtk playwright-cli -s=c03 tracing-start',
  'rtk playwright-cli -s=c03 run-code --filename /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials/case03-web-credentials.fn.js > /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials/interaction.raw.txt',
  'rtk playwright-cli -s=c03 network > /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials/network-log.txt',
  'rtk playwright-cli -s=c03 state-save /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials/state-after.json',
  'rtk playwright-cli -s=c03 tracing-stop > /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials/tracing-stop.txt',
  'rtk node /Users/yvan/AIWorkspace/credbridge/qa_reports/tee-full-20260413T232909/web-credentials/build-case03-report.mjs',
  'rtk playwright-cli -s=c03 close',
].join('\n');

fs.writeFileSync(path.join(outDir, 'result.json'), JSON.stringify(result, null, 2));
fs.writeFileSync(path.join(outDir, 'notes.md'), notes);
fs.writeFileSync(path.join(outDir, 'commands.txt'), commands + '\n');

if (created.length) {
  const existing = fs.existsSync(residualPath) ? fs.readFileSync(residualPath, 'utf8') : '';
  const lines = created
    .filter(item => !existing.includes(item.credential_id))
    .map(item => `- credential | ${item.credential_id} | ${item.label}`);
  if (lines.length) {
    const prefix = existing.endsWith('\n') || existing.length === 0 ? '' : '\n';
    fs.appendFileSync(residualPath, `${prefix}${lines.join('\n')}\n`);
  }
}
