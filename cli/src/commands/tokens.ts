import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import { createSdk, parseOptions } from "./common.js";

export async function runTokens(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);
  const sdk = createSdk(config);
  const client = sdk.client;

  switch (subcommand) {
    case "create": {
      const expiresIn = options["expires-in"]
        ? Number(options["expires-in"])
        : 3600;
      const scopeRaw = options.scope as string | undefined;
      const scopes = scopeRaw
        ? scopeRaw
            .split(",")
            .map((s) => s.trim())
            .filter(Boolean)
        : ["credential:read"];
      const result = await sdk.token.create({ scopes, expiresIn });
      printResult(result, config.output);
      return;
    }
    case "list": {
      const result = await client.get("/tokens");
      printResult(result, config.output);
      return;
    }
    case "get": {
      const tokenId = options._[0];
      if (!tokenId) {
        throw new Error("Usage: toani tokens get <token-id>");
      }
      const result = await client.get(`/tokens/${tokenId}`);
      printResult(result, config.output);
      return;
    }
    case "verify": {
      const token = options.token as string | undefined;
      if (token) sdk.token.setToken(token);
      const result = await sdk.token.verify();
      printResult({ valid: result }, config.output);
      return;
    }
    case "stats": {
      const result = await sdk.token.stats();
      printResult(result, config.output);
      return;
    }
    case "revoke": {
      const tokenId = options["token-id"] as string | undefined;
      if (tokenId) {
        const result = await client.post(`/tokens/${tokenId}/revoke`, {});
        printResult(result, config.output);
        return;
      }
      const current = sdk.token.getTokenInfo();
      if (!current?.tokenId) {
        throw new Error("Usage: toani tokens revoke --token-id <id>");
      }
      const result = await client.post(`/tokens/${current.tokenId}/revoke`, {});
      printResult(result, config.output);
      return;
    }
    default:
      throw new Error(
        "Usage: toani tokens <create|list|get|verify|stats|revoke> [options]",
      );
  }
}
