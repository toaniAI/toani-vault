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
  | "insufficient_permissions"
  | "dns"
  | "refused"
  | "timeout"
  | "network"
  | "unexpected_status";

export type TokenAccessMode =
  | "usage"
  | "management"
  | "mixed"
  | "session_profile"
  | "unknown";

export type TokenKind = "web_session" | "api_access" | "unknown";

export type ValidationProbeName =
  | "authMe"
  | "credentials"
  | "tokens";

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
  mode?: TokenAccessMode;
  tokenKind?: TokenKind;
  probes?: Partial<Record<ValidationProbeName, ValidationProbeResult>>;
}

export interface ValidationProbeResult {
  body?: Record<string, unknown> | null;
  error?: unknown;
  ok: boolean;
  path: string;
  reason?: ValidationReason;
  status: number;
}

export interface ReachabilityResult {
  ok: boolean;
  status: number;
  url: string;
  reason?: ReachabilityReason;
  error?: unknown;
}

function isReachableHttpStatus(status: number): boolean {
  return status !== 404;
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
    new URL("/api/v1/auth/me", baseUrl).toString(),
  ];
  let lastError: ReachabilityResult | null = null;
  let lastHttpResponse: ReachabilityResult | null = null;

  for (const url of candidates) {
    try {
      const response = await fetch(url, {
        signal: AbortSignal.timeout(8_000),
      });
      const result = { ok: true, status: response.status, url };
      if (isReachableHttpStatus(response.status)) {
        return result;
      }
      lastHttpResponse = result;
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
    lastHttpResponse ??
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

async function readJsonBody(
  response: { json(): Promise<unknown> },
): Promise<Record<string, unknown> | null> {
  try {
    return (await response.json()) as Record<string, unknown>;
  } catch {
    return null;
  }
}

async function probeTokenAccess(
  baseUrl: string,
  token: string,
  path: string,
): Promise<ValidationProbeResult> {
  const url = new URL(path, baseUrl).toString();

  try {
    const response = await fetch(url, {
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(8_000),
    });

    if (response.status === 200) {
      return { ok: true, path, status: 200 };
    }

    if (response.status === 401) {
      return {
        ok: false,
        path,
        reason: "invalid_or_expired",
        status: 401,
      };
    }

    if (response.status === 403) {
      const body = await readJsonBody(response);
      const reason =
        body?.error === "insufficient_scope"
          ? "insufficient_scope"
          : "insufficient_permissions";

      return {
        body,
        ok: false,
        path,
        reason,
        status: 403,
      };
    }

    return {
      ok: false,
      path,
      reason: "unexpected_status",
      status: response.status,
    };
  } catch (error) {
    const classified = classifyFetchError(error);
    return {
      error: classified.error,
      ok: false,
      path,
      reason: classified.reason,
      status: 0,
    };
  }
}

function resolveTokenMode(
  authMe: ValidationProbeResult,
  credentials: ValidationProbeResult,
  tokens: ValidationProbeResult,
): Pick<ValidationResult, "mode" | "ok" | "reason" | "status" | "tokenKind"> {
  const usageReady = credentials.ok;
  const managementReady = tokens.ok;
  const sessionReady = authMe.ok;

  if (usageReady && managementReady) {
    return {
      mode: "mixed",
      ok: true,
      status: 200,
      tokenKind: sessionReady ? "web_session" : "api_access",
    };
  }

  if (usageReady) {
    return {
      mode: "usage",
      ok: true,
      status: 200,
      tokenKind: sessionReady ? "web_session" : "api_access",
    };
  }

  if (managementReady) {
    return {
      mode: "management",
      ok: true,
      status: 200,
      tokenKind: sessionReady ? "web_session" : "api_access",
    };
  }

  if (sessionReady) {
    return {
      mode: "session_profile",
      ok: true,
      status: 200,
      tokenKind: "web_session",
    };
  }

  return {
    mode: "unknown",
    ok: false,
    reason: "unexpected_status",
    status: authMe.status || credentials.status || tokens.status,
    tokenKind: "unknown",
  };
}

export async function validateToken(
  baseUrl: string,
  token: string,
): Promise<ValidationResult> {
  const [authMe, credentials, tokens] = await Promise.all([
    probeTokenAccess(baseUrl, token, "/api/v1/auth/me"),
    probeTokenAccess(baseUrl, token, "/api/v1/credentials?page=1&page_size=1"),
    probeTokenAccess(baseUrl, token, "/api/v1/tokens?page=1&page_size=1"),
  ]);
  const probes = { authMe, credentials, tokens };

  const networkFailure = [authMe, credentials, tokens].find(
    (probe) =>
      probe.reason === "dns" ||
      probe.reason === "refused" ||
      probe.reason === "timeout" ||
      probe.reason === "network",
  );
  if (networkFailure) {
    return {
      body: networkFailure.body,
      error: networkFailure.error,
      ok: false,
      probes,
      reason: networkFailure.reason,
      status: networkFailure.status,
    };
  }

  const invalidProbe = [authMe, credentials, tokens].find(
    (probe) => probe.reason === "invalid_or_expired",
  );
  if (invalidProbe) {
    return {
      body: invalidProbe.body,
      ok: false,
      probes,
      reason: "invalid_or_expired",
      status: 401,
    };
  }

  const resolved = resolveTokenMode(authMe, credentials, tokens);
  if (resolved.ok) {
    return {
      ok: true,
      probes,
      status: resolved.status,
      mode: resolved.mode,
      tokenKind: resolved.tokenKind,
    };
  }

  const scopeFailure = [credentials, tokens, authMe].find(
    (probe) =>
      probe.reason === "insufficient_scope" ||
      probe.reason === "insufficient_permissions",
  );
  if (scopeFailure) {
    return {
      body: scopeFailure.body,
      ok: false,
      probes,
      reason: scopeFailure.reason,
      status: scopeFailure.status,
      mode: resolved.mode,
      tokenKind: resolved.tokenKind,
    };
  }

  return {
    ok: false,
    probes,
    reason: "unexpected_status",
    status: resolved.status,
    mode: resolved.mode,
    tokenKind: resolved.tokenKind,
  };
}
