use crate::models::{CanonicalMapping, ImportSummary, LookupResponse, UrlListLoad};
use crate::parser;
use crate::query::search;
use crate::state::AppState;
use serde::Deserialize;
use std::fs;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub async fn import_file(state: State<'_, AppState>, path: String) -> Result<ImportSummary, String> {
    let mappings = state.canonical_mappings.read().clone();
    let import = tauri::async_runtime::spawn_blocking(move || parser::import_path(&path, &mappings))
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
    state: State<'_, AppState>,
    path: String,
) -> Result<UrlListLoad, String> {
    let mappings = state.canonical_mappings.read().clone();
    let import = tauri::async_runtime::spawn_blocking(move || parser::import_path(&path, &mappings))
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
    Ok(search::lookup_multi(&imps, &urls, &metrics))
}

#[tauri::command]
pub fn remove_import(state: State<'_, AppState>, batch_id: String) -> Result<(), String> {
    state.imports.write().remove(&batch_id);
    Ok(())
}

#[tauri::command]
pub fn list_canonical_mappings(state: State<'_, AppState>) -> Vec<CanonicalMapping> {
    state.canonical_mappings.read().clone()
}

#[tauri::command]
pub fn add_canonical_mapping(
    state: State<'_, AppState>,
    source_pattern: String,
    target_canonical_path: String,
    rule_type: Option<String>,
    host_pattern: Option<String>,
    export_profile: Option<String>,
    priority: Option<i32>,
    notes: Option<String>,
) -> Result<CanonicalMapping, String> {
    let source_pattern = source_pattern.trim();
    let target_canonical_path = target_canonical_path.trim();
    if source_pattern.is_empty() || target_canonical_path.is_empty() {
        return Err("source pattern and target canonical path are required".to_string());
    }

    let mapping = CanonicalMapping {
        mapping_id: Uuid::new_v4().to_string(),
        source_pattern: source_pattern.to_string(),
        target_canonical_path: target_canonical_path.to_string(),
        rule_type: rule_type.unwrap_or_else(|| "path_map".to_string()),
        active: true,
        host_pattern: host_pattern
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty()),
        export_profile: export_profile
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        priority: priority.unwrap_or(100),
        notes: notes
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    };

    {
        let mut mappings = state.canonical_mappings.write();
        mappings.push(mapping.clone());
        let current = mappings.clone();
        let mut imports = state.imports.write();
        for import in imports.values_mut() {
            parser::reindex_canonical_mappings(import, &current);
        }
    }
    state.save_to_disk()?;

    Ok(mapping)
}

#[tauri::command]
pub fn remove_canonical_mapping(state: State<'_, AppState>, mapping_id: String) -> Result<(), String> {
    let mut mappings = state.canonical_mappings.write();
    let before = mappings.len();
    mappings.retain(|mapping| mapping.mapping_id != mapping_id);
    if before == mappings.len() {
        return Err("mapping not found".to_string());
    }

    let current = mappings.clone();
    let mut imports = state.imports.write();
    for import in imports.values_mut() {
        parser::reindex_canonical_mappings(import, &current);
    }
    state.save_to_disk()?;

    Ok(())
}

#[tauri::command]
pub fn update_canonical_mapping(
    state: State<'_, AppState>,
    mapping_id: String,
    source_pattern: Option<String>,
    target_canonical_path: Option<String>,
    rule_type: Option<String>,
    host_pattern: Option<String>,
    export_profile: Option<String>,
    priority: Option<i32>,
    active: Option<bool>,
    notes: Option<String>,
) -> Result<CanonicalMapping, String> {
    let mut mappings = state.canonical_mappings.write();
    let mapping = mappings
        .iter_mut()
        .find(|mapping| mapping.mapping_id == mapping_id)
        .ok_or_else(|| "mapping not found".to_string())?;

    if let Some(value) = source_pattern {
        let value = value.trim();
        if value.is_empty() {
            return Err("source pattern cannot be empty".to_string());
        }
        mapping.source_pattern = value.to_string();
    }
    if let Some(value) = target_canonical_path {
        let value = value.trim();
        if value.is_empty() {
            return Err("target canonical path cannot be empty".to_string());
        }
        mapping.target_canonical_path = value.to_string();
    }
    if let Some(value) = rule_type {
        let value = value.trim();
        if !value.is_empty() {
            mapping.rule_type = value.to_string();
        }
    }
    if let Some(value) = host_pattern {
        let value = value.trim().to_ascii_lowercase();
        mapping.host_pattern = if value.is_empty() { None } else { Some(value) };
    }
    if let Some(value) = export_profile {
        let value = value.trim().to_string();
        mapping.export_profile = if value.is_empty() { None } else { Some(value) };
    }
    if let Some(value) = priority {
        mapping.priority = value;
    }
    if let Some(value) = notes {
        let value = value.trim().to_string();
        mapping.notes = if value.is_empty() { None } else { Some(value) };
    }
    if let Some(value) = active {
        mapping.active = value;
    }

    let updated = mapping.clone();
    let current = mappings.clone();
    let mut imports = state.imports.write();
    for import in imports.values_mut() {
        parser::reindex_canonical_mappings(import, &current);
    }
    state.save_to_disk()?;

    Ok(updated)
}

