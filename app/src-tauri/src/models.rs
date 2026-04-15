use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub raw_row_id: usize,
    pub source_url: String,
    pub normalized_url: String,
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
    pub metric_columns: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupHit {
    pub query: String,
    pub normalized_query: String,
    pub matched: bool,
    pub ambiguous: bool,
    pub match_count: usize,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResponse {
    pub hits: Vec<LookupHit>,
    pub missing_metrics: Vec<String>,
    pub searched_files: usize,
}
