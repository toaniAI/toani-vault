import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import { createSdk, parseOptions, requireArg } from "./common.js";
import { saveConfig } from "../config/store.js";

function bearerFromConfig(config: CliConfig): string | undefined {
  return config.token;
}

function requireBearer(config: CliConfig): string {
  const bearer = bearerFromConfig(config);
  if (!bearer) {
    throw new Error(
      "No bearer token found. Configure one with `--token` or `toani config init --token ...` first.",
    );
  }
  return bearer;
}

export async function runAuth(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const sdk = createSdk(config);
  const options = parseOptions(rest);

  switch (subcommand) {
    case "status": {
      printResult(
        {
          currentProfile: config.currentProfile ?? "default",
          credentialSource: config.token ? "token" : "none",
          tenantId: config.currentTenantId,
          tokenPresent: Boolean(config.token),
          baseUrl: config.baseUrl,
        },
        config.output,
      );
      return;
    }
    case "me": {
      requireBearer(config);
      const me = await sdk.auth.me();
      printResult(me, config.output);
      return;
    }
    case "logout": {
      saveConfig({ ...config, token: undefined, automationToken: undefined });
      printResult(
        {
          ok: true,
          localTokenCleared: true,
        },
        config.output,
      );
      return;
    }
    case "use-tenant": {
      const tenantId = options._[0];
      if (!tenantId) {
        throw new Error("Usage: toani auth use-tenant <tenant-id>");
      }
      saveConfig({ ...config, currentTenantId: tenantId });
      printResult({ ok: true, currentTenantId: tenantId }, config.output);
      return;
    }
    case "memberships": {
      requireBearer(config);
      const memberships = await sdk.auth.memberships();
      printResult(memberships, config.output);
      return;
    }
    case "token": {
      const nested = options._[0];

      if (nested === "create") {
        requireBearer(config);
        const name = requireArg(
          options,
          "name",
          "Usage: toani auth token create --name <name> --scope <scope1,scope2> [--ttl-seconds 86400] [--save]",
        );
        const scopes = requireArg(options, "scope")
          .split(",")
          .map((scope) => scope.trim())
          .filter(Boolean);
        const ttlRaw = options["ttl-seconds"];
        const ttlSeconds =
          typeof ttlRaw === "string" ? Number(ttlRaw) : undefined;
        const response = await sdk.auth.createAutomationToken(
          {
            name,
            description: options.description as string | undefined,
            scopes,
            ttlSeconds,
            createdVia: "cli",
          },
        );
        const shouldSave = Boolean(options.save);
        if (shouldSave) {
          saveConfig({
            ...config,
            token: response.tokenValue,
          });
        }
        printResult({ ...response, saved: shouldSave }, config.output);
        return;
      }

      if (nested === "list") {
        requireBearer(config);
        const items = await sdk.auth.listAutomationTokens();
        printResult(items, config.output);
        return;
      }

      if (nested === "get") {
        requireBearer(config);
        const tokenId = options._[1];
        if (!tokenId) {
          throw new Error("Usage: toani auth token get <token-id>");
        }
        const item = await sdk.auth.getAutomationToken(tokenId);
        printResult(item, config.output);
        return;
      }

      if (nested === "revoke") {
        requireBearer(config);
        const tokenId =
          options._[1] ?? (options["token-id"] as string | undefined);
        if (!tokenId) {
          throw new Error("Usage: toani auth token revoke <token-id>");
        }
        const item = await sdk.auth.revokeAutomationToken(tokenId);
        printResult(item, config.output);
        return;
      }

      throw new Error(
        "Usage: toani auth token <create|list|get|revoke> [options]",
      );
    }
    case "access-token": {
      const nested = options._[0];
      if (nested === "create") {
        const scopesRaw = options.scope as string | undefined;
        if (!scopesRaw) {
          throw new Error(
            "Usage: toani auth access-token create --scope <scope1,scope2> [--ttl-seconds <seconds>] [--store]",
          );
        }
        const scopes = scopesRaw
          .split(",")
          .map((scope) => scope.trim())
          .filter(Boolean);
        const ttlRaw = options["ttl-seconds"];
        const ttlSeconds =
          typeof ttlRaw === "string" ? Number(ttlRaw) : undefined;
        requireBearer(config);
        const result = await sdk.auth.createAccessToken({
          scopes,
          ttlSeconds,
        });

        const shouldStore = options.store !== false;
        if (shouldStore) {
          saveConfig({
            ...config,
            token: result.accessToken,
          });
        }

        printResult(
          {
            ...result,
            stored: shouldStore,
          },
          config.output,
        );
        return;
      }
      if (nested === "revoke") {
        const tokenId = requireArg(
          options,
          "token-id",
          "Usage: toani auth access-token revoke --token-id <token-id>",
        );
        requireBearer(config);
        const revoked = await sdk.auth.revokeAccessToken(tokenId);
        printResult({ revoked, tokenId }, config.output);
        return;
      }
      throw new Error(
        "Usage: toani auth access-token <create|revoke> [options]",
      );
    }
    default:
      throw new Error(
        "Usage: toani auth <status|logout|me|memberships|use-tenant|token|access-token> [options]",
      );
  }
}
