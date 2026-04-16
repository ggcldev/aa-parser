use crate::models::{CanonicalMapping, ImportSummary};
use crate::parser::normalizer::{ExportProfile, UrlValueKind};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::HashMap;
use std::path::PathBuf;

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
    pub canonical_path_key: Option<String>,
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
    pub by_canonical_path: HashMap<String, Vec<usize>>,
    pub canonical_mappings: Vec<CanonicalMapping>,
}

pub struct AppState {
    pub imports: RwLock<HashMap<String, Import>>,
    pub canonical_mappings: RwLock<Vec<CanonicalMapping>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedMappings {
    canonical_mappings: Vec<CanonicalMapping>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::load_from_disk()
    }
}

impl AppState {
    pub fn save_to_disk(&self) -> Result<(), String> {
        let snapshot = PersistedMappings {
            canonical_mappings: self.canonical_mappings.read().clone(),
        };

        let path = mappings_persistence_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create state directory: {}", e))?;
        }

        let payload = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| format!("failed to serialize mappings state: {}", e))?;
        fs::write(path, payload).map_err(|e| format!("failed to persist mappings state: {}", e))
    }

    fn load_from_disk() -> Self {
        let path = mappings_persistence_path();
        let Ok(payload) = fs::read_to_string(path) else {
            return Self {
                imports: RwLock::new(HashMap::new()),
                canonical_mappings: RwLock::new(Vec::new()),
            };
        };

        let Ok(snapshot) = serde_json::from_str::<PersistedMappings>(&payload) else {
            return Self {
                imports: RwLock::new(HashMap::new()),
                canonical_mappings: RwLock::new(Vec::new()),
            };
        };

        Self {
            imports: RwLock::new(HashMap::new()),
            canonical_mappings: RwLock::new(snapshot.canonical_mappings),
        }
    }
}

fn mappings_persistence_path() -> PathBuf {
    if let Ok(path) = std::env::var("AA_PARSER_MAPPINGS_FILE") {
        return PathBuf::from(path);
    }
    if let Ok(path) = std::env::var("AA_PARSER_STATE_FILE") {
        return PathBuf::from(path);
    }

    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".aa-parser").join("mappings-v1.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CanonicalMapping;

    #[test]
    fn persists_and_loads_canonical_mappings() {
        let tmp = std::env::temp_dir().join(format!(
            "aa-parser-mappings-{}.json",
            std::process::id()
        ));
        std::env::set_var("AA_PARSER_MAPPINGS_FILE", &tmp);

        let state = AppState {
            imports: RwLock::new(HashMap::new()),
            canonical_mappings: RwLock::new(vec![CanonicalMapping {
                mapping_id: "m1".to_string(),
                source_pattern: "/se/en/*".to_string(),
                target_canonical_path: "/".to_string(),
                rule_type: "path_map".to_string(),
                active: true,
                host_pattern: None,
                export_profile: None,
                priority: 100,
                notes: None,
            }]),
        };
        state.save_to_disk().unwrap();

        let loaded = AppState::default();
        assert_eq!(loaded.imports.read().len(), 0);
        assert_eq!(loaded.canonical_mappings.read().len(), 1);

        let _ = fs::remove_file(&tmp);
        std::env::remove_var("AA_PARSER_MAPPINGS_FILE");
    }
}
