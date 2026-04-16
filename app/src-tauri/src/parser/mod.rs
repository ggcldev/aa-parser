pub mod csv_parser;
pub mod header_mapper;
pub mod normalizer;
pub mod xlsx_parser;

use crate::models::ImportSummary;
use crate::parser::normalizer::{
    build_match_forms, ExportProfile, UrlValueKind,
};
use crate::state::{Import, StoredRow};
use anyhow::{anyhow, Result};
use csv::StringRecord;
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

    match ext.as_str() {
        "csv" | "tsv" | "txt" => import_csv(p),
        "xlsx" | "xls" | "xlsm" => import_xlsx(p),
        other => Err(anyhow!("unsupported file type: .{}", other)),
    }
}

fn import_csv(path: &Path) -> Result<Import> {
    let mut reader = csv_parser::reader(path)?;
    let rows = reader
        .records()
        .map(|record| record.map(record_to_row).map_err(anyhow::Error::from));

    import_rows(path, rows)
}

fn import_xlsx(path: &Path) -> Result<Import> {
    let range = xlsx_parser::first_sheet_range(path)?;

    let rows = range
        .rows()
        .map(|row| Ok(row.iter().map(xlsx_parser::cell_to_string).collect::<Vec<_>>()));

    import_rows(path, rows)
}

