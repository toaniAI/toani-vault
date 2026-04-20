import { mkdtempSync, rmSync, symlinkSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { isDirectExecution, parseGlobalArgs } from "../src/index.js";

const tempPaths: string[] = [];

describe("parseGlobalArgs", () => {
  it("only parses global flags before the command group", () => {
    expect(
      parseGlobalArgs([
        "config",
        "init",
        "--url",
        "https://dev-credbridge.bitkinetic.com/",
        "--token",
        "v4.local.test",
      ]),
    ).toEqual({
      rest: [
        "config",
        "init",
        "--url",
        "https://dev-credbridge.bitkinetic.com/",
        "--token",
        "v4.local.test",
      ],
      output: undefined,
      baseUrl: undefined,
      token: undefined,
    });
  });

  it("preserves support for leading global flags", () => {
    expect(
      parseGlobalArgs([
        "--output",
        "json",
        "--token",
        "v4.local.test",
        "config",
        "init",
        "--url",
        "https://dev-credbridge.bitkinetic.com/",
      ]),
    ).toEqual({
      rest: [
        "config",
        "init",
        "--url",
        "https://dev-credbridge.bitkinetic.com/",
      ],
      output: "json",
      baseUrl: undefined,
      token: "v4.local.test",
    });
  });

  it("leaves command-local flags attached to new top-level commands", () => {
    expect(parseGlobalArgs(["login", "--skip-validate"])).toEqual({
      rest: ["login", "--skip-validate"],
      output: undefined,
      baseUrl: undefined,
      token: undefined,
    });
  });
});

describe("isDirectExecution", () => {
  afterEach(() => {
    for (const tempPath of tempPaths.splice(0)) {
      try {
        rmSync(path.dirname(tempPath), { recursive: true, force: true });
      } catch {
        // Ignore cleanup failures for already-removed temp paths.
      }
    }
  });

  it("treats a symlinked CLI entry as direct execution", () => {
    const realEntry = path.resolve("src/index.ts");
    const tempDir = mkdtempSync(path.join(os.tmpdir(), "toani-cli-"));
    const symlinkPath = path.join(tempDir, "toani");
    symlinkSync(realEntry, symlinkPath);
    tempPaths.push(symlinkPath);

    expect(
      isDirectExecution(symlinkPath, new URL("../src/index.ts", import.meta.url).href),
    ).toBe(true);
  });
});
