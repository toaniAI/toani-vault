import type { CliConfig } from "../types/cli.js";
import { keychain } from "../lib/keychain.js";
import { getConfigPath, saveConfig } from "../config/store.js";
import { printResult } from "../output/print.js";
import { parseOptions } from "./common.js";

export async function runConfig(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);

  switch (subcommand) {
    case "init": {
      const tokenOption = options.token as string | undefined;
      if (tokenOption) {
        keychain.set(tokenOption);
      }
      const next: CliConfig = {
        ...config,
        baseUrl: (options.url as string | undefined) ?? config.baseUrl,
        token: tokenOption ?? config.token,
        credentialSource: tokenOption ? "keychain" : config.credentialSource,
      };
      saveConfig(next);
      printResult(
        {
          ok: true,
          configPath: getConfigPath(),
          baseUrl: next.baseUrl,
          tokenConfigured: Boolean(next.token),
        },
        config.output,
      );
      return;
    }
    case "show": {
      printResult(
        {
          configPath: getConfigPath(),
          baseUrl: config.baseUrl,
          tokenConfigured: Boolean(config.token),
          output: config.output,
          timeout: config.timeout,
          currentProfile: config.currentProfile ?? "default",
        },
        config.output,
      );
      return;
    }
    default:
      throw new Error("Usage: toani config <init|show> [options]");
  }
}