fn import_rows<I>(path: &Path, rows: I) -> Result<Import>
where
    I: IntoIterator<Item = Result<Vec<String>>>,
{
    let mut iter = rows.into_iter();

    let mut headers: Option<Vec<String>> = None;
    while let Some(row) = iter.next() {
        let row = row?;
        if is_skippable(&row) {
            continue;
        }
        if has_multiple_cells(&row) || looks_like_header(&row) {
            headers = Some(row.iter().map(|s| trim_cell(s)).collect());
            break;
        }
    }

    let mut headers = headers.ok_or_else(|| anyhow!("file is empty"))?;

    let mut first_data_row: Option<Vec<String>> = None;
    while let Some(row) = iter.next() {
        let row = row?;
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        first_data_row = Some(row);
        break;
    }

    let mapping = header_mapper::map(&headers, first_data_row.as_ref());

    if let Some(name) = &mapping.url_header_override {
        if let Some(first) = headers.first_mut() {
            *first = name.clone();
        }
    }

    let url_idx = mapping.url.ok_or_else(|| {
        anyhow!(
            "could not detect a URL column. Headers seen: {}",
            headers
                .iter()
                .map(|s| format!("'{}'", s))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    let metric_columns: Vec<String> = mapping.metrics.iter().map(|(name, _)| name.clone()).collect();
    let metric_indexes: HashMap<String, usize> = metric_columns
        .iter()
        .enumerate()
        .map(|(idx, name)| (name.clone(), idx))
        .collect();

    let mut warnings = mapping.warnings.clone();
    let mut rows = Vec::new();
    let mut by_raw_exact: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_raw_without_fragment: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_normalized_url: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_page_identity: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_host_and_path: HashMap<String, Vec<usize>> = HashMap::new();
    let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
    let mut raw_length_counts: HashMap<usize, usize> = HashMap::new();
    let mut full_like_rows = 0usize;
    let mut path_like_rows = 0usize;
    let mut query_rows = 0usize;
    let mut http_prefixed_rows = 0usize;
    let mut slash_prefixed_rows = 0usize;
    let mut other_prefixed_rows = 0usize;

    if let Some(row) = first_data_row {
        if !mapping.skip_first_data_row {
            process_row(
                row,
                url_idx,
                &mapping.metrics,
                &mut rows,
                &mut by_raw_exact,
                &mut by_raw_without_fragment,
                &mut by_normalized_url,
                &mut by_page_identity,
                &mut by_host_and_path,
                &mut by_path,
                &mut raw_length_counts,
                &mut full_like_rows,
                &mut path_like_rows,
                &mut query_rows,
                &mut http_prefixed_rows,
                &mut slash_prefixed_rows,
                &mut other_prefixed_rows,
            );
        }
    }

    for row in iter {
        process_row(
            row?,
            url_idx,
            &mapping.metrics,
            &mut rows,
            &mut by_raw_exact,
            &mut by_raw_without_fragment,
            &mut by_normalized_url,
            &mut by_page_identity,
            &mut by_host_and_path,
            &mut by_path,
            &mut raw_length_counts,
            &mut full_like_rows,
            &mut path_like_rows,
            &mut query_rows,
            &mut http_prefixed_rows,
            &mut slash_prefixed_rows,
            &mut other_prefixed_rows,
        );
    }

    if rows.is_empty() {
        warnings.push("no data rows found after header row".to_string());
    }

    if metric_columns.is_empty() {
        warnings.push("no metric columns detected — only URL matching will work".to_string());
    }

    let url_kind = if full_like_rows >= path_like_rows {
        UrlValueKind::FullUrl
    } else {
        UrlValueKind::PathOnly
    };
    let export_profile = classify_export_profile(
        http_prefixed_rows,
        slash_prefixed_rows,
        other_prefixed_rows,
        query_rows,
        rows.len(),
    );
    let match_mode = classify_match_mode(export_profile);
    if match_mode == "MIXED_MODE" {
        warnings.push(
            "Mixed export format detected in URL column. Choose FULL_URL_MODE or PATH_MODE manually before lookup."
                .to_string(),
        );
    }

    let raw_truncation_cap = detect_truncation_cap(&headers[url_idx], &raw_length_counts);
    let truncation_cap =
        raw_truncation_cap.and_then(|raw_cap| resolve_primary_cap(&rows, url_kind, raw_cap));

    if let (Some(raw_cap), Some(cap)) = (raw_truncation_cap, truncation_cap) {
        for row in &mut rows {
            row.likely_truncated = row.raw_length == raw_cap || primary_key_len(row, url_kind) == cap;
        }
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload")
        .to_string();

    let summary = ImportSummary {
        batch_id: Uuid::new_v4().to_string(),
        file_name,
        row_count: rows.len(),
        url_column: headers[url_idx].clone(),
        match_mode: match_mode.to_string(),
        url_kind: match url_kind {
            UrlValueKind::FullUrl => "full_url".to_string(),
            UrlValueKind::PathOnly => "path_only".to_string(),
        },
        export_profile: export_profile.as_str().to_string(),
        truncation_cap,
        metric_columns,
        warnings,
    };

    let import = Import {
        summary,
        rows,
        metric_indexes,
        url_kind,
        export_profile,
        by_raw_exact,
        by_raw_without_fragment,
        by_normalized_url,
        by_page_identity,
        by_host_and_path,
        by_path,
    };
    Ok(import)
}

#[allow(clippy::too_many_arguments)]
fn process_row(
    raw: Vec<String>,
    url_idx: usize,
    metrics: &[(String, usize)],
    rows: &mut Vec<StoredRow>,
    by_raw_exact: &mut HashMap<String, Vec<usize>>,
    by_raw_without_fragment: &mut HashMap<String, Vec<usize>>,
    by_normalized_url: &mut HashMap<String, Vec<usize>>,
    by_page_identity: &mut HashMap<String, Vec<usize>>,
    by_host_and_path: &mut HashMap<String, Vec<usize>>,
    by_path: &mut HashMap<String, Vec<usize>>,
    raw_length_counts: &mut HashMap<usize, usize>,
    full_like_rows: &mut usize,
    path_like_rows: &mut usize,
    query_rows: &mut usize,
    http_prefixed_rows: &mut usize,
    slash_prefixed_rows: &mut usize,
    other_prefixed_rows: &mut usize,
) {
    if raw.iter().all(|cell| cell.trim().is_empty()) {
        return;
    }

    let source_url = raw.get(url_idx).cloned().unwrap_or_default();
    if source_url.trim().is_empty() {
        return;
    }
    let source_trimmed = source_url.trim().to_ascii_lowercase();
    if source_trimmed.starts_with("http://") || source_trimmed.starts_with("https://") {
        *http_prefixed_rows += 1;
    } else if source_trimmed.starts_with('/') {
        *slash_prefixed_rows += 1;
    } else {
        *other_prefixed_rows += 1;
    }

    let forms = build_match_forms(&source_url);
    if forms.path_key.is_empty() {
        return;
    }

    let row_idx = rows.len();
    by_raw_exact
        .entry(forms.raw_exact_key.clone())
        .or_default()
        .push(row_idx);
    by_raw_without_fragment
        .entry(forms.raw_without_fragment_key.clone())
        .or_default()
        .push(row_idx);

    if let Some(full) = &forms.normalized_url_key {
        by_normalized_url.entry(full.clone()).or_default().push(row_idx);
        if source_url.contains('?') {
            *query_rows += 1;
        }
        *full_like_rows += 1;
    } else {
        *path_like_rows += 1;
    }

    if let Some(identity) = &forms.page_identity_key {
        by_page_identity
            .entry(identity.clone())
            .or_default()
            .push(row_idx);
    }
    if let Some(host_and_path) = &forms.host_and_path_key {
        by_host_and_path
            .entry(host_and_path.clone())
            .or_default()
            .push(row_idx);
    }

    by_path.entry(forms.path_key.clone()).or_default().push(row_idx);

    let metric_values = metrics
        .iter()
        .map(|(_, idx)| raw.get(*idx).cloned().unwrap_or_default())
        .collect();
    let raw_length = source_url.chars().count();
    *raw_length_counts.entry(raw_length).or_insert(0) += 1;

    rows.push(StoredRow {
        raw_row_id: row_idx,
        source_url,
        raw_exact_key: forms.raw_exact_key,
        raw_without_fragment_key: forms.raw_without_fragment_key,
        raw_length,
        scheme: forms.scheme,
        authority: forms.authority,
        query_string: forms.query_string,
        fragment: forms.fragment,
        locale_prefix: forms.locale_prefix,
        normalized_url_key: forms.normalized_url_key,
        page_identity_key: forms.page_identity_key,
        host_and_path_key: forms.host_and_path_key,
        path_key: forms.path_key,
        tracking_params: forms.tracking_params,
        functional_params: forms.functional_params,
        unknown_params: forms.unknown_params,
        metric_values,
        likely_truncated: false,
    });
}

fn classify_export_profile(
    http_prefixed_rows: usize,
    slash_prefixed_rows: usize,
    other_prefixed_rows: usize,
    query_rows: usize,
    total_rows: usize,
) -> ExportProfile {
    if total_rows == 0 {
        return ExportProfile::Unknown;
    }

    let http_majority = http_prefixed_rows.saturating_mul(100) / total_rows >= 80;
    let path_majority = slash_prefixed_rows.saturating_mul(100) / total_rows >= 80;
    let only_http = http_prefixed_rows > 0 && slash_prefixed_rows == 0 && other_prefixed_rows == 0;
    let only_path = slash_prefixed_rows > 0 && http_prefixed_rows == 0 && other_prefixed_rows == 0;

    if only_http || http_majority {
        if query_rows.saturating_mul(100) / http_prefixed_rows.max(1) >= 25 {
            return ExportProfile::FullUrlWithQuery;
        }
        return ExportProfile::FullUrl;
    }

    if only_path || path_majority {
        return ExportProfile::PathOnly;
    }

    ExportProfile::Unknown
}

fn classify_match_mode(export_profile: ExportProfile) -> &'static str {
    match export_profile {
        ExportProfile::FullUrl | ExportProfile::FullUrlWithQuery => "FULL_URL_MODE",
        ExportProfile::PathOnly => "PATH_MODE",
        ExportProfile::HostAndPath | ExportProfile::Unknown => "MIXED_MODE",
    }
}

fn detect_truncation_cap(url_header: &str, raw_length_counts: &HashMap<usize, usize>) -> Option<usize> {
    if let Some(cap) = parse_header_character_cap(url_header) {
        return Some(cap);
    }

    let (&max_len, &count) = raw_length_counts.iter().max_by_key(|(len, _)| *len)?;
    if count >= 3 && looks_like_truncation_cap(max_len) {
        Some(max_len)
    } else {
        None
    }
}

fn resolve_primary_cap(rows: &[StoredRow], url_kind: UrlValueKind, raw_cap: usize) -> Option<usize> {
    let mut normalized_counts: HashMap<usize, usize> = HashMap::new();
    for row in rows.iter().filter(|row| row.raw_length == raw_cap) {
        *normalized_counts
            .entry(primary_key_len(row, url_kind))
            .or_insert(0) += 1;
    }

    normalized_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(len, _)| len)
}

fn parse_header_character_cap(header: &str) -> Option<usize> {
    let lower = header.to_ascii_lowercase();
    if !lower.contains("char") {
        return None;
    }

    let mut digits = String::new();
    for ch in lower.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if !digits.is_empty() {
            break;
        }
    }

    digits
        .parse::<usize>()
        .ok()
        .filter(|cap| (32..=4096).contains(cap))
}

fn looks_like_truncation_cap(len: usize) -> bool {
    matches!(len, 50 | 64 | 80 | 100 | 128 | 150 | 200 | 250 | 255 | 256 | 500 | 1024)
}

fn primary_key_len(row: &StoredRow, url_kind: UrlValueKind) -> usize {
    match url_kind {
        UrlValueKind::FullUrl => row
            .normalized_url_key
            .as_ref()
            .map(|value| value.chars().count())
            .unwrap_or_else(|| row.path_key.chars().count()),
        UrlValueKind::PathOnly => row.path_key.chars().count(),
    }
}

fn trim_cell(cell: &str) -> String {
    cell.trim().trim_start_matches('\u{FEFF}').to_string()
}

fn record_to_row(record: StringRecord) -> Vec<String> {
    record
        .iter()
        .enumerate()
        .map(|(idx, cell)| {
            if idx == 0 {
                cell.trim_start_matches('\u{FEFF}').to_string()
            } else {
                cell.to_string()
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_character_cap_from_header() {
        assert_eq!(parse_header_character_cap("Full URL - 255 characters"), Some(255));
        assert_eq!(parse_header_character_cap("Page Path"), None);
    }

    #[test]
    fn accepts_known_truncation_cap_shapes() {
        assert!(looks_like_truncation_cap(100));
        assert!(looks_like_truncation_cap(255));
        assert!(!looks_like_truncation_cap(212));
    }
}
