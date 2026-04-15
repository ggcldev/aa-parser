use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook_auto, Data, Reader};
use std::path::Path;

/// Returns ALL rows from the first sheet (no header split). The caller is
/// responsible for skipping comment/preamble rows and picking the real header.
pub fn read(path: &Path) -> Result<Vec<Vec<String>>> {
    let mut wb = open_workbook_auto(path)
        .with_context(|| format!("failed to open spreadsheet {}", path.display()))?;

    let sheet_name = wb
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("workbook has no sheets"))?;

    let range = wb
        .worksheet_range(&sheet_name)
        .with_context(|| format!("could not read sheet {}", sheet_name))?;

    let mut rows = Vec::new();
    for r in range.rows() {
        let row: Vec<String> = r.iter().map(cell_to_string).collect();
        rows.push(row);
    }
    Ok(rows)
}

fn cell_to_string(c: &Data) -> String {
    match c {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{}", f)
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#ERR:{:?}", e),
    }
}
