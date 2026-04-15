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
        selector: params.selector as string | undefined,
        value: params.value as string | { $credential: string } | undefined,
        url: params.url as string | undefined,
        method: params.method as string | undefined,
        headers: params.headers as
          | Record<string, string | { $credential: string }>
          | undefined,
        body: params.body,
        script: params.script as string | undefined,
        bindings: params.bindings as
          | Record<string, string | { $credential: string }>
          | undefined,
        attribute: params.attribute as string | undefined,
        timeout: params.timeout_ms as number | undefined,
      });
      printResult(result, config.output);
      return;
    }
    case "export-dom": {
      const sessionId = options._[0];
      if (!sessionId) {
        throw new Error("Usage: toani sandbox export-dom <sessionId> [options]");
      }
      const selectors = options["extra-sensitive-selectors"];
      const extraSensitiveSelectors =
        typeof selectors === "string"
          ? (() => {
              let parsed: unknown;
              try {
                parsed = JSON.parse(selectors);
              } catch (error) {
                throw new Error(
                  `Invalid JSON for --extra-sensitive-selectors: ${(error as Error).message}`,
                );
              }
              if (!Array.isArray(parsed) || parsed.some((item) => typeof item !== "string")) {
                throw new Error(
                  "--extra-sensitive-selectors must be a JSON array of strings",
                );
              }
              return parsed;
            })()
          : undefined;
      const result = await sdk.sandbox.exportDom(
        sessionId,
        {
          format: options.format as "html" | "text" | "json" | undefined,
          rootSelector: options["root-selector"] as string | undefined,
          includeText: parseBooleanOption(options["include-text"]),
          includeMetadata: parseBooleanOption(options["include-metadata"]),
          extraSensitiveSelectors,
          maxBytes: options["max-bytes"] as number | undefined,
        },
      );
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
        "Usage: toani sandbox <create-session|list-sessions|get-session|terminate|execute|export-dom|get-operation|stats> [options]",
      );
  }
}
