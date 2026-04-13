import type { CliConfig } from "../types/cli.js";
import { printResult } from "../output/print.js";
import { createSdk, parseJsonOption, parseOptions } from "./common.js";

export async function runAudit(
  config: CliConfig,
  argv: string[],
): Promise<void> {
  const [subcommand, ...rest] = argv;
  const options = parseOptions(rest);
  const sdk = createSdk(config);

  switch (subcommand) {
    case "logs": {
      const result = await sdk.audit.listLogs({
        startTime: options.from ? Number(options.from) : undefined,
        endTime: options.to ? Number(options.to) : undefined,
        action: options.action as string | undefined,
        service: options.service as string | undefined,
        outcome: options.outcome as string | undefined,
        pageSize: options.limit ? Number(options.limit) : undefined,
      });
      printResult(result, config.output);
      return;
    }
    case "export": {
      const result = await sdk.audit.exportLogs({
        format: (options.format as "json" | "csv" | undefined) ?? "json",
        startTime: options.from ? Number(options.from) : undefined,
        endTime: options.to ? Number(options.to) : undefined,
      });
      printResult(result, config.output);
      return;
    }
    case "verify": {
      const payload = options.payload
        ? parseJsonOption(options, "payload")
        : {};
      const result = await sdk.audit.verifyLog({
        id: payload.id as string | undefined,
        logIndex: payload.logIndex ? Number(payload.logIndex) : undefined,
      });
      printResult(result, config.output);
      return;
    }
    default:
      throw new Error("Usage: toani audit <logs|export|verify> [options]");
  }
}
