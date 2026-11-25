//! Managing libraries (FR-14.1 to FR-14.3, FR-14.5): create, open, rename, remove from
//! the list, and point a missing one to its new folder.
//!
//! The app database only remembers folders (`library_registry`). Everything shown
//! about a library is read from the library folder itself.
//!
//! Isolation (FR-14.4): papers, summaries, exports, `[#n]` links, and AI features must
//! get their connection from `library_for_reading` or `library_for_writing`, never
//! from `ActiveLibrary` directly.
//! Nothing here ever deletes a library folder or anything in it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::db::library::{self, OpenedLibrary};
use crate::db::{DbError, LibraryProblem};

/// Subfolder of a library folder that holds the PDF copies (FR-01.1).
pub const PDF_FOLDER: &str = "pdfs";

/// The one library that is currently open (FR-14.2).
pub struct ActiveLibrary {
    pub id: i64,
    /// New for every opening. A request carrying an older number is refused, so a
    /// screen left over from another library can never read or change this one.
    pub session: u64,
    pub library: OpenedLibrary,
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// One entry of the libraries list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySummary {
    pub id: i64,
    pub folder_path: String,
    /// `None` when the library cannot be read (missing, damaged, ...).
    pub name: Option<String>,
    pub paper_count: Option<i64>,
    pub last_opened_at: Option<String>,
    pub problem: Option<LibraryProblem>,
    /// Set only on the result of opening a library; needed to work with its data.
    pub session: Option<u64>,
}

/// What went wrong, as a code the UI can act on. The UI has its own fixed texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    Damaged,
    NewerVersion,
    /// The chosen folder already holds a library.
    AlreadyExists,
    InvalidName,
    /// The id is not in the list.
    NotRegistered,
    /// The request was for a library that is no longer the open one.
    NotActive,
    /// Anything else: disk full, no permission, file locked, ...
    Other,
}

#[derive(Debug, Serialize)]
pub struct LibraryError {
    pub code: ErrorCode,
    /// Technical detail for troubleshooting. Not for showing to the user.
    pub detail: String,
}

impl LibraryError {
    fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        LibraryError { code, detail: detail.into() }
    }
}

impl From<DbError> for LibraryError {
    fn from(e: DbError) -> Self {
        let code = match e {
            DbError::NotFound(_) => ErrorCode::NotFound,
            DbError::Damaged(_) | DbError::NotSlreader => ErrorCode::Damaged,
            DbError::NewerVersion { .. } => ErrorCode::NewerVersion,
            DbError::AlreadyExists(_) => ErrorCode::AlreadyExists,
            DbError::InvalidName => ErrorCode::InvalidName,
            DbError::Sqlite(_) | DbError::Io(_) => ErrorCode::Other,
        };
        LibraryError::new(code, e.to_string())
    }
}

impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        LibraryError::new(ErrorCode::Other, e.to_string())
    }
}

impl From<std::io::Error> for LibraryError {
    fn from(e: std::io::Error) -> Self {
        LibraryError::new(ErrorCode::Other, e.to_string())
    }
}

type Result<T> = std::result::Result<T, LibraryError>;

