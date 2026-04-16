mod models;
mod parser;
mod query;
mod state;
mod commands;

use state::AppState;

#[doc(hidden)]
pub mod __dbg {
    pub use crate::parser::import_path;
    pub use crate::parser::normalizer::MatchForms;
    pub use crate::parser::normalizer::build_match_forms;
    pub use crate::parser::normalizer::normalize_url;
    pub use crate::query::search::lookup_multi;
    pub use crate::state::Import;
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::import_file,
            commands::list_imports,
            commands::load_lookup_file,
            commands::lookup_urls,
            commands::all_metrics,
            commands::remove_import,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
