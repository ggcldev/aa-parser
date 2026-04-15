use aa_parser_lib::__dbg::{import_path, normalize_url};
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

    println!("\n== first 5 rows ==");
    for r in import.rows.iter().take(5) {
        println!("source: {}", r.source_url);
        println!("normalized: {}", r.normalized_url);
        println!("metrics: {:?}", r.metrics);
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
        let n = normalize_url(q);
        let hit = import.by_normalized.get(&n);
        println!("query: {}", q);
        println!("  normalized: {}", n);
        println!("  hit: {:?}", hit.map(|v| v.len()));
    }
}
