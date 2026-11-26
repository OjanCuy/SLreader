//! The reference list: reading a CSV file or rows pasted from Excel, and replacing
//! the library's list with them (FR-05.2).
//!
//! The separator (tab, semicolon, comma) and an English or Indonesian header row are
//! recognized. A row without a reference number or a title is skipped and counted.
//! An existing list is replaced only when the user has confirmed it, and never by an
//! import without a single usable row. Papers keep their metadata; the database only
//! removes their link to an old reference (`ON DELETE SET NULL`).
//!
//! A paper is linked to one reference, and a reference to at most one paper (FR-05.4).
//! Linking fills the paper's metadata (FR-05.3) and replaces filled fields only when
//! the user has confirmed it. Nothing here touches the decision.
//!
//! Every function that touches the database receives the connection of the open
//! library from `libraries::library_for_*` (FR-14.4).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::libraries::{ErrorCode, LibraryError};
use crate::metadata::{self, MAX_REF_NO, MAX_YEAR, Metadata};

/// A reference list of several thousand rows is far below this.
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// In order of preference when two separators read the text equally well.
const SEPARATORS: [char; 3] = ['\t', ';', ','];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub no: i64,
    pub title: String,
    pub year: Option<i64>,
    pub source_database: Option<String>,
    pub doi: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedList {
    pub rows: Vec<Reference>,
    /// Rows without a usable reference number or without a title.
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ImportResult {
    /// No usable row; the old list is unchanged.
    Empty,
    /// A list already exists and the user has not confirmed replacing it.
    NeedsConfirmation { existing: usize },
    Imported {
        imported: usize,
        skipped: usize,
        /// Reference numbers that appear on more than one imported row, ascending.
        duplicates: Vec<i64>,
    },
}

/// A reference of the list, for the choice on the Decision tab (FR-05.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceChoice {
    pub id: String,
    pub no: i64,
    pub title: String,
    pub year: Option<i64>,
    /// The paper linked to it, if any.
    pub linked_paper_id: Option<String>,
    /// That paper's file name, or its title when it has no file.
    pub linked_paper_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum LinkResult {
    /// Another paper uses the reference; nothing changed.
    Taken { paper: String },
    /// Linking would replace filled fields and the user has not confirmed it.
    NeedsConfirmation,
    /// The paper's metadata now, and the other papers with its reference number.
    Linked { metadata: Metadata, same_ref_no: Vec<String> },
}

/// Which column holds which field.
#[derive(Debug, Clone, Copy)]
struct Columns {
    no: usize,
    title: usize,
    year: Option<usize>,
    source_database: Option<usize>,
    doi: Option<usize>,
}

/// Without a header row, the columns are in the order of FR-05.2.
const DEFAULT_COLUMNS: Columns =
    Columns { no: 0, title: 1, year: Some(2), source_database: Some(3), doi: Some(4) };

/// The columns named by a header row, or `None` if the row names no reference number
/// and title column.
fn header(row: &[String]) -> Option<Columns> {
    let names: Vec<String> = row
        .iter()
        .map(|cell| cell.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect())
        .collect();
    // The first name wins, so "Reference No" is preferred over a row counter "No".
    let find = |wanted: &[&str]| wanted.iter().find_map(|name| names.iter().position(|n| n == name));
    Some(Columns {
        no: find(&[
            "referenceno",
            "noreferensi",
            "referencenumber",
            "nomorreferensi",
            "refno",
            "noref",
            "no",
            "nomor",
            "number",
        ])?,
        title: find(&["title", "judul", "judulartikel"])?,
        year: find(&["year", "tahun", "publicationyear", "tahunterbit"]),
        source_database: find(&[
            "sourcedatabase",
            "sumberdatabase",
            "databasesumber",
            "database",
            "source",
            "sumber",
        ]),
        doi: find(&["doi"]),
    })
}

/// Splits CSV text into rows of cells. A cell in quotes may hold the separator, line
/// breaks, and doubled quotes. A quote that does not enclose a whole cell, as in
/// `"Smart" ponds`, is ordinary text.
fn split_rows(text: &str, separator: char) -> Vec<Vec<String>> {
    let chars: Vec<char> = text.chars().collect();
    let ends_cell = |c: Option<&char>| c.is_none_or(|&c| c == separator || c == '\n' || c == '\r');
    // The content of a quoted cell starting at `start`, and where it ends.
    let quoted = |start: usize| {
        let mut cell = String::new();
        let mut i = start + 1;
        while i < chars.len() {
            if chars[i] != '"' {
                cell.push(chars[i]);
                i += 1;
            } else if chars.get(i + 1) == Some(&'"') {
                cell.push('"');
                i += 2;
            } else {
                return ends_cell(chars.get(i + 1)).then_some((cell, i + 1));
            }
        }
        None
    };

    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (cell, mut end) = match (chars[i] == '"').then(|| quoted(i)).flatten() {
            Some(found) => found,
            None => {
                let end = (i..chars.len()).find(|&at| ends_cell(chars.get(at))).unwrap_or(chars.len());
                (chars[i..end].iter().collect(), end)
            }
        };
        row.push(cell);
        match chars.get(end) {
            Some(&c) if c == separator => {
                // A separator at the very end leaves one empty cell.
                if end + 1 == chars.len() {
                    row.push(String::new());
                }
                end += 1;
            }
            Some(&c) => {
                end += if c == '\r' && chars.get(end + 1) == Some(&'\n') { 2 } else { 1 };
                rows.push(std::mem::take(&mut row));
            }
            None => {}
        }
        i = end;
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn number(cell: &str, max: i64) -> Option<i64> {
    let cell = cell.trim();
    if cell.is_empty() || !cell.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    cell.parse().ok().filter(|n| *n <= max)
}

fn read_with(text: &str, separator: char) -> (bool, ParsedList) {
    let mut rows = split_rows(text, separator);
    rows.retain(|row| row.iter().any(|cell| !cell.trim().is_empty()));
    let named = rows.first().and_then(|row| header(row));
    let columns = named.unwrap_or(DEFAULT_COLUMNS);
    let mut list = ParsedList::default();
    for row in rows.iter().skip(usize::from(named.is_some())) {
        let cell = |at: usize| row.get(at).map_or("", |cell| cell.trim());
        let optional = |at: Option<usize>| at.map(cell).filter(|c| !c.is_empty()).map(String::from);
        // A title that had line breaks inside its cell becomes one line.
        let title = cell(columns.title).split_whitespace().collect::<Vec<_>>().join(" ");
        match number(cell(columns.no), MAX_REF_NO) {
            Some(no) if !title.is_empty() => list.rows.push(Reference {
                no,
                title,
                year: columns.year.and_then(|at| number(cell(at), MAX_YEAR)),
                source_database: optional(columns.source_database),
                doi: optional(columns.doi),
            }),
            _ => list.skipped += 1,
        }
    }
    (named.is_some(), list)
}

/// Reads the text with each separator and keeps the best reading: one with a header
/// row first, then the one with the most usable rows.
pub fn parse(text: &str) -> ParsedList {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    SEPARATORS
        .iter()
        .map(|&separator| read_with(text, separator))
        // `min_by_key` keeps the first of equals, which follows `SEPARATORS`.
        .min_by_key(|(named, list)| std::cmp::Reverse((*named, list.rows.len())))
        .map(|(_, list)| list)
        .unwrap_or_default()
}

pub fn count(conn: &Connection) -> Result<usize, LibraryError> {
    Ok(conn.query_row("SELECT count(*) FROM reference", [], |r| r.get::<_, i64>(0))? as usize)
}

/// Replaces the library's reference list with the rows in `text`. With an existing
/// list, nothing changes unless `replace` is set (the user's confirmation). The old
/// list goes and the new one arrives in one transaction.
pub fn import(conn: &Connection, text: &str, replace: bool) -> Result<ImportResult, LibraryError> {
    let list = parse(text);
    if list.rows.is_empty() {
        return Ok(ImportResult::Empty);
    }
    let existing = count(conn)?;
    if existing > 0 && !replace {
        return Ok(ImportResult::NeedsConfirmation { existing });
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute("DELETE FROM reference", [])?;
    {
        let mut insert = transaction.prepare(
            "INSERT INTO reference (no, title, year, source_database, doi) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for row in &list.rows {
            insert.execute(params![row.no, row.title, row.year, row.source_database, row.doi])?;
        }
    }
    transaction.commit()?;

    let mut seen = BTreeMap::<i64, usize>::new();
    for row in &list.rows {
        *seen.entry(row.no).or_default() += 1;
    }
    Ok(ImportResult::Imported {
        imported: list.rows.len(),
        skipped: list.skipped,
        duplicates: seen.into_iter().filter(|(_, times)| *times > 1).map(|(no, _)| no).collect(),
    })
}

/// The reference list in the order it was imported.
pub fn list(conn: &Connection) -> Result<Vec<ReferenceChoice>, LibraryError> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.no, r.title, r.year, p.id, p.file_path, p.title
         FROM reference AS r LEFT JOIN paper AS p ON p.reference_id = r.id
         ORDER BY r.rowid",
    )?;
    let rows = stmt.query_map([], |r| {
        let linked_paper_id: Option<String> = r.get(4)?;
        let file_path: Option<String> = r.get(5)?;
        let title: Option<String> = r.get(6)?;
        Ok(ReferenceChoice {
            id: r.get(0)?,
            no: r.get(1)?,
            title: r.get(2)?,
            year: r.get(3)?,
            linked_paper_name: file_path
                .map(|path| path.rsplit('/').next().unwrap_or(&path).to_string())
                .or(title),
            linked_paper_id,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn unknown(detail: &str) -> LibraryError {
    LibraryError::new(ErrorCode::Other, detail.to_string())
}

/// The metadata of a paper after linking `reference`: its values, and the paper's own
/// where the reference has none, so linking never empties a field.
fn filled_from(reference: &Reference, current: &Metadata) -> Metadata {
    let text = |new: &Option<String>, old: &str| new.clone().unwrap_or_else(|| old.to_string());
    Metadata {
        ref_no: Some(reference.no),
        title: reference.title.clone(),
        year: reference.year.or(current.year),
        source_database: text(&reference.source_database, &current.source_database),
        doi: text(&reference.doi, &current.doi),
    }
}

/// Whether a field the paper already has would get another value.
fn replaces(current: &Metadata, new: &Metadata) -> bool {
    let text = |old: &str, new: &str| !old.trim().is_empty() && old.trim() != new.trim();
    current.ref_no.is_some_and(|no| new.ref_no != Some(no))
        || current.year.is_some_and(|year| new.year != Some(year))
        || text(&current.title, &new.title)
        || text(&current.source_database, &new.source_database)
        || text(&current.doi, &new.doi)
}

/// Links a paper to a reference and fills its metadata from it (FR-05.3, FR-05.4).
/// `replace` is the user's confirmation to change fields that are already filled.
pub fn link(
    conn: &Connection,
    paper_id: &str,
    reference_id: &str,
    replace: bool,
) -> Result<LinkResult, LibraryError> {
    let reference = conn
        .query_row(
            "SELECT no, title, year, source_database, doi FROM reference WHERE id = ?1",
            [reference_id],
            |r| {
                Ok(Reference {
                    no: r.get(0)?,
                    title: r.get(1)?,
                    year: r.get(2)?,
                    source_database: r.get(3)?,
                    doi: r.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| unknown("no such reference"))?;
    if let Some(other) = list(conn)?
        .into_iter()
        .find(|choice| choice.id == reference_id && choice.linked_paper_id.as_deref().is_some_and(|id| id != paper_id))
    {
        return Ok(LinkResult::Taken { paper: other.linked_paper_name.unwrap_or_default() });
    }
    let current = metadata::get_metadata(conn, paper_id)?.metadata;
    let new = filled_from(&reference, &current);
    if !replace && replaces(&current, &new) {
        return Ok(LinkResult::NeedsConfirmation);
    }
    let optional = |text: &str| Some(text.trim()).filter(|text| !text.is_empty()).map(String::from);
    // One statement, so the link and the metadata change together.
    conn.execute(
        "UPDATE paper SET reference_id = ?1, ref_no = ?2, title = ?3, year = ?4, source_database = ?5, doi = ?6
         WHERE id = ?7",
        params![
            reference_id,
            new.ref_no,
            new.title,
            new.year,
            optional(&new.source_database),
            optional(&new.doi),
            paper_id
        ],
    )?;
    let described = metadata::get_metadata(conn, paper_id)?;
    Ok(LinkResult::Linked { metadata: described.metadata, same_ref_no: described.same_ref_no })
}

/// Removes a paper's link; its metadata stays.
pub fn unlink(conn: &Connection, paper_id: &str) -> Result<(), LibraryError> {
    if conn.execute("UPDATE paper SET reference_id = NULL WHERE id = ?1", [paper_id])? == 0 {
        return Err(unknown("no such paper"));
    }
    Ok(())
}

/// The characters Windows-1252 puts at 0x80 to 0x9F; elsewhere it matches Latin-1.
const WINDOWS_1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// Text as Excel saves it: UTF-8, UTF-16 with a byte order mark ("Unicode Text"), or
/// otherwise the Windows code page of a plain "CSV". `None` for a binary file.
fn decode(bytes: &[u8]) -> Option<String> {
    let utf16 = |rest: &[u8], unit: fn([u8; 2]) -> u16| {
        let (pairs, odd) = rest.as_chunks::<2>();
        if !odd.is_empty() {
            return None;
        }
        String::from_utf16(&pairs.iter().map(|&pair| unit(pair)).collect::<Vec<_>>()).ok()
    };
    let text = match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, u16::from_le_bytes)?,
        [0xFE, 0xFF, rest @ ..] => utf16(rest, u16::from_be_bytes)?,
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_string(),
            Err(_) => bytes
                .iter()
                .map(|&b| match b {
                    0x80..=0x9F => WINDOWS_1252[usize::from(b - 0x80)],
                    _ => char::from(b),
                })
                .collect(),
        },
    };
    // An .xlsx or .xls file chosen by mistake.
    if text.contains('\0') {
        return None;
    }
    Some(text.strip_prefix('\u{feff}').map(String::from).unwrap_or(text))
}

/// The text of a chosen CSV file, or `None` when it cannot be read as one.
pub fn read_file(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    decode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;

    fn library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        (dir, conn)
    }

    fn row(no: i64, title: &str) -> Reference {
        Reference { no, title: title.into(), year: None, source_database: None, doi: None }
    }

    fn full(no: i64, title: &str) -> Reference {
        Reference {
            year: Some(2021),
            source_database: Some("Scopus".into()),
            doi: Some("10.1000/x".into()),
            ..row(no, title)
        }
    }

    fn titles(conn: &Connection) -> Vec<String> {
        let mut stmt = conn.prepare("SELECT title FROM reference ORDER BY rowid").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn reads_every_separator_with_and_without_a_header() {
        for separator in ["\t", ";", ","] {
            let data = ["46", "Tilapia ponds", "2021", "Scopus", "10.1000/x"].join(separator);
            let head = ["Reference No", "Title", "Year", "Source Database", "DOI"].join(separator);
            for text in [data.clone(), format!("{head}\r\n{data}\r\n")] {
                let list = parse(&text);
                assert_eq!(list.rows, [full(46, "Tilapia ponds")], "{text:?}");
                assert_eq!(list.skipped, 0);
            }
        }
    }

    #[test]
    fn recognizes_indonesian_headers_in_any_order() {
        let text = "DOI;Judul;Sumber Database;Tahun;No. Referensi\n10.1000/x;Kolam nila;Scopus;2021;46";
        assert_eq!(parse(text).rows, [full(46, "Kolam nila")]);
    }

    #[test]
    fn prefers_the_reference_number_over_a_row_counter_and_ignores_other_columns() {
        let text = "No\tAuthors\tReference No\tTitle\n1\tKim\t46\tTilapia";
        assert_eq!(parse(text).rows, [row(46, "Tilapia")]);
    }

    #[test]
    fn a_title_may_hold_other_separators() {
        let title = "Fish, water, feed, and cost: a review";
        for text in [format!("1;{title}"), format!("1\t{title}"), format!("1,\"{title}\"")] {
            assert_eq!(parse(&text).rows, [row(1, title)], "{text:?}");
        }
    }

    #[test]
    fn reads_quoted_cells_and_keeps_stray_quotes() {
        let text = "1;\"A; \"\"B\"\"\r\nsecond line\"\n2;\"Smart\" ponds\n3;\"never closed\n4;Next";
        assert_eq!(
            parse(text).rows,
            [row(1, "A; \"B\" second line"), row(2, "\"Smart\" ponds"), row(3, "\"never closed"), row(4, "Next")]
        );
    }

    #[test]
    fn skips_and_counts_incomplete_rows_but_not_blank_lines() {
        let text = "Reference No;Title;Year\n1;A;2020\n;No number;2020\n2;;2020\nx3;B\n-4;C\n\n;;\n5;E;n.d.";
        let list = parse(text);
        assert_eq!(list.rows, [Reference { year: Some(2020), ..row(1, "A") }, row(5, "E")]);
        assert_eq!(list.skipped, 4);
    }

    #[test]
    fn finds_nothing_in_text_without_usable_rows() {
        for text in ["", "  \n\n", "just a sentence", "Reference No;Title;Year\n"] {
            assert!(parse(text).rows.is_empty(), "{text:?}");
        }
    }

    #[test]
    fn imports_and_reports_duplicate_numbers() {
        let (_dir, conn) = library();
        let result = import(&conn, "46;A\n12;B\n46;C\n12;D\n46;E\n7;F\n;G", false).unwrap();
        assert_eq!(result, ImportResult::Imported { imported: 6, skipped: 1, duplicates: vec![12, 46] });
        assert_eq!(count(&conn).unwrap(), 6);
    }

    #[test]
    fn replaces_an_old_list_only_after_confirmation_and_keeps_papers() {
        let (_dir, conn) = library();
        import(&conn, "46;Old", false).unwrap();
        conn.execute_batch(
            "INSERT INTO paper (reference_id, ref_no, title, decision, assessment_note)
             SELECT id, 46, 'Old', 'include', 'note' FROM reference;",
        )
        .unwrap();

        let asked = import(&conn, "1;New", false).unwrap();
        assert_eq!(asked, ImportResult::NeedsConfirmation { existing: 1 });
        assert_eq!(titles(&conn), ["Old"]);

        import(&conn, "1;New\n2;Newer", true).unwrap();
        assert_eq!(titles(&conn), ["New", "Newer"]);
        let paper: (Option<String>, i64, String, String, String) = conn
            .query_row("SELECT reference_id, ref_no, title, decision, assessment_note FROM paper", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap();
        assert_eq!(paper, (None, 46, "Old".into(), "include".into(), "note".into()));
    }

    #[test]
    fn an_import_without_rows_never_removes_the_old_list() {
        let (_dir, conn) = library();
        import(&conn, "46;Old", false).unwrap();
        assert_eq!(import(&conn, "nothing here", true).unwrap(), ImportResult::Empty);
        assert_eq!(titles(&conn), ["Old"]);
    }

    #[test]
    fn a_failed_replacement_keeps_the_old_list() {
        let (_dir, conn) = library();
        import(&conn, "46;Old", false).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_second BEFORE INSERT ON reference WHEN new.no = 2
             BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
        assert!(import(&conn, "1;New\n2;Fails", true).is_err());
        assert_eq!(titles(&conn), ["Old"]);
    }

    fn paper(conn: &Connection, file: &str) -> String {
        conn.query_row("INSERT INTO paper (file_path) VALUES (?1) RETURNING id", [file], |r| r.get(0)).unwrap()
    }

    fn linked_to(conn: &Connection, paper: &str) -> Option<String> {
        conn.query_row("SELECT reference_id FROM paper WHERE id = ?1", [paper], |r| r.get(0)).unwrap()
    }

    fn metadata_of(conn: &Connection, paper: &str) -> Metadata {
        metadata::get_metadata(conn, paper).unwrap().metadata
    }

    #[test]
    fn linking_fills_an_empty_paper_without_asking() {
        // FR-05.3.
        let (_dir, conn) = library();
        import(&conn, "46;Tilapia ponds;2021;Scopus;10.1000/x", false).unwrap();
        let kim = paper(&conn, "pdfs/kim.pdf");
        let id = list(&conn).unwrap()[0].id.clone();

        let result = link(&conn, &kim, &id, false).unwrap();
        let expected = Metadata {
            ref_no: Some(46),
            title: "Tilapia ponds".into(),
            year: Some(2021),
            source_database: "Scopus".into(),
            doi: "10.1000/x".into(),
        };
        assert_eq!(result, LinkResult::Linked { metadata: expected.clone(), same_ref_no: vec![] });
        assert_eq!(metadata_of(&conn, &kim), expected);
        let choice = &list(&conn).unwrap()[0];
        assert_eq!(choice.linked_paper_id.as_deref(), Some(kim.as_str()));
        assert_eq!(choice.linked_paper_name.as_deref(), Some("kim.pdf"));
        // Choosing it again changes nothing and asks nothing.
        assert!(matches!(link(&conn, &kim, &id, false).unwrap(), LinkResult::Linked { .. }));
    }

    #[test]
    fn replaces_filled_fields_only_after_confirmation_and_never_empties_one() {
        let (_dir, conn) = library();
        import(&conn, "46;Tilapia ponds", false).unwrap();
        let kim = paper(&conn, "pdfs/kim.pdf");
        conn.execute(
            "UPDATE paper SET title = 'My title', year = 2020, doi = 'mine', decision = 'exclude', assessment_note = 'n'",
            [],
        )
        .unwrap();
        let id = list(&conn).unwrap()[0].id.clone();
        let before = metadata_of(&conn, &kim);

        assert_eq!(link(&conn, &kim, &id, false).unwrap(), LinkResult::NeedsConfirmation);
        assert_eq!(metadata_of(&conn, &kim), before);
        assert_eq!(linked_to(&conn, &kim), None);

        link(&conn, &kim, &id, true).unwrap();
        assert_eq!(
            metadata_of(&conn, &kim),
            Metadata { ref_no: Some(46), title: "Tilapia ponds".into(), ..before }
        );
        let kept: (String, String) = conn
            .query_row("SELECT decision, assessment_note FROM paper", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(kept, ("exclude".into(), "n".into()));
    }

    #[test]
    fn one_reference_links_to_one_paper() {
        // FR-05.4.
        let (_dir, conn) = library();
        import(&conn, "46;A\n47;B", false).unwrap();
        let (first, second) = (paper(&conn, "pdfs/kim-2021.pdf"), paper(&conn, "pdfs/b.pdf"));
        let ids: Vec<String> = list(&conn).unwrap().into_iter().map(|choice| choice.id).collect();
        link(&conn, &first, &ids[0], false).unwrap();

        let refused = link(&conn, &second, &ids[0], true).unwrap();
        assert_eq!(refused, LinkResult::Taken { paper: "kim-2021.pdf".into() });
        assert_eq!(linked_to(&conn, &second), None);
        assert_eq!(metadata_of(&conn, &second), Metadata::default());

        // Moving to another reference frees the first one.
        link(&conn, &first, &ids[1], true).unwrap();
        assert!(matches!(link(&conn, &second, &ids[0], false).unwrap(), LinkResult::Linked { .. }));
    }

    #[test]
    fn unlinking_keeps_the_metadata() {
        let (_dir, conn) = library();
        import(&conn, "46;A", false).unwrap();
        let kim = paper(&conn, "pdfs/kim.pdf");
        let id = list(&conn).unwrap()[0].id.clone();
        link(&conn, &kim, &id, false).unwrap();
        unlink(&conn, &kim).unwrap();
        assert_eq!(linked_to(&conn, &kim), None);
        assert_eq!(metadata_of(&conn, &kim).title, "A");
        assert_eq!(list(&conn).unwrap()[0].linked_paper_id, None);
        assert!(unlink(&conn, "nope").is_err());
        assert!(link(&conn, &kim, "nope", false).is_err());
    }

    #[test]
    fn only_a_new_reference_number_releases_the_link() {
        let (_dir, conn) = library();
        import(&conn, "46;A", false).unwrap();
        let kim = paper(&conn, "pdfs/kim.pdf");
        let id = list(&conn).unwrap()[0].id.clone();
        link(&conn, &kim, &id, false).unwrap();

        let edited = Metadata { title: "Other".into(), year: Some(1999), ..metadata_of(&conn, &kim) };
        metadata::set_metadata(&conn, &kim, &edited).unwrap();
        assert_eq!(linked_to(&conn, &kim), Some(id));
        metadata::set_metadata(&conn, &kim, &Metadata { ref_no: Some(4), ..edited }).unwrap();
        assert_eq!(linked_to(&conn, &kim), None);
    }

    #[test]
    fn decodes_the_encodings_excel_writes_and_refuses_binary_files() {
        let text = "1;Café “ponds”";
        assert_eq!(decode(format!("\u{feff}{text}").as_bytes()).as_deref(), Some(text));
        for bytes_of in [u16::to_le_bytes, u16::to_be_bytes] {
            let utf16: Vec<u8> = std::iter::once(0xFEFF).chain(text.encode_utf16()).flat_map(bytes_of).collect();
            assert_eq!(decode(&utf16).as_deref(), Some(text));
        }
        assert_eq!(decode(b"1;Caf\xE9 \x93ponds\x94").as_deref(), Some(text));
        assert_eq!(decode(b"PK\x03\x04\x00\x00"), None);
    }

    #[test]
    fn reads_a_file_and_refuses_a_missing_or_huge_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("list.csv");
        assert_eq!(read_file(&path), None);
        std::fs::write(&path, "1;A").unwrap();
        assert_eq!(read_file(&path).as_deref(), Some("1;A"));
        std::fs::write(&path, vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert_eq!(read_file(&path), None);
    }

    #[test]
    fn the_result_has_the_shape_the_ui_expects() {
        let asked = serde_json::to_value(ImportResult::NeedsConfirmation { existing: 3 }).unwrap();
        assert_eq!(asked, serde_json::json!({ "status": "needs_confirmation", "existing": 3 }));
    }
}
