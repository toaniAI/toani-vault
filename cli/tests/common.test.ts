import { describe, expect, it } from "vitest";
import { createSdk, parseOptions } from "../src/commands/common.js";
import type { CliConfig } from "../src/types/cli.js";

describe("parseOptions", () => {
  it("parses key value and positional args", () => {
    const parsed = parseOptions(["create", "--foo", "bar", "--flag"]);
    expect(parsed._).toEqual(["create"]);
    expect(parsed.foo).toBe("bar");
    expect(parsed.flag).toBe(true);
  });
});

describe("createSdk", () => {
  it("fails with dashboard guidance when token is missing", () => {
    const config: CliConfig = {
      baseUrl: "https://api.example.com",
      output: "table",
      timeout: 30000,
      currentProfile: "default",
      profiles: { default: {} },
      credentialSource: "none",
    };

    expect(() => createSdk(config)).toThrowError(
      /First sign up or sign in through the Dashboard: https:\/\/dashboard\.toani\.ai\/login/,
    );
    expect(() => createSdk(config)).toThrowError(
      /create or copy an access token from the Dashboard Tokens page: https:\/\/dashboard\.toani\.ai\/tokens/,
    );
    expect(() => createSdk(config)).toThrowError(
      /Recommended command: toani login/,
    );
    expect(() => createSdk(config)).toThrowError(
      /Compatibility path: toani config init --url https:\/\/api\.example\.com --token <BEARER_TOKEN>/,
    );
  });
});
