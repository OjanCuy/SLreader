//! App settings in `app.sqlite`, shared by all libraries (FR-08.5, PRD 7a AppSetting).

use std::ops::RangeInclusive;

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

/// The column separator of exported CSV files (FR-06.4, FR-08.3).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CsvDelimiter {
    #[default]
    #[serde(rename = ";")]
    Semicolon,
    #[serde(rename = ",")]
    Comma,
}

impl CsvDelimiter {
    const KEY: &str = "csv_delimiter";

    /// The character, which is also the stored value.
    pub fn character(self) -> char {
        match self {
            CsvDelimiter::Semicolon => ';',
            CsvDelimiter::Comma => ',',
        }
    }
}

/// The app's colors: those of Windows, or always light or dark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    const KEY: &str = "theme";

    fn stored(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// The zoom levels of the reader (1.0 is 100%); the same limits as `reader/layout.ts`.
const ZOOM_RANGE: RangeInclusive<f64> = 0.25..=4.0;
const ZOOM_KEY: &str = "zoom";
/// The stored value of "Fit width".
const FIT_WIDTH: &str = "fit";

fn read(app: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    app.query_row("SELECT value FROM app_setting WHERE key = ?1", [key], |r| r.get(0)).optional()
}

fn write(app: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    app.execute(
        "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// The chosen separator; a semicolon when none or an unknown one is stored.
pub fn csv_delimiter(app: &Connection) -> rusqlite::Result<CsvDelimiter> {
    Ok(match read(app, CsvDelimiter::KEY)?.as_deref() {
        Some(",") => CsvDelimiter::Comma,
        _ => CsvDelimiter::Semicolon,
    })
}

pub fn set_csv_delimiter(app: &Connection, delimiter: CsvDelimiter) -> rusqlite::Result<()> {
    write(app, CsvDelimiter::KEY, &delimiter.character().to_string())
}

/// The chosen theme; "system" when none or an unknown one is stored.
pub fn theme(app: &Connection) -> rusqlite::Result<Theme> {
    Ok(match read(app, Theme::KEY)?.as_deref() {
        Some("light") => Theme::Light,
        Some("dark") => Theme::Dark,
        _ => Theme::System,
    })
}

pub fn set_theme(app: &Connection, theme: Theme) -> rusqlite::Result<()> {
    write(app, Theme::KEY, theme.stored())
}

/// The remembered zoom of the reader (FR-08.5). `None` is "Fit width", also when
/// nothing or an unusable value is stored.
pub fn zoom(app: &Connection) -> rusqlite::Result<Option<f64>> {
    Ok(read(app, ZOOM_KEY)?
        .and_then(|value| value.parse().ok())
        .filter(|zoom| ZOOM_RANGE.contains(zoom)))
}

pub fn set_zoom(app: &Connection, zoom: Option<f64>) -> rusqlite::Result<()> {
    let value = match zoom {
        Some(zoom) if zoom.is_finite() => {
            zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()).to_string()
        }
        _ => FIT_WIDTH.to_string(),
    };
    write(app, ZOOM_KEY, &value)
}

/// The remembered page mode; "normal" when none or an unknown one is stored.
pub fn page_mode(app: &Connection) -> rusqlite::Result<PageMode> {
    Ok(match read(app, PageMode::KEY)?.as_deref() {
        Some("dark") => PageMode::Dark,
        Some("sepia") => PageMode::Sepia,
        _ => PageMode::Normal,
    })
}

pub fn set_page_mode(app: &Connection, mode: PageMode) -> rusqlite::Result<()> {
    write(app, PageMode::KEY, mode.stored())
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
    fn the_csv_separator_is_a_semicolon_unless_a_comma_is_stored() {
        let dir = tempfile::tempdir().unwrap();
        let app = open_app_db(dir.path()).unwrap();
        assert_eq!(csv_delimiter(&app).unwrap(), CsvDelimiter::Semicolon);
        for (stored, expected) in
            [(",", CsvDelimiter::Comma), (";", CsvDelimiter::Semicolon), ("|", CsvDelimiter::Semicolon)]
        {
            app.execute("REPLACE INTO app_setting (key, value) VALUES ('csv_delimiter', ?1)", [stored]).unwrap();
            assert_eq!(csv_delimiter(&app).unwrap(), expected);
        }
    }

    #[test]
    fn the_ui_and_the_database_use_the_same_names() {
        for mode in [PageMode::Normal, PageMode::Dark, PageMode::Sepia] {
            assert_eq!(serde_json::to_value(mode).unwrap(), mode.stored());
        }
        for theme in [Theme::System, Theme::Light, Theme::Dark] {
            assert_eq!(serde_json::to_value(theme).unwrap(), theme.stored());
        }
        for delimiter in [CsvDelimiter::Semicolon, CsvDelimiter::Comma] {
            let name = serde_json::to_value(delimiter).unwrap();
            assert_eq!(name, delimiter.character().to_string());
        }
    }

    #[test]
    fn remembers_the_app_settings_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let app = open_app_db(dir.path()).unwrap();
        assert_eq!(theme(&app).unwrap(), Theme::System);
        assert_eq!(zoom(&app).unwrap(), None);
        for choice in [Theme::Light, Theme::System, Theme::Dark] {
            set_theme(&app, choice).unwrap();
            assert_eq!(theme(&app).unwrap(), choice);
        }
        for choice in [CsvDelimiter::Comma, CsvDelimiter::Semicolon, CsvDelimiter::Comma] {
            set_csv_delimiter(&app, choice).unwrap();
            assert_eq!(csv_delimiter(&app).unwrap(), choice);
        }
        set_zoom(&app, Some(1.25)).unwrap();
        drop(app);
        let app = open_app_db(dir.path()).unwrap();
        assert_eq!(theme(&app).unwrap(), Theme::Dark);
        assert_eq!(csv_delimiter(&app).unwrap(), CsvDelimiter::Comma);
        assert_eq!(zoom(&app).unwrap(), Some(1.25));
        set_zoom(&app, None).unwrap();
        assert_eq!(zoom(&app).unwrap(), None);
    }

    #[test]
    fn keeps_the_zoom_within_the_readers_limits() {
        let dir = tempfile::tempdir().unwrap();
        let app = open_app_db(dir.path()).unwrap();
        for (given, expected) in
            [(0.01, Some(0.25)), (9.0, Some(4.0)), (f64::NAN, None), (f64::INFINITY, None)]
        {
            set_zoom(&app, Some(given)).unwrap();
            assert_eq!(zoom(&app).unwrap(), expected);
        }
        for stored in ["huge", "0", "100", "NaN", ""] {
            app.execute("REPLACE INTO app_setting (key, value) VALUES ('zoom', ?1)", [stored])
                .unwrap();
            assert_eq!(zoom(&app).unwrap(), None, "{stored}");
        }
    }

    #[test]
    fn an_unknown_theme_means_system() {
        let dir = tempfile::tempdir().unwrap();
        let app = open_app_db(dir.path()).unwrap();
        app.execute("INSERT INTO app_setting (key, value) VALUES ('theme', 'neon')", []).unwrap();
        assert_eq!(theme(&app).unwrap(), Theme::System);
    }
}
