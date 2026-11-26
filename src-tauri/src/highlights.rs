//! Highlights on a paper's pages (FR-03.1, FR-03.2) and the library's highlight
//! categories (FR-08.6).
//!
//! A highlight keeps its page, its category, the selected text, and where it is on the
//! page as rectangles in shares (0 to 1) of the page's width and height, as the page
//! is shown (its own rotation applied). Shares do not depend on the zoom, so a
//! highlight sits on the same words at every zoom level.
//!
//! Each library has 1 to 8 categories, each with a name and a color of a fixed palette
//! of 8 (T-61). Managing them is T-62; the schema enforces the rules (migration 002).
//!
//! Every function receives the connection of the open library from
//! `libraries::library_for_*` (FR-14.4). Nothing here touches `paper.decision`.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::libraries::{ErrorCode, LibraryError};

/// The palette (PRD 7a). Each color is used by one category of a library at most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaletteColor {
    Green,
    Red,
    Blue,
    Yellow,
    Purple,
    Orange,
    Pink,
    Gray,
}

impl PaletteColor {
    pub const ALL: [PaletteColor; 8] = [
        PaletteColor::Green,
        PaletteColor::Red,
        PaletteColor::Blue,
        PaletteColor::Yellow,
        PaletteColor::Purple,
        PaletteColor::Orange,
        PaletteColor::Pink,
        PaletteColor::Gray,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            PaletteColor::Green => "green",
            PaletteColor::Red => "red",
            PaletteColor::Blue => "blue",
            PaletteColor::Yellow => "yellow",
            PaletteColor::Purple => "purple",
            PaletteColor::Orange => "orange",
            PaletteColor::Pink => "pink",
            PaletteColor::Gray => "gray",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.as_str() == text)
    }
}

/// A highlight category of the library, as the UI gets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightCategory {
    pub id: String,
    pub label: String,
    pub color: PaletteColor,
    pub sort_order: i64,
}

/// An area of the page: left edge, top edge, width, and height, as shares of the page.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A highlight as the UI gets it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub id: String,
    /// 1-based.
    pub page: u32,
    pub category_id: String,
    pub text: String,
    pub rects: Vec<Rect>,
    pub note: String,
    pub extraction_tag_id: Option<String>,
    pub created_at: String,
}

/// What the reader sends when the user picks a category for a selection.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewHighlight {
    pub page: u32,
    pub category_id: String,
    pub text: String,
    pub rects: Vec<Rect>,
}

/// The parts of a highlight the user can change in the Highlights tab (FR-03.3). All
/// of them are sent every time. Page, text, and position never change.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightEdit {
    pub category_id: String,
    /// `None` is "No tag".
    pub extraction_tag_id: Option<String>,
    pub note: String,
}

/// An extraction tag of the library, for the tag choice of a highlight. Tags are
/// managed in `criteria.rs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtractionTag {
    pub id: String,
    pub label: String,
}

/// More rectangles than this is not a selection of text on one page.
const MAX_RECTS: usize = 2000;

/// Room for rounding at the page edges.
const EDGE: f64 = 1e-6;

fn invalid(detail: &str) -> LibraryError {
    LibraryError::new(ErrorCode::Other, format!("invalid highlight: {detail}"))
}

fn check(new: &NewHighlight) -> Result<(), LibraryError> {
    if new.page < 1 {
        return Err(invalid("page must be 1 or more"));
    }
    if new.text.trim().is_empty() {
        return Err(invalid("no text"));
    }
    if new.rects.is_empty() || new.rects.len() > MAX_RECTS {
        return Err(invalid("wrong number of rectangles"));
    }
    let inside = |r: &Rect| {
        [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite())
            && r.x >= -EDGE
            && r.y >= -EDGE
            && r.w > 0.0
            && r.h > 0.0
            && r.x + r.w <= 1.0 + EDGE
            && r.y + r.h <= 1.0 + EDGE
    };
    if !new.rects.iter().all(inside) {
        return Err(invalid("a rectangle is outside the page"));
    }
    Ok(())
}

