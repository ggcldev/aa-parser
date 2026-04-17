use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UrlValueKind {
    FullUrl,
    PathOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportProfile {
    FullUrl,
    FullUrlWithQuery,
    HostAndPath,
    PathOnly,
    KeywordExport,
    Unknown,
}

impl ExportProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportProfile::FullUrl => "full_url_export",
            ExportProfile::FullUrlWithQuery => "full_url_with_query_export",
            ExportProfile::HostAndPath => "host_and_path_export",
            ExportProfile::PathOnly => "path_only_export",
            ExportProfile::KeywordExport => "keyword_export",
            ExportProfile::Unknown => "unknown_export",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchForms {
    pub kind: UrlValueKind,
    pub raw_exact_key: String,
    pub scheme: Option<String>,
    pub authority: Option<String>,
    pub query_string: Option<String>,
    pub fragment: Option<String>,
    pub locale_prefix: Option<String>,
    pub normalized_url_key: Option<String>,
    pub page_identity_key: Option<String>,
    pub host_and_path_key: Option<String>,
    pub path_key: String,
    pub has_fragment: bool,
    pub tracking_params: Vec<String>,
    pub functional_params: Vec<String>,
    pub unknown_params: Vec<String>,
    pub raw_without_fragment_key: String,
}

const INVISIBLE_URL_CHARS: &[char] = &[
    '\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}', '\u{00AD}', '\u{200E}', '\u{200F}',
    '\u{2028}', '\u{2029}', '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}',
    '\u{2060}', '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}', '\u{FFFE}',
];

const TRACKING_PARAMS: &[&str] = &["gclid", "fbclid", "msclkid", "source"];
const FUNCTIONAL_PARAMS: &[&str] = &[
    "q",
    "query",
    "search",
    "page",
    "p",
    "offset",
    "limit",
    "sort",
    "filter",
    "lang",
    "locale",
    "category",
    "tab",
    "view",
    "id",
];

struct ParamBuckets {
    tracking: Vec<String>,
    functional: Vec<String>,
    unknown: Vec<String>,
}

pub fn normalize_url(input: &str) -> String {
    build_match_forms(input).path_key
}

pub fn build_match_forms(input: &str) -> MatchForms {
    let cleaned = sanitize_input(input);
    if cleaned.is_empty() {
        return MatchForms {
            kind: UrlValueKind::PathOnly,
            raw_exact_key: String::new(),
            scheme: None,
            authority: None,
            query_string: None,
            fragment: None,
            locale_prefix: None,
            normalized_url_key: None,
            page_identity_key: None,
            host_and_path_key: None,
            path_key: String::new(),
            has_fragment: false,
            tracking_params: Vec::new(),
            functional_params: Vec::new(),
            unknown_params: Vec::new(),
            raw_without_fragment_key: String::new(),
        };
    }

    let raw_without_fragment = cleaned
        .split('#')
        .next()
        .unwrap_or(&cleaned)
        .trim()
        .to_string();

    if let Some(parsed) = parse_url_like(&cleaned) {
        let scheme = parsed.scheme().to_ascii_lowercase();
        let authority = normalized_authority(&parsed);
        let path = canonicalize_path(parsed.path());
        let locale_prefix = detect_locale_prefix(&path);
        let fragment = parsed.fragment().map(|value| value.to_string());
        let has_fragment = fragment.is_some();
        let normalized_query = normalize_query(parsed.query());
        let buckets = classify_query_params(normalized_query.as_deref());
        let normalized_url_key = build_full_key(&scheme, &authority, &path, normalized_query.as_deref());
        let page_identity_key = build_full_key(
            &scheme,
            &authority,
            &path,
            drop_tracking_params(normalized_query.as_deref()).as_deref(),
        );
        let host_and_path_key = format!("{}{}", authority, path);

        return MatchForms {
            kind: UrlValueKind::FullUrl,
            raw_exact_key: cleaned,
            scheme: Some(scheme),
            authority: Some(authority),
            query_string: normalized_query.clone(),
            fragment,
            locale_prefix,
            normalized_url_key: Some(normalized_url_key),
            page_identity_key: Some(page_identity_key),
            host_and_path_key: Some(host_and_path_key),
            path_key: path,
            has_fragment,
            tracking_params: buckets.tracking,
            functional_params: buckets.functional,
            unknown_params: buckets.unknown,
            raw_without_fragment_key: raw_without_fragment,
        };
    }

    let path = canonicalize_path(cleaned.split('?').next().unwrap_or(&cleaned));
    MatchForms {
        kind: UrlValueKind::PathOnly,
        raw_exact_key: cleaned,
        scheme: None,
        authority: None,
        query_string: None,
        fragment: None,
        locale_prefix: detect_locale_prefix(&path),
        normalized_url_key: None,
        page_identity_key: None,
        host_and_path_key: None,
        path_key: path,
        has_fragment: false,
        tracking_params: Vec::new(),
        functional_params: Vec::new(),
        unknown_params: Vec::new(),
        raw_without_fragment_key: raw_without_fragment,
    }
}

fn sanitize_input(input: &str) -> String {
    input
        .trim()
        .chars()
        .filter(|c| !INVISIBLE_URL_CHARS.contains(c))
        .collect::<String>()
        .replace("&amp;", "&")
        .replace("&#38;", "&")
}

fn parse_url_like(input: &str) -> Option<Url> {
    let candidate = if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else if input.starts_with("//") {
        format!("https:{}", input)
    } else if input.starts_with('/') || input.starts_with('?') || input.contains(' ') {
        return None;
    } else if input.contains('.') {
        format!("https://{}", input)
    } else {
        return None;
    };

    Url::parse(&candidate).ok()
}

fn normalized_authority(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    match url.port() {
        Some(80) | Some(443) | None => host,
        Some(port) => format!("{}:{}", host, port),
    }
}

fn canonicalize_path(path: &str) -> String {
    let mut p = if path.is_empty() {
        "/".to_string()
    } else {
        path.trim().to_string()
    };
    if !p.starts_with('/') {
        p = format!("/{}", p);
    }

    while p.contains("//") {
        p = p.replace("//", "/");
    }

    if p.len() > 1 {
        p = p.trim_end_matches('/').to_string();
    }
    if p.is_empty() {
        p = "/".to_string();
    }
    p
}

fn detect_locale_prefix(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
    if segments.len() < 2 {
        return None;
    }
    let first = segments[0];
    let second = segments[1];
    if is_locale_segment(first) && is_language_segment(second) {
        return Some(format!("{}/{}", first, second));
    }
    None
}

fn is_locale_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() == 2 && bytes.iter().all(|b| b.is_ascii_lowercase())
}

