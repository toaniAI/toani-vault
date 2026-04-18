import { describe, expect, it } from "vitest";
import packageJson from "../package.json" assert { type: "json" };
import { getCliVersion } from "../src/index.js";

describe("CLI version", () => {
  it("uses package.json as the single version source", () => {
    expect(getCliVersion()).toBe(packageJson.version);
  });
});
