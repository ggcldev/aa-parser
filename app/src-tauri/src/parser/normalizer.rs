use url::Url;

/// Canonical form for matching: path only, lowercased, trailing-slash stripped,
/// fragments + query strings removed, and common index-page suffixes dropped.
///
/// We deliberately drop scheme + host so that a row in an Adobe Workspace
/// export listed as `/careers/open-jobs` matches a user-pasted query like
/// `https://www.hitachienergy.com/careers/open-jobs?utm_source=newsletter`.
pub fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Strip fragment first regardless of form.
    let no_frag = trimmed.split('#').next().unwrap_or(trimmed);

    let with_scheme: String = if no_frag.starts_with("http://") || no_frag.starts_with("https://") {
        no_frag.to_string()
    } else if no_frag.starts_with("//") {
        format!("https:{}", no_frag)
    } else if no_frag.starts_with('/') {
        return canonicalize_path(no_frag.split('?').next().unwrap_or(no_frag));
    } else if no_frag.contains('.') && !no_frag.contains(' ') && !no_frag.starts_with('?') {
        // bare host like "example.com/foo"
        format!("https://{}", no_frag)
    } else {
        return canonicalize_path(no_frag.split('?').next().unwrap_or(no_frag));
    };

    match Url::parse(&with_scheme) {
        Ok(u) => canonicalize_path(u.path()),
        Err(_) => canonicalize_path(no_frag.split('?').next().unwrap_or(no_frag)),
    }
}

fn canonicalize_path(path: &str) -> String {
    let mut p = if path.is_empty() { "/".to_string() } else { path.to_string() };
    p = p.to_ascii_lowercase();

    // Collapse repeated slashes.
    while p.contains("//") {
        p = p.replace("//", "/");
    }

    // Drop common index/default suffixes so /foo and /foo/index.html match.
    for suffix in [
        "/index.html",
        "/index.htm",
        "/index.php",
        "/index.aspx",
        "/default.aspx",
        "/default.html",
        "/default.htm",
    ] {
        if p.ends_with(suffix) {
            p.truncate(p.len() - suffix.len());
            break;
        }
    }

    if p.len() > 1 {
        p = p.trim_end_matches('/').to_string();
    }
    if p.is_empty() {
        p = "/".to_string();
    }
    p
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
    fn trailing_slash() {
        assert_eq!(
            normalize_url("https://www.example.com/foo/"),
            normalize_url("https://www.example.com/foo")
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(
            normalize_url("/Careers/Open-Jobs"),
            normalize_url("/careers/open-jobs")
        );
    }

    #[test]
    fn fragment_stripped() {
        assert_eq!(
            normalize_url("https://example.com/foo#bar"),
            normalize_url("/foo")
        );
    }

    #[test]
    fn root() {
        assert_eq!(normalize_url("https://example.com/"), "/");
        assert_eq!(normalize_url("/"), "/");
    }

    #[test]
    fn query_string_dropped() {
        assert_eq!(
            normalize_url("https://www.example.com/careers/open-jobs?utm_source=foo"),
            normalize_url("/careers/open-jobs")
        );
    }

    #[test]
    fn index_html_collapsed() {
        assert_eq!(
            normalize_url("/products/index.html"),
            normalize_url("/products")
        );
    }

    #[test]
    fn double_slash() {
        assert_eq!(normalize_url("/foo//bar"), normalize_url("/foo/bar"));
    }
}
