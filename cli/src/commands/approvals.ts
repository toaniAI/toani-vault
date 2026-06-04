import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import { createSdk, parseOptions, requireArg } from "./common.js";
import { generateCanonicalRequestId } from "../../../sdk-typescript/src/request-id.js";

const CREATE_USAGE =
  "Usage: toani-vault approvals create --business-type <type> --business-id <id>";
const STATUS_USAGE = "Usage: toani-vault approvals status <approvalId>";
const WAIT_USAGE =
  "Usage: toani-vault approvals wait <approvalId> [--timeout-ms <ms>] [--poll-interval-ms <ms>]";
const GENERATE_REQUEST_ID_USAGE =
  "Usage: toani-vault approvals generate-request-id";
const REJECTED_EXIT_CODE = 20;
const CANCELLED_EXIT_CODE = 21;
const TIMEOUT_EXIT_CODE = 124;
const RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE = "credential_runtime_access";
const TERMINAL_EXIT_CODES: Record<string, number> = {
  rejected: REJECTED_EXIT_CODE,
  cancelled: CANCELLED_EXIT_CODE,
};
const TERMINAL_STATUSES = new Set(["approved", "rejected", "cancelled"]);

interface ApprovalWaitResult {
  approval_id: string;
  tenant_id: string;
  requested_by: string;
  business_type: string;
  business_id: string;
  status: string;
  created_at: string;
  updated_at: string;
  processed_by?: string;
  processed_at?: string;
  remark?: string;
  result_code?: string;
  result_payload?: Record<string, unknown>;
  business_result_written_at?: string;
  wait_result?: "timeout";
  timeout_ms?: number;
}

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

function parseNumberOption(value: unknown, name: string): number | undefined {
  if (value === undefined) return undefined;
  if (typeof value === "number") return value;
  if (typeof value !== "string") {
    throw new Error(`Invalid number for --${name}: ${String(value)}`);
  }

  const parsed = Number(value);
  if (!Number.isFinite(parsed) || parsed < 0) {
    throw new Error(`Invalid number for --${name}: ${value}`);
  }

  return parsed;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForTerminalApproval(
  sdk: ReturnType<typeof createSdk>,
  approvalId: string,
  options: Record<string, unknown>,
  config: CliConfig,
): Promise<{ data: ApprovalWaitResult; exitCode?: number }> {
  const timeoutMs =
    parseNumberOption(options["timeout-ms"], "timeout-ms") ?? config.timeout;
  const pollIntervalMs =
    parseNumberOption(options["poll-interval-ms"], "poll-interval-ms") ?? 1_000;
  const startedAt = Date.now();

  while (true) {
    const detail = await sdk.approvals.get(approvalId);

    if (TERMINAL_STATUSES.has(detail.status)) {
      return {
        data: detail,
        exitCode: TERMINAL_EXIT_CODES[detail.status],
      };
    }

    const elapsedMs = Date.now() - startedAt;
    if (elapsedMs >= timeoutMs) {
      return {
        data: {
          ...detail,
          wait_result: "timeout",
          timeout_ms: timeoutMs,
        },
        exitCode: TIMEOUT_EXIT_CODE,
      };
    }

    const remainingMs = timeoutMs - elapsedMs;
    await sleep(Math.min(pollIntervalMs, remainingMs));
  }
}

async function printWaitResult(
  sdk: ReturnType<typeof createSdk>,
  approvalId: string,
  options: Record<string, unknown>,
  config: CliConfig,
): Promise<void> {
  const result = await waitForTerminalApproval(sdk, approvalId, options, config);
  printResult(result.data, config.output);
  if (result.exitCode && result.exitCode !== 0) {
    process.exitCode = result.exitCode;
  }
}

export async function runApprovals(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);

  switch (subcommand) {
    case "create": {
      const sdk = createSdk(config);
      const businessType = requireArg(options, "business-type", CREATE_USAGE);
      const businessId = requireArg(options, "business-id", CREATE_USAGE);
      const result = await sdk.approvals.create({
        businessType,
        businessId,
      });
      const waitForTerminal = parseBooleanOption(options.wait, "wait") ?? false;
      if (waitForTerminal) {
        await printWaitResult(sdk, result.approval_id, options, config);
        return;
      }
      printResult(result, config.output);
      return;
    }
    case "status": {
      const sdk = createSdk(config);
      const approvalId = options._[0];
      if (!approvalId) {
        throw new Error(STATUS_USAGE);
      }
      const result = await sdk.approvals.get(approvalId);
      printResult(result, config.output);
      return;
    }
    case "wait": {
      const sdk = createSdk(config);
      const approvalId = options._[0];
      if (!approvalId) {
        throw new Error(WAIT_USAGE);
      }
      await printWaitResult(sdk, approvalId, options, config);
      return;
    }
    case "generate-request-id": {
      if (options._.length > 0) {
        throw new Error(GENERATE_REQUEST_ID_USAGE);
      }
      printResult(
        {
          request_id: generateCanonicalRequestId(),
          business_type: RUNTIME_CREDENTIAL_APPROVAL_BUSINESS_TYPE,
          single_use: true,
        },
        config.output,
      );
      return;
    }
    default:
      throw new Error(
        "Usage: toani-vault approvals <create|status|wait|generate-request-id> [options]",
      );
  }
}
