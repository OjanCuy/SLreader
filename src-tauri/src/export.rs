//! Exports of the open library as CSV (FR-06.4): the decisions (FR-06.1) and the
//! extraction, the tagged highlight texts of each paper (FR-06.2). The Markdown files
//! of the highlights and notes (FR-06.3) are in `export/markdown.rs`.
//!
//! Only reads the library. A CSV file is written next to its target and then renamed over
//! it, so a failed export (the file is open in Excel, the disk is full) leaves an
//! existing file as it was.
//!
//! Every function that reads the library receives the connection of the open library
//! from `libraries::library_for_reading` (FR-14.4).

pub mod markdown;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use rusqlite::Connection;

use crate::libraries::{ErrorCode, LibraryError};
use crate::settings::CsvDelimiter;

/// The header of the decisions file, in this order and wording (FR-06.1).
const DECISIONS_HEADER: [&str; 5] =
    ["Reference No", "Title", "Full-Text Decision", "Exclusion Reason", "Assessment Notes"];

/// The columns of the extraction file before the one column per tag.
const EXTRACTION_HEADER: [&str; 3] = ["Reference No", "Title", "Full-Text Decision"];

/// Between the highlight texts in one cell, whatever the column separator (FR-06.2).
const TEXT_SEPARATOR: &str = "; ";

/// The order of the papers in both files: by reference number; papers without one come
/// last, in the order they were added.
const PAPER_ORDER: &str = "p.ref_no IS NULL, p.ref_no, p.created_at, p.rowid";

/// Excel reads the file as UTF-8 only with this byte order mark.
const BOM: &str = "\u{feff}";

/// A text cell that Excel does not run as a formula: one starting like a formula gets
/// an apostrophe in front.
fn text_cell(text: &str) -> String {
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_string()
    }
}

/// Adds one row, quoting cells that hold the separator, a quote, or a line break
/// (FR-06.4).
fn push_row<S: AsRef<str>>(csv: &mut String, cells: &[S], separator: char) {
    for (at, cell) in cells.iter().enumerate() {
        if at > 0 {
            csv.push(separator);
        }
        let cell = cell.as_ref();
        if cell.contains([separator, '"', '\n', '\r']) {
            csv.push('"');
            csv.push_str(&cell.replace('"', "\"\""));
            csv.push('"');
        } else {
            csv.push_str(cell);
        }
    }
    csv.push_str("\r\n");
}

/// The word for a stored decision, as both files write it.
fn decision_word(stored: &str) -> Result<&'static str, LibraryError> {
    match stored {
        "include" => Ok("Include"),
        "exclude" => Ok("Exclude"),
        "undecided" => Ok("Undecided"),
        other => Err(LibraryError::new(ErrorCode::Other, format!("unknown decision {other}"))),
    }
}

/// A paper without a title is known by its file name.
fn shown_title(title: String, file_path: Option<String>) -> String {
    match file_path {
        Some(path) if title.trim().is_empty() => path.rsplit('/').next().unwrap_or(path.as_str()).to_string(),
        _ => title,
    }
}

/// The decisions of every paper, ordered by reference number.
pub fn decisions_csv(conn: &Connection, delimiter: CsvDelimiter) -> Result<String, LibraryError> {
    let separator = delimiter.character();
    let mut csv = BOM.to_string();
    push_row(&mut csv, &DECISIONS_HEADER, separator);
    // The reason counts only while the paper is excluded (FR-04.3).
    let mut stmt = conn.prepare(&format!(
        "SELECT p.ref_no, p.title, p.file_path, p.decision, r.code, p.assessment_note
         FROM paper AS p
         LEFT JOIN exclusion_reason AS r ON r.id = p.exclusion_reason_id AND p.decision = 'exclude'
         ORDER BY {PAPER_ORDER}",
    ))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let ref_no: Option<i64> = row.get(0)?;
        let title = shown_title(row.get(1)?, row.get(2)?);
        let decision: String = row.get(3)?;
        let reason: Option<String> = row.get(4)?;
        let note: String = row.get(5)?;
        let cells = [
            ref_no.map(|no| no.to_string()).unwrap_or_default(),
            text_cell(&title),
            decision_word(&decision)?.to_string(),
            text_cell(reason.as_deref().unwrap_or_default()),
            text_cell(&note),
        ];
        push_row(&mut csv, &cells, separator);
    }
    Ok(csv)
}

