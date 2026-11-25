//! Text of a paper, page by page (FR-01.6).
//!
//! The text is stored as `text_chunk` rows that never cross a page, so every quote
//! points to one page. Chat (T-45), suggestions (T-46), and the semantic index (T-49)
//! all use these chunks; T-49 only adds embeddings to them and must not recreate them,
//! because deleting a chunk also deletes the citations that quote it.

use std::path::Path;

use pdfium_render::prelude::*;
use rusqlite::{Connection, params};

/// Chunks are cut at line ends once they reach about this many characters.
pub const CHUNK_TARGET_CHARS: usize = 1200;

/// A PDF with less selectable text than this per page, on average, is treated as a
/// scan without a text layer ("no_text"). Scans often carry a short stamped line per
/// page, such as "Downloaded from ...", which stays well below this.
const MIN_CHARS_PER_PAGE: usize = 30;

/// Loads pdfium.dll. The library is loaded once per run; later calls reuse it.
pub fn load_pdfium(dll: &Path) -> Result<Pdfium, PdfiumError> {
    match Pdfium::bind_to_library(dll) {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        // Already loaded by an earlier call: this reuses it.
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Ok(Pdfium::default()),
        Err(error) => Err(error),
    }
}

/// The text of every page, in order. Fails for damaged and password-protected PDFs.
pub fn extract_pages(pdfium: &Pdfium, pdf: &Path) -> Result<Vec<String>, PdfiumError> {
    let document = pdfium.load_pdf_from_file(pdf, None)?;
    document
        .pages()
        .iter()
        .map(|page| page.text().map(|text| text.all()))
        .collect()
}

/// False for scans and other PDFs without a usable text layer.
pub fn has_text(pages: &[String]) -> bool {
    let chars: usize = pages
        .iter()
        .map(|page| page.chars().filter(|c| !c.is_whitespace()).count())
        .sum();
    !pages.is_empty() && chars >= MIN_CHARS_PER_PAGE * pages.len()
}

/// Splits one page into chunks of whole lines, each about `CHUNK_TARGET_CHARS` long.
/// Line breaks are kept; blank lines and spaces at line ends are dropped.
pub fn chunk_page(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0;

    let push = |chunks: &mut Vec<String>, current: &mut String, current_chars: &mut usize| {
        if !current.is_empty() {
            chunks.push(std::mem::take(current));
            *current_chars = 0;
        }
    };

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        for piece in split_long_line(line) {
            let piece_chars = piece.chars().count();
            if current_chars > 0 && current_chars + 1 + piece_chars > CHUNK_TARGET_CHARS {
                push(&mut chunks, &mut current, &mut current_chars);
            }
            if current_chars > 0 {
                current.push('\n');
                current_chars += 1;
            }
            current.push_str(&piece);
            current_chars += piece_chars;
        }
    }
    push(&mut chunks, &mut current, &mut current_chars);
    chunks
}

