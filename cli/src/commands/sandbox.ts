import type { OperationType } from "../../../sdk-typescript/src/index.js";
import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import {
  createSdk,
  parseJsonOption,
  parseOptions,
  generateRequestId,
  requireArg,
} from "./common.js";

function parseNumberOption(value: unknown, name: string): number | undefined {
  if (value === undefined) return undefined;
  if (typeof value === "number") return value;
  if (typeof value !== "string") return undefined;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    throw new Error(`Invalid number for --${name}: ${value}`);
  }
  return parsed;
}

function readStringValue(
  value: unknown,
  source: string,
  field: string,
): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string") {
    throw new Error(`Invalid ${field} in ${source}: expected string`);
  }
  const normalized = value.trim();
  if (!normalized) {
    throw new Error(`Invalid ${field} in ${source}: must not be empty`);
  }
  return normalized;
}

function takeStringParam(
  params: Record<string, unknown>,
  snakeKey: string,
  camelKey: string,
): string | undefined {
  const snakeValue = readStringValue(params[snakeKey], "--params", snakeKey);
  const camelValue = readStringValue(params[camelKey], "--params", camelKey);
  delete params[snakeKey];
  delete params[camelKey];
  return snakeValue ?? camelValue;
}

export async function runSandbox(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);
  const sdk = createSdk(config);

  switch (subcommand) {
    case "request": {
      const operationType = requireArg(
        options,
        "operation-type",
      ) as OperationType;
      const params = options.params ? parseJsonOption(options, "params") : {};
      const paramCredentialId = takeStringParam(
        params,
        "credential_id",
        "credentialId",
      );
      const paramServiceId = takeStringParam(params, "service_id", "serviceId");
      const paramRequestId = takeStringParam(params, "request_id", "requestId");
      const credentialId =
        readStringValue(options["credential-id"], "--credential-id", "credential-id") ??
        paramCredentialId;
      const serviceId =
        readStringValue(options["service-id"], "--service-id", "service-id") ??
        paramServiceId;
      const requestId =
        readStringValue(options["request-id"], "--request-id", "request-id") ??
        paramRequestId ??
        generateRequestId();
      const result = await sdk.sandbox.request({
        operationType,
        credentialId,
        serviceId,
        requestId,
        description: options.description as string | undefined,
        parameters: params,
        method: params.method as string | undefined,
        headers: params.headers as
          | Record<string, string | { $credential: string }>
          | undefined,
        body: params.body,
        timeout:
          typeof params.timeout_ms === "number"
            ? params.timeout_ms
            : typeof params.timeout === "number"
              ? params.timeout
              : undefined,
      });
      printResult(result, config.output);
      return;
    }
    case "get-request": {
      const operationId = options._[0];
      if (!operationId)
        throw new Error("Usage: toani-vault sandbox get-request <operationId>");
      const result = await sdk.sandbox.getRequest(operationId);
      printResult(result, config.output);
      return;
    }
    default:
      throw new Error(
        "Usage: toani-vault sandbox <request|get-request> [options]",
      );
  }
}
