//! Exports of the open library: the CSV of the decisions (FR-06.1, FR-06.4).
//!
//! Only reads the library. A file is written next to its target and then renamed over
//! it, so a failed export (the file is open in Excel, the disk is full) leaves an
//! existing file as it was.
//!
//! Every function that reads the library receives the connection of the open library
//! from `libraries::library_for_reading` (FR-14.4).

use std::fs;
use std::io::Write;
use std::path::Path;

use rusqlite::Connection;

use crate::libraries::{ErrorCode, LibraryError};
use crate::settings::CsvDelimiter;

/// The header of the decisions file, in this order and wording (FR-06.1).
const DECISIONS_HEADER: [&str; 5] =
    ["Reference No", "Title", "Full-Text Decision", "Exclusion Reason", "Assessment Notes"];

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

/// The decisions of every paper, by reference number; papers without one come last, in
/// the order they were added.
pub fn decisions_csv(conn: &Connection, delimiter: CsvDelimiter) -> Result<String, LibraryError> {
    let separator = delimiter.character();
    let mut csv = BOM.to_string();
    push_row(&mut csv, &DECISIONS_HEADER, separator);
    // The reason counts only while the paper is excluded (FR-04.3).
    let mut stmt = conn.prepare(
        "SELECT p.ref_no, p.title, p.file_path, p.decision, r.code, p.assessment_note
         FROM paper AS p
         LEFT JOIN exclusion_reason AS r ON r.id = p.exclusion_reason_id AND p.decision = 'exclude'
         ORDER BY p.ref_no IS NULL, p.ref_no, p.created_at, p.rowid",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let ref_no: Option<i64> = row.get(0)?;
        let title: String = row.get(1)?;
        let file_path: Option<String> = row.get(2)?;
        let decision: String = row.get(3)?;
        let reason: Option<String> = row.get(4)?;
        let note: String = row.get(5)?;
        let decision = match decision.as_str() {
            "include" => "Include",
            "exclude" => "Exclude",
            "undecided" => "Undecided",
            other => {
                return Err(LibraryError::new(ErrorCode::Other, format!("unknown decision {other}")));
            }
        };
        // A paper without a title is known by its file name.
        let title = match file_path {
            Some(path) if title.trim().is_empty() => path.rsplit('/').next().unwrap_or(path.as_str()).to_string(),
            _ => title,
        };
        let cells = [
            ref_no.map(|no| no.to_string()).unwrap_or_default(),
            text_cell(&title),
            decision.to_string(),
            text_cell(reason.as_deref().unwrap_or_default()),
            text_cell(&note),
        ];
        push_row(&mut csv, &cells, separator);
    }
    Ok(csv)
}

/// Writes an exported file. On failure an existing file at `path` is untouched.
pub fn write_file(path: &Path, contents: &str) -> Result<(), LibraryError> {
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
    written.and_then(|()| fs::rename(&temporary, path)).map_err(|error| {
        // Only the file made here.
        let _ = fs::remove_file(&temporary);
        failed(error)
    })
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
        assert_eq!(conn.total_changes(), before);
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
    fn a_file_open_elsewhere_is_not_touched() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("decisions.csv");
        fs::write(&path, "old").unwrap();
        // As Excel does: others may read the file, nobody may replace it.
        let held = fs::OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
        assert!(write_file(&path, "new").is_err());
        drop(held);
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
