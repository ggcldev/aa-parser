use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook_auto, Data, Range, Reader};
use std::path::Path;

/// Open the first worksheet without cloning it into an additional row matrix.
pub fn first_sheet_range(path: &Path) -> Result<Range<Data>> {
    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("failed to open spreadsheet {}", path.display()))?;

    let sheet_name = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("workbook has no sheets"))?;

    workbook
        .worksheet_range(&sheet_name)
        .with_context(|| format!("could not read sheet {}", sheet_name))
}

pub fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(value) => value.clone(),
        Data::Float(value) => {
            if value.fract() == 0.0 && value.abs() < 1e15 {
                format!("{}", *value as i64)
            } else {
                format!("{}", value)
            }
        }
        Data::Int(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => value.to_string(),
        Data::DateTimeIso(value) => value.clone(),
        Data::DurationIso(value) => value.clone(),
        Data::Error(err) => format!("#ERR:{:?}", err),
    }
}