/// All registered libraries, most recently opened first (FR-14.2).
/// Also records which folders are currently missing (FR-14.5).
pub fn list(app: &Connection) -> Result<Vec<LibrarySummary>> {
    let rows: Vec<(i64, String, Option<String>)> = app
        .prepare(
            "SELECT id, folder_path, last_opened_at FROM library_registry
             ORDER BY last_opened_at DESC NULLS LAST, id DESC",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;

    rows.into_iter()
        .map(|(id, folder_path, last_opened_at)| summarize(app, id, folder_path, last_opened_at))
        .collect()
}

fn summarize(
    app: &Connection,
    id: i64,
    folder_path: String,
    last_opened_at: Option<String>,
) -> Result<LibrarySummary> {
    // One broken library must not hide the others.
    let (name, paper_count, problem) = match library::peek_library(Path::new(&folder_path)) {
        Ok(peek) => (peek.name, peek.paper_count, peek.problem),
        Err(e) => (None, None, e.library_problem()),
    };
    set_missing(app, id, problem == Some(LibraryProblem::NotFound))?;
    Ok(LibrarySummary {
        id,
        folder_path,
        name,
        paper_count,
        last_opened_at,
        problem,
        session: None,
    })
}

/// Creates a new library in `folder`, registers it, and makes it active (FR-14.1, FR-01.1).
pub fn create(
    app: &Connection,
    active: &mut Option<ActiveLibrary>,
    folder: &str,
    name: &str,
) -> Result<LibrarySummary> {
    let folder = normalize(folder)?;
    if name.trim().is_empty() {
        return Err(DbError::InvalidName.into());
    }
    if !folder.is_dir() {
        return Err(LibraryError::new(ErrorCode::NotFound, "the folder does not exist"));
    }
    if folder.join(library::LIBRARY_DB_FILE).exists() {
        return Err(DbError::AlreadyExists(folder).into());
    }
    // Keeps whatever is already in an existing `pdfs` folder.
    std::fs::create_dir_all(folder.join(PDF_FOLDER))?;
    let conn = library::create_library(&folder, name)?;
    let id = register(app, &folder)?;
    activate(app, active, id, OpenedLibrary { conn, read_only: None })
}

/// "Open library": registers an existing library folder and makes it active (FR-14.3).
/// A folder that cannot be opened is not added to the list.
pub fn open_folder(
    app: &Connection,
    active: &mut Option<ActiveLibrary>,
    folder: &str,
) -> Result<LibrarySummary> {
    let folder = normalize(folder)?;
    let opened = library::open_library(&folder)?;
    let id = register(app, &folder)?;
    activate(app, active, id, opened)
}

/// Opens a library from the list. If it fails, the previously active one stays active.
pub fn open(
    app: &Connection,
    active: &mut Option<ActiveLibrary>,
    id: i64,
) -> Result<LibrarySummary> {
    let folder = folder_of(app, id)?;
    match library::open_library(&folder) {
        Ok(opened) => activate(app, active, id, opened),
        Err(e) => {
            if e.library_problem() == Some(LibraryProblem::NotFound) {
                set_missing(app, id, true)?;
            }
            Err(e.into())
        }
    }
}

/// Renames a library. The name is stored in the library folder (FR-14.3).
pub fn rename(
    app: &Connection,
    active: &mut Option<ActiveLibrary>,
    id: i64,
    name: &str,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::InvalidName.into());
    }
    // Reuse the open connection for the active library.
    let temporary;
    let target = match active {
        Some(current) if current.id == id => &current.library,
        _ => {
            temporary = library::open_library(&folder_of(app, id)?)?;
            &temporary
        }
    };
    if target.read_only.is_some() {
        return Err(LibraryError::new(ErrorCode::NewerVersion, "the library is read-only"));
    }
    target.conn.execute("UPDATE library SET name = ?1", [name])?;
    Ok(())
}

/// "Remove from list": forgets the folder. The folder and its files stay (FR-14.3).
pub fn remove(app: &Connection, active: &mut Option<ActiveLibrary>, id: i64) -> Result<()> {
    if app.execute("DELETE FROM library_registry WHERE id = ?1", [id])? == 0 {
        return Err(not_registered(id));
    }
    if active.as_ref().is_some_and(|current| current.id == id) {
        *active = None;
    }
    Ok(())
}

/// "Locate folder": points a list entry to the place the library moved to (FR-14.5).
/// The folder must hold a readable library. Nothing in either folder is changed.
pub fn locate(
    app: &Connection,
    active: &mut Option<ActiveLibrary>,
    id: i64,
    folder: &str,
) -> Result<()> {
    folder_of(app, id)?;
    let folder = normalize(folder)?;
    library::peek_library(&folder)?;
    let path = path_text(&folder)?;

    if find_registered(app, path)?.is_some_and(|other| other != id) {
        // The new place is already in the list, so only the old entry is dropped.
        return remove(app, active, id);
    }
    app.execute(
        "UPDATE library_registry SET folder_path = ?1, missing = 0 WHERE id = ?2",
        params![path, id],
    )?;
    if active.as_ref().is_some_and(|current| current.id == id) {
        *active = None;
    }
    Ok(())
}

