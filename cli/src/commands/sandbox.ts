import type { OperationType } from "../../../sdk-typescript/src/index.js";
import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import {
  createSdk,
  parseJsonOption,
  parseOptions,
  requireArg,
} from "./common.js";

function parseBooleanOption(value: unknown): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value === "boolean") return value;
  if (typeof value !== "string") return undefined;
  const normalized = value.trim().toLowerCase();
  if (["true", "1", "yes", "on"].includes(normalized)) return true;
  if (["false", "0", "no", "off"].includes(normalized)) return false;
  return undefined;
}

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

function parseJsonStringArray(
  value: unknown,
  name: string,
): string[] | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string") return undefined;
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch (error) {
    throw new Error(`Invalid JSON for --${name}: ${(error as Error).message}`);
  }
  if (
    !Array.isArray(parsed) ||
    parsed.some((item) => typeof item !== "string")
  ) {
    throw new Error(`--${name} must be a JSON array of strings`);
  }
  return parsed;
}

function resolveSessionId(options: Record<string, unknown>): string {
  const positional =
    Array.isArray(options._) && typeof options._[0] === "string"
      ? options._[0]
      : undefined;
  const named =
    typeof options["session-id"] === "string"
      ? (options["session-id"] as string)
      : undefined;
  return positional ?? named ?? "";
}

