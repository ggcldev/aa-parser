pub mod csv_parser;
pub mod xlsx_parser;
pub mod header_mapper;
pub mod normalizer;

use crate::models::{ImportSummary, Row};
use crate::state::Import;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

pub fn import_path(path: &str) -> Result<Import> {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    let all_rows: Vec<Vec<String>> = match ext.as_str() {
        "csv" | "tsv" | "txt" => csv_parser::read(p)?,
        "xlsx" | "xls" | "xlsm" => xlsx_parser::read(p)?,
        other => return Err(anyhow!("unsupported file type: .{}", other)),
    };

    if all_rows.is_empty() {
        return Err(anyhow!("file is empty"));
    }

    // --- Find the real header row ---
    // Skip rows that are blank, comment-only (`#...`), or single-cell preamble.
    // Stop at the first row that contains at least 2 non-empty cells, OR a
    // single non-comment cell that looks like a column name.
    let mut header_idx: Option<usize> = None;
    for (i, row) in all_rows.iter().enumerate() {
        if is_skippable(row) {
            continue;
        }
        if has_multiple_cells(row) || looks_like_header(row) {
            header_idx = Some(i);
            break;
        }
    }
    let header_idx = header_idx
        .ok_or_else(|| anyhow!("could not find a header row in this file"))?;

    let mut headers: Vec<String> = all_rows[header_idx]
        .iter()
        .map(|s| s.trim().to_string())
        .collect();
    let mut raw_rows: Vec<Vec<String>> = all_rows[header_idx + 1..]
        .iter()
        .filter(|r| !r.iter().all(|c| c.trim().is_empty()))
        .cloned()
        .collect();

    let mapping = header_mapper::map(&headers, raw_rows.first());

    if let Some(name) = &mapping.url_header_override {
        if let Some(first) = headers.first_mut() {
            *first = name.clone();
        }
    }
    if mapping.skip_first_data_row && !raw_rows.is_empty() {
        raw_rows.remove(0);
    }

    let url_idx = mapping
        .url
        .ok_or_else(|| anyhow!(
            "could not detect a URL column. Headers seen: {}",
            headers.iter().map(|s| format!("'{}'", s)).collect::<Vec<_>>().join(", ")
        ))?;

    let mut warnings = Vec::new();
    let mut rows = Vec::with_capacity(raw_rows.len());
    let mut by_normalized: HashMap<String, Vec<usize>> = HashMap::new();

    for (i, raw) in raw_rows.into_iter().enumerate() {
        let source_url = raw.get(url_idx).cloned().unwrap_or_default();
        if source_url.trim().is_empty() {
            continue;
        }
        let normalized_url = normalizer::normalize_url(&source_url);

        let mut metrics = std::collections::BTreeMap::new();
        for (col_name, col_idx) in &mapping.metrics {
            if let Some(v) = raw.get(*col_idx) {
                metrics.insert(col_name.clone(), v.clone());
            }
        }

        let row = Row {
            raw_row_id: i,
            source_url,
            normalized_url: normalized_url.clone(),
            metrics,
            extras: std::collections::BTreeMap::new(),
            source_file: None,
            batch_id: None,
        };
        by_normalized
            .entry(normalized_url)
            .or_default()
            .push(rows.len());
        rows.push(row);
    }

    if rows.is_empty() {
        warnings.push("no data rows found after header row".to_string());
    }

    // Detect Adobe path-truncation caps. Adobe exports truncate Page Path
    // dimension values at a fixed character limit (commonly 100). If many
    // indexed paths share the same length ≥ 50, that length is almost
    // certainly a truncation cap and we can use it as a prefix-fallback.
    let mut len_counts: HashMap<usize, usize> = HashMap::new();
    for k in by_normalized.keys() {
        let n = k.chars().count();
        if n >= 50 {
            *len_counts.entry(n).or_insert(0) += 1;
        }
    }
    let mut truncation_lens: Vec<usize> = len_counts
        .into_iter()
        .filter(|(_, c)| *c >= 3)
        .map(|(l, _)| l)
        .collect();
    truncation_lens.sort_unstable_by(|a, b| b.cmp(a));

    let metric_columns: Vec<String> = mapping.metrics.iter().map(|(n, _)| n.clone()).collect();
    if metric_columns.is_empty() {
        warnings.push("no metric columns detected — only URL matching will work".to_string());
    }

    let file_name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload")
        .to_string();

    let summary = ImportSummary {
        batch_id: Uuid::new_v4().to_string(),
        file_name,
        row_count: rows.len(),
        url_column: headers[url_idx].clone(),
        metric_columns,
        warnings,
    };

    Ok(Import {
        summary,
        rows,
        by_normalized,
        truncation_lens,
    })
}

/// A row is skippable as preamble if it's all blank, or if every non-empty cell
/// is a comment line (starts with `#` after trimming optional surrounding quotes).
fn is_skippable(row: &[String]) -> bool {
    let non_empty: Vec<&str> = row.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if non_empty.is_empty() {
        return true;
    }
    non_empty.iter().all(|s| {
        let s = s.trim_matches('"').trim();
        s.starts_with('#') || s.is_empty()
    })
}

fn has_multiple_cells(row: &[String]) -> bool {
    row.iter().filter(|s| !s.trim().is_empty()).count() >= 2
}

/// Special case: a single-cell row that doesn't look like a comment — e.g.
/// the user's CSV has just one column ("URL"). Treat it as a header.
fn looks_like_header(row: &[String]) -> bool {
    if let Some(first) = row.iter().find(|s| !s.trim().is_empty()) {
        let t = first.trim();
        return !t.starts_with('#') && t.len() < 200;
    }
    false
}