/// A highlight's text without the line breaks it got from the PDF page.
fn one_line(text: &str) -> String {
    text.split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One row per paper and one column per extraction tag, in the order of the tags in
/// Settings (FR-06.2). A cell holds the texts of the paper's highlights with that tag,
/// by page, then oldest first. Highlights without a tag are not part of the file.
pub fn extraction_csv(conn: &Connection, delimiter: CsvDelimiter) -> Result<String, LibraryError> {
    let separator = delimiter.character();
    let mut stmt = conn.prepare("SELECT id, label FROM extraction_tag ORDER BY sort_order, rowid")?;
    let tags: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    // The texts of each paper and tag.
    let mut texts: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT paper_id, extraction_tag_id, text FROM highlight
         WHERE extraction_tag_id IS NOT NULL ORDER BY page, created_at, rowid",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let text: String = row.get(2)?;
        texts.entry((row.get(0)?, row.get(1)?)).or_default().push(one_line(&text));
    }

    let mut csv = BOM.to_string();
    let mut header: Vec<String> = EXTRACTION_HEADER.map(String::from).to_vec();
    header.extend(tags.iter().map(|(_, label)| text_cell(label)));
    push_row(&mut csv, &header, separator);

    let mut stmt = conn.prepare(&format!(
        "SELECT p.id, p.ref_no, p.title, p.file_path, p.decision FROM paper AS p ORDER BY {PAPER_ORDER}",
    ))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let paper: String = row.get(0)?;
        let ref_no: Option<i64> = row.get(1)?;
        let title = shown_title(row.get(2)?, row.get(3)?);
        let decision: String = row.get(4)?;
        let mut cells = vec![
            ref_no.map(|no| no.to_string()).unwrap_or_default(),
            text_cell(&title),
            decision_word(&decision)?.to_string(),
        ];
        for (tag, _) in &tags {
            let found = texts.remove(&(paper.clone(), tag.clone())).unwrap_or_default();
            cells.push(text_cell(&found.join(TEXT_SEPARATOR)));
        }
        push_row(&mut csv, &cells, separator);
    }
    Ok(csv)
}

/// How writing an exported file ended, if not with an error.
#[derive(Debug, PartialEq, Eq)]
pub enum Written {
    Saved,
    /// Another app holds the file, e.g. Excel. It was not changed (PRD Section 8).
    InUse,
}

/// Whether another app has the file open in a way that keeps it from being replaced.
#[cfg(windows)]
fn held_elsewhere(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    const DELETE: u32 = 0x0001_0000;
    const SHARE_ALL: u32 = 7;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    // Asks for the right to replace the file, without touching it.
    fs::OpenOptions::new()
        .access_mode(DELETE)
        .share_mode(SHARE_ALL)
        .open(path)
        .is_err_and(|error| error.raw_os_error() == Some(ERROR_SHARING_VIOLATION))
}

#[cfg(not(windows))]
fn held_elsewhere(_path: &Path) -> bool {
    false
}

/// A name like `path`'s that no file in its folder has: "decisions (2).csv", then
/// "decisions (3).csv", and so on.
pub fn free_name(path: &Path) -> String {
    let part = |part: Option<&std::ffi::OsStr>| part.unwrap_or_default().to_string_lossy().into_owned();
    let stem = part(path.file_stem());
    let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    // A number the name already has is counted on, not repeated.
    let base = stem
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once(" ("))
        .filter(|(_, number)| number.parse::<u32>().is_ok())
        .map_or(stem.as_str(), |(base, _)| base);
    (2..1000)
        .map(|number| format!("{base} ({number}){extension}"))
        .find(|name| *name != part(path.file_name()) && !path.with_file_name(name).exists())
        // The save dialog still asks before it replaces a file.
        .unwrap_or_else(|| format!("{base} (new){extension}"))
}

