import type { CredentialType } from "../../../sdk-typescript/src/index.js";
import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import { createSdk, parseOptions } from "./common.js";

const RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE = "credential_runtime_access";

type CredentialSummary = {
  credentialId?: string;
  credential_id?: string;
  requiresApproval?: boolean;
  requires_approval?: boolean;
};

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

function requiresApproval(value: CredentialSummary): boolean {
  return value.requiresApproval === true || value.requires_approval === true;
}

function printApprovalFlowForCredential(credentialId?: string): void {
  console.log("Approval required before sandbox execution:");
  console.log("1. toani-vault approvals generate-request-id");
  console.log(
    `2. toani-vault approvals create --business-type ${RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE} --business-id <request_id> [--wait]`,
  );
  const targetHint = credentialId
    ? `--credential-id ${credentialId}`
    : "--credential-id <credential_id>";
  console.log(
    `3. toani-vault sandbox request --operation-type http_request ${targetHint} --request-id <request_id> --params '{...}'`,
  );
  console.log(
    "4. request_id is single-use after a successful execution; generate a new one for the next approved run.",
  );
}

function printApprovalSummaryForList(): void {
  console.log("Some listed credentials require runtime approval before sandbox execution.");
  console.log(
    "Use `toani-vault credentials get <credentialId>` to confirm the target, then run `toani-vault approvals generate-request-id` before `toani-vault approvals create` and `toani-vault sandbox request --request-id <request_id>`.",
  );
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
      if (
        config.output === "table" &&
        Array.isArray(result.items) &&
        result.items.some((item) => requiresApproval(item))
      ) {
        printApprovalSummaryForList();
      }
      return;
    }
    case "get": {
      const credentialId = options._[0];
      if (!credentialId) {
        throw new Error("Usage: toani-vault credentials get <credentialId>");
      }
      const result = await sdk.credentials.get(credentialId);
      printResult(result, config.output);
      if (config.output === "table" && requiresApproval(result)) {
        printApprovalFlowForCredential(result.credentialId);
      }
      return;
    }
    default:
      throw new Error(
        "Usage: toani-vault credentials <list|get> [options]",
      );
  }
}
