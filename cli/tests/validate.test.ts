import { afterEach, describe, expect, it, vi } from "vitest";

const fetchMock = vi.hoisted(() => vi.fn());

vi.mock("undici", () => ({
  fetch: fetchMock,
}));

import { isPasetoToken, validateToken } from "../src/lib/validate.js";

describe("isPasetoToken", () => {
  it("accepts long v4 tokens and rejects short/unknown values", () => {
    expect(isPasetoToken(`v4.local.${"a".repeat(120)}`)).toBe(true);
    expect(isPasetoToken(`v4.public.${"b".repeat(120)}`)).toBe(true);
    expect(isPasetoToken("v4.local.short")).toBe(false);
    expect(isPasetoToken("bearer test")).toBe(false);
  });
});

describe("validateToken", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("classifies 401 and 403 responses", async () => {
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
