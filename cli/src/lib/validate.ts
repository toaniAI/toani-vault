import { fetch } from "undici";

export const DASHBOARD_BASE_URL = "https://dev-credbridge.bitkinetic.com";
export const DASHBOARD_LOGIN_URL = `${DASHBOARD_BASE_URL}/login`;
export const DASHBOARD_TOKENS_URL = `${DASHBOARD_BASE_URL}/tokens`;
export const DASHBOARD_CREDENTIALS_URL = `${DASHBOARD_BASE_URL}/credentials`;
export const DEFAULT_API_BASE_URL = "https://dev-credbridge.bitkinetic.com";

export type ValidationReason =
  | "invalid_or_expired"
  | "insufficient_scope"
  | "dns"
  | "refused"
  | "timeout"
  | "network"
  | "unexpected_status";

export interface ValidationResult {
  ok: boolean;
  status: number;
  reason?: ValidationReason;
  body?: Record<string, unknown> | null;
  error?: unknown;
}

export function isPasetoToken(value: string | undefined | null): boolean {
  if (!value || typeof value !== "string") {
    return false;
  }

  return (
    (value.startsWith("v4.local.") || value.startsWith("v4.public.")) &&
    value.length > 100
  );
}

export async function validateToken(
  baseUrl: string,
  token: string,
): Promise<ValidationResult> {
  const url = new URL("/api/v1/sandbox/stats", baseUrl).toString();

  try {
    const response = await fetch(url, {
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(8_000),
    });

    if (response.status === 200) {
      return { ok: true, status: 200 };
    }

    if (response.status === 401) {
      return { ok: false, status: 401, reason: "invalid_or_expired" };
    }

    if (response.status === 403) {
      let body: Record<string, unknown> | null = null;
      try {
        body = (await response.json()) as Record<string, unknown>;
      } catch {
        body = null;
      }

      return {
        ok: false,
        status: 403,
        reason: "insufficient_scope",
        body,
      };
    }

    return {
      ok: false,
      status: response.status,
      reason: "unexpected_status",
    };
  } catch (error) {
    const candidate = error as {
      name?: string;
      code?: string;
      cause?: { code?: string };
    };
    const code = candidate.cause?.code ?? candidate.code;

    if (candidate.name === "TimeoutError" || code === "ABORT_ERR") {
      return { ok: false, status: 0, reason: "timeout", error };
    }
    if (code === "ENOTFOUND") {
      return { ok: false, status: 0, reason: "dns", error };
    }
    if (code === "ECONNREFUSED") {
      return { ok: false, status: 0, reason: "refused", error };
    }

    return { ok: false, status: 0, reason: "network", error };
  }
}