/// Records the opening time and swaps in the new active library.
fn activate(
    app: &Connection,
    active: &mut Option<ActiveLibrary>,
    id: i64,
    library: OpenedLibrary,
) -> Result<LibrarySummary> {
    app.execute(
        "UPDATE library_registry
         SET last_opened_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), missing = 0
         WHERE id = ?1",
        [id],
    )?;
    let session = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    *active = Some(ActiveLibrary { id, session, library });
    let (folder_path, last_opened_at): (String, Option<String>) = app.query_row(
        "SELECT folder_path, last_opened_at FROM library_registry WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let summary = summarize(app, id, folder_path, last_opened_at)?;
    Ok(LibrarySummary { session: Some(session), ..summary })
}

/// The open library's database, for reading. `session` is the number the caller got
/// when it opened the library.
pub fn library_for_reading(active: &Option<ActiveLibrary>, session: u64) -> Result<&Connection> {
    Ok(&current(active, session)?.library.conn)
}

/// Like `library_for_reading`, but refuses a read-only library.
pub fn library_for_writing(active: &Option<ActiveLibrary>, session: u64) -> Result<&Connection> {
    let current = current(active, session)?;
    if current.library.read_only.is_some() {
        return Err(LibraryError::new(ErrorCode::NewerVersion, "the library is read-only"));
    }
    Ok(&current.library.conn)
}

fn current(active: &Option<ActiveLibrary>, session: u64) -> Result<&ActiveLibrary> {
    active
        .as_ref()
        .filter(|current| current.session == session)
        .ok_or_else(|| LibraryError::new(ErrorCode::NotActive, "this library is no longer open"))
}

/// Makes the path absolute with consistent separators, so one folder is listed once.
fn normalize(folder: &str) -> Result<PathBuf> {
    if folder.trim().is_empty() {
        return Err(LibraryError::new(ErrorCode::NotFound, "no folder was given"));
    }
    // `components` drops a trailing separator.
    Ok(std::path::absolute(folder)?.components().collect())
}

/// Adds the folder to the list, or returns the id it already has.
fn register(app: &Connection, folder: &Path) -> Result<i64> {
    let path = path_text(folder)?;
    if let Some(id) = find_registered(app, path)? {
        return Ok(id);
    }
    Ok(app.query_row(
        "INSERT INTO library_registry (folder_path) VALUES (?1) RETURNING id",
        [path],
        |r| r.get(0),
    )?)
}

fn path_text(folder: &Path) -> Result<&str> {
    folder
        .to_str()
        .ok_or_else(|| LibraryError::new(ErrorCode::Other, "the folder path is not valid Unicode"))
}

fn find_registered(app: &Connection, path: &str) -> Result<Option<i64>> {
    let rows: Vec<(i64, String)> = app
        .prepare("SELECT id, folder_path FROM library_registry")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows.iter().find(|(_, known)| same_path(known, path)).map(|(id, _)| *id))
}

/// Windows paths ignore upper and lower case.
fn same_path(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.to_lowercase() == b.to_lowercase() } else { a == b }
}

fn folder_of(app: &Connection, id: i64) -> Result<PathBuf> {
    app.query_row("SELECT folder_path FROM library_registry WHERE id = ?1", [id], |r| {
        r.get::<_, String>(0)
    })
    .optional()?
    .map(PathBuf::from)
    .ok_or_else(|| not_registered(id))
}

fn not_registered(id: i64) -> LibraryError {
    LibraryError::new(ErrorCode::NotRegistered, format!("no library with id {id}"))
}

