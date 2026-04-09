import { ToaniVaultSDK } from '@toani/vault-sdk';
import type { CliConfig, ParsedOptions } from '../types/cli.js';
import { fail } from '../output/print.js';

export function parseOptions(argv: string[]): ParsedOptions {
  const options: ParsedOptions = { _: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token.startsWith('--')) {
      const key = token.slice(2);
      const next = argv[i + 1];
      if (!next || next.startsWith('--')) {
        options[key] = true;
      } else {
        options[key] = next;
        i += 1;
      }
      continue;
    }
    options._.push(token);
  }
  return options;
}

export function createSdk(config: CliConfig): ToaniVaultSDK {
  return new ToaniVaultSDK({
    baseUrl: config.baseUrl,
    token: config.token,
    timeout: config.timeout,
  });
}

export function requireArg(
  options: ParsedOptions,
  key: string,
  message?: string
): string {
  const value = options[key];
  if (typeof value !== 'string' || value.length === 0) {
    fail(message ?? `Missing required option --${key}`);
  }
  return value;
}

export function parseJsonOption(options: ParsedOptions, key: string): Record<string, unknown> {
  const raw = options[key];
  if (typeof raw !== 'string') {
    fail(`Missing required option --${key}`);
  }
  try {
    return JSON.parse(raw) as Record<string, unknown>;
  } catch (error) {
    fail(`Invalid JSON for --${key}: ${(error as Error).message}`);
  }
}

export function capabilityMissing(name: string): never {
  fail(`SDK capability missing: ${name}`);
}
