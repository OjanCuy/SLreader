//! SQLite storage (PRD Section 7a).
//!
//! Two kinds of database:
//! - a library database (`library.sqlite`) inside every library folder: see `library.rs`;
//! - the app database (`app.sqlite`) in the app data folder: see `app.rs`.
//!
//! Both use the same rules, implemented here:
//! - The schema version is stored in `PRAGMA user_version`; migration N brings it to N.
//! - Migrations only go forward. A database from a newer SLReader is never changed.
//! - Before an existing database is migrated, a copy of it is saved next to it.
//! - A file that is some other program's SQLite database is never changed.
//! - `inspect` looks at an existing file read-only first, so a damaged or newer
//!   database is recognised without writing anything to it (T-07). The one exception
//!   is SQLite's own crash recovery: see `recover_hot_journal`.

pub mod app;
pub mod library;

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, ErrorCode, OpenFlags};
use serde::Serialize;

const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Everything that can go wrong while opening a database.
#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    /// Made by a newer SLReader. T-07 opens these read-only.
    NewerVersion { found: i64, supported: i64 },
    /// A SQLite file that does not belong to SLReader.
    NotSlreader,
    /// `create_library` was asked to use a folder that already holds a library.
    AlreadyExists(PathBuf),
    /// `open_library` found no library database in the folder.
    NotFound(PathBuf),
    InvalidName,
    /// The file is not a readable SLReader database: corrupt, truncated, or empty.
    /// The text is for logs only; the UI shows the message from the design brief.
    Damaged(String),
}

/// A problem the UI explains with a fixed message (Brief Section 7, S-01).
/// Sent to the UI as snake_case text, e.g. "newer_version".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryProblem {
    /// Opened read-only: "Created with a newer SLReader. Update the app to edit this library."
    NewerVersion,
    /// Not opened: "This library's database is damaged." + "Restore from backup".
    Damaged,
    /// "Folder not found." + "Locate folder" (FR-14.5).
    NotFound,
    /// Opened read-only: "A sync conflict was found. The library opened read-only."
    SyncConflict,
}

impl DbError {
    /// The fixed-message problem behind this error, if there is one.
    /// Other errors (disk full, file locked, ...) are not called damage.
    pub fn library_problem(&self) -> Option<LibraryProblem> {
        match self {
            DbError::Damaged(_) | DbError::NotSlreader => Some(LibraryProblem::Damaged),
            DbError::NewerVersion { .. } => Some(LibraryProblem::NewerVersion),
            DbError::NotFound(_) => Some(LibraryProblem::NotFound),
            _ => None,
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Sqlite(e) => write!(f, "database error: {e}"),
            DbError::Io(e) => write!(f, "file error: {e}"),
            DbError::NewerVersion { found, supported } => write!(
                f,
                "database version {found} is newer than this app supports ({supported})"
            ),
            DbError::NotSlreader => write!(f, "this file is not an SLReader database"),
            DbError::AlreadyExists(path) => write!(f, "a library already exists at {}", path.display()),
            DbError::NotFound(path) => write!(f, "no library found at {}", path.display()),
            DbError::InvalidName => write!(f, "the library name is empty"),
            DbError::Damaged(detail) => write!(f, "the database is damaged: {detail}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        if is_damage(&e) { DbError::Damaged(e.to_string()) } else { DbError::Sqlite(e) }
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError::Io(e)
    }
}

/// True for SQLite errors that mean the file itself is broken.
/// "Busy", "locked", "read-only", "disk full" and the like are not damage.
fn is_damage(e: &rusqlite::Error) -> bool {
    matches!(e.sqlite_error_code(), Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt))
}

/// The migrations of one kind of database.
pub struct Schema {
    /// Written to `PRAGMA application_id` so other programs' databases are recognised.
    pub application_id: i32,
    /// SQL of migration 1, 2, 3, ... Never edit a migration once it is released: add a new one.
    pub migrations: &'static [&'static str],
}

impl Schema {
    pub fn latest_version(&self) -> i64 {
        self.migrations.len() as i64
    }
}

/// Opens a database file and brings it to the latest schema version.
/// With `create` false, a missing file is an error instead of a new database.
pub(crate) fn open(path: &Path, schema: &Schema, create: bool) -> Result<Connection, DbError> {
    let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if create {
        flags |= OpenFlags::SQLITE_OPEN_CREATE;
    }
    let mut conn = Connection::open_with_flags(path, flags)?;
    // Foreign keys are off by default in SQLite and must be turned on for every connection.
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    migrate(&mut conn, path, schema)?;
    Ok(conn)
}

/// Opens an existing database read-only and checks it without writing anything.
/// Returns the read-only connection and the file's schema version, which may be
/// newer than this app knows. Errors: `NotSlreader`, `Damaged`, or a plain I/O or
/// SQLite error (e.g. the file is locked).
pub(crate) fn inspect(path: &Path, schema: &Schema) -> Result<(Connection, i64), DbError> {
    match inspect_read_only(path, schema) {
        Err(DbError::Sqlite(e)) if needs_crash_recovery(&e) => {
            recover_hot_journal(path, schema)?;
            inspect_read_only(path, schema)
        }
        other => other,
    }
}

