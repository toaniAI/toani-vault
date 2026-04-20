import fs from "node:fs";
import path from "node:path";

export interface EnvTokenMatch {
  token: string;
  source: string;
}

export function readTokenFromEnv(cwd = process.cwd()): EnvTokenMatch | null {
  const processValue = process.env.TOANI_VAULT_TOKEN?.trim();
  if (processValue) {
    return { token: processValue, source: "process.env" };
  }

  const candidates = [cwd, path.join(cwd, "..")];
  for (const directory of candidates) {
    const envPath = path.join(directory, ".env");
    try {
      const content = fs.readFileSync(envPath, "utf8");
      const match = content.match(/^TOANI_VAULT_TOKEN\s*=\s*"?([^"\n]+)"?/m);
      if (match?.[1]?.trim()) {
        return { token: match[1].trim(), source: envPath };
      }
    } catch {
      // Ignore missing or unreadable .env files.
    }
  }

  return null;
}
