import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { CliConfig, OutputFormat } from '../types/cli.js';

const CONFIG_DIR = path.join(os.homedir(), '.toani');
const CONFIG_PATH = path.join(CONFIG_DIR, 'config.json');

const DEFAULT_CONFIG: CliConfig = {
  baseUrl: 'https://dev-credbridge.bitkinetic.com/',
  output: 'table',
  timeout: 30000,
};

export function getConfigPath(): string {
  return CONFIG_PATH;
}

export function loadConfig(): CliConfig {
  if (!fs.existsSync(CONFIG_PATH)) {
    return { ...DEFAULT_CONFIG };
  }
  const raw = fs.readFileSync(CONFIG_PATH, 'utf8');
  const parsed = JSON.parse(raw) as Partial<CliConfig>;
  return {
    ...DEFAULT_CONFIG,
    ...parsed,
    output: (parsed.output === 'json' ? 'json' : 'table') as OutputFormat,
    timeout: typeof parsed.timeout === 'number' ? parsed.timeout : DEFAULT_CONFIG.timeout,
  };
}

export function saveConfig(config: CliConfig): void {
  if (!fs.existsSync(CONFIG_DIR)) {
    fs.mkdirSync(CONFIG_DIR, { recursive: true });
  }
  fs.writeFileSync(CONFIG_PATH, JSON.stringify(config, null, 2), 'utf8');
}
