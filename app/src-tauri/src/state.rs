use crate::models::ImportSummary;
use crate::parser::normalizer::{ExportProfile, UrlValueKind};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRow {
    pub raw_row_id: usize,
    pub source_url: String,
    pub raw_exact_key: String,
    pub raw_without_fragment_key: String,
    pub raw_length: usize,
    pub scheme: Option<String>,
    pub authority: Option<String>,
    pub query_string: Option<String>,
    pub fragment: Option<String>,
    pub locale_prefix: Option<String>,
    pub normalized_url_key: Option<String>,
    pub page_identity_key: Option<String>,
    pub host_and_path_key: Option<String>,
    pub path_key: String,
    pub tracking_params: Vec<String>,
    pub functional_params: Vec<String>,
    pub unknown_params: Vec<String>,
    pub metric_values: Vec<String>,
    pub likely_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub summary: ImportSummary,
    pub rows: Vec<StoredRow>,
    pub metric_indexes: HashMap<String, usize>,
    pub url_kind: UrlValueKind,
    pub export_profile: ExportProfile,
    pub by_raw_exact: HashMap<String, Vec<usize>>,
    pub by_raw_without_fragment: HashMap<String, Vec<usize>>,
    pub by_normalized_url: HashMap<String, Vec<usize>>,
    pub by_page_identity: HashMap<String, Vec<usize>>,
    pub by_host_and_path: HashMap<String, Vec<usize>>,
    pub by_path: HashMap<String, Vec<usize>>,
}

pub struct AppState {
    pub imports: RwLock<HashMap<String, Import>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            imports: RwLock::new(HashMap::new()),
        }
    }
}
