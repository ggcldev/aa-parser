use crate::models::{LookupHit, LookupResponse, Row};
use crate::parser::normalizer::normalize_url;
use crate::state::Import;
use std::collections::HashSet;

pub fn lookup_multi(
    imports: &[&Import],
    queries: &[String],
    requested_metrics: &[String],
) -> LookupResponse {
    let mut hits = Vec::with_capacity(queries.len());

    for q in queries {
        let q_trim = q.trim();
        if q_trim.is_empty() {
            continue;
        }
        let normalized = normalize_url(q_trim);
        // Alternate: strip `/xx/yy/` locale prefix. Adobe's "Page Path (AEM)"
        // export has paths without locale (e.g. `/careers/open-jobs`), while
        // pasted URLs often carry `/se/en/…`, `/it/it/…`, etc. We try both.
        let stripped = strip_locale(&normalized);
        let candidates: Vec<&str> = if stripped != normalized {
            vec![normalized.as_str(), stripped.as_str()]
        } else {
            vec![normalized.as_str()]
        };

        let mut all_rows: Vec<Row> = Vec::new();
        for import in imports {
            let mut matched_idxs: Option<&Vec<usize>> = None;
            for cand in &candidates {
                if let Some(idxs) = import.by_normalized.get(*cand) {
                    matched_idxs = Some(idxs);
                    break;
                }
                // Truncation-prefix fallback: Adobe caps dimension values at
                // a fixed length (commonly 100). If the candidate is longer,
                // try the cap-length prefix.
                let q_len = cand.chars().count();
                for &cap in &import.truncation_lens {
                    if q_len > cap {
                        let prefix: String = cand.chars().take(cap).collect();
                        if let Some(idxs) = import.by_normalized.get(&prefix) {
                            matched_idxs = Some(idxs);
                            break;
                        }
                    }
                }
                if matched_idxs.is_some() {
                    break;
                }
            }
            if let Some(idxs) = matched_idxs {
                for i in idxs {
                    let mut row = filter_row(&import.rows[*i], requested_metrics);
                    row.source_file = Some(import.summary.file_name.clone());
                    row.batch_id = Some(import.summary.batch_id.clone());
                    all_rows.push(row);
                }
            }
        }

        let match_count = all_rows.len();
        hits.push(LookupHit {
            query: q.clone(),
            normalized_query: normalized,
            matched: match_count > 0,
            ambiguous: match_count > 1,
            match_count,
            rows: all_rows,
        });
    }

    let mut all_metrics: HashSet<&str> = HashSet::new();
    for import in imports {
        for m in &import.summary.metric_columns {
            all_metrics.insert(m.as_str());
        }
    }
    let missing_metrics: Vec<String> = requested_metrics
        .iter()
        .filter(|m| !all_metrics.contains(m.as_str()))
        .cloned()
        .collect();

    LookupResponse {
        hits,
        missing_metrics,
        searched_files: imports.len(),
    }
}

/// Strip a leading `/xx/yy/` locale prefix from a normalized path.
/// Adobe's "Page Path (AEM)" exports omit locale segments, while pasted URLs
/// often include them (e.g. `/se/en/careers/open-jobs` → `/careers/open-jobs`).
/// Handles both 2-letter codes (`/xx/yy/`) and longer variants (`/xx/yy-zz/`).
/// Returns the original string unchanged if no locale prefix is detected.
fn strip_locale(path: &str) -> String {
    // Matches patterns like /se/en/…  /it/it/…  /ca/en-us/…
    // i.e. /<2-letter>/<2+ letter, maybe hyphen-suffix>/…
    let bytes = path.as_bytes();
    if bytes.len() < 4 || bytes[0] != b'/' {
        return path.to_string();
    }
    // First segment: must be exactly 2 ASCII-lowercase letters
    if !(bytes[1].is_ascii_lowercase() && bytes[2].is_ascii_lowercase() && bytes[3] == b'/') {
        return path.to_string();
    }
    // Second segment: 2+ ASCII-lowercase letters, optionally followed by hyphen + letters
    let rest = &path[4..]; // after "/xx/"
    if let Some(slash_pos) = rest.find('/') {
        let seg = &rest[..slash_pos];
        if seg.len() >= 2 && seg.bytes().all(|b| b.is_ascii_lowercase() || b == b'-') {
            return rest[slash_pos..].to_string(); // "/careers/open-jobs"
        }
    }
    path.to_string()
}

fn filter_row(row: &Row, requested_metrics: &[String]) -> Row {
    let metrics = if requested_metrics.is_empty() {
        row.metrics.clone()
    } else {
        let mut m = std::collections::BTreeMap::new();
        for k in requested_metrics {
            if let Some(v) = row.metrics.get(k) {
                m.insert(k.clone(), v.clone());
            }
        }
        m
    };
    Row {
        raw_row_id: row.raw_row_id,
        source_url: row.source_url.clone(),
        normalized_url: row.normalized_url.clone(),
        metrics,
        extras: row.extras.clone(),
        source_file: None,
        batch_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_locale_basic() {
        assert_eq!(strip_locale("/se/en/careers/open-jobs"), "/careers/open-jobs");
        assert_eq!(strip_locale("/it/it/products/foo"), "/products/foo");
        assert_eq!(strip_locale("/ca/en/about"), "/about");
    }

    #[test]
    fn strip_locale_with_region() {
        assert_eq!(strip_locale("/us/en-us/careers"), "/careers");
        assert_eq!(strip_locale("/br/pt-br/page/sub"), "/page/sub");
    }

    #[test]
    fn strip_locale_no_match() {
        // Too short / no second segment
        assert_eq!(strip_locale("/se"), "/se");
        assert_eq!(strip_locale("/se/"), "/se/");
        // First segment >2 chars — not a locale
        assert_eq!(strip_locale("/usa/en/foo"), "/usa/en/foo");
        // Second segment only 1 char
        assert_eq!(strip_locale("/se/e/foo"), "/se/e/foo");
        // Already locale-free
        assert_eq!(strip_locale("/careers/open-jobs"), "/careers/open-jobs");
    }

    #[test]
    fn strip_locale_root_after_strip() {
        assert_eq!(strip_locale("/se/en/"), "/");
    }
}
