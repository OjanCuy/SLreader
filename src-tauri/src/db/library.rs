//! The library database: one `library.sqlite` per library folder.
//! It holds the library's name, research questions, papers, and everything about them,
//! so copying the folder to another computer carries all of it (FR-07.4).

use std::path::Path;

use rusqlite::Connection;

use super::{DbError, LibraryProblem, Schema};

pub const LIBRARY_DB_FILE: &str = "library.sqlite";

/// Most papers one library can hold (FR-01.4). The schema enforces the same number.
pub const MAX_PAPERS: i64 = 250;

pub const LIBRARY_SCHEMA: Schema = Schema {
    // "SLRL" in ASCII.
    application_id: 0x534C_524C,
    migrations: &[include_str!("migrations/library/001_initial.sql")],
};

/// Creates `library.sqlite` in an existing folder. Fails if the folder already has one.
pub fn create_library(folder: &Path, name: &str) -> Result<Connection, DbError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::InvalidName);
    }
    let path = folder.join(LIBRARY_DB_FILE);
    if path.exists() {
        return Err(DbError::AlreadyExists(path));
    }
    // Claim the file name first: this fails if the file appeared in the meantime,
    // so the clean-up below can only ever remove a file made right here.
    std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
    let created = super::open(&path, &LIBRARY_SCHEMA, false).and_then(|conn| {
        conn.execute("INSERT INTO library (name) VALUES (?1)", [name])?;
        Ok(conn)
    });
    if created.is_err() {
        // A half-made library (disk full, ...) would block every later attempt.
        // Best effort: the original error matters more than a failed clean-up.
        let _ = std::fs::remove_file(&path);
    }
    created
}

/// What the libraries list shows about a folder (FR-14.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPeek {
    /// `None` when a newer SLReader stores it in a way this version cannot read.
    pub name: Option<String>,
    pub paper_count: Option<i64>,
    pub problem: Option<LibraryProblem>,
}

/// Reads a library's name and paper count without opening it for editing:
/// nothing is migrated, and a missing or damaged library is reported, not repaired.
pub fn peek_library(folder: &Path) -> Result<LibraryPeek, DbError> {
    let (conn, version) = check_library(folder)?;
    Ok(LibraryPeek {
        name: conn.query_row("SELECT name FROM library", [], |r| r.get(0)).ok(),
        paper_count: conn.query_row("SELECT count(*) FROM paper", [], |r| r.get(0)).ok(),
        problem: (version > LIBRARY_SCHEMA.latest_version()).then_some(LibraryProblem::NewerVersion),
    })
}

/// The read-only checks shared by opening and peeking. Returns the read-only
/// connection and the schema version.
fn check_library(folder: &Path) -> Result<(Connection, i64), DbError> {
    let path = folder.join(LIBRARY_DB_FILE);
    if !path.is_file() {
        return Err(DbError::NotFound(path));
    }
    let (conn, version) = super::inspect(&path, &LIBRARY_SCHEMA)?;
    let rows: Result<i64, _> = conn.query_row("SELECT count(*) FROM library", [], |r| r.get(0));
    match rows {
        Ok(1) => Ok((conn, version)),
        Ok(n) => Err(DbError::Damaged(format!("{n} rows in the library table"))),
        Err(e) => Err(DbError::Damaged(e.to_string())),
    }
}

/// An open library. A read-only one refuses every write at the SQLite level.
pub struct OpenedLibrary {
    pub conn: Connection,
    /// Why it is read-only, or `None` when it can be edited.
    pub read_only: Option<LibraryProblem>,
}

