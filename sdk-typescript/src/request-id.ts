import { randomBytes } from "node:crypto";

function encodeTimestampMs(bytes: Uint8Array, timestampMs: number): void {
  const timestamp = BigInt(timestampMs);
  for (let index = 5; index >= 0; index -= 1) {
    const shift = BigInt((5 - index) * 8);
    bytes[index] = Number((timestamp >> shift) & 0xffn);
  }
}

function formatUuid(bytes: Uint8Array): string {
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join("-");
}

export function generateUuidV7(timestampMs = Date.now()): string {
  const bytes = randomBytes(16);
  encodeTimestampMs(bytes, timestampMs);

  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  return formatUuid(bytes);
}

export function generateCanonicalRequestId(timestampMs = Date.now()): string {
  return `req_${timestampMs}_${generateUuidV7(timestampMs)}`;
}
