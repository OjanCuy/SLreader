//! Tauri commands for libraries. Thin wrappers: the logic and tests are in `libraries.rs`.
//! `pick_folder` shows the system folder dialog.
//! `async` keeps slow disks from freezing the window.

use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;
use tauri::{AppHandle, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::criteria::{self, ListKind};
use crate::decisions::{self, Decision, ExclusionReason, Screening, Summary};
use crate::export::{self, markdown, markdown::MarkdownExport};
use crate::extraction::Extraction;
use crate::highlights::{self, ExtractionTag, Highlight, HighlightCategory, HighlightEdit, NewHighlight};
use crate::libraries::{self, LibraryError, LibrarySummary};
use crate::metadata::{self, Metadata, PaperMetadata};
use crate::papers::{self, ImportOutcome, ImportReport, PaperListItem};
use crate::ref_links;
use crate::references;
use crate::settings::{self, CsvDelimiter, PageMode};
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

/// Shows the system file picker for one PDF ("Attach PDF"). `None` when the user cancels.
#[tauri::command(async)]
pub fn pick_pdf(app: AppHandle, window: WebviewWindow) -> Option<String> {
    app.dialog()
        .file()
        .set_parent(&window)
        .add_filter("PDF", &["pdf"])
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())
        .and_then(|path| path.to_str().map(String::from))
}

/// The bytes of a paper's PDF for the reader (FR-02.1), sent as binary, not JSON.
/// The file is read after the library lock is released, so a large PDF does not hold
/// up other commands.
#[tauri::command(async)]
pub fn read_paper_pdf(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
) -> Result<tauri::ipc::Response, LibraryError> {
    let path = {
        let current = lock(&current.0);
        let (conn, folder) = libraries::library_and_folder_for_reading(&current, session)?;
        papers::pdf_path(conn, folder, &paper_id)?
    };
    Ok(tauri::ipc::Response::new(papers::read_pdf(&path)?))
}

/// The user opened a password-protected paper with this password: extract its text
/// now (T-19). The password stays in memory until it has been tried once; it is never
/// saved, logged, or put in an error. Refused for a read-only library.
#[tauri::command(async)]
pub fn extract_locked_paper_text(
    current: State<'_, CurrentLibrary>,
    extraction: State<'_, Extraction>,
    session: u64,
    paper_id: String,
    password: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let (_, folder) = libraries::library_and_folder_for_writing(&current, session)?;
    extraction.extract_with_password(session, folder, &paper_id, password);
    Ok(())
}

/// "Attach PDF" for a paper whose file is missing.
#[tauri::command(async)]
pub fn attach_pdf(
    current: State<'_, CurrentLibrary>,
    extraction: State<'_, Extraction>,
    session: u64,
    paper_id: String,
    path: String,
) -> Result<ImportOutcome, LibraryError> {
    let current = lock(&current.0);
    let (conn, folder) = libraries::library_and_folder_for_writing(&current, session)?;
    let outcome = papers::attach_pdf(conn, folder, &paper_id, std::path::Path::new(&path))?;
    if outcome == ImportOutcome::Attached {
        extraction.work_on(session, folder);
    }
    Ok(outcome)
}

/// Deletes a paper and everything that belongs to it (FR-01.7). The UI has asked
/// the user to confirm (O-10).
#[tauri::command(async)]
pub fn delete_paper(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let (conn, folder) = libraries::library_and_folder_for_writing(&current, session)?;
    papers::delete_paper(conn, folder, &paper_id)
}

/// The page a paper was last read at (FR-02.3).
#[tauri::command(async)]
pub fn get_last_page(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
) -> Result<u32, LibraryError> {
    let current = lock(&current.0);
    let (conn, _) = libraries::library_and_folder_for_reading(&current, session)?;
    papers::last_page(conn, &paper_id)
}

