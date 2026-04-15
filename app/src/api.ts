import { invoke } from "@tauri-apps/api/core";

export type ImportSummary = {
  batch_id: string;
  file_name: string;
  row_count: number;
  url_column: string;
  metric_columns: string[];
  warnings: string[];
};

export type Row = {
  raw_row_id: number;
  source_url: string;
  normalized_url: string;
  metrics: Record<string, string>;
  extras: Record<string, string>;
  source_file?: string;
  batch_id?: string;
};

export type LookupHit = {
  query: string;
  normalized_query: string;
  matched: boolean;
  ambiguous: boolean;
  match_count: number;
  rows: Row[];
};

export type LookupResponse = {
  hits: LookupHit[];
  missing_metrics: string[];
  searched_files: number;
};

export const api = {
  importFile: (path: string) => invoke<ImportSummary>("import_file", { path }),
  listImports: () => invoke<ImportSummary[]>("list_imports"),
  allMetrics: () => invoke<string[]>("all_metrics"),
  lookupUrls: (urls: string[], metrics: string[]) =>
    invoke<LookupResponse>("lookup_urls", { urls, metrics }),
  removeImport: (batchId: string) =>
    invoke<void>("remove_import", { batchId }),
  clearImports: () => invoke<void>("clear_imports"),
};
