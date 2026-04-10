import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import { createSdk, parseOptions } from "./common.js";

function parseScopes(raw: unknown): string[] {
  if (typeof raw !== "string") {
    return [];
  }
  return raw
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
}

function requirePositional(value: string | undefined, usage: string): string {
  if (!value) {
    throw new Error(usage);
  }
  return value;
}

export async function runServiceAccounts(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);
  const sdk = createSdk(config);
  const client = sdk.client;

  switch (subcommand) {
    case "create": {
      const name = options.name as string | undefined;
      if (!name) {
        throw new Error(
          "Usage: toani service-accounts create --name <name> --scope <scope1,scope2> [--description <text>]",
        );
      }
      const scopeCeiling = parseScopes(options.scope);
      if (scopeCeiling.length === 0) {
        throw new Error(
          "Usage: toani service-accounts create --name <name> --scope <scope1,scope2> [--description <text>]",
        );
      }
      const response = await client.post("/service-accounts", {
        name,
        description: options.description as string | undefined,
        scope_ceiling: scopeCeiling,
      });
      printResult(response, config.output);
      return;
    }
    case "list": {
      const response = await client.get("/service-accounts");
      printResult(response, config.output);
      return;
    }
    case "get": {
      const id = requirePositional(
        options._[0],
        "Usage: toani service-accounts get <id>",
      );
      const response = await client.get(`/service-accounts/${id}`);
      printResult(response, config.output);
      return;
    }
    case "update": {
      const id = requirePositional(
        options._[0],
        "Usage: toani service-accounts update <id> [--name <name>] [--description <text>] [--status <active|disabled|deleted>] [--scope <scope1,scope2>]",
      );
      const response = await client.patch(`/service-accounts/${id}`, {
        name: options.name as string | undefined,
        description: options.description as string | undefined,
        status: options.status as "active" | "disabled" | "deleted" | undefined,
        scope_ceiling:
          typeof options.scope === "string"
            ? parseScopes(options.scope)
            : undefined,
      });
      printResult(response, config.output);
      return;
    }
    case "token": {
      const nested = options._[0];
      if (nested === "create") {
        const serviceAccountId = requirePositional(
          options._[1],
          "Usage: toani service-accounts token create <service-account-id> --scope <scope1,scope2> [--ttl-seconds 3600] [--display-name <name>]",
        );
        const scopes = parseScopes(options.scope);
        if (scopes.length === 0) {
          throw new Error(
            "Usage: toani service-accounts token create <service-account-id> --scope <scope1,scope2> [--ttl-seconds 3600] [--display-name <name>]",
          );
        }
        const ttlRaw = options["ttl-seconds"];
        const ttlSeconds =
          typeof ttlRaw === "string" ? Number(ttlRaw) : undefined;
        const response = await client.post(
          `/service-accounts/${serviceAccountId}/tokens`,
          {
            scopes,
            ttl_seconds: ttlSeconds,
            display_name: options["display-name"] as string | undefined,
          },
        );
        printResult(response, config.output);
        return;
      }
      if (nested === "list") {
        const serviceAccountId = requirePositional(
          options._[1],
          "Usage: toani service-accounts token list <service-account-id>",
        );
        const response = await client.get(
          `/service-accounts/${serviceAccountId}/tokens`,
        );
        printResult(response, config.output);
        return;
      }
      throw new Error(
        "Usage: toani service-accounts token <create|list> <service-account-id> [options]",
      );
    }
    default:
      throw new Error(
        "Usage: toani service-accounts <create|list|get|update|token> [options]",
      );
  }
}