fn is_language_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.len() == 2 && bytes.iter().all(|b| b.is_ascii_lowercase()) {
        return true;
    }
    bytes.len() == 5
        && matches!(bytes[2], b'-' | b'_')
        && bytes[0..2].iter().all(|b| b.is_ascii_lowercase())
        && bytes[3..5].iter().all(|b| b.is_ascii_lowercase())
}

fn normalize_query(query: Option<&str>) -> Option<String> {
    let query = query?.trim();
    if query.is_empty() {
        return None;
    }
    Some(query.to_string())
}

fn drop_tracking_params(query: Option<&str>) -> Option<String> {
    let query = query?;
    let parts: Vec<String> = query
        .split('&')
        .filter(|entry| !entry.trim().is_empty())
        .filter_map(|entry| {
            let key = entry.split('=').next().unwrap_or_default().to_ascii_lowercase();
            let is_tracking = key.starts_with("utm_") || TRACKING_PARAMS.contains(&key.as_str());
            (!is_tracking).then_some(entry.to_string())
        })
        .collect();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("&"))
    }
}

fn classify_query_params(query: Option<&str>) -> ParamBuckets {
    let mut buckets = ParamBuckets {
        tracking: Vec::new(),
        functional: Vec::new(),
        unknown: Vec::new(),
    };
    let Some(query) = query else {
        return buckets;
    };

    for entry in query.split('&').filter(|entry| !entry.trim().is_empty()) {
        let key = entry
            .split('=')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        if key.starts_with("utm_") || TRACKING_PARAMS.contains(&key.as_str()) {
            buckets.tracking.push(key);
        } else if FUNCTIONAL_PARAMS.contains(&key.as_str()) {
            buckets.functional.push(key);
        } else {
            buckets.unknown.push(key);
        }
    }

    buckets
}

fn build_full_key(scheme: &str, authority: &str, path: &str, query: Option<&str>) -> String {
    match query {
        Some(query) if !query.is_empty() => format!("{}://{}{}?{}", scheme, authority, path, query),
        _ => format!("{}://{}{}", scheme, authority, path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_url_and_path_match() {
        assert_eq!(
            normalize_url("https://www.hitachienergy.com/careers/open-jobs"),
            normalize_url("/careers/open-jobs")
        );
    }

    #[test]
    fn full_url_preserves_query_for_exact_matching() {
        let forms = build_match_forms(
            "https://www.example.com/products/item?utm_source=foo&x=1#ignore-me",
        );
        assert_eq!(
            forms.normalized_url_key.as_deref(),
            Some("https://www.example.com/products/item?utm_source=foo&x=1")
        );
        assert_eq!(
            forms.page_identity_key.as_deref(),
            Some("https://www.example.com/products/item?x=1")
        );
        assert_eq!(forms.tracking_params, vec!["utm_source".to_string()]);
        assert_eq!(forms.unknown_params, vec!["x".to_string()]);
    }

    #[test]
    fn tracking_only_query_rolls_up_to_page_identity() {
        let forms = build_match_forms("https://example.com/foo?utm_source=a&gclid=123");
        assert_eq!(forms.page_identity_key.as_deref(), Some("https://example.com/foo"));
    }

    #[test]
    fn trailing_slash() {
        assert_eq!(
            normalize_url("https://www.example.com/foo/"),
            normalize_url("https://www.example.com/foo")
        );
    }

    #[test]
    fn path_case_preserved() {
        assert_eq!(
            normalize_url("/Careers/Open-Jobs"),
            "/Careers/Open-Jobs"
        );
    }

    #[test]
    fn fragment_captured() {
        let forms = build_match_forms("https://example.com/foo#bar");
        assert_eq!(forms.fragment.as_deref(), Some("bar"));
        assert_eq!(forms.path_key, "/foo");
    }

}
