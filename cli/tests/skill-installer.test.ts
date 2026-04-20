import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  getDefaultSkillInstallChoice,
  getSkillInstallTargetLabel,
  installBundledSkill,
} from "../src/lib/skill-installer.js";

const tempDirs: string[] = [];

function createTempDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "toani-cli-skill-"));
  tempDirs.push(dir);
  return dir;
}

describe("skill installer", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    for (const dir of tempDirs.splice(0)) {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("installs SKILL.md into both Claude and Codex user skill directories", () => {
    const homeDir = createTempDir();
    const sourcePath = path.join(homeDir, "SKILL.md");
    fs.writeFileSync(sourcePath, "# bundled skill\n", "utf8");

    const report = installBundledSkill("both", { homeDir, sourcePath });

    expect(report.failures).toEqual([]);
    expect(report.outcomes).toHaveLength(2);
    expect(
      fs.readFileSync(
        path.join(homeDir, ".claude", "skills", "toani-vault-cli", "SKILL.md"),
        "utf8",
      ),
    ).toBe("# bundled skill\n");
    expect(
      fs.readFileSync(
        path.join(homeDir, ".codex", "skills", "toani-vault-cli", "SKILL.md"),
        "utf8",
      ),
    ).toBe("# bundled skill\n");
  });

  it("marks an existing installation as updated", () => {
    const homeDir = createTempDir();
    const sourcePath = path.join(homeDir, "SKILL.md");
    const destination = path.join(
      homeDir,
      ".codex",
      "skills",
      "toani-vault-cli",
      "SKILL.md",
    );
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    fs.writeFileSync(sourcePath, "# next\n", "utf8");
    fs.writeFileSync(destination, "# old\n", "utf8");

    const report = installBundledSkill("codex", { homeDir, sourcePath });

    expect(report.outcomes).toEqual([
      expect.objectContaining({
        target: "codex",
        status: "updated",
      }),
    ]);
    expect(fs.readFileSync(destination, "utf8")).toBe("# next\n");
  });

  it("throws when the bundled SKILL.md is missing", () => {
    const homeDir = createTempDir();

    expect(() =>
      installBundledSkill("claude", {
        homeDir,
        sourcePath: path.join(homeDir, "missing-SKILL.md"),
      }),
    ).toThrowError(/Bundled SKILL\.md not found/);
  });

  it("prefers both when Claude and Codex homes are present", () => {
    const homeDir = createTempDir();
    fs.mkdirSync(path.join(homeDir, ".claude"), { recursive: true });
    fs.mkdirSync(path.join(homeDir, ".codex"), { recursive: true });

    expect(getDefaultSkillInstallChoice(homeDir)).toBe("both");
    expect(getSkillInstallTargetLabel("claude")).toBe("Claude Code");
    expect(getSkillInstallTargetLabel("codex")).toBe("Codex");
  });
});
