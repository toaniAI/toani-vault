import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.hoisted(() => vi.fn());

vi.mock("undici", () => ({
  fetch: fetchMock,
}));

describe("validate constants", () => {
  afterEach(() => {
    vi.resetModules();
    delete process.env.TOANI_VAULT_DASHBOARD_BASE_URL;
  });

  it("points dashboard and default API URLs to production", async () => {
    const {
      DASHBOARD_BASE_URL,
      DASHBOARD_CREDENTIALS_URL,
      DASHBOARD_LOGIN_URL,
      DASHBOARD_TOKENS_URL,
      DEFAULT_API_BASE_URL,
    } = await import("../src/lib/validate.js");

    expect(DASHBOARD_BASE_URL).toBe("https://dashboard.toani.ai");
    expect(DASHBOARD_LOGIN_URL).toBe("https://dashboard.toani.ai/login");
    expect(DASHBOARD_TOKENS_URL).toBe("https://dashboard.toani.ai/tokens");
    expect(DASHBOARD_CREDENTIALS_URL).toBe(
      "https://dashboard.toani.ai/credentials",
    );
    expect(DEFAULT_API_BASE_URL).toBe("https://dashboard.toani.ai");
  });

  it("allows overriding dashboard and API defaults via env", async () => {
    vi.resetModules();
    process.env.TOANI_VAULT_DASHBOARD_BASE_URL = "https://vault.example.com/";

    const {
      DASHBOARD_BASE_URL,
      DASHBOARD_LOGIN_URL,
      DEFAULT_API_BASE_URL,
    } = await import("../src/lib/validate.js");

    expect(DASHBOARD_BASE_URL).toBe("https://vault.example.com");
    expect(DASHBOARD_LOGIN_URL).toBe("https://vault.example.com/login");
    expect(DEFAULT_API_BASE_URL).toBe("https://vault.example.com");
  });
});

describe("isPasetoToken", () => {
  it("accepts long v4 tokens and rejects short/unknown values", async () => {
    const { isPasetoToken } = await import("../src/lib/validate.js");

    expect(isPasetoToken(`v4.local.${"a".repeat(120)}`)).toBe(true);
    expect(isPasetoToken(`v4.public.${"b".repeat(120)}`)).toBe(true);
    expect(isPasetoToken("v4.local.short")).toBe(false);
    expect(isPasetoToken("bearer test")).toBe(false);
  });
});

describe("validateToken", () => {
  afterEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
    delete process.env.TOANI_VAULT_DASHBOARD_BASE_URL;
  });

  it("classifies 401 and 403 responses", async () => {
    const { validateToken } = await import("../src/lib/validate.js");

    fetchMock.mockResolvedValueOnce({ status: 401 });
    fetchMock.mockResolvedValueOnce({
      status: 403,
      json: vi.fn().mockResolvedValue({ required_scope: "credential:read" }),
    });

    await expect(
      validateToken("https://api.example.com", "v4.local.token"),
    ).resolves.toMatchObject({ ok: false, reason: "invalid_or_expired" });

    await expect(
      validateToken("https://api.example.com", "v4.local.token"),
    ).resolves.toMatchObject({
      ok: false,
      reason: "insufficient_scope",
      body: { required_scope: "credential:read" },
    });
  });

  it("classifies dns, refused, timeout, and generic network errors", async () => {
    const { validateToken } = await import("../src/lib/validate.js");

    fetchMock.mockRejectedValueOnce({ code: "ENOTFOUND" });
    fetchMock.mockRejectedValueOnce({ cause: { code: "ECONNREFUSED" } });
    fetchMock.mockRejectedValueOnce({ name: "TimeoutError" });
    fetchMock.mockRejectedValueOnce({ code: "EOTHER" });

    await expect(
      validateToken("https://api.example.com", "v4.local.token"),
    ).resolves.toMatchObject({ reason: "dns" });
    await expect(
      validateToken("https://api.example.com", "v4.local.token"),
    ).resolves.toMatchObject({ reason: "refused" });
    await expect(
      validateToken("https://api.example.com", "v4.local.token"),
    ).resolves.toMatchObject({ reason: "timeout" });
    await expect(
      validateToken("https://api.example.com", "v4.local.token"),
    ).resolves.toMatchObject({ reason: "network" });
  });
});