#[tauri::command]
pub fn reorder_canonical_mapping(
    state: State<'_, AppState>,
    mapping_id: String,
    direction: String,
) -> Result<(), String> {
    let mut mappings = state.canonical_mappings.write();
    if mappings.is_empty() {
        return Ok(());
    }

    let mut order: Vec<usize> = (0..mappings.len()).collect();
    order.sort_by(|a, b| mappings[*b].priority.cmp(&mappings[*a].priority).then(a.cmp(b)));

    let Some(pos) = order
        .iter()
        .position(|idx| mappings[*idx].mapping_id == mapping_id)
    else {
        return Err("mapping not found".to_string());
    };

    match direction.as_str() {
        "up" => {
            if pos == 0 {
                return Ok(());
            }
            order.swap(pos, pos - 1);
        }
        "down" => {
            if pos + 1 >= order.len() {
                return Ok(());
            }
            order.swap(pos, pos + 1);
        }
        _ => return Err("direction must be 'up' or 'down'".to_string()),
    }

    let mut next_priority = (order.len() as i32) * 10;
    for idx in order {
        mappings[idx].priority = next_priority;
        next_priority -= 10;
    }

    let current = mappings.clone();
    let mut imports = state.imports.write();
    for import in imports.values_mut() {
        parser::reindex_canonical_mappings(import, &current);
    }
    state.save_to_disk()?;

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct CanonicalMappingFileRow {
    source_pattern: String,
    target_canonical_path: String,
    rule_type: Option<String>,
    active: Option<bool>,
    host_pattern: Option<String>,
    export_profile: Option<String>,
    priority: Option<i32>,
    notes: Option<String>,
}

#[tauri::command]
pub fn import_canonical_mappings(state: State<'_, AppState>, path: String) -> Result<usize, String> {
    let raw = fs::read_to_string(&path).map_err(|e| format!("failed to read file: {}", e))?;
    let rows: Vec<CanonicalMappingFileRow> =
        serde_json::from_str(&raw).map_err(|e| format!("invalid JSON: {}", e))?;

    if rows.is_empty() {
        return Ok(0);
    }

    let mut added = 0usize;
    let mut mappings = state.canonical_mappings.write();
    for row in rows {
        let source_pattern = row.source_pattern.trim();
        let target_canonical_path = row.target_canonical_path.trim();
        if source_pattern.is_empty() || target_canonical_path.is_empty() {
            continue;
        }
        let exists = mappings.iter().any(|mapping| {
            mapping.source_pattern.eq_ignore_ascii_case(source_pattern)
                && mapping
                    .target_canonical_path
                    .eq_ignore_ascii_case(target_canonical_path)
        });
        if exists {
            continue;
        }

        mappings.push(CanonicalMapping {
            mapping_id: Uuid::new_v4().to_string(),
            source_pattern: source_pattern.to_string(),
            target_canonical_path: target_canonical_path.to_string(),
            rule_type: row.rule_type.unwrap_or_else(|| "path_map".to_string()),
            active: row.active.unwrap_or(true),
            host_pattern: row
                .host_pattern
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty()),
            export_profile: row
                .export_profile
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            priority: row.priority.unwrap_or(100),
            notes: row
                .notes
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        });
        added += 1;
    }

    let current = mappings.clone();
    let mut imports = state.imports.write();
    for import in imports.values_mut() {
        parser::reindex_canonical_mappings(import, &current);
    }
    state.save_to_disk()?;

    Ok(added)
}

#[tauri::command]
pub fn export_canonical_mappings(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let mappings = state.canonical_mappings.read();
    let json = serde_json::to_string_pretty(&*mappings)
        .map_err(|e| format!("failed to serialize mappings: {}", e))?;
    fs::write(path, json).map_err(|e| format!("failed to write file: {}", e))?;
    Ok(())
}
