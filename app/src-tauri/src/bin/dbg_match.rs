use aa_parser_lib::__dbg::{import_path, normalize_url};
use std::env;

fn main() {
    let mut args = env::args().skip(1);
    let queries_path = args.next().expect("usage: dbg_match <queries.xlsx|.csv> <import1> [import2...]");
    let import_paths: Vec<String> = args.collect();
    if import_paths.is_empty() {
        panic!("need at least one import file");
    }

    // Load queries: reuse import_path purely to read rows, then pick URL column.
    // Simpler: read raw rows and assume column 0 (or any col that looks like a URL).
    let q_import = import_path(&queries_path).expect("failed to read queries file");
    let queries: Vec<String> = q_import.rows.iter().map(|r| r.source_url.clone()).collect();
    println!("loaded {} queries from {}", queries.len(), queries_path);

    let imports: Vec<_> = import_paths
        .iter()
        .map(|p| {
            let imp = import_path(p).expect("failed to import");
            println!(
                "import: {} rows={} truncation_lens={:?}",
                imp.summary.file_name, imp.summary.row_count, imp.truncation_lens
            );
            imp
        })
        .collect();

    let mut exact = 0usize;
    let mut prefix = 0usize;
    let mut miss = 0usize;
    let mut sample_miss: Vec<(String, String)> = Vec::new();

    for q in &queries {
        let n = normalize_url(q);
        if n.is_empty() {
            continue;
        }
        let mut hit_kind = 0u8; // 0 miss, 1 exact, 2 prefix
        for imp in &imports {
            if imp.by_normalized.contains_key(&n) {
                hit_kind = hit_kind.max(1);
                continue;
            }
            let q_len = n.chars().count();
            for &cap in &imp.truncation_lens {
                if q_len > cap {
                    let pref: String = n.chars().take(cap).collect();
                    if imp.by_normalized.contains_key(&pref) {
                        hit_kind = hit_kind.max(2);
                        break;
                    }
                }
            }
        }
        match hit_kind {
            1 => exact += 1,
            2 => prefix += 1,
            _ => {
                miss += 1;
                if sample_miss.len() < 15 {
                    sample_miss.push((q.clone(), n.clone()));
                }
            }
        }
    }

    println!("\n== results ==");
    println!("total queries : {}", queries.len());
    println!("exact matches : {}", exact);
    println!("prefix matches: {}", prefix);
    println!("misses        : {}", miss);

    println!("\n== sample misses ==");
    for (q, n) in &sample_miss {
        println!("raw:  {}", q);
        println!("norm: {}", n);
        println!("---");
    }

    // Dump a few sample indexed keys to eyeball the shape
    if let Some(imp) = imports.first() {
        println!("\n== sample indexed keys ==");
        for k in imp.by_normalized.keys().take(10) {
            println!("  {}", k);
        }
    }
}
