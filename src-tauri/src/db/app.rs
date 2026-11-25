//! The app database: list of library folders, AI providers, and app settings.
//! Shared by all libraries; lives in the app data folder, never in a library folder.

use std::path::Path;

use rusqlite::Connection;

use super::{DbError, Schema};

pub const APP_DB_FILE: &str = "app.sqlite";

pub const APP_SCHEMA: Schema = Schema {
    // "SLRA" in ASCII.
    application_id: 0x534C_5241,
    migrations: &[include_str!("migrations/app/001_initial.sql")],
};

/// Opens (or creates) `app.sqlite` in `data_dir`, creating the folder if needed.
pub fn open_app_db(data_dir: &Path) -> Result<Connection, DbError> {
    std::fs::create_dir_all(data_dir)?;
    super::open(&data_dir.join(APP_DB_FILE), &APP_SCHEMA, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    fn app() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_app_db(&dir.path().join("nested")).unwrap();
        (dir, conn)
    }

    #[test]
    fn creates_the_folder_and_opens_again() {
        let (dir, conn) = app();
        drop(conn);
        assert!(open_app_db(&dir.path().join("nested")).is_ok());
    }

    #[test]
    fn registry_only_lists_folders() {
        // Rule 6 of CLAUDE.md: library name and research questions stay in the library folder.
        let (_dir, conn) = app();
        assert_eq!(columns(&conn, "library_registry"), ["id", "folder_path", "last_opened_at", "missing"]);
    }

    #[test]
    fn no_column_can_hold_an_api_key() {
        // Rule 2 of CLAUDE.md / FR-09.2: API keys live only in the OS keychain.
        let (_dir, conn) = app();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for table in tables {
            for column in columns(&conn, &table) {
                let lower = column.to_lowercase();
                assert!(
                    !["key", "secret", "token", "password"].iter().any(|w| lower.contains(w))
                        || (table == "app_setting" && column == "key"),
                    "{table}.{column} looks like it could hold a secret"
                );
            }
        }
    }

    #[test]
    fn only_one_provider_can_be_active() {
        let (_dir, conn) = app();
        let add = |name: &str, active: bool| {
            conn.execute(
                "INSERT INTO ai_provider (kind, name, model, is_active) VALUES ('claude', ?1, 'm', ?2)",
                rusqlite::params![name, active],
            )
        };
        add("a", true).unwrap();
        add("b", false).unwrap();
        add("c", false).unwrap();
        assert!(add("d", true).is_err());
    }

    #[test]
    fn a_folder_is_registered_once() {
        let (_dir, conn) = app();
        let add = || conn.execute("INSERT INTO library_registry (folder_path) VALUES ('C:/lib')", []);
        add().unwrap();
        assert!(add().is_err());
    }
}
