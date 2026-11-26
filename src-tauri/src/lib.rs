pub mod backup;
mod commands;
pub mod criteria;
pub mod db;
pub mod decisions;
pub mod export;
pub mod extraction;
pub mod highlights;
pub mod libraries;
pub mod metadata;
pub mod papers;
pub mod prototype;
pub mod ref_links;
pub mod references;
pub mod settings;
pub mod sync;
pub mod text;

use std::sync::Mutex;

use serde::Serialize;
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager};

/// The open app database (registry, AI providers, settings), shared by all commands.
pub struct AppDb(pub Mutex<rusqlite::Connection>);

/// The library the user is working in, if any. Always locked after `AppDb`.
#[derive(Default)]
pub struct CurrentLibrary(pub Mutex<Option<libraries::ActiveLibrary>>);

/// Basic facts about the running app, sent to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

/// Builds `AppInfo` from the values in Cargo.toml.
/// Kept separate from the command so it can be tested without a running app.
pub fn read_app_info() -> AppInfo {
    AppInfo {
        name: "SLReader".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Tauri command: the UI calls this as `invoke("app_info")`.
#[tauri::command]
fn app_info() -> AppInfo {
    read_app_info()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // %APPDATA%\io.github.ojancuy.slreader on Windows.
            let data_dir = app.path().app_data_dir()?;
            let conn = db::app::open_app_db(&data_dir)?;
            app.manage(AppDb(Mutex::new(conn)));
            app.manage(CurrentLibrary::default());

            // Text extraction in the background (FR-01.6). pdfium.dll ships with the app.
            let dll = app.path().resolve("pdfium/pdfium.dll", BaseDirectory::Resource)?;
            let handle = app.handle().clone();
            app.manage(extraction::Extraction::start(dll, move |session| {
                // The UI reloads the paper list of that library.
                let _ = handle.emit("paper-text-changed", session);
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            commands::list_libraries,
            commands::create_library,
            commands::open_library_folder,
            commands::open_library,
            commands::rename_library,
            commands::remove_library,
            commands::locate_library,
            commands::pick_folder,
            commands::is_cloud_folder,
            commands::pick_pdfs,
            commands::import_pdfs,
            commands::list_papers,
            commands::pick_pdf,
            commands::read_paper_pdf,
            commands::attach_pdf,
            commands::extract_locked_paper_text,
            commands::list_highlight_categories,
            commands::list_highlights,
            commands::add_highlight,
            commands::update_highlight,
            commands::delete_highlight,
            commands::list_extraction_tags,
            commands::get_screening,
            commands::set_decision,
            commands::set_exclusion_reason,
            commands::set_assessment_note,
            commands::list_exclusion_reasons,
            commands::get_summary,
            commands::get_metadata,
            commands::set_metadata,
            commands::import_references,
            commands::pick_reference_file,
            commands::list_references,
            commands::link_reference,
            commands::unlink_reference,
            commands::paper_for_ref_no,
            commands::count_note_links,
            commands::update_note_links,
            commands::export_decisions,
            commands::export_extraction,
            commands::export_markdown,
            commands::export_backup,
            commands::pick_backup,
            commands::check_restore,
            commands::restore_backup,
            commands::create_library_from_backup,
            commands::list_criteria,
            commands::add_criterion,
            commands::add_template_items,
            commands::update_criterion,
            commands::count_criterion_uses,
            commands::delete_criterion,
            commands::reorder_criteria,
            commands::get_research_questions,
            commands::set_research_questions,
            commands::delete_paper,
            commands::get_last_page,
            commands::save_last_page,
            commands::get_page_mode,
            commands::set_page_mode,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_has_name_and_cargo_version() {
        let info = read_app_info();
        assert_eq!(info.name, "SLReader");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn app_info_serializes_with_the_field_names_the_ui_expects() {
        let json = serde_json::to_value(read_app_info()).unwrap();
        assert!(json.get("name").is_some());
        assert!(json.get("version").is_some());
    }
}
