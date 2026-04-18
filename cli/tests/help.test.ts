import { describe, expect, it } from "vitest";
import { HELP_TEXT } from "../src/index.js";

describe("CLI help", () => {
  it("advertises the supported command groups only", () => {
    expect(HELP_TEXT).toContain("config");
    expect(HELP_TEXT).toContain("credentials");
    expect(HELP_TEXT).toContain("sandbox");
    expect(HELP_TEXT).toContain("list/get");
    expect(HELP_TEXT).toContain("bootstrap-page");
    expect(HELP_TEXT).not.toContain("auth");
    expect(HELP_TEXT).not.toContain("tokens");
    expect(HELP_TEXT).not.toContain("service-accounts");
  });
});
