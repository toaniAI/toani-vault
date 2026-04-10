import { CredentialType } from "../../../sdk-typescript/src/index.ts";
import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import {
  capabilityMissing,
  createSdk,
  parseJsonOption,
  parseOptions,
  requireArg,
} from "./common.js";

function toCredentialType(value: string): CredentialType {
  switch (value) {
    case "username_password":
      return CredentialType.UsernamePassword;
    case "oauth_refresh":
      return CredentialType.OAuthRefresh;
    case "api_key":
      return CredentialType.ApiKey;
    case "session_cookie":
      return CredentialType.SessionCookie;
    case "kyc_document":
      return CredentialType.KycDocument;
    default:
      throw new Error(`Unsupported credential_type: ${value}`);
  }
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
        credentialType: options["credential-type"]
          ? toCredentialType(options["credential-type"] as string)
          : undefined,
      });
      printResult(result, config.output);
      return;
    }
    case "get": {
      const id = options._[0];
      if (!id) throw new Error("Usage: toani credentials get <id>");
      const result = await sdk.credentials.get(id);
      printResult(result, config.output);
      return;
    }
    case "create": {
      const serviceId = requireArg(options, "service-id");
      const credentialType = toCredentialType(
        requireArg(options, "credential-type"),
      );
      const plaintextData = parseJsonOption(options, "data");
      const expiresAt = options["expires-at"]
        ? Number(options["expires-at"])
        : undefined;
      const created = await sdk.credentials.create({
        serviceId,
        credentialType,
        plaintextData,
        expiresAt,
      });
      printResult(created, config.output);
      return;
    }
    case "update":
      capabilityMissing("credentials.update");
    case "delete": {
      const id = options._[0];
      if (!id) throw new Error("Usage: toani credentials delete <id>");
      const deleted = await sdk.credentials.delete(id);
      printResult(deleted, config.output);
      return;
    }
    case "decrypt": {
      const id = options._[0];
      if (!id)
        throw new Error(
          "Usage: toani credentials decrypt <id> [--reason <text>]",
        );
      const reason = options.reason as string | undefined;
      const decrypted = await sdk.credentials.decrypt(id, reason);
      printResult(decrypted, config.output);
      return;
    }
    case "versions":
      capabilityMissing("credentials.versions");
    case "rollback":
      capabilityMissing("credentials.rollback");
    default:
      throw new Error(
        "Usage: toani credentials <list|get|create|update|delete|decrypt|versions|rollback> [options]",
      );
  }
}
