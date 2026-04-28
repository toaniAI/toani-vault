import { describe, expect, it } from "vitest";
import packageJson from "../package.json" assert { type: "json" };
import {
  formatCliVersion,
  getCliVersion,
  resolveVersionOutputFormat,
} from "../src/index.js";

describe("CLI version", () => {
  it("uses package.json as the single version source", () => {
    expect(getCliVersion()).toBe(packageJson.version);
  });

  it("renders the default version output as plain text", () => {
    expect(formatCliVersion(packageJson.version)).toBe(`v${packageJson.version}`);
  });

  it("keeps json output when requested before the version flag", () => {
    expect(resolveVersionOutputFormat("json", [])).toBe("json");
  });

  it("accepts json output when requested after the version flag", () => {
    expect(resolveVersionOutputFormat("table", ["--output", "json"])).toBe("json");
  });
});
