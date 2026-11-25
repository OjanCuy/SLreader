//! Highlights on a paper's pages (FR-03.1, FR-03.2).
//!
//! A highlight keeps its page, its color, the selected text, and where it is on the
//! page as rectangles in shares (0 to 1) of the page's width and height, as the page
//! is shown (its own rotation applied). Shares do not depend on the zoom, so a
//! highlight sits on the same words at every zoom level.
//!
//! Every function receives the connection of the open library from
//! `libraries::library_for_*` (FR-14.4). Nothing here touches `paper.decision`.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::libraries::{ErrorCode, LibraryError};

/// The 4 fixed colors and what they mean (FR-03.1, PRD 7a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightColor {
    /// Supports inclusion.
    Green,
    /// Supports exclusion.
    Red,
    /// Method.
    Blue,
    /// Quantitative result.
    Yellow,
}

impl HighlightColor {
    fn as_str(self) -> &'static str {
        match self {
            HighlightColor::Green => "green",
            HighlightColor::Red => "red",
            HighlightColor::Blue => "blue",
            HighlightColor::Yellow => "yellow",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "green" => HighlightColor::Green,
            "red" => HighlightColor::Red,
            "blue" => HighlightColor::Blue,
            "yellow" => HighlightColor::Yellow,
            _ => return None,
        })
    }
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
    pub color: HighlightColor,
    pub text: String,
    pub rects: Vec<Rect>,
    pub note: String,
    pub extraction_tag_id: Option<String>,
    pub created_at: String,
}

/// What the reader sends when the user picks a color for a selection.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewHighlight {
    pub page: u32,
    pub color: HighlightColor,
    pub text: String,
    pub rects: Vec<Rect>,
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

