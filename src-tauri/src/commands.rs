//! Tauri commands for libraries. Thin wrappers: the logic and tests are in `libraries.rs`.
//! `pick_folder` shows the system folder dialog.
//! `async` keeps slow disks from freezing the window.

use std::sync::{Mutex, MutexGuard};

use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::extraction::Extraction;
use crate::libraries::{self, LibraryError, LibrarySummary};
use crate::papers::{self, ImportReport, PaperListItem};
use crate::{AppDb, CurrentLibrary};

/// A panic while holding a lock must not disable every later command.
/// SQLite rolls unfinished changes back, so the connection stays usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts extracting the text of a library that was just opened (FR-01.6).
fn extract_in_background(extraction: &Extraction, opened: &LibrarySummary) {
    if let Some(session) = opened.session {
        extraction.work_on(session, std::path::Path::new(&opened.folder_path));
    }
}

#[tauri::command(async)]
pub fn list_libraries(app: State<'_, AppDb>) -> Result<Vec<LibrarySummary>, LibraryError> {
    libraries::list(&lock(&app.0))
}

#[tauri::command(async)]
pub fn create_library(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    extraction: State<'_, Extraction>,
    folder_path: String,
    name: String,
) -> Result<LibrarySummary, LibraryError> {
    let app = lock(&app.0);
    let opened = libraries::create(&app, &mut lock(&current.0), &folder_path, &name)?;
    extract_in_background(&extraction, &opened);
    Ok(opened)
}

#[tauri::command(async)]
pub fn open_library_folder(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    extraction: State<'_, Extraction>,
    folder_path: String,
) -> Result<LibrarySummary, LibraryError> {
    let app = lock(&app.0);
    let opened = libraries::open_folder(&app, &mut lock(&current.0), &folder_path)?;
    extract_in_background(&extraction, &opened);
    Ok(opened)
}

#[tauri::command(async)]
pub fn open_library(
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    extraction: State<'_, Extraction>,
    id: i64,
) -> Result<LibrarySummary, LibraryError> {
    let app = lock(&app.0);
    let opened = libraries::open(&app, &mut lock(&current.0), id)?;
    extract_in_background(&extraction, &opened);
    Ok(opened)
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

/// Whether to warn that this folder is synced to the cloud (PRD Section 8).
#[tauri::command(async)]
pub fn is_cloud_folder(folder_path: String) -> bool {
    crate::sync::is_cloud_folder(std::path::Path::new(&folder_path))
}

/// Shows the system file picker for PDFs. Empty when the user cancels.
#[tauri::command(async)]
pub fn pick_pdfs(app: AppHandle, window: WebviewWindow) -> Vec<String> {
    app.dialog()
        .file()
        .set_parent(&window)
        .add_filter("PDF", &["pdf"])
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|file| file.into_path().ok())
        .filter_map(|path| path.to_str().map(String::from))
        .collect()
}

/// Copies PDFs into the open library (FR-01.2 to FR-01.5). `allow_identical` is the
/// user's "Add anyway" for a file whose content is already in the library.
#[tauri::command(async)]
pub fn import_pdfs(
    current: State<'_, CurrentLibrary>,
    extraction: State<'_, Extraction>,
    session: u64,
    paths: Vec<String>,
    allow_identical: bool,
) -> Result<ImportReport, LibraryError> {
    let current = lock(&current.0);
    let (conn, folder) = libraries::library_and_folder_for_writing(&current, session)?;
    let paths: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
    let report = papers::import_pdfs(conn, folder, &paths, allow_identical)?;
    if report.added > 0 {
        extraction.work_on(session, folder);
    }
    Ok(report)
}

#[tauri::command(async)]
pub fn list_papers(
    current: State<'_, CurrentLibrary>,
    extraction: State<'_, Extraction>,
    session: u64,
) -> Result<Vec<PaperListItem>, LibraryError> {
    let current = lock(&current.0);
    let (conn, folder) = libraries::library_and_folder_for_reading(&current, session)?;
    papers::list_papers(conn, folder, &|id| extraction.still_extracting(id))
}
