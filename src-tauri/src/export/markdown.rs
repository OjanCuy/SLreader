//! "Highlights and notes (Markdown)" (FR-06.3): one file per paper with its metadata,
//! decision, assessment note, and highlights.
//!
//! The files go into a new folder made inside the one the user chose, so nothing that
//! exists is ever replaced. Text from the PDF and from the metadata is escaped to show
//! as written; the user's own notes are kept as typed.

use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use rusqlite::Connection;

use super::{PAPER_ORDER, decision_word, one_line, shown_title};
use crate::libraries::{ErrorCode, LibraryError};

/// The folder made for an export; later ones are "highlights (2)", and so on.
const FOLDER_NAME: &str = "highlights";

/// Longest file name without its ".md", in characters: leaves room for " (250)".
const MAX_STEM: usize = 200;

#[derive(Debug)]
pub struct MarkdownFile {
    pub name: String,
    pub contents: String,
}

/// What was written, for the message to the user.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownExport {
    pub folder_name: String,
    pub file_count: usize,
}

struct Paper {
    id: String,
    ref_no: Option<i64>,
    title: String,
    file_name: Option<String>,
    year: Option<i64>,
    source_database: Option<String>,
    doi: Option<String>,
    decision: String,
    /// Code and label; only while the paper is excluded (FR-04.3).
    reason: Option<(String, String)>,
    note: String,
}

