use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub raw_row_id: usize,
    pub source_url: String,
    pub normalized_url: String,
    pub match_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_score: Option<f32>,
    pub metrics: BTreeMap<String, String>,
    pub extras: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub batch_id: String,
    pub file_name: String,
    pub row_count: usize,
    pub url_column: String,
    pub match_mode: String,
    pub url_kind: String,
    pub export_profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation_cap: Option<usize>,
    pub metric_columns: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlListLoad {
    pub file_name: String,
    pub row_count: usize,
    pub url_column: String,
    pub warnings: Vec<String>,
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupHit {
    pub query: String,
    pub normalized_query: String,
    pub match_mode: String,
    pub status: String,
    pub notes: String,
    pub matched: bool,
    pub ambiguous: bool,
    pub match_count: usize,
    pub match_type: String,
    pub match_confidence: f32,
    pub export_profile: String,
    pub warnings: Vec<String>,
    pub discarded_variants: Vec<String>,
    pub query_input: String,
    pub import_profile: String,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_row_id: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_value: Option<String>,
    #[serde(default)]
    pub alternatives: Vec<String>,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResponse {
    pub hits: Vec<LookupHit>,
    pub missing_metrics: Vec<String>,
    pub searched_files: usize,
}