/// The library's highlight categories, in their order (FR-03.1).
/// A row with a color this version does not know is left out.
pub fn list_categories(conn: &Connection) -> Result<Vec<HighlightCategory>, LibraryError> {
    let mut stmt = conn.prepare("SELECT id, label, color, sort_order FROM highlight_category ORDER BY sort_order, rowid")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
    })?;
    let mut categories = Vec::new();
    for row in rows {
        let (id, label, color, sort_order) = row?;
        match PaletteColor::parse(&color) {
            Some(color) => categories.push(HighlightCategory { id, label, color, sort_order }),
            None => eprintln!("highlight category {id} has an unknown color and is not shown"),
        }
    }
    Ok(categories)
}

fn exists(conn: &Connection, sql: &str, id: &str) -> Result<bool, LibraryError> {
    Ok(conn.query_row(sql, [id], |r| r.get::<_, i64>(0)).optional()?.is_some())
}

const CATEGORY_EXISTS: &str = "SELECT 1 FROM highlight_category WHERE id = ?1";

/// The library's extraction tags, in their order (FR-08.2).
pub fn list_tags(conn: &Connection) -> Result<Vec<ExtractionTag>, LibraryError> {
    let mut stmt = conn.prepare("SELECT id, label FROM extraction_tag ORDER BY sort_order, rowid")?;
    let rows = stmt.query_map([], |r| Ok(ExtractionTag { id: r.get(0)?, label: r.get(1)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Saves the category, tag, and note of a highlight (FR-03.3).
pub fn update_highlight(conn: &Connection, highlight_id: &str, edit: &HighlightEdit) -> Result<(), LibraryError> {
    if !exists(conn, CATEGORY_EXISTS, &edit.category_id)? {
        return Err(invalid("no such category"));
    }
    if let Some(tag) = &edit.extraction_tag_id
        && !exists(conn, "SELECT 1 FROM extraction_tag WHERE id = ?1", tag)?
    {
        return Err(invalid("no such tag"));
    }
    let changed = conn.execute(
        "UPDATE highlight SET highlight_category_id = ?1, extraction_tag_id = ?2, note = ?3 WHERE id = ?4",
        params![edit.category_id, edit.extraction_tag_id, edit.note, highlight_id],
    )?;
    if changed == 0 {
        return Err(invalid("no such highlight"));
    }
    Ok(())
}

/// Deletes one highlight and its note (FR-03.3). The UI has asked the user to confirm
/// (O-10). Deleting one that is already gone is fine.
pub fn delete_highlight(conn: &Connection, highlight_id: &str) -> Result<(), LibraryError> {
    conn.execute("DELETE FROM highlight WHERE id = ?1", [highlight_id])?;
    Ok(())
}

/// Saves a new highlight on a paper and returns it.
pub fn add_highlight(conn: &Connection, paper_id: &str, new: &NewHighlight) -> Result<Highlight, LibraryError> {
    check(new)?;
    if !exists(conn, "SELECT 1 FROM paper WHERE id = ?1", paper_id)? {
        return Err(LibraryError::new(ErrorCode::Other, format!("no paper with id {paper_id}")));
    }
    if !exists(conn, CATEGORY_EXISTS, &new.category_id)? {
        return Err(invalid("no such category"));
    }
    let rects = serde_json::to_string(&new.rects).map_err(|e| invalid(&e.to_string()))?;
    let id: String = conn.query_row(
        "INSERT INTO highlight (paper_id, page, highlight_category_id, text, rects)
         VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
        params![paper_id, new.page, new.category_id, new.text.trim(), rects],
        |r| r.get(0),
    )?;
    list_highlights(conn, paper_id)?
        .into_iter()
        .find(|highlight| highlight.id == id)
        .ok_or_else(|| invalid("not found after saving"))
}

/// The highlights of a paper, by page, then oldest first (FR-03.4).
/// A row whose rectangles cannot be read is left out instead of failing the whole list.
pub fn list_highlights(conn: &Connection, paper_id: &str) -> Result<Vec<Highlight>, LibraryError> {
    let mut stmt = conn.prepare(
        "SELECT id, page, highlight_category_id, text, rects, note, extraction_tag_id, created_at
         FROM highlight WHERE paper_id = ?1 ORDER BY page, created_at, rowid",
    )?;
    let rows = stmt.query_map([paper_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, u32>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, String>(7)?,
        ))
    })?;
    let mut highlights = Vec::new();
    for row in rows {
        let (id, page, category_id, text, rects, note, extraction_tag_id, created_at) = row?;
        let Ok(rects) = serde_json::from_str::<Vec<Rect>>(&rects) else {
            eprintln!("highlight {id} could not be read and is not shown");
            continue;
        };
        highlights.push(Highlight { id, page, category_id, text, rects, note, extraction_tag_id, created_at });
    }
    Ok(highlights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;

    fn library_with_paper() -> (tempfile::TempDir, Connection, String) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        let id = conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap();
        (dir, conn, id)
    }

    /// The id of the library's category with this color.
    fn category(conn: &Connection, color: PaletteColor) -> String {
        list_categories(conn).unwrap().into_iter().find(|c| c.color == color).unwrap().id
    }

    fn new(conn: &Connection, page: u32, color: PaletteColor, rects: Vec<Rect>) -> NewHighlight {
        NewHighlight { page, category_id: category(conn, color), text: "  Feed conversion ratio was 1.4  ".into(), rects }
    }

    const LINE: Rect = Rect { x: 0.12, y: 0.31, w: 0.5, h: 0.018 };

    #[test]
    fn lists_the_default_categories_in_order() {
        let (_dir, conn, _) = library_with_paper();
        let listed: Vec<(String, PaletteColor)> =
            list_categories(&conn).unwrap().into_iter().map(|c| (c.label, c.color)).collect();
        assert_eq!(
            listed,
            [
                ("Inclusion".to_string(), PaletteColor::Green),
                ("Exclusion".to_string(), PaletteColor::Red),
                ("Method".to_string(), PaletteColor::Blue),
                ("Result".to_string(), PaletteColor::Yellow),
            ]
        );
    }

    #[test]
    fn the_palette_matches_the_schema() {
        // Every palette color can be stored, and the names sent to the UI are the stored ones.
        let (_dir, conn, _) = library_with_paper();
        conn.execute("DELETE FROM highlight_category WHERE color <> 'green'", []).unwrap();
        for (index, color) in PaletteColor::ALL.iter().enumerate().skip(1) {
            conn.execute(
                "INSERT INTO highlight_category (label, color, sort_order) VALUES (?1, ?2, ?3)",
                params![format!("C{index}"), color.as_str(), index as i64],
            )
            .unwrap();
            assert_eq!(serde_json::to_value(color).unwrap(), color.as_str());
        }
        assert_eq!(list_categories(&conn).unwrap().len(), 8);
    }

    #[test]
    fn saves_and_lists_a_highlight_with_its_page_category_position_and_text() {
        let (_dir, conn, paper) = library_with_paper();
        let blue = category(&conn, PaletteColor::Blue);
        let saved = add_highlight(&conn, &paper, &new(&conn, 4, PaletteColor::Blue, vec![LINE])).unwrap();
        assert_eq!(saved.page, 4);
        assert_eq!(saved.category_id, blue);
        assert_eq!(saved.text, "Feed conversion ratio was 1.4");
        assert_eq!(saved.rects, [LINE]);
        assert_eq!(saved.note, "");
        assert_eq!(saved.extraction_tag_id, None);
        assert_eq!(list_highlights(&conn, &paper).unwrap(), [saved]);
    }

    #[test]
    fn stays_after_the_library_is_opened_again() {
        let (dir, conn, paper) = library_with_paper();
        let saved = add_highlight(&conn, &paper, &new(&conn, 2, PaletteColor::Green, vec![LINE])).unwrap();
        drop(conn);
        let conn = crate::db::library::open_library(dir.path()).unwrap().conn;
        assert_eq!(list_highlights(&conn, &paper).unwrap(), [saved]);
    }

    #[test]
    fn lists_by_page_and_only_for_the_paper() {
        let (_dir, conn, paper) = library_with_paper();
        let other: String = conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap();
        add_highlight(&conn, &paper, &new(&conn, 5, PaletteColor::Red, vec![LINE])).unwrap();
        add_highlight(&conn, &paper, &new(&conn, 1, PaletteColor::Yellow, vec![LINE])).unwrap();
        add_highlight(&conn, &other, &new(&conn, 3, PaletteColor::Green, vec![LINE])).unwrap();
        let pages: Vec<u32> = list_highlights(&conn, &paper).unwrap().iter().map(|h| h.page).collect();
        assert_eq!(pages, [1, 5]);
    }

    #[test]
    fn reads_what_the_ui_sends() {
        let parsed: NewHighlight = serde_json::from_str(
            r#"{"page":1,"categoryId":"abc","text":"t","rects":[{"x":0,"y":0,"w":1,"h":1}]}"#,
        )
        .unwrap();
        assert_eq!(parsed.category_id, "abc");
    }

    #[test]
    fn refuses_highlights_that_are_not_on_the_page_or_without_a_category() {
        let (_dir, conn, paper) = library_with_paper();
        let red = |page, rects| new(&conn, page, PaletteColor::Red, rects);
        let bad = [
            red(0, vec![LINE]),
            red(1, vec![]),
            red(1, vec![Rect { x: 0.9, y: 0.1, w: 0.2, h: 0.02 }]),
            red(1, vec![Rect { x: -0.1, y: 0.1, w: 0.2, h: 0.02 }]),
            red(1, vec![Rect { x: 0.1, y: 0.1, w: 0.0, h: 0.02 }]),
            red(1, vec![Rect { x: f64::NAN, y: 0.1, w: 0.2, h: 0.02 }]),
            red(1, vec![LINE; MAX_RECTS + 1]),
            NewHighlight { text: " \n ".into(), ..red(1, vec![LINE]) },
            NewHighlight { category_id: "no-such-category".into(), ..red(1, vec![LINE]) },
        ];
        for highlight in &bad {
            assert!(add_highlight(&conn, &paper, highlight).is_err(), "{highlight:?}");
        }
        assert!(add_highlight(&conn, "no-such-paper", &red(1, vec![LINE])).is_err());
        assert!(list_highlights(&conn, &paper).unwrap().is_empty());
        // The whole page is fine.
        let page = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        assert!(add_highlight(&conn, &paper, &red(1, vec![page])).is_ok());
    }

    #[test]
    fn adding_a_highlight_never_changes_the_decision() {
        // Rule 1 of CLAUDE.md: only the user's own decision action changes it.
        let (_dir, conn, paper) = library_with_paper();
        conn.execute("UPDATE paper SET decision = 'include' WHERE id = ?1", [&paper]).unwrap();
        let before: String =
            conn.query_row("SELECT decision || last_page || text_status FROM paper", [], |r| r.get(0)).unwrap();
        add_highlight(&conn, &paper, &new(&conn, 1, PaletteColor::Red, vec![LINE])).unwrap();
        let after: String =
            conn.query_row("SELECT decision || last_page || text_status FROM paper", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }

    fn add_tag(conn: &Connection, label: &str, order: i64) -> String {
        conn.query_row(
            "INSERT INTO extraction_tag (label, sort_order) VALUES (?1, ?2) RETURNING id",
            params![label, order],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn lists_tags_in_their_order() {
        let (_dir, conn, _) = library_with_paper();
        assert!(list_tags(&conn).unwrap().is_empty());
        add_tag(&conn, "Sample size", 1);
        add_tag(&conn, "FCR", 0);
        let labels: Vec<String> = list_tags(&conn).unwrap().into_iter().map(|t| t.label).collect();
        assert_eq!(labels, ["FCR", "Sample size"]);
    }

    #[test]
    fn changes_only_the_category_tag_and_note() {
        let (dir, conn, paper) = library_with_paper();
        let tag = add_tag(&conn, "FCR", 0);
        let saved = add_highlight(&conn, &paper, &new(&conn, 2, PaletteColor::Green, vec![LINE])).unwrap();
        let other = add_highlight(&conn, &paper, &new(&conn, 3, PaletteColor::Green, vec![LINE])).unwrap();
        let edit = HighlightEdit {
            category_id: category(&conn, PaletteColor::Red),
            extraction_tag_id: Some(tag.clone()),
            note: "  Table 3, see [#46]\n".into(),
        };
        update_highlight(&conn, &saved.id, &edit).unwrap();
        drop(conn);
        let conn = crate::db::library::open_library(dir.path()).unwrap().conn;

        let expected = Highlight {
            category_id: edit.category_id.clone(),
            extraction_tag_id: Some(tag),
            note: edit.note.clone(),
            ..saved.clone()
        };
        assert_eq!(list_highlights(&conn, &paper).unwrap(), [expected, other]);

        // "No tag" and an empty note.
        let cleared = HighlightEdit { extraction_tag_id: None, note: String::new(), ..edit };
        update_highlight(&conn, &saved.id, &cleared).unwrap();
        let listed = list_highlights(&conn, &paper).unwrap();
        assert_eq!((listed[0].extraction_tag_id.clone(), listed[0].note.as_str()), (None, ""));
    }

    #[test]
    fn refuses_a_change_to_something_that_does_not_exist() {
        let (_dir, conn, paper) = library_with_paper();
        let saved = add_highlight(&conn, &paper, &new(&conn, 1, PaletteColor::Green, vec![LINE])).unwrap();
        let good = HighlightEdit {
            category_id: saved.category_id.clone(),
            extraction_tag_id: None,
            note: "n".into(),
        };
        let no_category = HighlightEdit { category_id: "nope".into(), ..good.clone() };
        let no_tag = HighlightEdit { extraction_tag_id: Some("nope".into()), ..good.clone() };
        assert!(update_highlight(&conn, &saved.id, &no_category).is_err());
        assert!(update_highlight(&conn, &saved.id, &no_tag).is_err());
        assert!(update_highlight(&conn, "no-such-highlight", &good).is_err());
        assert_eq!(list_highlights(&conn, &paper).unwrap(), [saved]);
    }

    #[test]
    fn deletes_only_that_highlight_and_leaves_the_decision() {
        let (_dir, conn, paper) = library_with_paper();
        conn.execute("UPDATE paper SET decision = 'exclude' WHERE id = ?1", [&paper]).unwrap();
        let first = add_highlight(&conn, &paper, &new(&conn, 1, PaletteColor::Red, vec![LINE])).unwrap();
        let second = add_highlight(&conn, &paper, &new(&conn, 2, PaletteColor::Red, vec![LINE])).unwrap();

        delete_highlight(&conn, &first.id).unwrap();
        assert_eq!(list_highlights(&conn, &paper).unwrap(), [second]);
        // Already gone: nothing else is touched.
        delete_highlight(&conn, &first.id).unwrap();
        let (decision, papers, categories): (String, i64, i64) = conn
            .query_row(
                "SELECT decision, (SELECT count(*) FROM paper), (SELECT count(*) FROM highlight_category) FROM paper",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((decision.as_str(), papers, categories), ("exclude", 1, 4));
    }

    #[test]
    fn a_damaged_row_is_left_out_of_the_list() {
        let (_dir, conn, paper) = library_with_paper();
        add_highlight(&conn, &paper, &new(&conn, 1, PaletteColor::Red, vec![LINE])).unwrap();
        conn.execute(
            "INSERT INTO highlight (paper_id, page, highlight_category_id, text, rects)
             VALUES (?1, 2, ?2, 't', '{\"not\":\"a list\"}')",
            [&paper, &category(&conn, PaletteColor::Red)],
        )
        .unwrap();
        assert_eq!(list_highlights(&conn, &paper).unwrap().len(), 1);
    }
}