fn set_missing(app: &Connection, id: i64, missing: bool) -> Result<()> {
    app.execute(
        "UPDATE library_registry SET missing = ?1 WHERE id = ?2",
        params![missing, id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::app::open_app_db;
    use crate::db::library::{LIBRARY_DB_FILE, LIBRARY_SCHEMA};

    /// An app database plus an empty folder to put libraries in.
    struct Setup {
        root: tempfile::TempDir,
        app: Connection,
        active: Option<ActiveLibrary>,
    }

    fn setup() -> Setup {
        let root = tempfile::tempdir().unwrap();
        let app = open_app_db(&root.path().join("appdata")).unwrap();
        Setup { root, app, active: None }
    }

    impl Setup {
        fn folder(&self, name: &str) -> String {
            let path = self.root.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            path.to_str().unwrap().to_string()
        }

        fn create(&mut self, folder: &str, name: &str) -> Result<LibrarySummary> {
            create(&self.app, &mut self.active, folder, name)
        }

        fn list(&self) -> Vec<LibrarySummary> {
            list(&self.app).unwrap()
        }

        fn active_id(&self) -> Option<i64> {
            self.active.as_ref().map(|a| a.id)
        }

        fn add_paper(&self) {
            let conn = &self.active.as_ref().unwrap().library.conn;
            conn.execute("INSERT INTO paper DEFAULT VALUES", []).unwrap();
        }

        fn missing_flag(&self, id: i64) -> bool {
            self.app
                .query_row("SELECT missing FROM library_registry WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        }
    }

    fn make_newer(folder: &str) {
        let conn = Connection::open(Path::new(folder).join(LIBRARY_DB_FILE)).unwrap();
        conn.pragma_update(None, "user_version", LIBRARY_SCHEMA.latest_version() + 1).unwrap();
    }

    #[test]
    fn create_makes_the_database_and_pdfs_folder_and_activates_it() {
        let mut s = setup();
        let folder = s.folder("review");
        let summary = s.create(&folder, "  Aquaculture  ").unwrap();

        assert!(Path::new(&folder).join(LIBRARY_DB_FILE).is_file());
        assert!(Path::new(&folder).join(PDF_FOLDER).is_dir());
        assert_eq!(summary.name.as_deref(), Some("Aquaculture"));
        assert_eq!(summary.paper_count, Some(0));
        assert_eq!(summary.problem, None);
        assert!(summary.last_opened_at.is_some());
        assert_eq!(s.active_id(), Some(summary.id));
        // The list is the same entry, without the session number.
        assert_eq!(s.list(), [LibrarySummary { session: None, ..summary }]);
    }

    #[test]
    fn create_refuses_a_folder_that_has_a_library() {
        let mut s = setup();
        let folder = s.folder("review");
        s.create(&folder, "First").unwrap();

        let error = s.create(&folder, "Second").unwrap_err();
        assert_eq!(error.code, ErrorCode::AlreadyExists);
        assert_eq!(s.list().len(), 1);
        assert_eq!(s.list()[0].name.as_deref(), Some("First"));
    }

    #[test]
    fn create_keeps_files_already_in_the_folder() {
        let mut s = setup();
        let folder = s.folder("review");
        let old_pdf = Path::new(&folder).join(PDF_FOLDER).join("old.pdf");
        std::fs::create_dir_all(old_pdf.parent().unwrap()).unwrap();
        std::fs::write(&old_pdf, "keep").unwrap();

        s.create(&folder, "Review").unwrap();
        assert_eq!(std::fs::read_to_string(old_pdf).unwrap(), "keep");
    }

    #[test]
    fn create_rejects_bad_input_without_touching_the_disk() {
        let mut s = setup();
        let folder = s.folder("review");
        assert_eq!(s.create(&folder, "  ").unwrap_err().code, ErrorCode::InvalidName);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);

        let gone = format!("{folder}-gone");
        assert_eq!(s.create(&gone, "Name").unwrap_err().code, ErrorCode::NotFound);
        assert!(!Path::new(&gone).exists());
        assert_eq!(s.create("", "Name").unwrap_err().code, ErrorCode::NotFound);
        assert!(s.list().is_empty());
    }

    #[test]
    fn removing_from_the_list_keeps_the_folder_and_all_data() {
        // FR-14.3 and its acceptance criterion: reopen the folder and everything is back.
        let mut s = setup();
        let folder = s.folder("review");
        let id = s.create(&folder, "Review").unwrap().id;
        s.add_paper();

        remove(&s.app, &mut s.active, id).unwrap();
        assert!(s.list().is_empty());
        assert_eq!(s.active_id(), None);
        assert!(Path::new(&folder).join(LIBRARY_DB_FILE).is_file());
        assert!(Path::new(&folder).join(PDF_FOLDER).is_dir());

        let again = open_folder(&s.app, &mut s.active, &folder).unwrap();
        assert_eq!(again.name.as_deref(), Some("Review"));
        assert_eq!(again.paper_count, Some(1));
    }

    #[test]
    fn removing_another_library_keeps_the_active_one() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let a = s.create(&a, "A").unwrap().id;
        let b = s.create(&b, "B").unwrap().id;

        remove(&s.app, &mut s.active, a).unwrap();
        assert_eq!(s.active_id(), Some(b));
        assert_eq!(remove(&s.app, &mut s.active, a).unwrap_err().code, ErrorCode::NotRegistered);
    }

    #[test]
    fn the_same_folder_is_listed_once() {
        let mut s = setup();
        let folder = s.folder("review");
        let id = s.create(&folder, "Review").unwrap().id;

        let with_slash = format!("{folder}{}", std::path::MAIN_SEPARATOR);
        assert_eq!(open_folder(&s.app, &mut s.active, &with_slash).unwrap().id, id);
        if cfg!(windows) {
            let upper = folder.to_uppercase();
            assert_eq!(open_folder(&s.app, &mut s.active, &upper).unwrap().id, id);
        }
        assert_eq!(s.list().len(), 1);
    }

    #[test]
    fn open_folder_does_not_register_a_folder_without_a_library() {
        let mut s = setup();
        let empty = s.folder("empty");
        let error = open_folder(&s.app, &mut s.active, &empty).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);

        let broken = s.folder("broken");
        std::fs::write(Path::new(&broken).join(LIBRARY_DB_FILE), "not a database").unwrap();
        let error = open_folder(&s.app, &mut s.active, &broken).unwrap_err();
        assert_eq!(error.code, ErrorCode::Damaged);
        assert!(s.list().is_empty());
        assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
    }

    #[test]
    fn switching_keeps_only_one_library_active() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let a = s.create(&a, "A").unwrap().id;
        s.add_paper();
        let b = s.create(&b, "B").unwrap().id;
        assert_eq!(s.active_id(), Some(b));

        let opened = open(&s.app, &mut s.active, a).unwrap();
        assert_eq!(s.active_id(), Some(a));
        assert_eq!(opened.paper_count, Some(1));
    }

    #[test]
    fn lists_the_most_recently_opened_first() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        s.create(&a, "A").unwrap();
        s.create(&b, "B").unwrap();
        // Set the times by hand: both libraries may be created within the same millisecond.
        s.app
            .execute_batch(
                "UPDATE library_registry SET last_opened_at =
                   CASE WHEN folder_path LIKE '%a' THEN '2026-02-01' ELSE '2026-01-01' END",
            )
            .unwrap();

        let names: Vec<_> = s.list().into_iter().map(|l| l.name.unwrap()).collect();
        assert_eq!(names, ["A", "B"]);
    }

    #[test]
    fn a_missing_folder_is_flagged_and_does_not_hide_the_others() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let a_id = s.create(&a, "A").unwrap().id;
        let b_id = s.create(&b, "B").unwrap().id;
        s.active = None;
        std::fs::rename(&a, format!("{a}-moved")).unwrap();

        let libraries = s.list();
        assert_eq!(libraries.len(), 2);
        let missing = libraries.iter().find(|l| l.id == a_id).unwrap();
        assert_eq!(missing.problem, Some(LibraryProblem::NotFound));
        assert_eq!(missing.name, None);
        assert!(s.missing_flag(a_id));
        assert!(!s.missing_flag(b_id));

        // The folder comes back (e.g. a drive is plugged in again).
        std::fs::rename(format!("{a}-moved"), &a).unwrap();
        s.list();
        assert!(!s.missing_flag(a_id));
    }

    #[test]
    fn a_failed_open_keeps_the_previous_library_active() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let a_id = s.create(&a, "A").unwrap().id;
        let b_id = s.create(&b, "B").unwrap().id;
        std::fs::rename(&a, format!("{a}-moved")).unwrap();

        assert_eq!(open(&s.app, &mut s.active, a_id).unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(s.active_id(), Some(b_id));
        assert!(s.missing_flag(a_id));
        // Opening never recreates the folder.
        assert!(!Path::new(&a).exists());
        assert_eq!(open(&s.app, &mut s.active, 999).unwrap_err().code, ErrorCode::NotRegistered);
    }

    #[test]
    fn renames_the_active_and_an_inactive_library() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let a = s.create(&a, "A").unwrap().id;
        let b = s.create(&b, "B").unwrap().id;

        rename(&s.app, &mut s.active, a, " Alpha ").unwrap();
        rename(&s.app, &mut s.active, b, "Beta").unwrap();
        assert_eq!(s.active_id(), Some(b));

        let name = |id| s.list().into_iter().find(|l| l.id == id).unwrap().name.unwrap();
        assert_eq!((name(a).as_str(), name(b).as_str()), ("Alpha", "Beta"));
    }

    #[test]
    fn rename_rejects_an_empty_name() {
        let mut s = setup();
        let folder = s.folder("a");
        let id = s.create(&folder, "A").unwrap().id;
        let error = rename(&s.app, &mut s.active, id, "   ").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidName);
        assert_eq!(s.list()[0].name.as_deref(), Some("A"));
    }

    #[test]
    fn a_newer_library_opens_read_only_and_cannot_be_renamed() {
        let mut s = setup();
        let folder = s.folder("a");
        let id = s.create(&folder, "A").unwrap().id;
        s.active = None;
        make_newer(&folder);

        // Not active.
        let error = rename(&s.app, &mut s.active, id, "New").unwrap_err();
        assert_eq!(error.code, ErrorCode::NewerVersion);
        // Active.
        let opened = open(&s.app, &mut s.active, id).unwrap();
        assert_eq!(opened.problem, Some(LibraryProblem::NewerVersion));
        let error = rename(&s.app, &mut s.active, id, "New").unwrap_err();
        assert_eq!(error.code, ErrorCode::NewerVersion);
        assert_eq!(s.list()[0].name.as_deref(), Some("A"));
    }

    #[test]
    fn locate_points_a_moved_library_to_its_new_folder() {
        let mut s = setup();
        let folder = s.folder("a");
        let id = s.create(&folder, "A").unwrap().id;
        s.add_paper();
        s.active = None;
        let moved = format!("{folder}-moved");
        std::fs::rename(&folder, &moved).unwrap();
        s.list();

        locate(&s.app, &mut s.active, id, &moved).unwrap();
        assert!(!s.missing_flag(id));
        let libraries = s.list();
        assert_eq!(libraries.len(), 1);
        assert_eq!(libraries[0].id, id);
        assert_eq!(libraries[0].folder_path, moved);
        assert_eq!(libraries[0].problem, None);
        assert_eq!(libraries[0].paper_count, Some(1));
    }

    #[test]
    fn locate_rejects_a_folder_without_a_readable_library() {
        let mut s = setup();
        let folder = s.folder("a");
        let id = s.create(&folder, "A").unwrap().id;
        let empty = s.folder("empty");
        let broken = s.folder("broken");
        std::fs::write(Path::new(&broken).join(LIBRARY_DB_FILE), "not a database").unwrap();

        let error = locate(&s.app, &mut s.active, id, &empty).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        let error = locate(&s.app, &mut s.active, id, &broken).unwrap_err();
        assert_eq!(error.code, ErrorCode::Damaged);
        assert_eq!(s.list()[0].folder_path, folder);
        assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
    }

    #[test]
    fn locating_a_folder_that_is_already_listed_drops_only_the_old_entry() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let a_id = s.create(&a, "A").unwrap().id;
        let b_id = s.create(&b, "B").unwrap().id;
        let moved = format!("{a}-moved");
        std::fs::rename(&a, &moved).unwrap();

        locate(&s.app, &mut s.active, a_id, &b).unwrap();
        let ids: Vec<i64> = s.list().iter().map(|l| l.id).collect();
        assert_eq!(ids, [b_id]);
        assert!(Path::new(&moved).join(LIBRARY_DB_FILE).is_file());
        let error = locate(&s.app, &mut s.active, a_id, &b).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotRegistered);
    }

    // ---- T-10: data is isolated per active library (FR-14.4) ----

    fn titles(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("SELECT title FROM paper ORDER BY title").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(std::result::Result::unwrap).collect()
    }

    fn add(s: &Setup, session: u64, title: &str) {
        let conn = library_for_writing(&s.active, session).unwrap();
        conn.execute("INSERT INTO paper (title) VALUES (?1)", [title]).unwrap();
        conn.execute(
            "INSERT INTO exclusion_reason (code, label, sort_order) VALUES (?1, 'x', 0)",
            [title],
        )
        .unwrap();
    }

    #[test]
    fn each_library_shows_only_its_own_data() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let first = s.create(&a, "A").unwrap();
        add(&s, first.session.unwrap(), "from A");
        let second = s.create(&b, "B").unwrap();
        add(&s, second.session.unwrap(), "from B");

        let b_conn = library_for_reading(&s.active, second.session.unwrap()).unwrap();
        assert_eq!(titles(b_conn), ["from B"]);

        let again = open(&s.app, &mut s.active, first.id).unwrap();
        let a_conn = library_for_reading(&s.active, again.session.unwrap()).unwrap();
        assert_eq!(titles(a_conn), ["from A"]);
        let reasons: i64 = a_conn
            .query_row("SELECT count(*) FROM exclusion_reason", [], |r| r.get(0))
            .unwrap();
        assert_eq!(reasons, 1);
    }

    #[test]
    fn an_old_session_cannot_touch_the_library_that_replaced_it() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let first = s.create(&a, "A").unwrap();
        let old = first.session.unwrap();
        let second = s.create(&b, "B").unwrap();
        assert_ne!(second.session.unwrap(), old);

        let refused = |s: &Setup| {
            let read = library_for_reading(&s.active, old).err().unwrap().code;
            let write = library_for_writing(&s.active, old).err().unwrap().code;
            (read, write) == (ErrorCode::NotActive, ErrorCode::NotActive)
        };
        assert!(refused(&s));
        // Even reopening the first library does not revive the old number.
        open(&s.app, &mut s.active, first.id).unwrap();
        assert!(refused(&s));
        // Nor does having no library open.
        s.active = None;
        assert!(refused(&s));
    }

    #[test]
    fn a_failed_switch_keeps_the_current_session_working() {
        let mut s = setup();
        let (a, b) = (s.folder("a"), s.folder("b"));
        let a_id = s.create(&a, "A").unwrap().id;
        let session = s.create(&b, "B").unwrap().session.unwrap();
        std::fs::rename(&a, format!("{a}-moved")).unwrap();

        assert!(open(&s.app, &mut s.active, a_id).is_err());
        assert!(library_for_writing(&s.active, session).is_ok());
    }

    #[test]
    fn a_read_only_library_can_be_read_but_not_written() {
        let mut s = setup();
        let folder = s.folder("a");
        let id = s.create(&folder, "A").unwrap().id;
        s.active = None;
        make_newer(&folder);

        let session = open(&s.app, &mut s.active, id).unwrap().session.unwrap();
        assert!(library_for_reading(&s.active, session).is_ok());
        let error = library_for_writing(&s.active, session).err().unwrap();
        assert_eq!(error.code, ErrorCode::NewerVersion);
    }

    #[test]
    fn the_ui_receives_the_field_names_and_codes_it_expects() {
        let mut s = setup();
        let folder = s.folder("a");
        let json = serde_json::to_value(s.create(&folder, "A").unwrap()).unwrap();
        for field in ["id", "folderPath", "name", "paperCount", "lastOpenedAt", "problem", "session"] {
            assert!(json.get(field).is_some(), "{field} is missing");
        }
        let error = serde_json::to_value(s.create(&folder, "B").unwrap_err()).unwrap();
        assert_eq!(error["code"], "already_exists");
    }
}
