import { fetch } from "undici";

const FALLBACK_DASHBOARD_BASE_URL = "https://dashboard.toani.ai";

function resolveDashboardBaseUrl(): string {
  const configured = process.env.TOANI_VAULT_DASHBOARD_BASE_URL?.trim();
  if (!configured) {
    return FALLBACK_DASHBOARD_BASE_URL;
  }

  return configured.endsWith("/") ? configured.slice(0, -1) : configured;
}

export const DASHBOARD_BASE_URL = resolveDashboardBaseUrl();
export const DASHBOARD_LOGIN_URL = `${DASHBOARD_BASE_URL}/login`;
export const DASHBOARD_TOKENS_URL = `${DASHBOARD_BASE_URL}/tokens`;
export const DASHBOARD_CREDENTIALS_URL = `${DASHBOARD_BASE_URL}/credentials`;
export const DEFAULT_API_BASE_URL = DASHBOARD_BASE_URL;

export type ValidationReason =
  | "invalid_or_expired"
  | "insufficient_scope"
  | "dns"
  | "refused"
  | "timeout"
  | "network"
  | "unexpected_status";

export type ReachabilityReason =
  | "dns"
  | "refused"
  | "timeout"
  | "network";

export interface ValidationResult {
  ok: boolean;
  status: number;
  reason?: ValidationReason;
  body?: Record<string, unknown> | null;
  error?: unknown;
}

export interface ReachabilityResult {
  ok: boolean;
  status: number;
  url: string;
  reason?: ReachabilityReason;
  error?: unknown;
}

function classifyFetchError(error: unknown): {
  reason: ReachabilityReason;
  error: unknown;
} {
  const candidate = error as {
    name?: string;
    code?: string;
    cause?: { code?: string };
  };
  const code = candidate.cause?.code ?? candidate.code;

  if (candidate.name === "TimeoutError" || code === "ABORT_ERR") {
    return { reason: "timeout", error };
  }
  if (code === "ENOTFOUND") {
    return { reason: "dns", error };
  }
  if (code === "ECONNREFUSED") {
    return { reason: "refused", error };
  }

  return { reason: "network", error };
}

export async function checkBaseUrlReachability(
  baseUrl: string,
): Promise<ReachabilityResult> {
  const candidates = [
    new URL("/api/v1/health", baseUrl).toString(),
    new URL("/health", baseUrl).toString(),
  ];
  let lastError: ReachabilityResult | null = null;

  for (const url of candidates) {
    try {
      const response = await fetch(url, {
        signal: AbortSignal.timeout(8_000),
      });
      return { ok: true, status: response.status, url };
    } catch (error) {
      const classified = classifyFetchError(error);
      lastError = {
        ok: false,
        status: 0,
        url,
        reason: classified.reason,
        error: classified.error,
      };
    }
  }

  return (
    lastError ?? {
      ok: false,
      status: 0,
      url: candidates[0],
      reason: "network",
    }
  );
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
    const classified = classifyFetchError(error);
    return {
      ok: false,
      status: 0,
      reason: classified.reason,
      error: classified.error,
    };
  }
}
