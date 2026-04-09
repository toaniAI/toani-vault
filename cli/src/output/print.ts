import type { OutputFormat } from '../types/cli.js';

export function printResult(data: unknown, format: OutputFormat): void {
  if (format === 'json') {
    console.log(JSON.stringify(data, null, 2));
    return;
  }

  if (Array.isArray(data)) {
    console.table(data);
    return;
  }

  if (isPlainObject(data)) {
    const values = Object.values(data);
    const hasArrayPayload = values.some((v) => Array.isArray(v));
    if (hasArrayPayload) {
      for (const [k, v] of Object.entries(data)) {
        if (Array.isArray(v)) {
          console.log(`${k}:`);
          console.table(v);
        } else {
          console.log(`${k}: ${String(v)}`);
        }
      }
      return;
    }
    console.table([data]);
    return;
  }

  console.log(String(data));
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function warn(message: string): void {
  console.error(`Warning: ${message}`);
}

export function fail(message: string): never {
  throw new Error(message);
}

