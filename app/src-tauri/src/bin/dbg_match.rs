use aa_parser_lib::__dbg::{import_path, lookup_multi};
use std::collections::BTreeMap;
use std::env;

fn main() {
    let mut args = env::args().skip(1);
    let queries_path = args
        .next()
        .expect("usage: dbg_match <queries.xlsx|.csv> <import1> [import2...]");
    let import_paths: Vec<String> = args.collect();
    if import_paths.is_empty() {
        panic!("need at least one import file");
    }

    let queries_import = import_path(&queries_path).expect("failed to read queries file");
    let queries: Vec<String> = queries_import
        .rows
        .iter()
        .map(|row| row.source_url.clone())
        .collect();
    println!("loaded {} queries from {}", queries.len(), queries_path);

    let imports: Vec<_> = import_paths
        .iter()
        .map(|path| {
            let import = import_path(path).expect("failed to import");
            println!(
                "import: {} rows={} url_kind={:?} truncation_cap={:?}",
                import.summary.file_name,
                import.summary.row_count,
                import.url_kind,
                import.summary.truncation_cap
            );
            import
        })
        .collect();
    let import_refs: Vec<&_> = imports.iter().collect();

    let response = lookup_multi(&import_refs, &queries, &[], None);
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut sample_misses = Vec::new();

    for hit in &response.hits {
        *by_type.entry(hit.match_type.clone()).or_insert(0) += 1;
        if !hit.matched && sample_misses.len() < 15 {
            sample_misses.push((hit.query.clone(), hit.normalized_query.clone()));
        }
    }

    println!("\n== results ==");
    println!("total queries : {}", response.hits.len());
    for (match_type, count) in by_type {
        println!("{:14}: {}", match_type, count);
    }

    println!("\n== sample misses ==");
    for (raw, normalized) in &sample_misses {
        println!("raw:  {}", raw);
        println!("norm: {}", normalized);
        println!("---");
    }
}
