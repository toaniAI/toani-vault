import type { CliConfig } from "../types/cli.js";
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
      const next: CliConfig = {
        ...config,
        baseUrl: (options.url as string | undefined) ?? config.baseUrl,
        token: options.token as string | undefined,
        automationToken: undefined,
        sessionToken: undefined,
        currentTenantId: options["tenant-id"] as string | undefined,
      };
      saveConfig(next);
      printResult(
        { ok: true, configPath: getConfigPath(), config: next },
        config.output,
      );
      return;
    }
    case "show": {
      printResult({ ...config, configPath: getConfigPath() }, config.output);
      return;
    }
    case "set": {
      const key = options._[0];
      const value = options._[1];
      if (!key || !value) {
        throw new Error("Usage: toani config set <key> <value>");
      }
      const next: CliConfig = { ...config };
      if (key === "baseUrl") next.baseUrl = value;
      else if (key === "token") next.token = value;
      else if (key === "currentTenantId") next.currentTenantId = value;
      else if (key === "timeout") next.timeout = Number(value);
      else if (key === "output")
        next.output = value === "json" ? "json" : "table";
      else throw new Error(`Unknown config key: ${key}`);
      saveConfig(next);
      printResult({ ok: true, key, value }, config.output);
      return;
    }
    case "get": {
      const key = options._[0];
      if (!key) {
        throw new Error("Usage: toani config get <key>");
      }
      printResult(
        { key, value: config[key as keyof CliConfig] },
        config.output,
      );
      return;
    }
    case "profile": {
      const nested = options._[0];
      if (nested === "create") {
        const name = options._[1];
        if (!name) {
          throw new Error("Usage: toani config profile create <name>");
        }
        const next: CliConfig = {
          ...config,
          profiles: {
            ...(config.profiles ?? {}),
            [name]: {
          baseUrl: config.baseUrl,
          token: config.token,
          output: config.output,
          timeout: config.timeout,
        },
          },
        };
        saveConfig(next);
        printResult({ ok: true, created: name }, config.output);
        return;
      }
      if (nested === "use") {
        const name = options._[1];
        if (!name) {
          throw new Error("Usage: toani config profile use <name>");
        }
        const profile = config.profiles?.[name];
        if (!profile) {
          throw new Error(`Unknown profile: ${name}`);
        }
        const next: CliConfig = {
          ...config,
          currentProfile: name,
          baseUrl: profile.baseUrl ?? config.baseUrl,
          token: profile.token ?? profile.automationToken,
          automationToken: undefined,
          sessionToken: undefined,
          currentTenantId: profile.currentTenantId,
          output: profile.output ?? config.output,
          timeout: profile.timeout ?? config.timeout,
        };
        saveConfig(next);
        printResult({ ok: true, currentProfile: name }, config.output);
        return;
      }
      if (nested === "show") {
        printResult(
          {
            currentProfile: config.currentProfile ?? "default",
            profiles: config.profiles ?? {},
          },
          config.output,
        );
        return;
      }
      throw new Error(
        "Usage: toani config profile <create|use|show> [options]",
      );
    }
    default:
      throw new Error(
        "Usage: toani config <init|show|set|get|profile> [options]",
      );
  }
}
