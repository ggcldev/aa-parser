use crate::models::{LookupHit, LookupResponse, Row};
use crate::parser::normalizer::{build_match_forms, ExportProfile};
use crate::state::Import;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MatchMode {
    FullUrl,
    Path,
    Mixed,
}

impl MatchMode {
    fn as_str(self) -> &'static str {
        match self {
            MatchMode::FullUrl => "FULL_URL_MODE",
            MatchMode::Path => "PATH_MODE",
            MatchMode::Mixed => "MIXED_MODE",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    Url,
    Keyword,
}

pub fn lookup_multi(
    imports: &[&Import],
    queries: &[String],
    requested_metrics: &[String],
    match_mode_override: Option<String>,
) -> LookupResponse {
    lookup_multi_with_mode(
        imports,
        queries,
        requested_metrics,
        match_mode_override,
        QueryMode::Url,
    )
}

pub fn lookup_multi_with_mode(
    imports: &[&Import],
    queries: &[String],
    requested_metrics: &[String],
    match_mode_override: Option<String>,
    query_mode: QueryMode,
) -> LookupResponse {
    let mut hits = Vec::with_capacity(queries.len());
    let requested_missing = missing_metrics(imports, requested_metrics);
    let keyword_haystacks = if query_mode == QueryMode::Keyword {
        Some(
            imports
                .iter()
                .map(|import| {
                    import
                        .rows
                        .iter()
                        .map(|row| build_keyword_haystack(&row.source_url))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    for query in queries {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            continue;
        }

        let forms = build_match_forms(trimmed);
        let keyword_query = if query_mode == QueryMode::Keyword {
            Some(build_keyword_query(trimmed))
        } else {
            None
        };
        let mut all_rows = Vec::new();
        let mut effective_modes = Vec::new();
        let mut invalid_url_for_full_mode = false;
        let mut mixed_mode_blocked = false;

        for (import_idx, import) in imports.iter().enumerate() {
            let mode = effective_mode(import, match_mode_override.as_deref());
            effective_modes.push(mode.as_str().to_string());

            if query_mode == QueryMode::Keyword {
                if let Some(keyword_query) = keyword_query.as_ref() {
                    if keyword_query.tokens.is_empty() {
                        continue;
                    }
                    let ids = collect_keyword_ids(
                        keyword_haystacks
                            .as_ref()
                            .and_then(|all| all.get(import_idx))
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                        keyword_query,
                    );
                    if !ids.is_empty() {
                        all_rows.extend(materialize_rows(
                            import,
                            ids,
                            requested_metrics,
                            "KEYWORD_EXACT_MATCH",
                        ));
                    }
                }
                continue;
            }

            match mode {
                MatchMode::Mixed => {
                    mixed_mode_blocked = true;
                }
                MatchMode::FullUrl => {
                    let Some(full_key) = forms.normalized_url_key.as_ref() else {
                        invalid_url_for_full_mode = true;
                        continue;
                    };
                    let ids = import
                        .by_normalized_url
                        .get(full_key)
                        .cloned()
                        .unwrap_or_default();
                    if !ids.is_empty() {
                        all_rows.extend(materialize_rows(
                            import,
                            ids,
                            requested_metrics,
                            "EXACT_FULL_URL_MATCH",
                        ));
                    }
                }
                MatchMode::Path => {
                    let ids = import
                        .by_path
                        .get(&forms.path_key)
                        .cloned()
                        .unwrap_or_default();
                    if !ids.is_empty() {
                        all_rows.extend(materialize_rows(
                            import,
                            ids,
                            requested_metrics,
                            "EXACT_PATH_MATCH",
                        ));
                    }
                }
            }
        }

        let mut notes = Vec::new();
        if !requested_missing.is_empty() {
            notes.push(format!(
                "Missing metric in export: {}",
                requested_missing.join(", ")
            ));
        }

        let (mode_label, status, matched, ambiguous, match_type, confidence) =
            if query_mode == QueryMode::Keyword {
                let (status, matched, ambiguous, match_type, confidence) = if all_rows.is_empty() {
                    (
                        "No keyword match found".to_string(),
                        false,
                        false,
                        "NO_MATCH".to_string(),
                        0.0,
                    )
                } else {
                    (
                        "Matched".to_string(),
                        true,
                        false,
                        "KEYWORD_MATCH".to_string(),
                        if all_rows.len() == 1 { 1.0 } else { 0.8 },
                    )
                };
                (
                    "KEYWORD_MODE".to_string(),
                    status,
                    matched,
                    ambiguous,
                    match_type,
                    confidence,
                )
            } else {
                let unique_modes = dedup(effective_modes);
                let mode_label = if unique_modes.len() == 1 {
                    unique_modes[0].clone()
                } else {
                    "MIXED_MODE".to_string()
                };

                let (status, matched, ambiguous, match_type, confidence) = if mixed_mode_blocked
                    && match_mode_override.is_none()
                {
                    (
                        "Mixed export format".to_string(),
                        false,
                        false,
                        "NO_MATCH".to_string(),
                        0.0,
                    )
                } else if invalid_url_for_full_mode && all_rows.is_empty() {
                    (
                        "Invalid URL".to_string(),
                        false,
                        false,
                        "NO_MATCH".to_string(),
                        0.0,
                    )
                } else if all_rows.is_empty() {
                    (
                        "No exact match found".to_string(),
                        false,
                        false,
                        "NO_MATCH".to_string(),
                        0.0,
                    )
                } else if all_rows.len() > 1 {
                    (
                        "Duplicate exact matches found".to_string(),
                        false,
                        true,
                        "EXACT_DUPLICATE".to_string(),
                        1.0,
                    )
                } else {
                    ("Matched".to_string(), true, false, "EXACT_MATCH".to_string(), 1.0)
                };
                (
                    mode_label,
                    status,
                    matched,
                    ambiguous,
                    match_type,
                    confidence,
                )
            };

        if status == "Mixed export format" {
            notes.push(
                "Choose FULL_URL_MODE or PATH_MODE manually for mixed Adobe exports.".to_string(),
            );
        }
        if query_mode == QueryMode::Keyword && !all_rows.is_empty() {
            notes.push(format!("{} keyword{} matched.", all_rows.len(), if all_rows.len() == 1 { "" } else { "s" }));
        }

        hits.push(LookupHit {
            query: query.clone(),
            normalized_query: if query_mode == QueryMode::Keyword {
                keyword_query
                    .as_ref()
                    .map(|k| k.tokens.join(" "))
                    .unwrap_or_default()
            } else {
                forms.path_key.clone()
            },
            match_mode: mode_label,
            status,
            notes: notes.join(" · "),
            matched,
            ambiguous,
            match_count: all_rows.len(),
            match_type,
            match_confidence: confidence,
            export_profile: summarize_profiles(imports),
            warnings: Vec::new(),
            discarded_variants: Vec::new(),
            query_input: query.clone(),
            import_profile: summarize_profiles(imports),
            confidence,
            matched_row_id: all_rows.first().map(|row| row.raw_row_id),
            matched_value: all_rows.first().map(|row| row.source_url.clone()),
            alternatives: all_rows
                .iter()
                .skip(1)
                .take(5)
                .map(|row| row.source_url.clone())
                .collect(),
            rows: all_rows,
        });
    }

    LookupResponse {
        hits,
        missing_metrics: requested_missing,
        searched_files: imports.len(),
    }
}

#[derive(Clone)]
struct KeywordHaystack {
    normal: String,
    compact: String,
}

#[derive(Clone)]
struct KeywordQuery {
    tokens: Vec<String>,
    compact: String,
}

fn build_keyword_haystack(input: &str) -> KeywordHaystack {
    let normal = normalize_keyword_text(input);
    let compact = normal.chars().filter(|c| !c.is_whitespace()).collect();
    KeywordHaystack { normal, compact }
}

fn build_keyword_query(input: &str) -> KeywordQuery {
    let normal = normalize_keyword_text(input);
    let tokens = normal
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect::<Vec<_>>();
    let compact = tokens.join("");
    KeywordQuery { tokens, compact }
}

fn normalize_keyword_text(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect_keyword_ids(haystacks: &[KeywordHaystack], query: &KeywordQuery) -> Vec<usize> {
    let query_normal = query.tokens.join(" ");
    haystacks
        .iter()
        .enumerate()
        .filter(|(_, haystack)| haystack.normal == query_normal)
        .map(|(idx, _)| idx)
        .collect()
}

fn effective_mode(import: &Import, override_mode: Option<&str>) -> MatchMode {
    match import.export_profile {
        ExportProfile::FullUrl | ExportProfile::FullUrlWithQuery => MatchMode::FullUrl,
        ExportProfile::PathOnly => MatchMode::Path,
        // Keyword imports are only used in keyword query mode; return Path
        // as a harmless default if URL mode is attempted against them.
        ExportProfile::KeywordExport => MatchMode::Path,
        ExportProfile::HostAndPath | ExportProfile::Unknown => match override_mode {
            Some("FULL_URL_MODE") => MatchMode::FullUrl,
            Some("PATH_MODE") => MatchMode::Path,
            _ => MatchMode::Mixed,
        },
    }
}

fn materialize_rows(
    import: &Import,
    mut ids: Vec<usize>,
    requested_metrics: &[String],
    row_match_type: &str,
) -> Vec<Row> {
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .map(|idx| row_to_response(import, idx, requested_metrics, row_match_type))
        .collect()
}

fn row_to_response(
    import: &Import,
    idx: usize,
    requested_metrics: &[String],
    row_match_type: &str,
) -> Row {
    let row = &import.rows[idx];
    let metrics = if requested_metrics.is_empty() {
        import
            .summary
            .metric_columns
            .iter()
            .enumerate()
            .filter_map(|(metric_idx, name)| {
                let value = row.metric_values.get(metric_idx)?;
                if value.is_empty() {
                    None
                } else {
                    Some((name.clone(), value.clone()))
                }
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        requested_metrics
            .iter()
            .filter_map(|name| {
                let metric_idx = import.metric_indexes.get(name)?;
                let value = row.metric_values.get(*metric_idx)?;
                if value.is_empty() {
                    None
                } else {
                    Some((name.clone(), value.clone()))
                }
            })
            .collect::<BTreeMap<_, _>>()
    };

    let mut extras = BTreeMap::new();
    extras.insert("export_profile".to_string(), import.export_profile.as_str().to_string());

    Row {
        raw_row_id: row.raw_row_id,
        source_url: row.source_url.clone(),
        normalized_url: row
            .normalized_url_key
            .clone()
            .unwrap_or_else(|| row.path_key.clone()),
        match_type: row_match_type.to_string(),
        match_score: Some(1.0),
        metrics,
        extras,
        source_file: Some(import.summary.file_name.clone()),
        batch_id: Some(import.summary.batch_id.clone()),
    }
}

fn summarize_profiles(imports: &[&Import]) -> String {
    let mut profiles = imports
        .iter()
        .map(|import| import.export_profile.as_str().to_string())
        .collect::<Vec<_>>();
    profiles.sort();
    profiles.dedup();
    if profiles.len() == 1 {
        profiles.pop().unwrap_or_else(|| "unknown_export".to_string())
    } else {
        "mixed_export_profiles".to_string()
    }
}

fn dedup(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup();
    values
}

fn missing_metrics(imports: &[&Import], requested_metrics: &[String]) -> Vec<String> {
    let mut all_metrics: HashSet<&str> = HashSet::new();
    for import in imports {
        for metric in &import.summary.metric_columns {
            all_metrics.insert(metric.as_str());
        }
    }
    requested_metrics
        .iter()
        .filter(|metric| !all_metrics.contains(metric.as_str()))
        .cloned()
        .collect()
}