export async function runSandbox(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);
  const sdk = createSdk(config);

  switch (subcommand) {
    case "create-session": {
      const serviceId = requireArg(options, "service-id");
      const originalIntent = requireArg(options, "original-intent");
      const credentialId = options["credential-id"] as string | undefined;
      const startUrl = options["start-url"] as string | undefined;
      const created = await sdk.sandbox.createSession({
        serviceId,
        originalIntent,
        credentialId,
        startUrl,
      });
      printResult(created, config.output);
      return;
    }
    case "list-sessions": {
      const sessions = await sdk.sandbox.listSessions();
      printResult(sessions, config.output);
      return;
    }
    case "get-session": {
      const id = options._[0];
      if (!id) throw new Error("Usage: toani sandbox get-session <sessionId>");
      const session = await sdk.sandbox.getSession(id);
      printResult(session, config.output);
      return;
    }
    case "terminate": {
      const id = options._[0];
      if (!id) throw new Error("Usage: toani sandbox terminate <sessionId>");
      const closed = await sdk.sandbox.closeSession(id);
      printResult(closed, config.output);
      return;
    }
    case "pause": {
      const id = options._[0];
      if (!id) throw new Error("Usage: toani sandbox pause <sessionId>");
      const paused = await sdk.sandbox.pauseSession(id);
      printResult(paused, config.output);
      return;
    }
    case "resume": {
      const id = options._[0];
      if (!id) throw new Error("Usage: toani sandbox resume <sessionId>");
      const resumed = await sdk.sandbox.resumeSession(id);
      printResult(resumed, config.output);
      return;
    }
    case "bootstrap-page": {
      const sessionId = resolveSessionId(options);
      if (!sessionId) {
        throw new Error(
          "Usage: toani sandbox bootstrap-page <sessionId> --mode rocket_loader [--script-selectors '<json-array>'] [--include-plain-scripts true|false] [--replay-lifecycle-events true|false] [--wait-selector <selector>] [--wait-timeout-ms <ms>]",
        );
      }
      const mode = requireArg(
        options,
        "mode",
        "Usage: toani sandbox bootstrap-page <sessionId> --mode rocket_loader [--script-selectors '<json-array>'] [--include-plain-scripts true|false] [--replay-lifecycle-events true|false] [--wait-selector <selector>] [--wait-timeout-ms <ms>]",
      );
      if (mode !== "rocket_loader") {
        throw new Error(
          "bootstrap-page currently only supports --mode rocket_loader",
        );
      }
      if (
        options.params !== undefined ||
        options.script !== undefined ||
        options.bindings !== undefined
      ) {
        throw new Error(
          "bootstrap-page only accepts fixed bootstrap flags; raw scripts, bindings, and --params are not supported",
        );
      }
      const scriptSelectors = parseJsonStringArray(
        options["script-selectors"],
        "script-selectors",
      );
      const includePlainScripts = parseBooleanOption(
        options["include-plain-scripts"],
      );
      const replayLifecycleEvents = parseBooleanOption(
        options["replay-lifecycle-events"],
      );
      const waitTimeoutMs = parseNumberOption(
        options["wait-timeout-ms"],
        "wait-timeout-ms",
      );
      const result = await sdk.sandbox.bootstrapPage(sessionId, {
        mode,
        scriptSelectors,
        includePlainScripts,
        replayLifecycleEvents,
        waitSelector: options["wait-selector"] as string | undefined,
        waitTimeoutMs,
      });
      printResult(result, config.output);
      return;
    }
    case "execute": {
      const sessionId = options._[0];
      if (!sessionId)
        throw new Error(
          "Usage: toani sandbox execute <sessionId> --operation-type <type>",
        );
      const operationType = requireArg(
        options,
        "operation-type",
      ) as OperationType;
      const params = options.params ? parseJsonOption(options, "params") : {};
      const result = await sdk.sandbox.executeOperation(sessionId, {
        operationType,
        description: options.description as string | undefined,
        parameters: params,
        selector: params.selector as string | undefined,
        value: params.value as string | { $credential: string } | undefined,
        url: params.url as string | undefined,
        method: params.method as string | undefined,
        headers: params.headers as
          | Record<string, string | { $credential: string }>
          | undefined,
        body: params.body,
        script: params.script as string | undefined,
        bindings: params.bindings as Record<string, string> | undefined,
        attribute: params.attribute as string | undefined,
        timeout:
          typeof params.timeout_ms === "number"
            ? params.timeout_ms
            : typeof params.timeout === "number"
              ? params.timeout
              : undefined,
        waitCondition: params.wait_condition as
          | { visible?: boolean; attached?: boolean }
          | undefined,
      });
      printResult(result, config.output);
      return;
    }
    case "export-dom": {
      const sessionId = options._[0];
      if (!sessionId) {
        throw new Error("Usage: toani sandbox export-dom <sessionId> [options]");
      }
      const extraSensitiveSelectors = parseJsonStringArray(
        options["extra-sensitive-selectors"],
        "extra-sensitive-selectors",
      );
      const result = await sdk.sandbox.exportDom(
        sessionId,
        {
          format: options.format as "html" | "text" | "json" | undefined,
          rootSelector: options["root-selector"] as string | undefined,
          includeText: parseBooleanOption(options["include-text"]),
          includeMetadata: parseBooleanOption(options["include-metadata"]),
          extraSensitiveSelectors,
          maxBytes: parseNumberOption(options["max-bytes"], "max-bytes"),
        },
      );
      printResult(result, config.output);
      return;
    }
    case "export-data": {
      const sessionId = options._[0];
      if (!sessionId) {
        throw new Error(
          "Usage: toani sandbox export-data <sessionId> --selectors '<json-array>' [--format json|csv|pdf]",
        );
      }
      const selectors = parseJsonStringArray(options.selectors, "selectors");
      if (!selectors) {
        throw new Error(
          "Usage: toani sandbox export-data <sessionId> --selectors '<json-array>' [--format json|csv|pdf]",
        );
      }
      const format = (options.format ?? "json") as "json" | "csv" | "pdf";
      const result = await sdk.sandbox.exportData(sessionId, {
        format,
        selectors,
      });
      printResult(result, config.output);
      return;
    }
    case "get-operation": {
      const operationId = options._[0];
      if (!operationId)
        throw new Error("Usage: toani sandbox get-operation <operationId>");
      const result = await sdk.sandbox.getOperation(operationId);
      printResult(result, config.output);
      return;
    }
    case "stats": {
      const stats = await sdk.sandbox.getStats();
      printResult(stats, config.output);
      return;
    }
    default:
      throw new Error(
        "Usage: toani sandbox <create-session|list-sessions|get-session|terminate|pause|resume|bootstrap-page|execute|export-dom|export-data|get-operation|stats> [options]",
      );
  }
}
