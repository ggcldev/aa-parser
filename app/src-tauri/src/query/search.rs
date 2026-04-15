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

/// Strip a leading locale/region prefix from a normalized path.
///
/// Adobe's "Page Path (AEM)" exports omit locale segments, while pasted URLs
/// often carry them. Examples of prefixes we strip:
///   `/se/en/…`          → two 2-letter segments (country + lang)
///   `/ca/en-us/…`       → country + lang-region
///   `/uk-ie/en/…`       → hyphenated country + lang
///   `/africa/en/…`      → long region name + lang
///   `/middle-east/en/…` → hyphenated region + lang
///
/// Strategy: split the path into segments. If the first 1–2 segments look like
/// a locale prefix (short alphabetic tokens, optionally hyphenated), strip them.
/// We avoid false positives by requiring the segment after the locale to exist
/// and not look like it could itself be a standalone page slug (i.e. there must
/// be remaining path after stripping).
fn strip_locale(path: &str) -> String {
    let segments: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if segments.len() < 2 {
        return path.to_string();
    }

    /// Returns true if `seg` looks like a locale/region token:
    /// 2–12 ASCII-lowercase chars with optional hyphens or underscores (not at edges).
    /// e.g. "se", "en", "en-us", "uk-ie", "africa", "middle-east", "pt_br"
    fn is_locale_segment(seg: &str) -> bool {
        let len = seg.len();
        if !(2..=16).contains(&len) {
            return false;
        }
        seg.bytes().all(|b| b.is_ascii_lowercase() || b == b'-' || b == b'_')
            && !seg.starts_with('-')
            && !seg.ends_with('-')
            && !seg.starts_with('_')
            && !seg.ends_with('_')
    }

    // First, strip known CMS internal prefixes like "language-masters"
    let skip = if !segments.is_empty() && matches!(segments[0],
        "language-masters" | "content" | "cf" | "jcr"
    ) {
        1
    } else {
        0
    };
    let segs = &segments[skip..];

    // Strip two segments (region/country + language): /xx/yy/rest
    // The second segment (language) must be ≤ 5 chars (e.g. "en", "sv", "en-us",
    // "pt-br", "pt_br") to avoid false positives on real paths like /careers/open-jobs/…
    // The first segment (region) uses full is_locale_segment (≤ 16 chars).
    if segs.len() >= 3
        && is_locale_segment(segs[0])
        && segs[1].len() <= 5
        && is_locale_segment(segs[1])
    {
        let rest: String = format!("/{}", segs[2..].join("/"));
        return rest;
    }
    // After a CMS prefix, a single locale segment like "fr-ca" is the full locale
    // (language-masters uses combined locale codes). Strip it if ≤ 5 chars.
    if skip > 0 && segs.len() >= 2 && segs[0].len() <= 5 && is_locale_segment(segs[0]) {
        let rest: String = format!("/{}", segs[1..].join("/"));
        return rest;
    }
    // If we stripped a CMS prefix but no locale, still return without the prefix
    if skip > 0 && !segs.is_empty() {
        let rest: String = format!("/{}", segs.join("/"));
        return rest;
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
    fn strip_locale_long_country() {
        // Hyphenated country codes
        assert_eq!(
            strip_locale("/uk-ie/en/company/partners"),
            "/company/partners"
        );
        // Long region names
        assert_eq!(
            strip_locale("/africa/en/company/supplying/v"),
            "/company/supplying/v"
        );
        assert_eq!(
            strip_locale("/middle-east/en/products"),
            "/products"
        );
    }

    #[test]
    fn strip_locale_underscore_lang() {
        // pt_br style locale
        assert_eq!(
            strip_locale("/africa/pt_br/about-us/company-profile"),
            "/about-us/company-profile"
        );
    }

    #[test]
    fn strip_locale_cms_prefix() {
        // language-masters prefix
        assert_eq!(
            strip_locale("/language-masters/fr-ca/company/profile"),
            "/company/profile"
        );
        // content prefix with locale
        assert_eq!(
            strip_locale("/content/us/en/products"),
            "/products"
        );
    }

    #[test]
    fn strip_locale_no_match() {
        // Too short / single segment
        assert_eq!(strip_locale("/se"), "/se");
        // Already locale-free real paths — must NOT strip
        assert_eq!(strip_locale("/careers/open-jobs"), "/careers/open-jobs");
        assert_eq!(
            strip_locale("/products-and-solutions/transformers"),
            "/products-and-solutions/transformers"
        );
        assert_eq!(strip_locale("/contact-us"), "/contact-us");
        // Numeric or mixed segments — not locale
        assert_eq!(strip_locale("/123/en/foo"), "/123/en/foo");
        // Real deep paths must not be stripped
        assert_eq!(
            strip_locale("/careers/open-jobs/details/jid3-184948"),
            "/careers/open-jobs/details/jid3-184948"
        );
    }

    #[test]
    fn strip_locale_root_after_strip() {
        let result = strip_locale("/se/en/page");
        assert_eq!(result, "/page");
    }
}
