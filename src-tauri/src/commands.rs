//! Tauri commands for libraries. Thin wrappers: the logic and tests are in `libraries.rs`.
//! `pick_folder` shows the system folder dialog.
//! `async` keeps slow disks from freezing the window.

use std::sync::{Mutex, MutexGuard};

use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::libraries::{self, LibraryError, LibrarySummary};
use crate::{AppDb, CurrentLibrary};

/// A panic while holding a lock must not disable every later command.
/// SQLite rolls unfinished changes back, so the connection stays usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command(async)]
pub fn list_libraries(app: State<'_, AppDb>) -> Result<Vec<LibrarySummary>, LibraryError> {
    libraries::list(&lock(&app.0))
}

#[tauri::command(async)]
pub fn create_library(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    folder_path: String,
    name: String,
) -> Result<LibrarySummary, LibraryError> {
    let app = lock(&app.0);
    libraries::create(&app, &mut lock(&current.0), &folder_path, &name)
}

#[tauri::command(async)]
pub fn open_library_folder(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    folder_path: String,
) -> Result<LibrarySummary, LibraryError> {
    let app = lock(&app.0);
    libraries::open_folder(&app, &mut lock(&current.0), &folder_path)
}

#[tauri::command(async)]
pub fn open_library(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    id: i64,
) -> Result<LibrarySummary, LibraryError> {
    let app = lock(&app.0);
    libraries::open(&app, &mut lock(&current.0), id)
}

#[tauri::command(async)]
pub fn rename_library(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    id: i64,
    name: String,
) -> Result<(), LibraryError> {
    let app = lock(&app.0);
    libraries::rename(&app, &mut lock(&current.0), id, &name)
}

#[tauri::command(async)]
pub fn remove_library(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    id: i64,
) -> Result<(), LibraryError> {
    let app = lock(&app.0);
    libraries::remove(&app, &mut lock(&current.0), id)
}

#[tauri::command(async)]
pub fn locate_library(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    id: i64,
    folder_path: String,
) -> Result<(), LibraryError> {
    let app = lock(&app.0);
    libraries::locate(&app, &mut lock(&current.0), id, &folder_path)
}

/// Shows the system folder picker. `None` when the user cancels.
/// The dialog is opened from Rust, so the UI needs no extra permission.
/// Must stay `async`: the blocking call would freeze the window on the main thread.
#[tauri::command(async)]
pub fn pick_folder(app: AppHandle, window: WebviewWindow) -> Option<String> {
    app.dialog()
        .file()
        .set_parent(&window)
        .blocking_pick_folder()
        .and_then(|folder| folder.into_path().ok())
        .and_then(|path| path.to_str().map(String::from))
}