/// Writes an exported file. Unless it is saved, an existing file at `path` is untouched.
pub fn write_file(path: &Path, contents: &str) -> Result<Written, LibraryError> {
    let failed = |error: std::io::Error| LibraryError::new(ErrorCode::Other, error.to_string());
    let name = path
        .file_name()
        .ok_or_else(|| LibraryError::new(ErrorCode::Other, "not a file path"))?;
    // A name no existing file has; `create_new` refuses one that does.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let temporary = path.with_file_name(format!("{}.{stamp}.tmp", name.to_string_lossy()));
    let mut file = fs::File::create_new(&temporary).map_err(failed)?;
    let written = file.write_all(contents.as_bytes()).and_then(|()| file.sync_all());
    drop(file);
    // Only the file made here is removed.
    let discard = || {
        let _ = fs::remove_file(&temporary);
    };
    if let Err(error) = written {
        discard();
        return Err(failed(error));
    }
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(Written::Saved),
        Err(error) => {
            discard();
            if held_elsewhere(path) { Ok(Written::InUse) } else { Err(failed(error)) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;
    use rusqlite::params;

    fn library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        conn.execute(
            "INSERT INTO exclusion_reason (id, code, label, sort_order) VALUES ('e3', 'E3', 'Wrong population', 0)",
            [],
        )
        .unwrap();
        (dir, conn)
    }

    fn paper(conn: &Connection, ref_no: Option<i64>, title: &str, file: Option<&str>, decision: &str, note: &str) {
        conn.execute(
            "INSERT INTO paper (ref_no, title, file_path, decision, exclusion_reason_id, assessment_note)
             VALUES (?1, ?2, ?3, ?4, 'e3', ?5)",
            params![ref_no, title, file, decision, note],
        )
        .unwrap();
    }

    fn lines(conn: &Connection, delimiter: CsvDelimiter) -> Vec<String> {
        let csv = decisions_csv(conn, delimiter).unwrap();
        let body = csv.strip_prefix(BOM).expect("a byte order mark");
        assert!(body.ends_with("\r\n"));
        body.trim_end_matches("\r\n").split("\r\n").map(String::from).collect()
    }

    #[test]
    fn an_empty_library_exports_the_exact_header() {
        let (_dir, conn) = library();
        assert_eq!(
            decisions_csv(&conn, CsvDelimiter::Semicolon).unwrap().as_bytes(),
            b"\xEF\xBB\xBFReference No;Title;Full-Text Decision;Exclusion Reason;Assessment Notes\r\n"
        );
        assert_eq!(
            lines(&conn, CsvDelimiter::Comma),
            ["Reference No,Title,Full-Text Decision,Exclusion Reason,Assessment Notes"]
        );
    }

    #[test]
    fn writes_the_reason_only_for_excluded_papers_ordered_by_number() {
        // FR-06.1 and FR-04.3. Every paper keeps a reason in the database.
        let (_dir, conn) = library();
        paper(&conn, None, "Later", None, "undecided", "");
        paper(&conn, Some(46), "B", None, "exclude", "n");
        paper(&conn, Some(5), "A", None, "include", "");
        paper(&conn, None, " ", Some("pdfs/kim-2021.pdf"), "exclude", "");
        conn.execute("UPDATE paper SET exclusion_reason_id = NULL WHERE title = ' '", []).unwrap();
        assert_eq!(
            lines(&conn, CsvDelimiter::Semicolon)[1..],
            ["5;A;Include;;", "46;B;Exclude;E3;n", ";Later;Undecided;;", ";kim-2021.pdf;Exclude;;"]
        );
    }

    #[test]
    fn quotes_cells_with_the_separator_quotes_or_line_breaks() {
        let (_dir, conn) = library();
        paper(&conn, Some(1), "Fish; water, feed", None, "include", "Said \"no\"\nTwice é");
        let semicolon = decisions_csv(&conn, CsvDelimiter::Semicolon).unwrap();
        assert!(semicolon.ends_with("1;\"Fish; water, feed\";Include;;\"Said \"\"no\"\"\nTwice é\"\r\n"));
        let comma = decisions_csv(&conn, CsvDelimiter::Comma).unwrap();
        assert!(comma.contains("1,\"Fish; water, feed\",Include,,"));
        // A semicolon needs no quotes between commas.
        conn.execute("UPDATE paper SET title = 'A; B'", []).unwrap();
        assert!(decisions_csv(&conn, CsvDelimiter::Comma).unwrap().contains("1,A; B,Include"));
    }

    #[test]
    fn no_cell_starts_like_a_formula() {
        let (_dir, conn) = library();
        for (no, start) in ["=1+1", "+1", "- small sample", "@x", "\tx"].iter().enumerate() {
            paper(&conn, Some(no as i64), start, None, "include", start);
        }
        conn.execute("UPDATE exclusion_reason SET code = '=E'", []).unwrap();
        conn.execute("UPDATE paper SET decision = 'exclude' WHERE ref_no = 0", []).unwrap();
        let rows = lines(&conn, CsvDelimiter::Semicolon);
        assert_eq!(rows[1], "0;'=1+1;Exclude;'=E;'=1+1");
        assert_eq!(rows[3], "2;'- small sample;Include;;'- small sample");
        assert_eq!(rows[5], "4;'\tx;Include;;'\tx");
        // The stored text stays as typed.
        let stored: String = conn.query_row("SELECT title FROM paper WHERE ref_no = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(stored, "=1+1");
    }

    #[test]
    fn exporting_changes_nothing() {
        let (_dir, conn) = library();
        paper(&conn, Some(1), "A", None, "include", "n");
        let before = conn.total_changes();
        decisions_csv(&conn, CsvDelimiter::Semicolon).unwrap();
        extraction_csv(&conn, CsvDelimiter::Semicolon).unwrap();
        assert_eq!(conn.total_changes(), before);
    }

    fn tag(conn: &Connection, id: &str, label: &str, order: i64) {
        conn.execute(
            "INSERT INTO extraction_tag (id, label, sort_order) VALUES (?1, ?2, ?3)",
            params![id, label, order],
        )
        .unwrap();
    }

    /// A highlight on the paper with this reference number.
    fn highlight(conn: &Connection, ref_no: i64, tag: Option<&str>, page: u32, text: &str) {
        conn.execute(
            "INSERT INTO highlight (paper_id, page, highlight_category_id, extraction_tag_id, text, rects)
             VALUES ((SELECT id FROM paper WHERE ref_no = ?1), ?2,
                     (SELECT id FROM highlight_category WHERE color = 'blue'), ?3, ?4, '[]')",
            params![ref_no, page, tag, text],
        )
        .unwrap();
    }

    fn extraction_lines(conn: &Connection, delimiter: CsvDelimiter) -> Vec<String> {
        let csv = extraction_csv(conn, delimiter).unwrap();
        let body = csv.strip_prefix(BOM).expect("a byte order mark");
        assert!(body.ends_with("\r\n"));
        body.trim_end_matches("\r\n").split("\r\n").map(String::from).collect()
    }

    #[test]
    fn the_tag_columns_follow_the_order_in_settings() {
        // FR-06.2: "FCR, Species" puts FCR first, whichever was added first.
        let (_dir, conn) = library();
        tag(&conn, "s", "Species", 1);
        tag(&conn, "f", "FCR", 0);
        assert_eq!(
            extraction_lines(&conn, CsvDelimiter::Semicolon),
            ["Reference No;Title;Full-Text Decision;FCR;Species"]
        );
        conn.execute("UPDATE extraction_tag SET sort_order = 2 WHERE id = 'f'", []).unwrap();
        assert_eq!(
            extraction_lines(&conn, CsvDelimiter::Comma),
            ["Reference No,Title,Full-Text Decision,Species,FCR"]
        );
    }

    #[test]
    fn one_row_per_paper_with_the_texts_of_each_tag_by_page() {
        let (_dir, conn) = library();
        tag(&conn, "f", "FCR", 0);
        tag(&conn, "s", "Species", 1);
        paper(&conn, Some(46), "B", None, "exclude", "");
        paper(&conn, Some(5), "A", None, "include", "");
        paper(&conn, None, "", Some("pdfs/kim-2021.pdf"), "undecided", "");
        highlight(&conn, 5, Some("f"), 7, "later");
        highlight(&conn, 5, Some("f"), 3, "first");
        highlight(&conn, 5, Some("f"), 7, "latest");
        highlight(&conn, 5, Some("s"), 1, "Tilapia");
        highlight(&conn, 5, None, 1, "no tag");
        highlight(&conn, 46, Some("s"), 2, "Catfish");
        assert_eq!(
            extraction_lines(&conn, CsvDelimiter::Comma)[1..],
            [
                "5,A,Include,first; later; latest,Tilapia",
                "46,B,Exclude,,Catfish",
                ",kim-2021.pdf,Undecided,,",
            ]
        );
        // Between semicolons the joined texts need quotes (FR-06.4).
        assert_eq!(
            extraction_lines(&conn, CsvDelimiter::Semicolon)[1],
            "5;A;Include;\"first; later; latest\";Tilapia"
        );
    }

    #[test]
    fn highlight_texts_lose_their_line_breaks_and_nothing_else() {
        let (_dir, conn) = library();
        tag(&conn, "f", "FCR", 0);
        paper(&conn, Some(1), "A", None, "include", "");
        highlight(&conn, 1, Some("f"), 1, "The \"FCR\" \r\nwas  1.4\n\nµg – é\u{2028}end");
        assert_eq!(
            extraction_lines(&conn, CsvDelimiter::Semicolon)[1],
            "1;A;Include;\"The \"\"FCR\"\" was  1.4 µg – é end\""
        );
        // The stored text keeps them.
        let stored: String = conn.query_row("SELECT text FROM highlight", [], |r| r.get(0)).unwrap();
        assert!(stored.contains("\r\n"));
    }

    #[test]
    fn no_extraction_cell_starts_like_a_formula() {
        let (_dir, conn) = library();
        tag(&conn, "f", "=Tag", 0);
        paper(&conn, Some(1), "@Title", None, "include", "");
        highlight(&conn, 1, Some("f"), 1, "-0.5 change");
        highlight(&conn, 1, Some("f"), 2, "=2");
        assert_eq!(
            extraction_lines(&conn, CsvDelimiter::Comma),
            ["Reference No,Title,Full-Text Decision,'=Tag", "1,'@Title,Include,'-0.5 change; =2"]
        );
    }

    #[test]
    fn without_tags_only_the_paper_columns_are_written() {
        let (_dir, conn) = library();
        paper(&conn, Some(1), "A", None, "include", "");
        highlight(&conn, 1, None, 1, "no tag");
        assert_eq!(
            extraction_lines(&conn, CsvDelimiter::Semicolon),
            ["Reference No;Title;Full-Text Decision", "1;A;Include"]
        );
    }

    #[test]
    fn writes_and_replaces_a_file_without_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.csv");
        write_file(&path, "old").unwrap();
        write_file(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn a_file_open_elsewhere_is_reported_and_not_touched() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.csv");
        fs::write(&path, "old").unwrap();
        // Nothing shared, reading shared (as Excel does), reading and writing shared.
        for share in [0, 1, 3] {
            let held = fs::OpenOptions::new().read(true).share_mode(share).open(&path).unwrap();
            assert_eq!(write_file(&path, "new").unwrap(), Written::InUse, "share mode {share}");
            drop(held);
            assert_eq!(fs::read_to_string(&path).unwrap(), "old");
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_read_only_file_is_an_error_not_a_file_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.csv");
        fs::write(&path, "old").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions.clone()).unwrap();
        assert!(write_file(&path, "new").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        // So that the folder can be removed.
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions).unwrap();
    }

    #[test]
    fn suggests_a_name_that_no_file_has() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.csv");
        assert_eq!(free_name(&path), "decisions (2).csv");
        fs::write(dir.path().join("decisions (2).csv"), "kept").unwrap();
        assert_eq!(free_name(&path), "decisions (3).csv");
        // A locked "(3)" leads to "(4)", not "(3) (2)".
        assert_eq!(free_name(&dir.path().join("decisions (3).csv")), "decisions (4).csv");
        assert_eq!(free_name(&dir.path().join("notes (final).csv")), "notes (final) (2).csv");
    }
}
