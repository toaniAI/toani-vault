import type { CredentialType } from "../../../sdk-typescript/src/index.js";
import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import { createSdk, parseOptions } from "./common.js";

function parseBooleanOption(value: unknown, name: string): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value === "boolean") return value;
  if (typeof value !== "string") {
    throw new Error(`Invalid boolean for --${name}: ${String(value)}`);
  }

  const normalized = value.trim().toLowerCase();
  if (["true", "1", "yes", "on"].includes(normalized)) return true;
  if (["false", "0", "no", "off"].includes(normalized)) return false;
  throw new Error(`Invalid boolean for --${name}: ${value}`);
}

export async function runCredentials(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);
  const sdk = createSdk(config);

  switch (subcommand) {
    case "list": {
      const result = await sdk.credentials.list({
        serviceId: options["service-id"] as string | undefined,
        credentialType: options["credential-type"] as CredentialType | undefined,
        onlyValid: parseBooleanOption(options["only-valid"], "only-valid"),
      });
      printResult(result, config.output);
      return;
    }
    case "get": {
      const credentialId = options._[0];
      if (!credentialId) {
        throw new Error("Usage: toani credentials get <credentialId>");
      }
      const result = await sdk.credentials.get(credentialId);
      printResult(result, config.output);
      return;
    }
    default:
      throw new Error(
        "Usage: toani credentials <list|get> [options]",
      );
  }
}
