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
    // Checked first: once the library is loaded, binding no longer looks at the path.
    if !dll.is_file() {
        return Err(PdfiumError::IoError(std::io::ErrorKind::NotFound.into()));
    }
    match Pdfium::bind_to_library(dll) {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        // Already loaded by an earlier call: this reuses it.
        Err(PdfiumError::PdfiumLibraryBindingsAlreadyInitialized) => Ok(Pdfium::default()),
        Err(error) => Err(error),
    }
}

/// The text of every page, in order. Fails for damaged PDFs, and for password-protected
/// ones unless `password` is right. The password is only passed on to PDFium.
pub fn extract_pages(pdfium: &Pdfium, pdf: &Path, password: Option<&str>) -> Result<Vec<String>, PdfiumError> {
    let document = pdfium.load_pdf_from_file(pdf, password)?;
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
        build_pdf(pages, None)
    }

    /// Like `make_pdf`, but locked with a user password (PDF standard security
    /// handler, revision 2: RC4 with a 40-bit key).
    pub(crate) fn make_locked_pdf(pages: &[&str], password: &str) -> Vec<u8> {
        build_pdf(pages, Some(password))
    }

    fn build_pdf(pages: &[&str], password: Option<&str>) -> Vec<u8> {
        // Each object is a dictionary, and optionally the data of its stream.
        let mut objects: Vec<(String, Option<Vec<u8>>)> = Vec::new();
        let page_ids: Vec<usize> = (0..pages.len()).map(|i| 4 + i * 2).collect();
        objects.push(("<< /Type /Catalog /Pages 2 0 R >>".into(), None));
        let kids: Vec<String> = page_ids.iter().map(|id| format!("{id} 0 R")).collect();
        objects.push((format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()), None));
        objects.push(("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(), None));
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
            objects.push((
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
                     /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                    id + 1
                ),
                None,
            ));
            objects.push((String::new(), Some(stream.into_bytes())));
        }

        // Only stream data is encrypted: the dictionaries above hold no strings.
        let mut trailer_extra = String::new();
        if let Some(password) = password {
            let lock = Rc4Lock::new(password, b"slreader-test-id");
            for (index, (_, data)) in objects.iter_mut().enumerate() {
                if let Some(data) = data {
                    *data = rc4(&lock.object_key(index as u32 + 1), data);
                }
            }
            objects.push((
                format!(
                    "<< /Filter /Standard /V 1 /R 2 /Length 40 /P {} /O <{}> /U <{}> >>",
                    lock.permissions,
                    hex(&lock.owner),
                    hex(&lock.user)
                ),
                None,
            ));
            let id = hex(&lock.file_id);
            trailer_extra = format!(" /Encrypt {} 0 R /ID [<{id}> <{id}>]", objects.len());
        }

        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, (dict, data)) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
            match data {
                None => pdf.extend_from_slice(dict.as_bytes()),
                Some(data) => {
                    pdf.extend_from_slice(format!("<< /Length {} >>\nstream\n", data.len()).as_bytes());
                    pdf.extend_from_slice(data);
                    pdf.extend_from_slice(b"\nendstream");
                }
            }
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R{trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    /// The values of a revision 2 standard security handler (PDF 1.7, 7.6.3).
    struct Rc4Lock {
        key: [u8; 5],
        owner: Vec<u8>,
        user: Vec<u8>,
        permissions: i32,
        file_id: Vec<u8>,
    }

    const PASSWORD_PAD: [u8; 32] = [
        0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
        0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
    ];

    impl Rc4Lock {
        fn new(password: &str, file_id: &[u8]) -> Self {
            let padded = |text: &str| -> Vec<u8> {
                text.bytes().chain(PASSWORD_PAD).take(32).collect()
            };
            let permissions: i32 = -4;
            let owner_key = md5(&padded(&format!("owner-{password}")));
            let owner = rc4(&owner_key[..5], &padded(password));
            let mut seed = padded(password);
            seed.extend_from_slice(&owner);
            seed.extend_from_slice(&permissions.to_le_bytes());
            seed.extend_from_slice(file_id);
            let key: [u8; 5] = md5(&seed)[..5].try_into().unwrap();
            let user = rc4(&key, &PASSWORD_PAD);
            Rc4Lock { key, owner, user, permissions, file_id: file_id.to_vec() }
        }

        fn object_key(&self, number: u32) -> Vec<u8> {
            let mut seed = self.key.to_vec();
            seed.extend_from_slice(&number.to_le_bytes()[..3]);
            seed.extend_from_slice(&[0, 0]);
            md5(&seed)[..10].to_vec()
        }
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02X}")).collect()
    }

    fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
        let mut s: Vec<u8> = (0..=255).collect();
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        let (mut i, mut j) = (0u8, 0u8);
        data.iter()
            .map(|byte| {
                i = i.wrapping_add(1);
                j = j.wrapping_add(s[i as usize]);
                s.swap(i as usize, j as usize);
                byte ^ s[s[i as usize].wrapping_add(s[j as usize]) as usize]
            })
            .collect()
    }

    /// MD5 (RFC 1321), only for building locked test PDFs.
    fn md5(data: &[u8]) -> [u8; 16] {
        const SHIFTS: [u32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
        let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4_294_967_296.0) as u32).collect();
        let mut message = data.to_vec();
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        message.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());

        let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
        for block in message.chunks(64) {
            let m: Vec<u32> = block.chunks(4).map(|w| u32::from_le_bytes(w.try_into().unwrap())).collect();
            let [mut a, mut b, mut c, mut d] = state;
            for i in 0..64 {
                let (f, g) = match i / 16 {
                    0 => ((b & c) | (!b & d), i),
                    1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                    2 => (b ^ c ^ d, (3 * i + 5) % 16),
                    _ => (c ^ (b | !d), (7 * i) % 16),
                };
                let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
                a = d;
                d = c;
                c = b;
                b = b.wrapping_add(f.rotate_left(SHIFTS[(i / 16) * 4 + i % 4]));
            }
            for (part, value) in state.iter_mut().zip([a, b, c, d]) {
                *part = part.wrapping_add(value);
            }
        }
        let mut digest = [0u8; 16];
        for (chunk, part) in digest.chunks_mut(4).zip(state) {
            chunk.copy_from_slice(&part.to_le_bytes());
        }
        digest
    }

    #[test]
    fn md5_matches_the_reference_values() {
        assert_eq!(hex(&md5(b"")), "D41D8CD98F00B204E9800998ECF8427E");
        assert_eq!(
            hex(&md5(b"The quick brown fox jumps over the lazy dog")),
            "9E107D9D372BB6826BD81D3542A419D6"
        );
    }

    #[test]
    fn a_locked_pdf_needs_its_password() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("locked.pdf");
        std::fs::write(&pdf, make_locked_pdf(&[PAGE_ONE, "Results on page two"], "tilapia-19")).unwrap();
        let pdfium = pdfium();

        assert!(extract_pages(&pdfium, &pdf, None).is_err());
        assert!(extract_pages(&pdfium, &pdf, Some("wrong")).is_err());
        let pages = extract_pages(&pdfium, &pdf, Some("tilapia-19")).unwrap();
        assert!(pages[0].contains("Feed conversion ratio was 1.4 in treatment B."), "{:?}", pages[0]);
        assert!(pages[1].contains("Results on page two"));
        // A PDF without a password opens whatever password is given.
        let open = write_pdf(dir.path(), "open.pdf", &[PAGE_ONE]);
        assert!(extract_pages(&pdfium, &open, Some("anything")).is_ok());
    }

    const PAGE_ONE: &str = "Recirculating aquaculture systems for tilapia\nFeed conversion ratio was 1.4 in treatment B.";

    fn write_pdf(dir: &Path, name: &str, pages: &[&str]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, make_pdf(pages)).unwrap();
        path
    }

    #[test]
    fn a_missing_dll_is_an_error_even_after_pdfium_was_loaded() {
        let _loaded = pdfium();
        assert!(load_pdfium(Path::new("no-such-pdfium.dll")).is_err());
    }

    #[test]
    fn extracts_each_page_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = write_pdf(dir.path(), "a.pdf", &[PAGE_ONE, "", "Results on page three"]);
        let pages = extract_pages(&pdfium(), &pdf, None).unwrap();
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
        assert!(extract_pages(&pdfium(), &pdf, None).is_err());
        assert!(extract_pages(&pdfium(), &dir.path().join("missing.pdf"), None).is_err());
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
