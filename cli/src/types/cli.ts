export type OutputFormat = "json" | "table";
export type CredentialSource =
  | "explicit"
  | "env"
  | "keychain"
  | "config_legacy"
  | "legacy"
  | "none";

export interface CliProfile {
  baseUrl?: string;
  token?: string;
  sessionToken?: string;
  currentTenantId?: string;
  output?: OutputFormat;
  timeout?: number;
}

export interface CliConfig {
  baseUrl: string;
  token?: string;
  legacyToken?: string;
  sessionToken?: string;
  currentTenantId?: string;
  output: OutputFormat;
  timeout: number;
  currentProfile?: string;
  profiles?: Record<string, CliProfile>;
  credentialSource?: CredentialSource;
}

export interface ParsedOptions {
  _: string[];
  [key: string]: unknown;
}
