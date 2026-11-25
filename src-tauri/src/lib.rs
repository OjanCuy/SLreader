mod commands;
pub mod db;
pub mod libraries;

use std::sync::Mutex;

use serde::Serialize;
use tauri::Manager;

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
