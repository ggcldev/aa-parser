use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Open a CSV/TSV/TXT file as a streaming reader. Callers can iterate records
/// without first loading the full file into a giant string buffer.
pub fn reader(path: &Path) -> Result<csv::Reader<BufReader<File>>> {
    let delimiter = detect_delimiter(path)?;
    let file = File::open(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(BufReader::new(file)))
}

fn detect_delimiter(path: &Path) -> Result<u8> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("tsv") => return Ok(b'\t'),
        Some("csv") => return Ok(b','),
        _ => {}
    }

    let mut best = (b',', 0usize);

    for delimiter in [b'\t', b',', b';'] {
        let score = sniff_delimiter_score(reader_by_reopen(path)?, delimiter)?;
        if score > best.1 {
            best = (delimiter, score);
        }
    }

    Ok(best.0)
}

fn reader_by_reopen(path: &Path) -> Result<BufReader<File>> {
    let file = File::open(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(BufReader::new(file))
}

fn sniff_delimiter_score<R: BufRead>(reader: R, delimiter: u8) -> Result<usize> {
    let delim = delimiter as char;
    let mut score = 0usize;

    for line in reader.lines().take(12) {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts = trimmed.split(delim).count();
        if parts >= 2 {
            score += parts - 1;
        }
    }

    Ok(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_tsv_extension() {
        let path = write_temp_file("tsv", "URL\tVisits\n/example\t12\n");
        assert_eq!(detect_delimiter(&path).unwrap(), b'\t');
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn sniffs_tab_delimited_txt() {
        let path = write_temp_file("txt", "URL\tVisits\n/example\t12\n/other\t8\n");
        assert_eq!(detect_delimiter(&path).unwrap(), b'\t');
        fs::remove_file(path).unwrap();
    }

    fn write_temp_file(ext: &str, contents: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aa-parser-{}.{}", unique, ext));
        fs::write(&path, contents).unwrap();
        path
    }
}
