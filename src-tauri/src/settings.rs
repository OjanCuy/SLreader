//! App settings in `app.sqlite`, shared by all libraries (FR-08.5, PRD 7a AppSetting).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// How PDF pages are shown (FR-02.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageMode {
    #[default]
    Normal,
    Dark,
    Sepia,
}

impl PageMode {
    const KEY: &str = "view_mode";

    fn stored(self) -> &'static str {
        match self {
            PageMode::Normal => "normal",
            PageMode::Dark => "dark",
            PageMode::Sepia => "sepia",
        }
    }
}

/// The remembered page mode; "normal" when none or an unknown one is stored.
pub fn page_mode(app: &Connection) -> rusqlite::Result<PageMode> {
    let value: Option<String> = app
        .query_row("SELECT value FROM app_setting WHERE key = ?1", [PageMode::KEY], |r| r.get(0))
        .optional()?;
    Ok(match value.as_deref() {
        Some("dark") => PageMode::Dark,
        Some("sepia") => PageMode::Sepia,
        _ => PageMode::Normal,
    })
}

pub fn set_page_mode(app: &Connection, mode: PageMode) -> rusqlite::Result<()> {
    app.execute(
        "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![PageMode::KEY, mode.stored()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::app::open_app_db;

    #[test]
    fn remembers_the_page_mode_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let app = open_app_db(dir.path()).unwrap();
        assert_eq!(page_mode(&app).unwrap(), PageMode::Normal);
        for mode in [PageMode::Dark, PageMode::Sepia, PageMode::Normal, PageMode::Dark] {
            set_page_mode(&app, mode).unwrap();
            assert_eq!(page_mode(&app).unwrap(), mode);
        }
        drop(app);
        assert_eq!(page_mode(&open_app_db(dir.path()).unwrap()).unwrap(), PageMode::Dark);
    }

    #[test]
    fn an_unknown_value_means_normal() {
        let dir = tempfile::tempdir().unwrap();
        let app = open_app_db(dir.path()).unwrap();
        app.execute("INSERT INTO app_setting (key, value) VALUES ('view_mode', 'neon')", []).unwrap();
        assert_eq!(page_mode(&app).unwrap(), PageMode::Normal);
    }

    #[test]
    fn the_ui_and_the_database_use_the_same_names() {
        for mode in [PageMode::Normal, PageMode::Dark, PageMode::Sepia] {
            assert_eq!(serde_json::to_value(mode).unwrap(), mode.stored());
        }
    }
}