/// SQLite refuses to read a database with a "hot" journal (left by a crash or power
/// loss in the middle of a save) through a read-only connection.
fn needs_crash_recovery(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(err, _)
        if err.extended_code == rusqlite::ffi::SQLITE_READONLY_ROLLBACK)
}

/// Lets SQLite finish its own crash recovery: the half-written save is undone and the
/// database returns to its last complete save. Nothing the user saved is lost; only
/// the unfinished change, which never counted as saved, is discarded.
///
/// This is the only write before the read-only check. It is skipped when the file
/// header shows another program's database, or something that is not SQLite at all.
fn recover_hot_journal(path: &Path, schema: &Schema) -> Result<(), DbError> {
    let mut header = [0u8; 100];
    if std::fs::File::open(path)?.read_exact(&mut header).is_err() {
        return Err(DbError::Damaged("file shorter than a SQLite header".into()));
    }
    if &header[..16] != b"SQLite format 3\0" {
        return Err(DbError::Damaged("not a SQLite file".into()));
    }
    // Bytes 68-71 hold PRAGMA application_id (big-endian). 0 means "not set yet",
    // e.g. a crash during the very first save of a new library.
    let application_id = i32::from_be_bytes([header[68], header[69], header[70], header[71]]);
    if application_id != 0 && application_id != schema.application_id {
        return Err(DbError::NotSlreader);
    }

    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // The first read takes a lock, and that is when SQLite rolls the journal back.
    conn.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get::<_, i64>(0))?;
    Ok(())
}

fn inspect_read_only(path: &Path, schema: &Schema) -> Result<(Connection, i64), DbError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // A second guard on top of the read-only flag.
    conn.pragma_update(None, "query_only", true)?;

    let application_id: i32 = conn.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if application_id != schema.application_id {
        let has_tables: bool =
            conn.query_row("SELECT count(*) > 0 FROM sqlite_schema", [], |row| row.get(0))?;
        // An empty file has no id yet; the caller decides whether that is acceptable.
        if application_id != 0 || has_tables {
            return Err(DbError::NotSlreader);
        }
    }

    // quick_check reads every page and returns "ok" or a list of problems.
    let mut stmt = conn.prepare("PRAGMA quick_check")?;
    let problems: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    if problems != ["ok"] {
        return Err(DbError::Damaged(problems.join("; ")));
    }
    Ok((conn, version))
}

fn migrate(conn: &mut Connection, path: &Path, schema: &Schema) -> Result<(), DbError> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let application_id: i32 = conn.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let has_tables: bool =
        conn.query_row("SELECT count(*) > 0 FROM sqlite_schema", [], |row| row.get(0))?;

    let is_new = version == 0 && application_id == 0 && !has_tables;
    if !is_new && application_id != schema.application_id {
        return Err(DbError::NotSlreader);
    }
    if version > schema.latest_version() {
        return Err(DbError::NewerVersion { found: version, supported: schema.latest_version() });
    }
    if version == schema.latest_version() {
        return Ok(());
    }
    if !is_new {
        backup_before_migration(conn, path, version)?;
    }

    for (index, sql) in schema.migrations.iter().enumerate().skip(version as usize) {
        // Each migration and its version number are saved together, or not at all.
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "application_id", schema.application_id)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