/// Opens the library in `folder` (PRD Section 8):
/// - made by an older SLReader: migrated forward, after a backup copy;
/// - made by a newer SLReader: opened read-only, never migrated down;
/// - damaged, or not an SLReader library: `DbError::Damaged` / `NotSlreader`, file untouched;
/// - left half-saved by a crash: SQLite first rolls back to the last complete save.
///
/// Never creates a database: a folder without one is `DbError::NotFound`.
pub fn open_library(folder: &Path) -> Result<OpenedLibrary, DbError> {
    // Look first, read-only: nothing is written to a damaged or newer file.
    let (conn, version) = check_library(folder)?;
    if version > LIBRARY_SCHEMA.latest_version() {
        return Ok(OpenedLibrary { conn, read_only: Some(LibraryProblem::NewerVersion) });
    }
    drop(conn);

    let conn = super::open(&folder.join(LIBRARY_DB_FILE), &LIBRARY_SCHEMA, false)?;
    Ok(OpenedLibrary { conn, read_only: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn new_library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "Aquaculture review").unwrap();
        (dir, conn)
    }

    fn add_paper(conn: &Connection) -> String {
        conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap()
    }

    fn add_reason(conn: &Connection, code: &str) -> String {
        conn.query_row(
            "INSERT INTO exclusion_reason (code, label, sort_order) VALUES (?1, 'x', 0) RETURNING id",
            [code],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn name_and_research_questions_live_in_the_library_folder() {
        let (dir, conn) = new_library();
        conn.execute("UPDATE library SET research_questions = 'RQ1\nRQ2'", []).unwrap();
        drop(conn);

        let conn = open_library(dir.path()).unwrap().conn;
        let (name, rqs): (String, String) = conn
            .query_row("SELECT name, research_questions FROM library", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(name, "Aquaculture review");
        assert_eq!(rqs, "RQ1\nRQ2");
    }

    #[test]
    fn a_library_has_exactly_one_library_row() {
        let (_dir, conn) = new_library();
        assert!(conn.execute("INSERT INTO library (name) VALUES ('second')", []).is_err());
    }

    #[test]
    fn rejects_an_empty_name_without_creating_a_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(create_library(dir.path(), "   "), Err(DbError::InvalidName)));
        assert!(!dir.path().join(LIBRARY_DB_FILE).exists());
    }

    #[test]
    fn never_replaces_an_existing_library() {
        let (dir, conn) = new_library();
        add_paper(&conn);
        drop(conn);

        assert!(matches!(create_library(dir.path(), "Other"), Err(DbError::AlreadyExists(_))));
        assert_eq!(count(&open_library(dir.path()).unwrap().conn, "paper"), 1);
    }

    #[test]
    fn opening_a_folder_without_a_library_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(open_library(dir.path()), Err(DbError::NotFound(_))));
        assert!(!dir.path().join(LIBRARY_DB_FILE).exists());
    }

    #[test]
    fn a_new_paper_is_undecided_with_pending_text() {
        let (_dir, conn) = new_library();
        let id = add_paper(&conn);
        let (decision, status, page): (String, String, i64) = conn
            .query_row(
                "SELECT decision, text_status, last_page FROM paper WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((decision.as_str(), status.as_str(), page), ("undecided", "pending", 1));
        assert_eq!(id.len(), 32);
    }

    #[test]
    fn rejects_unknown_status_values() {
        let (_dir, conn) = new_library();
        let id = add_paper(&conn);
        assert!(conn.execute("UPDATE paper SET decision = 'maybe' WHERE id = ?1", [&id]).is_err());
        assert!(conn.execute("UPDATE paper SET text_status = 'done' WHERE id = ?1", [&id]).is_err());
        assert!(conn
            .execute(
                "INSERT INTO highlight (paper_id, page, color, text, rects) VALUES (?1, 1, 'purple', 't', '[]')",
                [&id],
            )
            .is_err());
    }

    #[test]
    fn stops_at_250_papers() {
        let (_dir, conn) = new_library();
        for _ in 0..MAX_PAPERS {
            add_paper(&conn);
        }
        let extra = conn.execute("INSERT INTO paper DEFAULT VALUES", []);
        assert!(extra.unwrap_err().to_string().contains("250"));
        assert_eq!(count(&conn, "paper"), MAX_PAPERS);
    }

    #[test]
    fn one_reference_links_to_one_paper() {
        let (_dir, conn) = new_library();
        let reference: String = conn
            .query_row("INSERT INTO reference (no, title) VALUES (46, 'T') RETURNING id", [], |r| r.get(0))
            .unwrap();
        let (a, b) = (add_paper(&conn), add_paper(&conn));
        let link = |paper: &str| {
            conn.execute("UPDATE paper SET reference_id = ?1 WHERE id = ?2", [&reference, paper])
        };
        link(&a).unwrap();
        assert!(link(&b).is_err());
    }

    #[test]
    fn replacing_the_reference_list_keeps_paper_metadata() {
        let (_dir, conn) = new_library();
        let reference: String = conn
            .query_row("INSERT INTO reference (no, title) VALUES (46, 'T') RETURNING id", [], |r| r.get(0))
            .unwrap();
        let paper = add_paper(&conn);
        conn.execute(
            "UPDATE paper SET reference_id = ?1, ref_no = 46, title = 'T' WHERE id = ?2",
            [&reference, &paper],
        )
        .unwrap();

        conn.execute("DELETE FROM reference", []).unwrap();
        let (link, no): (Option<String>, i64) = conn
            .query_row("SELECT reference_id, ref_no FROM paper", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((link, no), (None, 46));
    }

    #[test]
    fn deleting_a_reason_makes_papers_no_reason_and_keeps_their_decision() {
        // FR-08.1: the papers become "No reason"; the decision itself is the user's (FR-11.4).
        let (_dir, conn) = new_library();
        let reason = add_reason(&conn, "E3");
        let paper = add_paper(&conn);
        conn.execute(
            "UPDATE paper SET decision = 'exclude', exclusion_reason_id = ?1 WHERE id = ?2",
            [&reason, &paper],
        )
        .unwrap();

        conn.execute("DELETE FROM exclusion_reason WHERE id = ?1", [&reason]).unwrap();
        let (decision, link): (String, Option<String>) = conn
            .query_row("SELECT decision, exclusion_reason_id FROM paper", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((decision.as_str(), link), ("exclude", None));
    }

    #[test]
    fn a_paper_cannot_point_to_a_reason_that_does_not_exist() {
        let (_dir, conn) = new_library();
        let paper = add_paper(&conn);
        assert!(conn
            .execute("UPDATE paper SET exclusion_reason_id = 'nope' WHERE id = ?1", [&paper])
            .is_err());
    }

    #[test]
    fn deleting_a_tag_clears_it_from_highlights() {
        let (_dir, conn) = new_library();
        let paper = add_paper(&conn);
        let tag: String = conn
            .query_row("INSERT INTO extraction_tag (label, sort_order) VALUES ('FCR', 0) RETURNING id", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO highlight (paper_id, extraction_tag_id, page, color, text, rects)
             VALUES (?1, ?2, 3, 'blue', 'FCR 1.2', '[{\"x\":0.1,\"y\":0.2,\"w\":0.3,\"h\":0.02}]')",
            [&paper, &tag],
        )
        .unwrap();

        conn.execute("DELETE FROM extraction_tag", []).unwrap();
        let tag_after: Option<String> =
            conn.query_row("SELECT extraction_tag_id FROM highlight", [], |r| r.get(0)).unwrap();
        assert_eq!(tag_after, None);
        assert_eq!(count(&conn, "highlight"), 1);
    }

    #[test]
    fn highlight_rects_must_be_json() {
        let (_dir, conn) = new_library();
        let paper = add_paper(&conn);
        assert!(conn
            .execute(
                "INSERT INTO highlight (paper_id, page, color, text, rects) VALUES (?1, 1, 'red', 't', 'not json')",
                [&paper],
            )
            .is_err());
    }

    #[test]
    fn chat_thread_scope_matches_paper_id() {
        let (_dir, conn) = new_library();
        let paper = add_paper(&conn);
        let add = |scope: &str, paper_id: Option<&str>| {
            conn.execute("INSERT INTO chat_thread (scope, paper_id) VALUES (?1, ?2)", params![scope, paper_id])
        };
        add("paper", Some(&paper)).unwrap();
        add("library", None).unwrap();
        assert!(add("paper", None).is_err());
        assert!(add("library", Some(&paper)).is_err());
    }

    /// Builds a paper with every kind of child row; returns (paper id, library-chat message id).
    fn paper_with_everything(conn: &Connection) -> (String, String) {
        let paper = add_paper(conn);
        let one = |sql: &str, args: &[&str]| -> String {
            conn.query_row(sql, rusqlite::params_from_iter(args), |r| r.get(0)).unwrap()
        };
        one(
            "INSERT INTO highlight (paper_id, page, color, text, rects) VALUES (?1, 1, 'green', 't', '[]') RETURNING id",
            &[&paper],
        );
        let chunk = one(
            "INSERT INTO text_chunk (paper_id, page_start, page_end, text) VALUES (?1, 1, 1, 't') RETURNING id",
            &[&paper],
        );
        let suggestion = one(
            "INSERT INTO suggestion (paper_id, proposed_decision, rationale, model_label)
             VALUES (?1, 'include', 'r', 'm') RETURNING id",
            &[&paper],
        );
        one(
            "INSERT INTO citation (chunk_id, suggestion_id, quote, verified) VALUES (?1, ?2, 'q', 1) RETURNING id",
            &[&chunk, &suggestion],
        );
        let thread = one("INSERT INTO chat_thread (scope, paper_id) VALUES ('paper', ?1) RETURNING id", &[&paper]);
        one(
            "INSERT INTO chat_message (thread_id, role, content) VALUES (?1, 'user', 'q') RETURNING id",
            &[&thread],
        );

        // A library-wide chat that quotes this paper.
        let library_thread = one("INSERT INTO chat_thread (scope) VALUES ('library') RETURNING id", &[]);
        let library_message = one(
            "INSERT INTO chat_message (thread_id, role, content) VALUES (?1, 'assistant', 'a') RETURNING id",
            &[&library_thread],
        );
        one(
            "INSERT INTO citation (chunk_id, message_id, quote, verified) VALUES (?1, ?2, 'q', 0) RETURNING id",
            &[&chunk, &library_message],
        );
        (paper, library_message)
    }

    #[test]
    fn deleting_a_paper_deletes_everything_that_belongs_to_it() {
        // FR-01.7. The confirmation dialog comes first (T-15); here we check what goes.
        let (_dir, conn) = new_library();
        let (paper, library_message) = paper_with_everything(&conn);
        let (other, _) = paper_with_everything(&conn);

        conn.execute("DELETE FROM paper WHERE id = ?1", [&paper]).unwrap();

        for table in ["highlight", "text_chunk", "suggestion"] {
            let left: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table} WHERE paper_id = ?1"), [&paper], |r| r.get(0))
                .unwrap();
            assert_eq!(left, 0, "{table} rows of the deleted paper remain");
        }
        // The other paper keeps all of its rows.
        assert_eq!(count(&conn, "highlight"), 1);
        assert_eq!(count(&conn, "text_chunk"), 1);
        assert_eq!(count(&conn, "suggestion"), 1);
        // Paper chats go; library chats stay, minus the citation that quoted the deleted paper.
        assert_eq!(count(&conn, "chat_thread"), 3);
        let left_on_message: i64 = conn
            .query_row("SELECT count(*) FROM citation WHERE message_id = ?1", [&library_message], |r| r.get(0))
            .unwrap();
        assert_eq!(left_on_message, 0);
        assert_eq!(count(&conn, "citation"), 2);
        let other_exists: bool =
            conn.query_row("SELECT count(*) = 1 FROM paper WHERE id = ?1", [&other], |r| r.get(0)).unwrap();
        assert!(other_exists);
    }

    #[test]
    fn a_citation_belongs_to_exactly_one_message_or_suggestion() {
        let (_dir, conn) = new_library();
        let paper = add_paper(&conn);
        let chunk: String = conn
            .query_row(
                "INSERT INTO text_chunk (paper_id, page_start, page_end, text) VALUES (?1, 2, 3, 't') RETURNING id",
                [&paper],
                |r| r.get(0),
            )
            .unwrap();
        let none = conn.execute(
            "INSERT INTO citation (chunk_id, quote, verified) VALUES (?1, 'q', 1)",
            [&chunk],
        );
        assert!(none.is_err());
    }

    #[test]
    fn a_suggestion_does_not_touch_the_decision() {
        // FR-11.4 at the storage level: saving or accepting a suggestion row
        // leaves paper.decision alone; only the user's own update changes it.
        let (_dir, conn) = new_library();
        let reason = add_reason(&conn, "E3");
        let paper = add_paper(&conn);
        conn.execute(
            "INSERT INTO suggestion (paper_id, proposed_reason_id, proposed_decision, rationale, model_label)
             VALUES (?1, ?2, 'exclude', 'r', 'm')",
            [&paper, &reason],
        )
        .unwrap();
        conn.execute("UPDATE suggestion SET status = 'accepted'", []).unwrap();

        let decision: String =
            conn.query_row("SELECT decision FROM paper", [], |r| r.get(0)).unwrap();
        assert_eq!(decision, "undecided");
    }

    #[test]
    fn no_table_has_a_library_id_column() {
        // PRD 7a: library_id is implied by the database the row lives in.
        let (_dir, conn) = new_library();
        let found: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema AS t, pragma_table_info(t.name) AS c
                 WHERE t.type = 'table' AND c.name = 'library_id'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(found, 0);
    }

    #[test]
    fn opening_the_app_database_as_a_library_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        drop(super::super::app::open_app_db(dir.path()).unwrap());
        std::fs::rename(
            dir.path().join(super::super::app::APP_DB_FILE),
            dir.path().join(LIBRARY_DB_FILE),
        )
        .unwrap();
        assert!(matches!(open_library(dir.path()), Err(DbError::NotSlreader)));
    }

    // ---- T-07: newer versions and damaged databases (PRD Section 8) ----

    /// A library whose schema version is one ahead of this app, as a newer SLReader would leave it.
    fn newer_library() -> tempfile::TempDir {
        let (dir, conn) = new_library();
        add_paper(&conn);
        conn.execute_batch("ALTER TABLE paper ADD COLUMN from_the_future TEXT;").unwrap();
        conn.pragma_update(None, "user_version", LIBRARY_SCHEMA.latest_version() + 1).unwrap();
        drop(conn);
        dir
    }

    fn db_bytes(dir: &tempfile::TempDir) -> Vec<u8> {
        std::fs::read(dir.path().join(LIBRARY_DB_FILE)).unwrap()
    }

    fn files_in(dir: &tempfile::TempDir) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_current_library_opens_for_editing() {
        let (dir, conn) = new_library();
        drop(conn);
        let opened = open_library(dir.path()).unwrap();
        assert_eq!(opened.read_only, None);
        add_paper(&opened.conn);
    }

    #[test]
    fn a_newer_library_opens_read_only_and_stays_unchanged() {
        let dir = newer_library();
        let before = db_bytes(&dir);

        let opened = open_library(dir.path()).unwrap();
        assert_eq!(opened.read_only, Some(LibraryProblem::NewerVersion));
        // Reading works.
        assert_eq!(count(&opened.conn, "paper"), 1);
        // Every kind of write is refused.
        assert!(opened.conn.execute("INSERT INTO paper DEFAULT VALUES", []).is_err());
        assert!(opened.conn.execute("UPDATE paper SET decision = 'include'", []).is_err());
        assert!(opened.conn.execute("DELETE FROM paper", []).is_err());
        assert!(opened.conn.pragma_update(None, "user_version", 1).is_err());
        drop(opened);

        assert_eq!(db_bytes(&dir), before);
        // No migration backup and no journal left behind.
        assert_eq!(files_in(&dir), [LIBRARY_DB_FILE]);
    }

    fn assert_damaged_and_untouched(dir: &tempfile::TempDir) {
        let before = db_bytes(dir);
        let result = open_library(dir.path());
        let error = result.err().expect("a damaged library must not open");
        assert_eq!(error.library_problem(), Some(LibraryProblem::Damaged), "{error}");
        assert_eq!(db_bytes(dir), before);
        assert_eq!(files_in(dir), [LIBRARY_DB_FILE]);
    }

    /// A library with enough rows to fill several pages.
    fn library_with_data() -> tempfile::TempDir {
        let (dir, conn) = new_library();
        for i in 0..200 {
            conn.execute(
                "INSERT INTO paper (title, assessment_note) VALUES (?1, ?2)",
                params![format!("Paper {i}"), "note ".repeat(40)],
            )
            .unwrap();
        }
        drop(conn);
        dir
    }

    #[test]
    fn a_library_with_corrupt_pages_is_damaged() {
        let dir = library_with_data();
        let path = dir.path().join(LIBRARY_DB_FILE);
        let mut bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 4096 * 4);
        // Scribble over everything after the first page (the header stays readable).
        for byte in &mut bytes[4096..] {
            *byte = 0xA5;
        }
        std::fs::write(&path, bytes).unwrap();
        assert_damaged_and_untouched(&dir);
    }

    #[test]
    fn a_truncated_library_is_damaged() {
        let dir = library_with_data();
        let path = dir.path().join(LIBRARY_DB_FILE);
        let bytes = std::fs::read(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
        assert_damaged_and_untouched(&dir);
    }

    #[test]
    fn a_text_file_named_library_sqlite_is_damaged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LIBRARY_DB_FILE), "hello, this is not a database at all.").unwrap();
        assert_damaged_and_untouched(&dir);
    }

    #[test]
    fn an_empty_library_file_is_damaged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LIBRARY_DB_FILE), "").unwrap();
        assert_damaged_and_untouched(&dir);
    }

    #[test]
    fn a_library_without_its_library_row_is_damaged() {
        let (dir, conn) = new_library();
        conn.execute("DELETE FROM library", []).unwrap();
        drop(conn);
        assert_damaged_and_untouched(&dir);
    }

    // ---- Crash in the middle of a save (hot journal) ----

    /// Copies a library while a large save is half-written, which is what a crash or a
    /// power cut leaves on disk: `library.sqlite` plus a `library.sqlite-journal`.
    /// `before_crash` runs on the live library first. Returns the copy.
    fn crashed_copy(before_crash: impl Fn(&Connection)) -> tempfile::TempDir {
        let live = library_with_data();
        let conn = open_library(live.path()).unwrap().conn;
        before_crash(&conn);
        // A tiny cache forces SQLite to write changed pages into the file mid-save.
        conn.pragma_update(None, "cache_size", 2).unwrap();
        conn.execute_batch("BEGIN; UPDATE paper SET title = 'half-saved';").unwrap();

        let crashed = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(live.path()).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), crashed.path().join(entry.file_name())).unwrap();
        }
        conn.execute_batch("ROLLBACK;").unwrap();
        assert!(
            crashed.path().join("library.sqlite-journal").exists(),
            "the simulated crash must leave a journal behind"
        );
        crashed
    }

    fn titles_marked_half_saved(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM paper WHERE title = 'half-saved'", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn a_library_left_half_saved_by_a_crash_opens_at_its_last_complete_save() {
        let dir = crashed_copy(|_| {});
        let opened = open_library(dir.path()).unwrap();
        assert_eq!(opened.read_only, None);
        assert_eq!(count(&opened.conn, "paper"), 200);
        assert_eq!(titles_marked_half_saved(&opened.conn), 0);
        drop(opened);
        assert_eq!(files_in(&dir), [LIBRARY_DB_FILE]);
    }

    #[test]
    fn a_newer_library_left_half_saved_is_recovered_then_opened_read_only() {
        let dir = crashed_copy(|conn| {
            conn.pragma_update(None, "user_version", LIBRARY_SCHEMA.latest_version() + 1).unwrap();
        });
        let opened = open_library(dir.path()).unwrap();
        assert_eq!(opened.read_only, Some(LibraryProblem::NewerVersion));
        assert_eq!(count(&opened.conn, "paper"), 200);
        assert_eq!(titles_marked_half_saved(&opened.conn), 0);
        assert!(opened.conn.execute("DELETE FROM paper", []).is_err());
    }

    #[test]
    fn another_programs_half_saved_database_is_left_alone() {
        let dir = crashed_copy(|conn| {
            conn.pragma_update(None, "application_id", 0x1234_5678).unwrap();
        });
        let journal = dir.path().join("library.sqlite-journal");
        let (db_before, journal_before) = (db_bytes(&dir), std::fs::read(&journal).unwrap());

        assert!(matches!(open_library(dir.path()), Err(DbError::NotSlreader)));
        assert_eq!(db_bytes(&dir), db_before);
        assert_eq!(std::fs::read(&journal).unwrap(), journal_before);
    }
}
