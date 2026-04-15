use crate::models::{ImportSummary, LookupResponse};
use crate::parser;
use crate::query::search;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn import_file(state: State<'_, AppState>, path: String) -> Result<ImportSummary, String> {
    let import = parser::import_path(&path).map_err(|e| format!("{:#}", e))?;
    let summary = import.summary.clone();
    state.imports.write().insert(summary.batch_id.clone(), import);
    Ok(summary)
}

#[tauri::command]
pub fn list_imports(state: State<'_, AppState>) -> Vec<ImportSummary> {
    state
        .imports
        .read()
        .values()
        .map(|i| i.summary.clone())
        .collect()
}

#[tauri::command]
pub fn all_metrics(state: State<'_, AppState>) -> Vec<String> {
    let imports = state.imports.read();
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for imp in imports.values() {
        for m in &imp.summary.metric_columns {
            if seen.insert(m.clone()) {
                out.push(m.clone());
            }
        }
    }
    out
}

#[tauri::command]
pub fn lookup_urls(
    state: State<'_, AppState>,
    urls: Vec<String>,
    metrics: Vec<String>,
) -> Result<LookupResponse, String> {
    let imports = state.imports.read();
    if imports.is_empty() {
        return Err("upload at least one file first".to_string());
    }
    let imps: Vec<&_> = imports.values().collect();
    Ok(search::lookup_multi(&imps, &urls, &metrics))
}

#[tauri::command]
pub fn remove_import(state: State<'_, AppState>, batch_id: String) {
    state.imports.write().remove(&batch_id);
}

#[tauri::command]
pub fn clear_imports(state: State<'_, AppState>) {
    state.imports.write().clear();
}