/// Saves a new highlight on a paper and returns it.
pub fn add_highlight(conn: &Connection, paper_id: &str, new: &NewHighlight) -> Result<Highlight, LibraryError> {
    check(new)?;
    let exists: Option<i64> =
        conn.query_row("SELECT 1 FROM paper WHERE id = ?1", [paper_id], |r| r.get(0)).optional()?;
    if exists.is_none() {
        return Err(LibraryError::new(ErrorCode::Other, format!("no paper with id {paper_id}")));
    }
    let rects = serde_json::to_string(&new.rects).map_err(|e| invalid(&e.to_string()))?;
    let id: String = conn.query_row(
        "INSERT INTO highlight (paper_id, page, color, text, rects) VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
        params![paper_id, new.page, new.color.as_str(), new.text.trim(), rects],
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
        "SELECT id, page, color, text, rects, note, extraction_tag_id, created_at
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
        let (id, page, color, text, rects, note, extraction_tag_id, created_at) = row?;
        let (Some(color), Ok(rects)) = (HighlightColor::parse(&color), serde_json::from_str::<Vec<Rect>>(&rects))
        else {
            eprintln!("highlight {id} could not be read and is not shown");
            continue;
        };
        highlights.push(Highlight { id, page, color, text, rects, note, extraction_tag_id, created_at });
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

    fn new(page: u32, color: HighlightColor, rects: Vec<Rect>) -> NewHighlight {
        NewHighlight { page, color, text: "  Feed conversion ratio was 1.4  ".into(), rects }
    }

    const LINE: Rect = Rect { x: 0.12, y: 0.31, w: 0.5, h: 0.018 };

    #[test]
    fn saves_and_lists_a_highlight_with_its_page_position_and_text() {
        let (_dir, conn, paper) = library_with_paper();
        let saved = add_highlight(&conn, &paper, &new(4, HighlightColor::Blue, vec![LINE])).unwrap();
        assert_eq!(saved.page, 4);
        assert_eq!(saved.color, HighlightColor::Blue);
        assert_eq!(saved.text, "Feed conversion ratio was 1.4");
        assert_eq!(saved.rects, [LINE]);
        assert_eq!(saved.note, "");
        assert_eq!(saved.extraction_tag_id, None);
        assert_eq!(list_highlights(&conn, &paper).unwrap(), [saved]);
    }

    #[test]
    fn stays_after_the_library_is_opened_again() {
        let (dir, conn, paper) = library_with_paper();
        let saved = add_highlight(&conn, &paper, &new(2, HighlightColor::Green, vec![LINE])).unwrap();
        drop(conn);
        let conn = crate::db::library::open_library(dir.path()).unwrap().conn;
        assert_eq!(list_highlights(&conn, &paper).unwrap(), [saved]);
    }

    #[test]
    fn lists_by_page_and_only_for_the_paper() {
        let (_dir, conn, paper) = library_with_paper();
        let other: String = conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap();
        add_highlight(&conn, &paper, &new(5, HighlightColor::Red, vec![LINE])).unwrap();
        add_highlight(&conn, &paper, &new(1, HighlightColor::Yellow, vec![LINE])).unwrap();
        add_highlight(&conn, &other, &new(3, HighlightColor::Green, vec![LINE])).unwrap();
        let pages: Vec<u32> = list_highlights(&conn, &paper).unwrap().iter().map(|h| h.page).collect();
        assert_eq!(pages, [1, 5]);
    }

    #[test]
    fn colors_are_sent_as_the_stored_names() {
        let json = serde_json::to_string(&[HighlightColor::Green, HighlightColor::Red, HighlightColor::Blue, HighlightColor::Yellow]).unwrap();
        assert_eq!(json, r#"["green","red","blue","yellow"]"#);
        let parsed: NewHighlight =
            serde_json::from_str(r#"{"page":1,"color":"yellow","text":"t","rects":[{"x":0,"y":0,"w":1,"h":1}]}"#).unwrap();
        assert_eq!(parsed.color, HighlightColor::Yellow);
        assert!(serde_json::from_str::<NewHighlight>(r#"{"page":1,"color":"purple","text":"t","rects":[]}"#).is_err());
    }

    #[test]
    fn refuses_highlights_that_are_not_on_the_page() {
        let (_dir, conn, paper) = library_with_paper();
        let bad = [
            new(0, HighlightColor::Red, vec![LINE]),
            new(1, HighlightColor::Red, vec![]),
            new(1, HighlightColor::Red, vec![Rect { x: 0.9, y: 0.1, w: 0.2, h: 0.02 }]),
            new(1, HighlightColor::Red, vec![Rect { x: -0.1, y: 0.1, w: 0.2, h: 0.02 }]),
            new(1, HighlightColor::Red, vec![Rect { x: 0.1, y: 0.1, w: 0.0, h: 0.02 }]),
            new(1, HighlightColor::Red, vec![Rect { x: f64::NAN, y: 0.1, w: 0.2, h: 0.02 }]),
            new(1, HighlightColor::Red, vec![LINE; MAX_RECTS + 1]),
            NewHighlight { text: " \n ".into(), ..new(1, HighlightColor::Red, vec![LINE]) },
        ];
        for highlight in &bad {
            assert!(add_highlight(&conn, &paper, highlight).is_err(), "{highlight:?}");
        }
        assert!(add_highlight(&conn, "no-such-paper", &new(1, HighlightColor::Red, vec![LINE])).is_err());
        assert!(list_highlights(&conn, &paper).unwrap().is_empty());
        // The whole page is fine.
        let page = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        assert!(add_highlight(&conn, &paper, &new(1, HighlightColor::Red, vec![page])).is_ok());
    }

    #[test]
    fn adding_a_highlight_never_changes_the_decision() {
        // Rule 1 of CLAUDE.md: only the user's own decision action changes it.
        let (_dir, conn, paper) = library_with_paper();
        conn.execute("UPDATE paper SET decision = 'include' WHERE id = ?1", [&paper]).unwrap();
        let before: String =
            conn.query_row("SELECT decision || last_page || text_status FROM paper", [], |r| r.get(0)).unwrap();
        add_highlight(&conn, &paper, &new(1, HighlightColor::Red, vec![LINE])).unwrap();
        let after: String =
            conn.query_row("SELECT decision || last_page || text_status FROM paper", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn a_damaged_row_is_left_out_of_the_list() {
        let (_dir, conn, paper) = library_with_paper();
        add_highlight(&conn, &paper, &new(1, HighlightColor::Red, vec![LINE])).unwrap();
        conn.execute(
            "INSERT INTO highlight (paper_id, page, color, text, rects) VALUES (?1, 2, 'red', 't', '{\"not\":\"a list\"}')",
            [&paper],
        )
        .unwrap();
        assert_eq!(list_highlights(&conn, &paper).unwrap().len(), 1);
    }
}
