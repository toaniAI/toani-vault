import type { CliConfig } from '../types/cli.js';
import { getConfigPath, saveConfig } from '../config/store.js';
import { printResult } from '../output/print.js';
import { parseOptions } from './common.js';

export async function runConfig(config: CliConfig, argv: string[]): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);

  switch (subcommand) {
    case 'init': {
      const next: CliConfig = {
        ...config,
        baseUrl: (options.url as string | undefined) ?? config.baseUrl,
        token: options.token as string | undefined,
        sessionToken: options['session-token'] as string | undefined,
      };
      saveConfig(next);
      printResult({ ok: true, configPath: getConfigPath(), config: next }, config.output);
      return;
    }
    case 'show': {
      printResult({ ...config, configPath: getConfigPath() }, config.output);
      return;
    }
    case 'set': {
      const key = options._[0];
      const value = options._[1];
      if (!key || !value) {
        throw new Error('Usage: toani config set <key> <value>');
      }
      const next: CliConfig = { ...config };
      if (key === 'baseUrl') next.baseUrl = value;
      else if (key === 'token') next.token = value;
      else if (key === 'sessionToken') next.sessionToken = value;
      else if (key === 'timeout') next.timeout = Number(value);
      else if (key === 'output') next.output = value === 'json' ? 'json' : 'table';
      else throw new Error(`Unknown config key: ${key}`);
      saveConfig(next);
      printResult({ ok: true, key, value }, config.output);
      return;
    }
    case 'get': {
      const key = options._[0];
      if (!key) {
        throw new Error('Usage: toani config get <key>');
      }
      printResult({ key, value: config[key as keyof CliConfig] }, config.output);
      return;
    }
    default:
      throw new Error('Usage: toani config <init|show|set|get> [options]');
  }
}
