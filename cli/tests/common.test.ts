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
      /Dashboard 注册或登录账号：https:\/\/dev-credbridge\.bitkinetic\.com\/login/,
    );
    expect(() => createSdk(config)).toThrowError(
      /Dashboard Tokens 页面创建或复制访问凭证：https:\/\/dev-credbridge\.bitkinetic\.com\/tokens/,
    );
    expect(() => createSdk(config)).toThrowError(
      /toani config init --url https:\/\/api\.example\.com --token <BEARER_TOKEN>/,
    );
  });
});
