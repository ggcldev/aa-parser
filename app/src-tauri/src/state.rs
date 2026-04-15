use crate::models::{ImportSummary, Row};
use parking_lot::RwLock;
use std::collections::HashMap;

pub struct Import {
    pub summary: ImportSummary,
    pub rows: Vec<Row>,
    pub by_normalized: HashMap<String, Vec<usize>>,
    /// Lengths at which Adobe appears to have truncated paths in this export
    /// (e.g. 100). Used for prefix-fallback lookup when a full pasted URL
    /// exceeds the cap. Sorted descending (longest first).
    pub truncation_lens: Vec<usize>,
}

#[derive(Default)]
pub struct AppState {
    pub imports: RwLock<HashMap<String, Import>>,
}
