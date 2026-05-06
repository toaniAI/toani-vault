import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export type SkillInstallChoice = "claude" | "codex" | "both" | "skip";
export type SkillInstallTarget = Exclude<SkillInstallChoice, "both" | "skip">;
export type SkillInstallStatus = "installed" | "updated";

export interface SkillInstallOutcome {
  target: SkillInstallTarget;
  destinationDir: string;
  destinationFile: string;
  status: SkillInstallStatus;
}

export interface SkillInstallFailure {
  target: SkillInstallTarget;
  destinationDir: string;
  message: string;
}

export interface SkillInstallReport {
  sourcePath: string;
  outcomes: SkillInstallOutcome[];
  failures: SkillInstallFailure[];
}

export interface InstallBundledSkillOptions {
  homeDir?: string;
  sourcePath?: string;
}

const SKILL_DIR_NAME = "toani-vault-cli";
const SKILL_FILE_NAME = "SKILL.md";

const TARGETS: Record<
  SkillInstallTarget,
  { label: string; pathParts: string[] }
> = {
  claude: {
    label: "Claude Code",
    pathParts: [".claude", "skills", SKILL_DIR_NAME],
  },
  codex: {
    label: "Codex",
    pathParts: [".codex", "skills", SKILL_DIR_NAME],
  },
};

export function resolveBundledSkillPath(moduleUrl = import.meta.url): string {
  let currentDir = path.dirname(fileURLToPath(moduleUrl));

  for (let depth = 0; depth < 5; depth += 1) {
    const candidate = path.join(currentDir, SKILL_FILE_NAME);
    if (existsSync(candidate)) {
      return candidate;
    }

    const parentDir = path.dirname(currentDir);
    if (parentDir === currentDir) {
      break;
    }
    currentDir = parentDir;
  }

  return path.resolve(path.dirname(fileURLToPath(moduleUrl)), "..", "..", SKILL_FILE_NAME);
}

export function getSkillInstallTargetLabel(target: SkillInstallTarget): string {
  return TARGETS[target].label;
}

export function getDefaultSkillInstallChoice(
  homeDir = os.homedir(),
): Exclude<SkillInstallChoice, "skip"> {
  const hasClaudeRoot = existsSync(path.join(homeDir, ".claude"));
  const hasCodexRoot = existsSync(path.join(homeDir, ".codex"));

  if (hasClaudeRoot && hasCodexRoot) {
    return "both";
  }
  if (hasClaudeRoot) {
    return "claude";
  }
  if (hasCodexRoot) {
    return "codex";
  }
  return "codex";
}

function resolveTargets(choice: Exclude<SkillInstallChoice, "skip">): SkillInstallTarget[] {
  if (choice === "both") {
    return ["claude", "codex"];
  }
  return [choice];
}

export function installBundledSkill(
  choice: Exclude<SkillInstallChoice, "skip">,
  options: InstallBundledSkillOptions = {},
): SkillInstallReport {
  const homeDir = options.homeDir ?? os.homedir();
  const sourcePath = options.sourcePath ?? resolveBundledSkillPath();

  if (!existsSync(sourcePath)) {
    throw new Error(`Bundled SKILL.md not found at ${sourcePath}`);
  }

  const outcomes: SkillInstallOutcome[] = [];
  const failures: SkillInstallFailure[] = [];

  for (const target of resolveTargets(choice)) {
    const destinationDir = path.join(homeDir, ...TARGETS[target].pathParts);
    const destinationFile = path.join(destinationDir, SKILL_FILE_NAME);

    try {
      const existed = existsSync(destinationFile);
      mkdirSync(destinationDir, { recursive: true });
      copyFileSync(sourcePath, destinationFile);
      outcomes.push({
        target,
        destinationDir,
        destinationFile,
        status: existed ? "updated" : "installed",
      });
    } catch (error) {
      failures.push({
        target,
        destinationDir,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }

  return {
    sourcePath,
    outcomes,
    failures,
  };
}
