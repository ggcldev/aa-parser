use crate::models::{ImportSummary, LookupResponse, UrlListLoad};
use crate::parser;
use crate::query::search;
use crate::query::search::QueryMode;
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
    query_mode: Option<String>,
) -> Result<UrlListLoad, String> {
    let mode = match query_mode.as_deref() {
        Some("keyword") => parser::LookupValueMode::Keyword,
        _ => parser::LookupValueMode::Url,
    };
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        match parser::load_lookup_values(&path, mode) {
            Ok(loaded) => Ok(loaded),
            Err(err) => {
                // Fallback: if URL mode can't find a URL column, try keyword/query mode.
                // This handles sources like Search Console exports ("Top queries", etc.).
                if mode == parser::LookupValueMode::Url {
                    let msg = format!("{:#}", err);
                    if msg.contains("could not detect a URL column") {
                        let mut loaded =
                            parser::load_lookup_values(&path, parser::LookupValueMode::Keyword)?;
                        loaded.warnings.push(
                            "No URL column detected; loaded keyword/query column instead."
                                .to_string(),
                        );
                        return Ok(loaded);
                    }
                }
                Err(err)
            }
        }
    })
        .await
        .map_err(|e| format!("lookup file task failed: {}", e))?
        .map_err(|e| format!("{:#}", e))?;
    Ok(UrlListLoad {
        file_name: loaded.file_name,
        row_count: loaded.row_count,
        url_column: loaded.column_name,
        warnings: loaded.warnings,
        urls: loaded.values,
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
    query_mode: Option<String>,
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
    let mode = match query_mode.as_deref() {
        Some("keyword") => QueryMode::Keyword,
        _ => QueryMode::Url,
    };
    Ok(search::lookup_multi_with_mode(
        &imps,
        &urls,
        &metrics,
        match_mode_override,
        mode,
    ))
}

#[tauri::command]
pub fn remove_import(state: State<'_, AppState>, batch_id: String) -> Result<(), String> {
    state.imports.write().remove(&batch_id);
    Ok(())
}
