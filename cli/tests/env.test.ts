import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { readTokenFromEnv } from "../src/lib/env.js";

const tempDirs: string[] = [];

describe("readTokenFromEnv", () => {
  afterEach(() => {
    delete process.env.TOANI_VAULT_TOKEN;
    vi.restoreAllMocks();
    for (const directory of tempDirs.splice(0)) {
      fs.rmSync(directory, { recursive: true, force: true });
    }
  });

  it("prefers process.env", () => {
    process.env.TOANI_VAULT_TOKEN = "v4.local.process-token";

    expect(readTokenFromEnv("/tmp/example")).toEqual({
      token: "v4.local.process-token",
      source: "process.env",
    });
  });

  it("reads .env from the current directory", () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), "toani-env-"));
    tempDirs.push(directory);
    fs.writeFileSync(
      path.join(directory, ".env"),
      'TOANI_VAULT_TOKEN="v4.local.cwd-token"\n',
    );

    expect(readTokenFromEnv(directory)).toEqual({
      token: "v4.local.cwd-token",
      source: path.join(directory, ".env"),
    });
  });

  it("falls back to the parent directory .env", () => {
    const parent = fs.mkdtempSync(path.join(os.tmpdir(), "toani-env-parent-"));
    const child = path.join(parent, "child");
    fs.mkdirSync(child);
    tempDirs.push(parent);
    fs.writeFileSync(
      path.join(parent, ".env"),
      'TOANI_VAULT_TOKEN="v4.local.parent-token"\n',
    );

    expect(readTokenFromEnv(child)).toEqual({
      token: "v4.local.parent-token",
      source: path.join(parent, ".env"),
    });
  });
});
