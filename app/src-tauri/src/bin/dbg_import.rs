use aa_parser_lib::__dbg::{build_match_forms, import_path};
use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: dbg_import <file>");
    let import = import_path(&path).expect("import failed");
    println!("== summary ==");
    println!("file: {}", import.summary.file_name);
    println!("rows: {}", import.summary.row_count);
    println!("url col: {}", import.summary.url_column);
    println!("metrics: {:?}", import.summary.metric_columns);
    println!("warnings: {:?}", import.summary.warnings);
    println!("url kind: {:?}", import.url_kind);
    println!("profile: {:?}", import.export_profile);
    println!("truncation cap: {:?}", import.summary.truncation_cap);

    println!("\n== first 5 rows ==");
    for r in import.rows.iter().take(5) {
        println!("source: {}", r.source_url);
        println!("path: {}", r.path_key);
        println!(
            "normalized: {}",
            r.normalized_url_key.as_deref().unwrap_or("—")
        );
        println!(
            "identity: {}",
            r.page_identity_key.as_deref().unwrap_or("—")
        );
        let metrics: Vec<_> = import
            .summary
            .metric_columns
            .iter()
            .enumerate()
            .filter_map(|(idx, name)| r.metric_values.get(idx).map(|value| (name, value)))
            .collect();
        println!("metrics: {:?}", metrics);
        println!("---");
    }

    println!("\n== lookup tests ==");
    let queries = [
        "/careers/open-jobs",
        "https://www.hitachienergy.com/careers/open-jobs",
        "https://www.hitachienergy.com/careers/open-jobs?utm_source=foo",
        "/about-us/company-profile",
    ];
    for q in queries {
        let forms = build_match_forms(q);
        let full_hits = forms
            .normalized_url_key
            .as_ref()
            .and_then(|key| import.by_normalized_url.get(key))
            .map(|rows| rows.len());
        let path_hits = import.by_path.get(&forms.path_key).map(|rows| rows.len());
        println!("query: {}", q);
        println!("  path: {}", forms.path_key);
        println!("  full hits: {:?}", full_hits);
        println!("  path hits: {:?}", path_hits);
    }
}
