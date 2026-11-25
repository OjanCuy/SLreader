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

pub mod app;
pub mod library;

use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

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
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError::Io(e)
    }
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
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    migrate(&mut conn, path, schema)?;
    Ok(conn)
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
        assert!(matches!(open(&path, &ONE, false), Err(DbError::Sqlite(_))));
    }
}