/// A line longer than a chunk is cut between words.
fn split_long_line(line: &str) -> Vec<String> {
    if line.chars().count() <= CHUNK_TARGET_CHARS {
        return vec![line.to_string()];
    }
    let mut pieces = Vec::new();
    let mut piece = String::new();
    for word in line.split_whitespace() {
        if !piece.is_empty() && piece.chars().count() + 1 + word.chars().count() > CHUNK_TARGET_CHARS {
            pieces.push(std::mem::take(&mut piece));
        }
        if !piece.is_empty() {
            piece.push(' ');
        }
        piece.push_str(word);
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// Saves the extracted text and moves the paper from "pending" to "text_ready" or
/// "no_text" (PRD 7a). Returns false, changing nothing, when the paper is no longer
/// pending or no longer exists (e.g. deleted meanwhile).
pub fn save_text(conn: &mut Connection, paper_id: &str, pages: &[String]) -> rusqlite::Result<bool> {
    let ready = has_text(pages);
    let tx = conn.transaction()?;
    let changed = tx.execute(
        "UPDATE paper SET text_status = ?1 WHERE id = ?2 AND text_status = 'pending'",
        params![if ready { "text_ready" } else { "no_text" }, paper_id],
    )?;
    if changed == 0 {
        return Ok(false);
    }
    if ready {
        let mut insert = tx.prepare(
            "INSERT INTO text_chunk (paper_id, page_start, page_end, text) VALUES (?1, ?2, ?2, ?3)",
        )?;
        for (index, page) in pages.iter().enumerate() {
            for chunk in chunk_page(page) {
                insert.execute(params![paper_id, index as i64 + 1, chunk])?;
            }
        }
    }
    tx.commit()?;
    Ok(true)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::library::create_library;
    use std::path::PathBuf;

    /// pdfium.dll as fetched by `npm run fetch-pdfium`.
    pub(crate) fn test_dll() -> PathBuf {
        let dll = Path::new(env!("CARGO_MANIFEST_DIR")).join("pdfium").join("pdfium.dll");
        assert!(dll.is_file(), "pdfium.dll is missing: run `npm run fetch-pdfium` first");
        dll
    }

    pub(crate) fn pdfium() -> Pdfium {
        load_pdfium(&test_dll()).unwrap()
    }

    /// Builds a small valid PDF with one page per entry; each line of an entry becomes
    /// a line of text in Helvetica. An empty entry makes a page without text.
    pub(crate) fn make_pdf(pages: &[&str]) -> Vec<u8> {
        let mut objects: Vec<String> = Vec::new();
        let page_ids: Vec<usize> = (0..pages.len()).map(|i| 4 + i * 2).collect();
        objects.push("<< /Type /Catalog /Pages 2 0 R >>".into());
        let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
        objects.push(format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()));
        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into());
        for (page, id) in pages.iter().zip(&page_ids) {
            let mut stream = String::new();
            if !page.is_empty() {
                stream.push_str("BT /F1 11 Tf 14 TL 72 760 Td\n");
                for line in page.lines() {
                    let escaped = line.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
                    stream.push_str(&format!("({escaped}) Tj T*\n"));
                }
                stream.push_str("ET");
            }
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                 /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                id + 1
            ));
            objects.push(format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()));
        }

        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1)
                .as_bytes(),
        );
        pdf
    }

    const PAGE_ONE: &str = "Recirculating aquaculture systems for tilapia\nFeed conversion ratio was 1.4 in treatment B.";

    fn write_pdf(dir: &Path, name: &str, pages: &[&str]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, make_pdf(pages)).unwrap();
        path
    }

    #[test]
    fn extracts_each_page_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "a.pdf", &[PAGE_ONE, "", "Results on page three"]);
        let pages = extract_pages(&pdfium(), &pdf).unwrap();
        assert_eq!(pages.len(), 3);
        assert!(pages[0].contains("Feed conversion ratio was 1.4 in treatment B."), "{:?}", pages[0]);
        assert_eq!(pages[1].trim(), "");
        assert!(pages[2].contains("Results on page three"));
    }

    #[test]
    fn a_damaged_pdf_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("bad.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\nthis is not really a pdf\n%%EOF\n").unwrap();
        assert!(extract_pages(&pdfium(), &pdf).is_err());
        assert!(extract_pages(&pdfium(), &dir.path().join("missing.pdf")).is_err());
    }

    #[test]
    fn tells_scans_from_text() {
        let page = "word ".repeat(20);
        assert!(has_text(&[page.clone(), String::new()]));
        assert!(!has_text(&[]));
        assert!(!has_text(&[String::new(), "  \n ".into()]));
        // A stamped line on every page of a scan is not enough.
        let stamp = "Downloaded from journal.org".to_string();
        assert!(!has_text(&[stamp.clone(), stamp.clone(), stamp]));
    }

    #[test]
    fn chunks_keep_whole_lines_and_stay_near_the_target() {
        let line = "a".repeat(100);
        let page = vec![line.as_str(); 30].join("\r\n");
        let chunks = chunk_page(&page);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.chars().count() <= CHUNK_TARGET_CHARS);
            assert!(chunk.lines().all(|l| l == line));
        }
        assert_eq!(chunks.join("\n"), vec![line.as_str(); 30].join("\n"));
    }

    #[test]
    fn a_very_long_line_is_cut_between_words() {
        let words = "ikan ".repeat(600);
        let chunks = chunk_page(&words);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= CHUNK_TARGET_CHARS));
        assert!(chunks.iter().all(|c| c.split(' ').all(|w| w == "ikan")));
    }

    #[test]
    fn blank_pages_and_lines_make_no_chunks() {
        assert!(chunk_page("").is_empty());
        assert_eq!(chunk_page("  one  \n\n\n two\n"), ["one\ntwo"]);
    }

    fn library_with_paper() -> (tempfile::TempDir, Connection, String) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        let id = conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap();
        (dir, conn, id)
    }

    fn status(conn: &Connection, id: &str) -> String {
        conn.query_row("SELECT text_status FROM paper WHERE id = ?1", [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn saves_chunks_with_their_page_and_marks_text_ready() {
        let (_dir, mut conn, id) = library_with_paper();
        let pages = vec!["word ".repeat(20), String::new(), "third page text here".repeat(3)];
        assert!(save_text(&mut conn, &id, &pages).unwrap());

        assert_eq!(status(&conn, &id), "text_ready");
        let chunk_pages: Vec<(i64, i64)> = conn
            .prepare("SELECT page_start, page_end FROM text_chunk WHERE paper_id = ?1 ORDER BY page_start")
            .unwrap()
            .query_map([&id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(chunk_pages, [(1, 1), (3, 3)]);
    }

    #[test]
    fn a_scan_is_marked_no_text_without_chunks() {
        let (_dir, mut conn, id) = library_with_paper();
        assert!(save_text(&mut conn, &id, &[String::new(), String::new()]).unwrap());
        assert_eq!(status(&conn, &id), "no_text");
        let chunks: i64 = conn.query_row("SELECT count(*) FROM text_chunk", [], |r| r.get(0)).unwrap();
        assert_eq!(chunks, 0);
    }

    #[test]
    fn a_paper_that_is_no_longer_pending_is_left_alone() {
        // Allowed transitions: pending -> text_ready or no_text (PRD 7a).
        let (_dir, mut conn, id) = library_with_paper();
        let pages = vec!["word ".repeat(20)];
        assert!(save_text(&mut conn, &id, &pages).unwrap());
        assert!(!save_text(&mut conn, &id, &[String::new()]).unwrap());
        assert_eq!(status(&conn, &id), "text_ready");
        let chunks: i64 = conn.query_row("SELECT count(*) FROM text_chunk", [], |r| r.get(0)).unwrap();
        assert_eq!(chunks, 1);
        // A deleted paper changes nothing either.
        assert!(!save_text(&mut conn, "deleted", &pages).unwrap());
    }

    #[test]
    fn saving_never_changes_the_decision() {
        let (_dir, mut conn, id) = library_with_paper();
        conn.execute("UPDATE paper SET decision = 'exclude' WHERE id = ?1", [&id]).unwrap();
        save_text(&mut conn, &id, &["word ".repeat(20)]).unwrap();
        let decision: String =
            conn.query_row("SELECT decision FROM paper WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(decision, "exclude");
    }
}