/// Remembers the page being read. Refused for a read-only library.
#[tauri::command(async)]
pub fn save_last_page(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
    page: u32,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let (conn, _) = libraries::library_and_folder_for_writing(&current, session)?;
    papers::save_last_page(conn, &paper_id, page)
}

/// The library's highlight categories, in their order (FR-03.1, FR-08.6).
#[tauri::command(async)]
pub fn list_highlight_categories(
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<Vec<HighlightCategory>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    highlights::list_categories(conn)
}

/// The highlights of a paper, by page (FR-03.2).
#[tauri::command(async)]
pub fn list_highlights(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
) -> Result<Vec<Highlight>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    highlights::list_highlights(conn, &paper_id)
}

/// Saves a highlight the user just made (FR-03.1). Refused for a read-only library.
#[tauri::command(async)]
pub fn add_highlight(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
    highlight: NewHighlight,
) -> Result<Highlight, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    highlights::add_highlight(conn, &paper_id, &highlight)
}

/// The library's extraction tags, in their order (FR-08.2).
#[tauri::command(async)]
pub fn list_extraction_tags(
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<Vec<ExtractionTag>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    highlights::list_tags(conn)
}

/// Saves a highlight's category, tag, and note (FR-03.3). Refused for a read-only library.
#[tauri::command(async)]
pub fn update_highlight(
    current: State<'_, CurrentLibrary>,
    session: u64,
    highlight_id: String,
    edit: HighlightEdit,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    highlights::update_highlight(conn, &highlight_id, &edit)
}

/// Deletes a highlight the user has confirmed deleting (FR-03.3).
#[tauri::command(async)]
pub fn delete_highlight(
    current: State<'_, CurrentLibrary>,
    session: u64,
    highlight_id: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    highlights::delete_highlight(conn, &highlight_id)
}

/// The decision, exclusion reason, and assessment note of a paper (FR-04.1 to FR-04.4).
#[tauri::command(async)]
pub fn get_screening(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
) -> Result<Screening, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    decisions::screening(conn, &paper_id)
}

/// The user's decision (FR-04.1). Only the UI's decision control calls this: nothing
/// else may change a decision (FR-11.4).
#[tauri::command(async)]
pub fn set_decision(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
    decision: Decision,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    decisions::set_decision(conn, &paper_id, decision)
}

#[tauri::command(async)]
pub fn set_exclusion_reason(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
    reason_id: Option<String>,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    decisions::set_exclusion_reason(conn, &paper_id, reason_id.as_deref())
}

#[tauri::command(async)]
pub fn set_assessment_note(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
    note: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    decisions::set_assessment_note(conn, &paper_id, &note)
}

/// The counts for the summary bar (FR-04.5).
#[tauri::command(async)]
pub fn get_summary(current: State<'_, CurrentLibrary>, session: u64) -> Result<Summary, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    decisions::summary(conn)
}

/// The library's exclusion reasons, in their order (FR-08.1).
#[tauri::command(async)]
pub fn list_exclusion_reasons(
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<Vec<ExclusionReason>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    decisions::list_reasons(conn)
}

/// The metadata of a paper (FR-05.1).
#[tauri::command(async)]
pub fn get_metadata(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
) -> Result<PaperMetadata, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    metadata::get_metadata(conn, &paper_id)
}

/// Saves a paper's metadata; returns the other papers with the same reference number
/// (FR-05.4).
#[tauri::command(async)]
pub fn set_metadata(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
    metadata: Metadata,
) -> Result<Vec<String>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    metadata::set_metadata(conn, &paper_id, &metadata)
}

/// Replaces the reference list with the rows in `text` (FR-05.2). `replace` is the
/// user's confirmation; without it an existing list is kept.
#[tauri::command(async)]
pub fn import_references(
    current: State<'_, CurrentLibrary>,
    session: u64,
    text: String,
    replace: bool,
) -> Result<references::ImportResult, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    references::import(conn, &text, replace)
}

