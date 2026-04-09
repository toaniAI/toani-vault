export type OutputFormat = 'json' | 'table';

export interface CliConfig {
  baseUrl: string;
  token?: string;
  sessionToken?: string;
  output: OutputFormat;
  timeout: number;
}

export interface ParsedOptions {
  _: string[];
  [key: string]: unknown;
}

