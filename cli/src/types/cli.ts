export type OutputFormat = "json" | "table";

export interface CliProfile {
  baseUrl?: string;
  token?: string;
  automationToken?: string;
  sessionToken?: string;
  currentTenantId?: string;
  output?: OutputFormat;
  timeout?: number;
}

export interface CliConfig {
  baseUrl: string;
  token?: string;
  automationToken?: string;
  sessionToken?: string;
  currentTenantId?: string;
  output: OutputFormat;
  timeout: number;
  currentProfile?: string;
  profiles?: Record<string, CliProfile>;
  credentialSource?: "explicit" | "token" | "env" | "legacy" | "none";
}

export interface ParsedOptions {
  _: string[];
  [key: string]: unknown;
}