/// The reference list with the paper each reference is linked to (FR-05.3).
#[tauri::command(async)]
pub fn list_references(
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<Vec<references::ReferenceChoice>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    references::list(conn)
}

/// Links a paper to a reference (FR-05.3, FR-05.4). `replace` is the user's
/// confirmation to change metadata that is already filled in.
#[tauri::command(async)]
pub fn link_reference(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
    reference_id: String,
    replace: bool,
) -> Result<references::LinkResult, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    references::link(conn, &paper_id, &reference_id, replace)
}

#[tauri::command(async)]
pub fn unlink_reference(
    current: State<'_, CurrentLibrary>,
    session: u64,
    paper_id: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    references::unlink(conn, &paper_id)
}

/// The paper a `[#n]` link opens, if any (FR-05.5).
#[tauri::command(async)]
pub fn paper_for_ref_no(
    current: State<'_, CurrentLibrary>,
    session: u64,
    ref_no: i64,
) -> Result<Option<String>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    ref_links::paper_for(conn, ref_no)
}

/// How many notes still link to a reference number that was changed.
#[tauri::command(async)]
pub fn count_note_links(
    current: State<'_, CurrentLibrary>,
    session: u64,
    from: i64,
    to: i64,
) -> Result<usize, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    ref_links::count(conn, from, to)
}

/// Updates those links, after the user has confirmed it.
#[tauri::command(async)]
pub fn update_note_links(
    current: State<'_, CurrentLibrary>,
    session: u64,
    from: i64,
    to: i64,
) -> Result<usize, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    ref_links::update(conn, from, to)
}

