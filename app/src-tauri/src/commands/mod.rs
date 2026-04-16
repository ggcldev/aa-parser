use crate::models::{ImportSummary, LookupResponse, UrlListLoad};
use crate::parser;
use crate::query::search;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn import_file(state: State<'_, AppState>, path: String) -> Result<ImportSummary, String> {
    let import = tauri::async_runtime::spawn_blocking(move || parser::import_path(&path))
        .await
        .map_err(|e| format!("import task failed: {}", e))?
        .map_err(|e| format!("{:#}", e))?;
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
pub async fn load_lookup_file(
    _state: State<'_, AppState>,
    path: String,
) -> Result<UrlListLoad, String> {
    let import = tauri::async_runtime::spawn_blocking(move || parser::import_path(&path))
        .await
        .map_err(|e| format!("lookup file task failed: {}", e))?
        .map_err(|e| format!("{:#}", e))?;
    Ok(UrlListLoad {
        file_name: import.summary.file_name.clone(),
        row_count: import.summary.row_count,
        url_column: import.summary.url_column.clone(),
        warnings: import.summary.warnings.clone(),
        urls: import.rows.into_iter().map(|row| row.source_url).collect(),
    })
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
    batch_ids: Option<Vec<String>>,
    match_mode_override: Option<String>,
) -> Result<LookupResponse, String> {
    let imports = state.imports.read();
    if imports.is_empty() {
        return Err("upload at least one Adobe source first".to_string());
    }
    let imps: Vec<&_> = match batch_ids {
        Some(batch_ids) => batch_ids
            .into_iter()
            .filter_map(|batch_id| imports.get(&batch_id))
            .collect(),
        None => imports.values().collect(),
    };
    if imps.is_empty() {
        return Err("select at least one Adobe source".to_string());
    }
    Ok(search::lookup_multi(
        &imps,
        &urls,
        &metrics,
        match_mode_override,
    ))
}

#[tauri::command]
pub fn remove_import(state: State<'_, AppState>, batch_id: String) -> Result<(), String> {
    state.imports.write().remove(&batch_id);
    Ok(())
}
