import { invoke } from "@tauri-apps/api/core";

export type ImportSummary = {
  batch_id: string;
  file_name: string;
  row_count: number;
  url_column: string;
  match_mode: "FULL_URL_MODE" | "PATH_MODE" | "MIXED_MODE";
  url_kind: "full_url" | "path_only";
  export_profile:
    | "full_url_export"
    | "full_url_with_query_export"
    | "host_and_path_export"
    | "path_only_export"
    | "keyword_export"
    | "unknown_export";
  truncation_cap?: number;
  metric_columns: string[];
  warnings: string[];
};

export type UrlListLoad = {
  file_name: string;
  row_count: number;
  url_column: string;
  warnings: string[];
  urls: string[];
};

export type Row = {
  raw_row_id: number;
  source_url: string;
  normalized_url: string;
  match_type: string;
  match_score?: number;
  metrics: Record<string, string>;
  extras: Record<string, string>;
  source_file?: string;
  batch_id?: string;
};

export type LookupHit = {
  query: string;
  normalized_query: string;
  match_mode: "FULL_URL_MODE" | "PATH_MODE" | "MIXED_MODE" | "KEYWORD_MODE";
  status: string;
  notes: string;
  matched: boolean;
  ambiguous: boolean;
  match_count: number;
  match_type: string;
  match_confidence: number;
  export_profile: string;
  warnings: string[];
  discarded_variants: string[];
  rows: Row[];
};

export type LookupResponse = {
  hits: LookupHit[];
  missing_metrics: string[];
  searched_files: number;
};

export type QueryMode = "url" | "keyword";

export const api = {
  importFile: (path: string) => invoke<ImportSummary>("import_file", { path }),
  listImports: () => invoke<ImportSummary[]>("list_imports"),
  loadLookupFile: (path: string, queryMode?: QueryMode) =>
    invoke<UrlListLoad>("load_lookup_file", { path, queryMode }),
  allMetrics: () => invoke<string[]>("all_metrics"),
  lookupUrls: (
    urls: string[],
    metrics: string[],
    batchIds?: string[],
    matchModeOverride?: "FULL_URL_MODE" | "PATH_MODE",
    queryMode?: QueryMode,
  ) =>
    invoke<LookupResponse>("lookup_urls", {
      urls,
      metrics,
      batchIds,
      matchModeOverride,
      queryMode,
    }),
  removeImport: (batchId: string) =>
    invoke<void>("remove_import", { batchId }),
};