/// Asks where to save, then writes the CSV that `build` makes of the open library.
/// Resolves to the file name, or `None` when the user cancels.
fn save_csv(
    app_handle: &AppHandle,
    window: &WebviewWindow,
    app: &AppDb,
    current: &CurrentLibrary,
    session: u64,
    default_name: &str,
    build: fn(&Connection, CsvDelimiter) -> Result<String, LibraryError>,
) -> Result<Option<String>, LibraryError> {
    // No lock is held while the user chooses.
    let Some(path) = app_handle
        .dialog()
        .file()
        .set_parent(window)
        .set_file_name(default_name)
        .add_filter("CSV", &["csv"])
        .blocking_save_file()
        .and_then(|file| file.into_path().ok())
    else {
        return Ok(None);
    };
    let delimiter = settings::csv_delimiter(&lock(&app.0))?;
    let csv = {
        let current = lock(&current.0);
        let conn = libraries::library_for_reading(&current, session)?;
        build(conn, delimiter)?
    };
    export::write_file(&path, &csv)?;
    Ok(Some(path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
}

/// "Decisions (CSV)" (FR-06.1). Also for a read-only library.
#[tauri::command(async)]
pub fn export_decisions(
    app_handle: AppHandle,
    window: WebviewWindow,
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<Option<String>, LibraryError> {
    save_csv(&app_handle, &window, &app, &current, session, "decisions.csv", export::decisions_csv)
}

/// "Extraction (CSV)" (FR-06.2). Also for a read-only library.
#[tauri::command(async)]
pub fn export_extraction(
    app_handle: AppHandle,
    window: WebviewWindow,
    app: State<'_, AppDb>,
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<Option<String>, LibraryError> {
    save_csv(&app_handle, &window, &app, &current, session, "extraction.csv", export::extraction_csv)
}

/// "Highlights and notes (Markdown)" (FR-06.3): asks for a folder and writes one file
/// per paper into a new folder inside it. `None` when the user cancels. Also for a
/// read-only library.
#[tauri::command(async)]
pub fn export_markdown(
    app_handle: AppHandle,
    window: WebviewWindow,
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<Option<MarkdownExport>, LibraryError> {
    // No lock is held while the user chooses.
    let Some(parent) = app_handle
        .dialog()
        .file()
        .set_parent(&window)
        .blocking_pick_folder()
        .and_then(|folder| folder.into_path().ok())
    else {
        return Ok(None);
    };
    let files = {
        let current = lock(&current.0);
        let conn = libraries::library_for_reading(&current, session)?;
        markdown::markdown_files(conn)?
    };
    markdown::write_folder(&parent, &files).map(Some)
}

/// A file chosen with "Choose CSV file".
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceFile {
    file_name: String,
    /// `None` when the file cannot be read as text.
    text: Option<String>,
}

/// Shows the system file picker for a reference list and reads the chosen file.
/// `None` when the user cancels.
#[tauri::command(async)]
pub fn pick_reference_file(app: AppHandle, window: WebviewWindow) -> Option<ReferenceFile> {
    let path = app
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter("CSV", &["csv", "tsv", "txt"])
        .blocking_pick_file()
        .and_then(|file| file.into_path().ok())?;
    Some(ReferenceFile {
        file_name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        text: references::read_file(&path),
    })
}

/// A list of criteria or tags, in its order (FR-08.1, FR-08.2, FR-08.4).
#[tauri::command(async)]
pub fn list_criteria(
    current: State<'_, CurrentLibrary>,
    session: u64,
    kind: ListKind,
) -> Result<Vec<criteria::Item>, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    criteria::list(conn, kind)
}

/// Adds a criterion or tag at the end of its list; returns its id.
#[tauri::command(async)]
pub fn add_criterion(
    current: State<'_, CurrentLibrary>,
    session: u64,
    kind: ListKind,
    code: String,
    label: String,
) -> Result<String, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    criteria::add(conn, kind, &code, &label)
}

/// Renames a criterion or tag.
#[tauri::command(async)]
pub fn update_criterion(
    current: State<'_, CurrentLibrary>,
    session: u64,
    kind: ListKind,
    id: String,
    code: String,
    label: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    criteria::update(conn, kind, &id, &code, &label)
}

/// How many papers or highlights use an item, for the delete confirmation.
#[tauri::command(async)]
pub fn count_criterion_uses(
    current: State<'_, CurrentLibrary>,
    session: u64,
    kind: ListKind,
    id: String,
) -> Result<u32, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    criteria::uses(conn, kind, &id)
}

/// Deletes an item the user has confirmed deleting (O-10).
#[tauri::command(async)]
pub fn delete_criterion(
    current: State<'_, CurrentLibrary>,
    session: u64,
    kind: ListKind,
    id: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    criteria::delete(conn, kind, &id)
}

/// Saves the order of a list.
#[tauri::command(async)]
pub fn reorder_criteria(
    current: State<'_, CurrentLibrary>,
    session: u64,
    kind: ListKind,
    ids: Vec<String>,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    criteria::reorder(conn, kind, &ids)
}

/// The research questions of the library, one per line (FR-08.4).
#[tauri::command(async)]
pub fn get_research_questions(
    current: State<'_, CurrentLibrary>,
    session: u64,
) -> Result<String, LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_reading(&current, session)?;
    criteria::research_questions(conn)
}

/// Saves the research questions.
#[tauri::command(async)]
pub fn set_research_questions(
    current: State<'_, CurrentLibrary>,
    session: u64,
    text: String,
) -> Result<(), LibraryError> {
    let current = lock(&current.0);
    let conn = libraries::library_for_writing(&current, session)?;
    criteria::set_research_questions(conn, &text)
}

/// The page mode, an app setting shared by all libraries (FR-02.2).
#[tauri::command(async)]
pub fn get_page_mode(app: State<'_, AppDb>) -> Result<PageMode, LibraryError> {
    Ok(settings::page_mode(&lock(&app.0))?)
}

#[tauri::command(async)]
pub fn set_page_mode(app: State<'_, AppDb>, mode: PageMode) -> Result<(), LibraryError> {
    Ok(settings::set_page_mode(&lock(&app.0), mode)?)
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
