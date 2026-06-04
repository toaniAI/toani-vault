import { describe, expect, it } from "vitest";
import { HELP_TEXT } from "../src/index.js";

describe("CLI help", () => {
  it("advertises the supported command groups only", () => {
    expect(HELP_TEXT).toContain("login");
    expect(HELP_TEXT).toContain("doctor");
    expect(HELP_TEXT).toContain("config");
    expect(HELP_TEXT).toContain("credentials");
    expect(HELP_TEXT).toContain("approvals");
    expect(HELP_TEXT).toContain("sandbox");
    expect(HELP_TEXT).toContain("interactive onboarding");
    expect(HELP_TEXT).toContain("list/get");
    expect(HELP_TEXT).toContain("create/status/wait/generate-request-id");
    expect(HELP_TEXT).toContain("request/get-request");
    expect(HELP_TEXT).not.toContain("create-session");
    expect(HELP_TEXT).not.toContain("stats");
    expect(HELP_TEXT).not.toContain("auth");
    expect(HELP_TEXT).not.toContain("tokens");
    expect(HELP_TEXT).not.toContain("service-accounts");
  });
});
