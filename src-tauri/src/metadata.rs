//! The metadata of a paper: reference number, title, year, source database, and DOI
//! (FR-05.1), and the warning for a reference number used twice (FR-05.4).
//!
//! A reference number typed by hand may repeat: the user is warned, nothing is
//! refused. Saving metadata never touches the decision. A new reference number
//! releases the link to the reference list (`references.rs`); other fields keep it.
//!
//! Every function receives the connection of the open library from
//! `libraries::library_for_*` (FR-14.4).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::libraries::{ErrorCode, LibraryError};

/// Largest reference number and year the fields accept.
pub(crate) const MAX_REF_NO: i64 = 999_999_999;
pub(crate) const MAX_YEAR: i64 = 9999;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub ref_no: Option<i64>,
    pub title: String,
    pub year: Option<i64>,
    /// Empty when not filled in, like `doi`.
    pub source_database: String,
    pub doi: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperMetadata {
    pub metadata: Metadata,
    /// Other papers with the same reference number: their file name, or title.
    pub same_ref_no: Vec<String>,
}

fn invalid(detail: String) -> LibraryError {
    LibraryError::new(ErrorCode::Other, detail)
}

pub fn get_metadata(conn: &Connection, paper_id: &str) -> Result<PaperMetadata, LibraryError> {
    let metadata = conn
        .query_row(
            "SELECT ref_no, title, year, source_database, doi FROM paper WHERE id = ?1",
            [paper_id],
            |r| {
                Ok(Metadata {
                    ref_no: r.get(0)?,
                    title: r.get(1)?,
                    year: r.get(2)?,
                    source_database: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    doi: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| invalid(format!("no paper with id {paper_id}")))?;
    let same_ref_no = others_with(conn, paper_id, metadata.ref_no)?;
    Ok(PaperMetadata { metadata, same_ref_no })
}

fn others_with(conn: &Connection, paper_id: &str, ref_no: Option<i64>) -> Result<Vec<String>, LibraryError> {
    let Some(ref_no) = ref_no else { return Ok(Vec::new()) };
    let mut stmt = conn.prepare(
        "SELECT file_path, title FROM paper WHERE ref_no = ?1 AND id <> ?2 ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map(params![ref_no, paper_id], |r| {
        let file_path: Option<String> = r.get(0)?;
        let title: String = r.get(1)?;
        Ok(file_path.map(|path| path.rsplit('/').next().unwrap_or(&path).to_string()).unwrap_or(title))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Saves the metadata the user edited and returns the other papers that now share
/// the reference number.
pub fn set_metadata(conn: &Connection, paper_id: &str, new: &Metadata) -> Result<Vec<String>, LibraryError> {
    if new.ref_no.is_some_and(|no| !(0..=MAX_REF_NO).contains(&no)) {
        return Err(invalid("reference number out of range".into()));
    }
    if new.year.is_some_and(|year| !(0..=MAX_YEAR).contains(&year)) {
        return Err(invalid("year out of range".into()));
    }
    let optional = |text: &str| Some(text.trim().to_string()).filter(|text| !text.is_empty());
    let changed = conn.execute(
        // `ref_no` on the right is still the old number.
        "UPDATE paper SET reference_id = CASE WHEN ref_no IS ?1 THEN reference_id END,
                          ref_no = ?1, title = ?2, year = ?3, source_database = ?4, doi = ?5
         WHERE id = ?6",
        params![
            new.ref_no,
            // Kept as typed: trimming here would fight the user's typing.
            new.title,
            new.year,
            optional(&new.source_database),
            optional(&new.doi),
            paper_id
        ],
    )?;
    if changed == 0 {
        return Err(invalid(format!("no paper with id {paper_id}")));
    }
    others_with(conn, paper_id, new.ref_no)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::{create_library, open_library};

    fn library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        (dir, conn)
    }

    fn add(conn: &Connection, file: Option<&str>, title: &str) -> String {
        conn.query_row(
            "INSERT INTO paper (file_path, title) VALUES (?1, ?2) RETURNING id",
            params![file, title],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn filled(ref_no: Option<i64>) -> Metadata {
        Metadata {
            ref_no,
            title: "Tilapia in recirculating systems".into(),
            year: Some(2021),
            source_database: "Scopus".into(),
            doi: "10.1000/xyz".into(),
        }
    }

    #[test]
    fn a_new_paper_has_empty_metadata() {
        let (_dir, conn) = library();
        let paper = add(&conn, Some("pdfs/a.pdf"), "");
        assert_eq!(
            get_metadata(&conn, &paper).unwrap(),
            PaperMetadata { metadata: Metadata::default(), same_ref_no: vec![] }
        );
    }

    #[test]
    fn saves_every_field_across_reopening_and_can_clear_them() {
        let (dir, conn) = library();
        let paper = add(&conn, Some("pdfs/a.pdf"), "");
        assert!(set_metadata(&conn, &paper, &filled(Some(46))).unwrap().is_empty());
        drop(conn);
        let conn = open_library(dir.path()).unwrap().conn;
        assert_eq!(get_metadata(&conn, &paper).unwrap().metadata, filled(Some(46)));

        set_metadata(&conn, &paper, &Metadata { source_database: "  ".into(), ..Metadata::default() }).unwrap();
        assert_eq!(get_metadata(&conn, &paper).unwrap().metadata, Metadata::default());
        let stored: (Option<i64>, Option<String>, Option<String>) = conn
            .query_row("SELECT ref_no, source_database, doi FROM paper", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        assert_eq!(stored, (None, None, None));
    }

    #[test]
    fn warns_about_a_reference_number_used_twice_but_saves_it() {
        // FR-05.4.
        let (_dir, conn) = library();
        let first = add(&conn, Some("pdfs/kim-2021.pdf"), "Kim");
        let restored = add(&conn, None, "Restored paper");
        let third = add(&conn, Some("pdfs/c.pdf"), "");
        set_metadata(&conn, &first, &filled(Some(46))).unwrap();
        assert_eq!(set_metadata(&conn, &restored, &filled(Some(46))).unwrap(), ["kim-2021.pdf"]);
        assert_eq!(set_metadata(&conn, &third, &filled(Some(46))).unwrap().len(), 2);
        assert_eq!(
            get_metadata(&conn, &first).unwrap().same_ref_no,
            ["Tilapia in recirculating systems", "c.pdf"]
        );
        // No number, no warning, even when several papers have none.
        assert!(set_metadata(&conn, &third, &filled(None)).unwrap().is_empty());
        assert!(set_metadata(&conn, &restored, &filled(None)).unwrap().is_empty());
    }

    #[test]
    fn changes_only_the_metadata_of_that_paper() {
        let (_dir, conn) = library();
        let paper = add(&conn, Some("pdfs/a.pdf"), "");
        let other = add(&conn, Some("pdfs/b.pdf"), "Other");
        conn.execute(
            "UPDATE paper SET decision = 'exclude', assessment_note = 'n', last_page = 4 WHERE id = ?1",
            [&paper],
        )
        .unwrap();
        set_metadata(&conn, &paper, &filled(Some(1))).unwrap();
        let kept: String = conn
            .query_row(
                "SELECT decision || assessment_note || last_page || file_path FROM paper WHERE id = ?1",
                [&paper],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, "excluden4pdfs/a.pdf");
        assert_eq!(get_metadata(&conn, &other).unwrap().metadata.title, "Other");
    }

    #[test]
    fn refuses_unknown_papers_and_impossible_numbers() {
        let (_dir, conn) = library();
        let paper = add(&conn, None, "T");
        assert!(set_metadata(&conn, "nope", &filled(None)).is_err());
        assert!(get_metadata(&conn, "nope").is_err());
        assert!(set_metadata(&conn, &paper, &filled(Some(-1))).is_err());
        assert!(set_metadata(&conn, &paper, &filled(Some(MAX_REF_NO + 1))).is_err());
        assert!(set_metadata(&conn, &paper, &Metadata { year: Some(10_000), ..filled(None) }).is_err());
        assert_eq!(get_metadata(&conn, &paper).unwrap().metadata.title, "T");
    }
}