/// Text on one line that Markdown shows as written. PDF text is data: it must not turn
/// into a link, a remote image, HTML, or a heading.
fn literal(text: &str) -> String {
    let text = one_line(text);
    let chars: Vec<char> = text.chars().collect();
    let word = |at: usize| chars.get(at).is_some_and(|c| c.is_alphanumeric());
    // "1. " or "1) " at the start would make a numbered list; a DOI like "10.1000/x"
    // would not.
    let digits = chars.iter().take_while(|c| c.is_ascii_digit()).count();
    let number_mark = (digits > 0
        && matches!(chars.get(digits), Some('.' | ')'))
        && chars.get(digits + 1).is_none_or(|c| c.is_whitespace()))
    .then_some(digits);
    let mut out = String::with_capacity(text.len());
    for (at, &c) in chars.iter().enumerate() {
        let escape = match c {
            '\\' | '`' | '*' | '[' | ']' | '<' | '&' | '~' => true,
            // Inside a word, as in file names and DOIs, it formats nothing.
            '_' => !(at > 0 && word(at - 1) && word(at + 1)),
            '#' | '>' | '-' | '+' | '=' | '|' => at == 0,
            _ => number_mark == Some(at),
        };
        if escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A note as typed, with plain line ends.
fn as_typed(note: &str) -> String {
    note.replace("\r\n", "\n").replace('\r', "\n").trim().to_string()
}

fn field(out: &mut String, name: &str, value: &str) {
    if !value.trim().is_empty() {
        out.push_str(&format!("- {name}: {}\n", literal(value)));
    }
}

/// The highlights of a paper, by page, then oldest first.
fn highlight_blocks(conn: &Connection, paper_id: &str) -> Result<String, LibraryError> {
    let mut stmt = conn.prepare_cached(
        "SELECT h.page, c.label, h.text, t.label, h.note
         FROM highlight AS h
         JOIN highlight_category AS c ON c.id = h.highlight_category_id
         LEFT JOIN extraction_tag AS t ON t.id = h.extraction_tag_id
         WHERE h.paper_id = ?1 ORDER BY h.page, h.created_at, h.rowid",
    )?;
    let mut rows = stmt.query([paper_id])?;
    let mut out = String::new();
    while let Some(row) = rows.next()? {
        let page: u32 = row.get(0)?;
        let category: String = row.get(1)?;
        let text: String = row.get(2)?;
        let tag: Option<String> = row.get(3)?;
        let note = as_typed(&row.get::<_, String>(4)?);
        out.push_str(&format!("\n### Page {page} · {}\n\n> {}\n", literal(&category), literal(&text)));
        let mut details = String::new();
        field(&mut details, "Tag", tag.as_deref().unwrap_or_default());
        if !note.is_empty() {
            // Further lines are indented so they stay part of the note.
            details.push_str("- Note: ");
            for (at, line) in note.split('\n').enumerate() {
                if at > 0 {
                    details.push('\n');
                    if !line.is_empty() {
                        details.push_str("  ");
                    }
                }
                details.push_str(line);
            }
            details.push('\n');
        }
        if !details.is_empty() {
            out.push('\n');
            out.push_str(&details);
        }
    }
    Ok(out)
}

fn document(conn: &Connection, paper: &Paper) -> Result<String, LibraryError> {
    let heading = shown_title(paper.title.clone(), paper.file_name.clone());
    let mut out = format!("# {}\n\n", literal(&heading));
    field(&mut out, "File", paper.file_name.as_deref().unwrap_or_default());
    field(&mut out, "Reference No", &paper.ref_no.map(|no| no.to_string()).unwrap_or_default());
    field(&mut out, "Year", &paper.year.map(|year| year.to_string()).unwrap_or_default());
    field(&mut out, "Source database", paper.source_database.as_deref().unwrap_or_default());
    field(&mut out, "DOI", paper.doi.as_deref().unwrap_or_default());
    field(&mut out, "Full-Text Decision", decision_word(&paper.decision)?);
    if let Some((code, label)) = &paper.reason {
        field(&mut out, "Exclusion Reason", &format!("{code} {label}"));
    }
    let note = as_typed(&paper.note);
    if !note.is_empty() {
        out.push_str(&format!("\n## Assessment notes\n\n{note}\n"));
    }
    let highlights = highlight_blocks(conn, &paper.id)?;
    if !highlights.is_empty() {
        out.push_str("\n## Highlights\n");
        out.push_str(&highlights);
    }
    Ok(out)
}

/// A Windows file name without extension, made from a PDF's name (or any name).
pub(crate) fn safe_stem(file_name: &str) -> String {
    let stem = match file_name.rsplit_once('.') {
        Some((stem, extension)) if extension.eq_ignore_ascii_case("pdf") => stem,
        _ => file_name,
    };
    let cleaned: String = stem
        .chars()
        .map(|c| if c.is_control() || r#"<>:"/\|?*"#.contains(c) { '_' } else { c })
        .take(MAX_STEM)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        return "paper".to_string();
    }
    // Names Windows keeps for devices, with or without an extension.
    let device = cleaned.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && device.ends_with(|c: char| ('1'..='9').contains(&c)));
    if reserved { format!("_{cleaned}") } else { cleaned.to_string() }
}

/// A name no other file of this export has; Windows ignores the case of letters.
fn unique_name(stem: &str, taken: &mut HashSet<String>) -> String {
    let mut name = format!("{stem}.md");
    let mut number = 1;
    while !taken.insert(name.to_lowercase()) {
        number += 1;
        name = format!("{stem} ({number}).md");
    }
    name
}

/// One file per paper of the library, named after its PDF.
pub fn markdown_files(conn: &Connection) -> Result<Vec<MarkdownFile>, LibraryError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT p.id, p.ref_no, p.title, p.file_path, p.year, p.source_database, p.doi, p.decision,
                r.code, r.label, p.assessment_note
         FROM paper AS p
         LEFT JOIN exclusion_reason AS r ON r.id = p.exclusion_reason_id AND p.decision = 'exclude'
         ORDER BY {PAPER_ORDER}",
    ))?;
    let papers = stmt
        .query_map([], |row| {
            let path: Option<String> = row.get(3)?;
            let code: Option<String> = row.get(8)?;
            let label: Option<String> = row.get(9)?;
            Ok(Paper {
                id: row.get(0)?,
                ref_no: row.get(1)?,
                title: row.get(2)?,
                file_name: path.map(|path| path.rsplit('/').next().unwrap_or_default().to_string()),
                year: row.get(4)?,
                source_database: row.get(5)?,
                doi: row.get(6)?,
                decision: row.get(7)?,
                reason: code.zip(label),
                note: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut taken = HashSet::new();
    papers
        .iter()
        .map(|paper| {
            let source = paper.file_name.as_deref().unwrap_or(&paper.title);
            Ok(MarkdownFile {
                name: unique_name(&safe_stem(source), &mut taken),
                contents: document(conn, paper)?,
            })
        })
        .collect()
}

/// Makes a folder that did not exist yet inside `parent` and returns its name.
fn new_folder(parent: &Path) -> io::Result<String> {
    let mut name = FOLDER_NAME.to_string();
    for number in 2..10_000 {
        match fs::create_dir(parent.join(&name)) {
            Ok(()) => return Ok(name),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                name = format!("{FOLDER_NAME} ({number})");
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("no free folder name"))
}

/// Writes the files into a new folder inside `parent`. All or nothing: after a failure
/// the files and the folder made here are removed, and nothing else is touched.
pub fn write_folder(parent: &Path, files: &[MarkdownFile]) -> Result<MarkdownExport, LibraryError> {
    let failed = |error: io::Error| LibraryError::new(ErrorCode::Other, error.to_string());
    let folder_name = new_folder(parent).map_err(failed)?;
    let folder = parent.join(&folder_name);
    let mut written = Vec::new();
    let result = files.iter().try_for_each(|file| {
        let path = folder.join(&file.name);
        // Refuses a file that is already there.
        let mut handle = fs::File::create_new(&path)?;
        written.push(path);
        handle.write_all(file.contents.as_bytes())?;
        handle.sync_all()
    });
    if let Err(error) = result {
        for path in written {
            let _ = fs::remove_file(path);
        }
        // Only removes an empty folder.
        let _ = fs::remove_dir(&folder);
        return Err(failed(error));
    }
    Ok(MarkdownExport { folder_name, file_count: files.len() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;
    use rusqlite::params;

    fn library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        conn.execute_batch(
            "INSERT INTO exclusion_reason (id, code, label, sort_order) VALUES ('e3', 'E3', 'Wrong population', 0);
             INSERT INTO extraction_tag (id, label, sort_order) VALUES ('f', 'FCR', 0);",
        )
        .unwrap();
        (dir, conn)
    }

    fn highlight(conn: &Connection, paper: &str, page: u32, tag: Option<&str>, text: &str, note: &str) {
        conn.execute(
            "INSERT INTO highlight (paper_id, page, highlight_category_id, extraction_tag_id, text, rects, note)
             VALUES (?1, ?2, (SELECT id FROM highlight_category WHERE color = 'blue'), ?3, ?4, '[]', ?5)",
            params![paper, page, tag, text, note],
        )
        .unwrap();
    }

    fn file(name: &str, contents: &str) -> MarkdownFile {
        MarkdownFile { name: name.into(), contents: contents.into() }
    }

    #[test]
    fn a_file_holds_metadata_decision_note_and_highlights() {
        let (_dir, conn) = library();
        conn.execute(
            "INSERT INTO paper (id, ref_no, title, file_path, year, source_database, doi, decision,
                                exclusion_reason_id, assessment_note)
             VALUES ('p', 5, 'Feed trial A', 'pdfs/kim-2021.pdf', 2021, 'Scopus', '10.1000/x_y',
                     'exclude', 'e3', 'Pond study only. See [#12].\r\n**Check**')",
            [],
        )
        .unwrap();
        highlight(&conn, "p", 7, None, "Later", "");
        highlight(&conn, "p", 3, Some("f"), "The FCR\nwas 1.4", "compare with [#12]\n\nagain");
        let files = markdown_files(&conn).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "kim-2021.md");
        assert_eq!(
            files[0].contents,
            "# Feed trial A\n\n\
             - File: kim-2021.pdf\n\
             - Reference No: 5\n\
             - Year: 2021\n\
             - Source database: Scopus\n\
             - DOI: 10.1000/x_y\n\
             - Full-Text Decision: Exclude\n\
             - Exclusion Reason: E3 Wrong population\n\
             \n## Assessment notes\n\nPond study only. See [#12].\n**Check**\n\
             \n## Highlights\n\
             \n### Page 3 · Method\n\n> The FCR was 1.4\n\
             \n- Tag: FCR\n- Note: compare with [#12]\n\n  again\n\
             \n### Page 7 · Method\n\n> Later\n"
        );
    }

    #[test]
    fn every_paper_gets_a_file_and_empty_parts_are_left_out() {
        let (_dir, conn) = library();
        // A reason kept from an earlier decision does not count (FR-04.3).
        conn.execute(
            "INSERT INTO paper (file_path, decision, exclusion_reason_id) VALUES ('pdfs/a.pdf', 'include', 'e3')",
            [],
        )
        .unwrap();
        let files = markdown_files(&conn).unwrap();
        assert_eq!(files[0].contents, "# a.pdf\n\n- File: a.pdf\n- Full-Text Decision: Include\n");
    }

    #[test]
    fn pdf_text_and_metadata_cannot_become_markup() {
        assert_eq!(
            literal("![x](http://a.example/p.png) <img> *b* `c`"),
            "!\\[x\\](http://a.example/p.png) \\<img> \\*b\\* \\`c\\`"
        );
        assert_eq!(literal("# Title"), "\\# Title");
        assert_eq!(literal("- item"), "\\- item");
        assert_eq!(literal("1. First"), "1\\. First");
        assert_eq!(literal("kim_2021 _x_ 2.5 a-b µg"), "kim_2021 \\_x\\_ 2.5 a-b µg");
    }

    #[test]
    fn names_are_valid_for_windows() {
        assert_eq!(safe_stem("kim 2021.PDF"), "kim 2021");
        assert_eq!(safe_stem("a:b?c.pdf"), "a_b_c");
        assert_eq!(safe_stem("..\\..\\x.pdf"), ".._.._x");
        assert_eq!(safe_stem("name. .pdf"), "name");
        assert_eq!(safe_stem("..pdf"), "paper");
        assert_eq!(safe_stem("con.pdf"), "_con");
        assert_eq!(safe_stem("COM1.x.pdf"), "_COM1.x");
        assert_eq!(safe_stem("common.pdf"), "common");
        assert_eq!(safe_stem(&"é".repeat(300)).chars().count(), MAX_STEM);
    }

    #[test]
    fn papers_with_the_same_name_get_numbers() {
        let (_dir, conn) = library();
        for path in ["pdfs/a.pdf", "pdfs/A.PDF", "pdfs/a (2).pdf"] {
            conn.execute("INSERT INTO paper (file_path) VALUES (?1)", [path]).unwrap();
        }
        let names: Vec<String> = markdown_files(&conn).unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["a.md", "A (2).md", "a (2) (2).md"]);
    }

    #[test]
    fn building_the_files_changes_nothing() {
        let (_dir, conn) = library();
        conn.execute("INSERT INTO paper (file_path) VALUES ('pdfs/a.pdf')", []).unwrap();
        let before = conn.total_changes();
        markdown_files(&conn).unwrap();
        assert_eq!(conn.total_changes(), before);
    }

    #[test]
    fn each_export_makes_a_new_folder_and_replaces_nothing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "mine").unwrap();
        // A file with the folder's name is also left alone.
        fs::write(dir.path().join("highlights"), "a file").unwrap();
        let first = write_folder(dir.path(), &[file("a.md", "one")]).unwrap();
        let second = write_folder(dir.path(), &[file("a.md", "two")]).unwrap();
        assert_eq!(first, MarkdownExport { folder_name: "highlights (2)".into(), file_count: 1 });
        assert_eq!(second.folder_name, "highlights (3)");
        let read = |path: &str| fs::read_to_string(dir.path().join(path)).unwrap();
        assert_eq!(read("a.md"), "mine");
        assert_eq!(read("highlights"), "a file");
        assert_eq!(read("highlights (2)/a.md"), "one");
        assert_eq!(read("highlights (3)/a.md"), "two");
    }

    #[test]
    fn a_failed_export_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        // The second file cannot be made: its folder does not exist.
        let files = [file("a.md", "one"), file("missing/b.md", "two")];
        assert!(write_folder(dir.path(), &files).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn a_folder_that_cannot_be_used_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_folder(&dir.path().join("gone"), &[file("a.md", "one")]).is_err());
    }
}
