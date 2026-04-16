pub struct Mapping {
    pub url: Option<usize>,
    pub metrics: Vec<(String, usize)>,
    /// Adobe Workspace freeform tables put the dimension name + grand totals
    /// in the first data row. When detected, the importer must skip it.
    pub skip_first_data_row: bool,
    /// If the source headers had an empty first cell (Workspace format), this
    /// is the dimension name pulled from the first data row, used as the URL
    /// column header.
    pub url_header_override: Option<String>,
    /// Warnings surfaced to the UI when URL-column selection was ambiguous.
    pub warnings: Vec<String>,
}

const URL_ALIASES: &[&str] = &[
    "url",
    "page url",
    "page",
    "pages",
    "address",
    "landing page",
    "landing pages",
    "page path",
    "page name",
    "url destination",
    "path",
];

fn norm(s: &str) -> String {
    s.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_like_url(s: &str) -> bool {
    let t = s.trim();
    t.starts_with('/')
        || t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("www.")
}

pub fn map(headers: &[String], first_row: Option<&Vec<String>>) -> Mapping {
    let mut url_header_override: Option<String> = None;
    let mut skip_first_data_row = false;
    let mut warnings = Vec::new();

    let mut effective_headers: Vec<String> = headers.to_vec();

    // --- Adobe Workspace freeform table detection ---
    // Pattern: header row's first cell is empty, and the first data row's
    // first cell is the dimension *name* (e.g. "Page Path (AEM)"), with the
    // remaining cells being grand totals. We adopt the dimension name as the
    // URL column header and skip that totals row.
    if !headers.is_empty()
        && headers[0].trim().is_empty()
        && headers.iter().skip(1).any(|h| !h.trim().is_empty())
    {
        if let Some(fr) = first_row {
            if let Some(first_cell) = fr.first() {
                let fc = first_cell.trim();
                if !fc.is_empty() && !is_numeric(fc) {
                    effective_headers[0] = fc.to_string();
                    url_header_override = Some(fc.to_string());
                    skip_first_data_row = true;
                }
            }
        }
    }

    let normalized: Vec<String> = effective_headers.iter().map(|h| norm(h)).collect();

    // 1) Workspace override always wins.
    let mut url_idx: Option<usize> = if url_header_override.is_some() {
        Some(0)
    } else {
        None
    };

    // 2) Header alias match.
    if url_idx.is_none() {
        let exact_candidates: Vec<usize> = normalized
            .iter()
            .enumerate()
            .filter_map(|(idx, header)| URL_ALIASES.contains(&header.as_str()).then_some(idx))
            .collect();
        if let Some(first) = exact_candidates.first().copied() {
            url_idx = Some(first);
            if exact_candidates.len() > 1 {
                warnings.push(ambiguous_header_warning(&effective_headers, &exact_candidates, first));
            }
        }
    }

    // 3) Header substring match.
    if url_idx.is_none() {
        let substring_candidates: Vec<usize> = normalized
            .iter()
            .enumerate()
            .filter_map(|(idx, header)| {
                (header.contains("url")
                    || header.contains("page")
                    || header.contains("path")
                    || header.contains("address"))
                .then_some(idx)
            })
            .collect();
        if let Some(first) = substring_candidates.first().copied() {
            url_idx = Some(first);
            if substring_candidates.len() > 1 {
                warnings.push(ambiguous_header_warning(
                    &effective_headers,
                    &substring_candidates,
                    first,
                ));
            }
        }
    }

    // 4) Content-based fallback: pick the column whose values look like URLs.
    if url_idx.is_none() {
        if let Some(fr) = first_row {
            let content_candidates: Vec<usize> = fr
                .iter()
                .enumerate()
                .filter_map(|(idx, cell)| looks_like_url(cell).then_some(idx))
                .collect();
            if let Some(first) = content_candidates.first().copied() {
                url_idx = Some(first);
                if content_candidates.len() > 1 {
                    warnings.push(ambiguous_header_warning(
                        &effective_headers,
                        &content_candidates,
                        first,
                    ));
                }
            }
        }
    }

    // Metrics: every non-URL column with a non-empty header.
    let mut metrics = Vec::new();
    for (idx, h) in effective_headers.iter().enumerate() {
        if Some(idx) == url_idx {
            continue;
        }
        let label = h.trim();
        if label.is_empty() {
            continue;
        }
        metrics.push((label.to_string(), idx));
    }

    Mapping {
        url: url_idx,
        metrics,
        skip_first_data_row,
        url_header_override,
        warnings,
    }
}

fn is_numeric(s: &str) -> bool {
    let t = s.trim().replace(',', "").replace('%', "");
    !t.is_empty() && t.parse::<f64>().is_ok()
}

fn ambiguous_header_warning(headers: &[String], candidates: &[usize], chosen: usize) -> String {
    let labels = candidates
        .iter()
        .map(|idx| {
            headers
                .get(*idx)
                .map(|header| format!("'{}'", header.trim()))
                .unwrap_or_else(|| format!("column {}", idx + 1))
        })
        .collect::<Vec<_>>()
        .join(", ");

    let chosen_label = headers
        .get(chosen)
        .map(|header| format!("'{}'", header.trim()))
        .unwrap_or_else(|| format!("column {}", chosen + 1));

    format!(
        "multiple URL-like columns detected ({}); using {}",
        labels, chosen_label
    )
}
