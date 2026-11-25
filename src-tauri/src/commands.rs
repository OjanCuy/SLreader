//! Tauri commands for libraries. Thin wrappers: the logic and tests are in `libraries.rs`.
//! `async` keeps slow disks from freezing the window.

use std::sync::{Mutex, MutexGuard};

use tauri::State;

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
