use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Returns ALL rows from the CSV (no header split). The caller is responsible
/// for skipping comment/preamble rows and picking the real header row.
pub fn read(path: &Path) -> Result<Vec<Vec<String>>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let no_bom = raw.strip_prefix('\u{FEFF}').unwrap_or(&raw).to_string();

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(no_bom.as_bytes());

    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }
    Ok(rows)
}