/// Saves a consistent copy, e.g. `library-v1-backup.sqlite`, before migrating.
/// Existing copies are never overwritten (rule 8 of CLAUDE.md).
fn backup_before_migration(conn: &Connection, path: &Path, version: i64) -> Result<PathBuf, DbError> {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("database");
    let folder = path.parent().unwrap_or(Path::new("."));
    let mut target = folder.join(format!("{stem}-v{version}-backup.sqlite"));
    let mut counter = 2;
    while target.exists() {
        target = folder.join(format!("{stem}-v{version}-backup-{counter}.sqlite"));
        counter += 1;
    }
    let target_text = target.to_str().ok_or_else(|| {
        DbError::Io(std::io::Error::other("backup path is not valid Unicode"))
    })?;
    conn.execute("VACUUM INTO ?1", [target_text])?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = "CREATE TABLE note (id INTEGER PRIMARY KEY, text TEXT NOT NULL);";
    const V2: &str = "ALTER TABLE note ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;";
    const BROKEN: &str = "ALTER TABLE note ADD COLUMN pinned INTEGER; THIS IS NOT SQL;";

    const ONE: Schema = Schema { application_id: 42, migrations: &[V1] };
    const TWO: Schema = Schema { application_id: 42, migrations: &[V1, V2] };
    const TWO_BROKEN: Schema = Schema { application_id: 42, migrations: &[V1, BROKEN] };

    fn version(conn: &Connection) -> i64 {
        conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap()
    }

    fn notes(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("SELECT text FROM note ORDER BY id").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn creates_a_new_database_at_the_latest_version() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join("x.sqlite"), &TWO, true).unwrap();
        assert_eq!(version(&conn), 2);
        let foreign_keys: bool = conn.pragma_query_value(None, "foreign_keys", |r| r.get(0)).unwrap();
        assert!(foreign_keys);
    }

    #[test]
    fn does_not_create_a_file_when_create_is_false() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sqlite");
        assert!(open(&path, &ONE, false).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn migrates_forward_keeping_data_and_saving_a_backup_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sqlite");
        let conn = open(&path, &ONE, true).unwrap();
        conn.execute("INSERT INTO note (text) VALUES ('keep me')", []).unwrap();
        drop(conn);

        let conn = open(&path, &TWO, false).unwrap();
        assert_eq!(version(&conn), 2);
        assert_eq!(notes(&conn), ["keep me"]);

        let backup = Connection::open(dir.path().join("x-v1-backup.sqlite")).unwrap();
        assert_eq!(version(&backup), 1);
        assert_eq!(notes(&backup), ["keep me"]);
    }

    #[test]
    fn never_overwrites_an_older_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sqlite");
        std::fs::write(dir.path().join("x-v1-backup.sqlite"), "older").unwrap();
        drop(open(&path, &ONE, true).unwrap());

        drop(open(&path, &TWO, false).unwrap());
        assert_eq!(std::fs::read_to_string(dir.path().join("x-v1-backup.sqlite")).unwrap(), "older");
        assert!(dir.path().join("x-v1-backup-2.sqlite").exists());
    }

    #[test]
    fn a_failed_migration_leaves_the_database_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sqlite");
        let conn = open(&path, &ONE, true).unwrap();
        conn.execute("INSERT INTO note (text) VALUES ('keep me')", []).unwrap();
        drop(conn);

        assert!(matches!(open(&path, &TWO_BROKEN, false), Err(DbError::Sqlite(_))));

        let conn = open(&path, &ONE, false).unwrap();
        assert_eq!(version(&conn), 1);
        assert_eq!(notes(&conn), ["keep me"]);
        // The half-applied column was rolled back too.
        assert!(conn.prepare("SELECT pinned FROM note").is_err());
    }

    #[test]
    fn refuses_a_newer_database_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sqlite");
        drop(open(&path, &TWO, true).unwrap());
        let before = std::fs::read(&path).unwrap();

        let result = open(&path, &ONE, false);
        assert!(matches!(result, Err(DbError::NewerVersion { found: 2, supported: 1 })));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!dir.path().join("x-v2-backup.sqlite").exists());
    }

    #[test]
    fn refuses_another_programs_database_without_changing_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.sqlite");
        let other = Connection::open(&path).unwrap();
        other.execute_batch("CREATE TABLE stuff (x); INSERT INTO stuff VALUES (1);").unwrap();
        drop(other);
        let before = std::fs::read(&path).unwrap();

        assert!(matches!(open(&path, &ONE, false), Err(DbError::NotSlreader)));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn rejects_a_file_that_is_not_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.sqlite");
        std::fs::write(&path, "this is not a database, just some text that is long enough").unwrap();
        assert!(matches!(open(&path, &ONE, false), Err(DbError::Damaged(_))));
    }

    fn sqlite_error(code: i32) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None)
    }

    #[test]
    fn only_broken_files_count_as_damage() {
        use rusqlite::ffi;
        assert!(is_damage(&sqlite_error(ffi::SQLITE_CORRUPT)));
        assert!(is_damage(&sqlite_error(ffi::SQLITE_NOTADB)));
        for code in [
            ffi::SQLITE_BUSY,
            ffi::SQLITE_LOCKED,
            ffi::SQLITE_FULL,
            ffi::SQLITE_READONLY,
            ffi::SQLITE_CANTOPEN,
            ffi::SQLITE_IOERR,
        ] {
            assert!(!is_damage(&sqlite_error(code)), "code {code} is not damage");
        }
    }

    #[test]
    fn problems_map_to_the_fixed_messages() {
        assert_eq!(DbError::Damaged("x".into()).library_problem(), Some(LibraryProblem::Damaged));
        assert_eq!(DbError::NotSlreader.library_problem(), Some(LibraryProblem::Damaged));
        assert_eq!(
            DbError::NewerVersion { found: 2, supported: 1 }.library_problem(),
            Some(LibraryProblem::NewerVersion)
        );
        assert_eq!(DbError::NotFound(PathBuf::new()).library_problem(), Some(LibraryProblem::NotFound));
        let busy = DbError::Sqlite(sqlite_error(rusqlite::ffi::SQLITE_BUSY));
        assert_eq!(busy.library_problem(), None);
        assert_eq!(serde_json::to_value(LibraryProblem::NewerVersion).unwrap(), "newer_version");
        assert_eq!(serde_json::to_value(LibraryProblem::NotFound).unwrap(), "not_found");
    }
}
