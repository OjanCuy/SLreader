//! Papers in the open library: importing PDFs (FR-01.2 to FR-01.5), listing them,
//! and deleting them (FR-01.7).
//!
//! Every function here receives the connection and folder of the open library from
//! `libraries::library_and_folder_for_*`, so it can only touch that library (FR-14.4).
//!
//! Files are copied into `<library>/pdfs/` and never overwrite anything there.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;

use crate::db::library::MAX_PAPERS;
use crate::libraries::{ErrorCode, LibraryError, PDF_FOLDER};

/// What happened to one file of an import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOutcome {
    /// A new paper, decision "Undecided" (FR-01.2).
    Added,
    /// The file was given to an existing paper that had none yet, e.g. one
    /// restored from a backup (FR-01.5).
    Attached,
    /// A paper with this file name already has its file (FR-01.5).
    AlreadyInLibrary,
    /// Same content as a paper under another name; not imported until the user
    /// chooses "Add anyway" (PRD Section 8).
    Identical,
    /// Not a PDF, damaged, or could not be read (FR-01.3).
    NotPdf,
    /// The library already holds 250 papers (FR-01.4).
    OverLimit,
    /// Copying or saving failed: disk full, no permission, ...
    WriteFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFile {
    pub file_name: String,
    /// The file that was given, so the UI can ask to import it again.
    pub path: String,
    pub outcome: ImportOutcome,
    /// Set for `Identical`: the file name of the paper with the same content.
    pub identical_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// One entry per given file, in the same order.
    pub files: Vec<ImportedFile>,
    /// Files that are now in the library, attached ones included.
    pub added: usize,
}

/// A paper as the paper list shows it for now (the full row comes with T-38).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperListItem {
    pub id: String,
    pub file_name: Option<String>,
    pub title: String,
    /// The paper has a file name but the file is not in the library folder.
    pub file_missing: bool,
    /// "pending", "text_ready", or "no_text" (FR-01.6).
    pub text_status: String,
    /// Pending, and the text is expected to be extracted in this run.
    pub extracting: bool,
}

/// Copies the given files into the library and creates a paper for each (FR-01.2).
/// One bad file never stops the others (FR-01.3). With `allow_identical` false, a file
/// with the same content as an existing paper is reported instead of imported.
pub fn import_pdfs(
    conn: &Connection,
    folder: &Path,
    sources: &[PathBuf],
    allow_identical: bool,
) -> Result<ImportReport, LibraryError> {
    // Names already used by papers, compared without case like Windows does.
    let mut known: Vec<(String, String)> = conn
        .prepare("SELECT id, file_path FROM paper WHERE file_path IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut count: i64 = conn.query_row("SELECT count(*) FROM paper", [], |r| r.get(0))?;

    let mut files = Vec::with_capacity(sources.len());
    for source in sources {
        let file_name = source
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| source.to_string_lossy().into_owned());
        let identical_to = if allow_identical {
            None
        } else {
            find_identical(folder, source, &file_name, &known)
        };
        let outcome = import_one(
            conn,
            folder,
            source,
            &file_name,
            identical_to.is_some(),
            &mut known,
            &mut count,
        )?;
        let identical_to = identical_to.filter(|_| outcome == ImportOutcome::Identical);
        files.push(ImportedFile {
            file_name,
            path: source.to_string_lossy().into_owned(),
            outcome,
            identical_to,
        });
    }
    let added = files
        .iter()
        .filter(|f| matches!(f.outcome, ImportOutcome::Added | ImportOutcome::Attached))
        .count();
    Ok(ImportReport { files, added })
}

fn import_one(
    conn: &Connection,
    folder: &Path,
    source: &Path,
    file_name: &str,
    is_identical: bool,
    known: &mut Vec<(String, String)>,
    count: &mut i64,
) -> Result<ImportOutcome, LibraryError> {
    if source.file_name().and_then(|n| n.to_str()).is_none() || !looks_like_pdf(source) {
        return Ok(ImportOutcome::NotPdf);
    }
    let wanted = format!("{PDF_FOLDER}/{file_name}");

    if let Some((_, stored)) = known.iter().find(|(_, path)| same_name(path, &wanted)) {
        let target = folder.join(stored);
        if target.exists() {
            return Ok(ImportOutcome::AlreadyInLibrary);
        }
        // FR-01.5: a paper with this name but no file yet. It is already counted.
        return Ok(match copy_new(source, &target) {
            Ok(()) => ImportOutcome::Attached,
            Err(_) => ImportOutcome::WriteFailed,
        });
    }

    if *count >= MAX_PAPERS {
        return Ok(ImportOutcome::OverLimit);
    }
    if is_identical {
        return Ok(ImportOutcome::Identical);
    }

    let Ok(stored) = place_file(folder, source, file_name, known) else {
        return Ok(ImportOutcome::WriteFailed);
    };
    let inserted: rusqlite::Result<String> = conn.query_row(
        "INSERT INTO paper (file_path) VALUES (?1) RETURNING id",
        params![stored.relative],
        |r| r.get(0),
    );
    match inserted {
        Ok(id) => {
            known.push((id, stored.relative));
            *count += 1;
            Ok(ImportOutcome::Added)
        }
        Err(_) => {
            // Only a file copied just now is removed; an adopted one stays.
            if stored.copied {
                let _ = std::fs::remove_file(folder.join(&stored.relative));
            }
            Ok(ImportOutcome::WriteFailed)
        }
    }
}

/// The file name of a paper whose PDF has exactly the same content as `source`,
/// stored under a different name. Files of another size are ruled out at once.
fn find_identical(
    folder: &Path,
    source: &Path,
    file_name: &str,
    known: &[(String, String)],
) -> Option<String> {
    let wanted = format!("{PDF_FOLDER}/{file_name}");
    known
        .iter()
        .filter(|(_, path)| !same_name(path, &wanted))
        .find(|(_, path)| same_content(source, &folder.join(path)).unwrap_or(false))
        .map(|(_, path)| display_name(path))
}

/// "pdfs/kim.pdf" -> "kim.pdf"
fn display_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

struct Stored {
    /// e.g. "pdfs/kim-2021.pdf"
    relative: String,
    /// False when an identical file was already in place.
    copied: bool,
}

/// Puts the file in `pdfs/` without overwriting anything. A file already there that no
/// paper uses (e.g. left by a crash during an earlier import) is reused if identical;
/// otherwise the new file gets a free name such as "kim-2021 (2).pdf".
fn place_file(
    folder: &Path,
    source: &Path,
    file_name: &str,
    known: &[(String, String)],
) -> io::Result<Stored> {
    std::fs::create_dir_all(folder.join(PDF_FOLDER))?;
    let (stem, extension) = match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (file_name, String::new()),
    };
    for number in 1.. {
        let candidate = if number == 1 {
            file_name.to_string()
        } else {
            format!("{stem} ({number}){extension}")
        };
        let relative = format!("{PDF_FOLDER}/{candidate}");
        if known.iter().any(|(_, path)| same_name(path, &relative)) {
            continue;
        }
        let target = folder.join(&relative);
        match copy_new(source, &target) {
            Ok(()) => return Ok(Stored { relative, copied: true }),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if same_content(source, &target)? {
                    return Ok(Stored { relative, copied: false });
                }
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("the loop only ends by returning")
}

/// Copies to a path that must not exist yet. A half-written copy is removed.
fn copy_new(source: &Path, target: &Path) -> io::Result<()> {
    let mut output = OpenOptions::new().write(true).create_new(true).open(target)?;
    let result = File::open(source)
        .and_then(|mut input| io::copy(&mut input, &mut output))
        .and_then(|_| output.sync_all());
    if result.is_err() {
        drop(output);
        let _ = std::fs::remove_file(target);
    }
    result
}

/// A quick check, not a full parse: PDFs start with "%PDF-" and a complete file ends
/// with "%%EOF". Catches other file types and most cut-off downloads.
fn looks_like_pdf(path: &Path) -> bool {
    let check = || -> io::Result<bool> {
        let mut file = File::open(path)?;
        if !file.metadata()?.is_file() {
            return Ok(false);
        }
        let mut head = [0u8; 1024];
        let read = read_up_to(&mut file, &mut head)?;
        if !contains(&head[..read], b"%PDF-") {
            return Ok(false);
        }
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(4096)))?;
        let mut tail = [0u8; 4096];
        let read = read_up_to(&mut file, &mut tail)?;
        Ok(contains(&tail[..read], b"%%EOF"))
    };
    check().unwrap_or(false)
}

fn read_up_to(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (File::open(a)?, File::open(b)?);
    let (mut buf_a, mut buf_b) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let read = read_up_to(&mut a, &mut buf_a)?;
        if read_up_to(&mut b, &mut buf_b)? != read || buf_a[..read] != buf_b[..read] {
            return Ok(false);
        }
        if read == 0 {
            return Ok(true);
        }
    }
}

/// Windows file names ignore upper and lower case.
fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Where a paper's PDF is, for a stored path such as "pdfs/kim.pdf". A path that
/// points anywhere else (e.g. "../x" in a damaged or edited database) gives `None`,
/// so no file outside `pdfs/` is ever read or written for a paper.
pub fn pdf_location(folder: &Path, stored: &str) -> Option<PathBuf> {
    let (first, name) = stored.split_once('/')?;
    let plain = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':'])
        && !name.ends_with(['.', ' ']);
    (first == PDF_FOLDER && plain).then(|| folder.join(PDF_FOLDER).join(name))
}

fn unknown_paper(paper_id: &str) -> LibraryError {
    LibraryError::new(ErrorCode::Other, format!("no paper with id {paper_id}"))
}

/// The file the reader opens for a paper (FR-02.1). `NotFound` when the paper has no
/// file in the library folder: the reader shows "PDF file is missing.".
pub fn pdf_path(conn: &Connection, folder: &Path, paper_id: &str) -> Result<PathBuf, LibraryError> {
    let stored: Option<String> = conn
        .query_row("SELECT file_path FROM paper WHERE id = ?1", [paper_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| unknown_paper(paper_id))?;
    stored
        .as_deref()
        .and_then(|stored| pdf_location(folder, stored))
        .filter(|path| path.is_file())
        .ok_or_else(|| LibraryError::new(ErrorCode::NotFound, "the paper has no PDF file"))
}

/// Reads a PDF found by `pdf_path`. Called without holding the library lock.
pub fn read_pdf(path: &Path) -> Result<Vec<u8>, LibraryError> {
    std::fs::read(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => LibraryError::new(ErrorCode::NotFound, e.to_string()),
        _ => e.into(),
    })
}

/// "Attach PDF" for a paper whose file is missing (Brief Section 7, S-02 Reader).
/// Copies `source` into the library as that paper's file and never overwrites a file:
/// the paper keeps its stored name, so backups and the reference list still match.
/// A paper without a usable stored name gets the source's name, made unique.
///
/// Outcomes: `Attached`, `AlreadyInLibrary` (the paper has its file after all),
/// `NotPdf`, or `WriteFailed`. Nothing else about the paper changes.
pub fn attach_pdf(
    conn: &Connection,
    folder: &Path,
    paper_id: &str,
    source: &Path,
) -> Result<ImportOutcome, LibraryError> {
    let stored: Option<String> = conn
        .query_row("SELECT file_path FROM paper WHERE id = ?1", [paper_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| unknown_paper(paper_id))?;
    let target = stored.as_deref().and_then(|stored| pdf_location(folder, stored));
    if target.as_ref().is_some_and(|path| path.is_file()) {
        return Ok(ImportOutcome::AlreadyInLibrary);
    }
    let Some(file_name) = source.file_name().and_then(|n| n.to_str()) else {
        return Ok(ImportOutcome::NotPdf);
    };
    if !looks_like_pdf(source) {
        return Ok(ImportOutcome::NotPdf);
    }

    if let Some(target) = target {
        let copied = std::fs::create_dir_all(folder.join(PDF_FOLDER)).and_then(|()| copy_new(source, &target));
        return Ok(match copied {
            Ok(()) => ImportOutcome::Attached,
            Err(_) => ImportOutcome::WriteFailed,
        });
    }

    // No usable name: store it like an import, beside the other papers' files.
    let known: Vec<(String, String)> = conn
        .prepare("SELECT id, file_path FROM paper WHERE file_path IS NOT NULL AND id <> ?1")?
        .query_map([paper_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let Ok(placed) = place_file(folder, source, file_name, &known) else {
        return Ok(ImportOutcome::WriteFailed);
    };
    match conn.execute("UPDATE paper SET file_path = ?1 WHERE id = ?2", params![placed.relative, paper_id]) {
        Ok(1) => Ok(ImportOutcome::Attached),
        _ => {
            if placed.copied {
                let _ = std::fs::remove_file(folder.join(&placed.relative));
            }
            Ok(ImportOutcome::WriteFailed)
        }
    }
}

/// Deletes a paper, its PDF copy in the library, and everything that belongs to it:
/// highlights, text chunks, suggestions, paper chats, and their citations (FR-01.7).
/// The database removes those through `ON DELETE CASCADE`. The reference list, the
/// reasons and tags, and the user's original file are not touched. The UI asks for
/// confirmation first (O-10).
///
/// It is all or nothing: the PDF is first renamed out of the way, which fails while
/// another program locks it, and gets its name back if the database cannot be saved.
/// It is only removed once the paper is gone for good.
pub fn delete_paper(conn: &Connection, folder: &Path, paper_id: &str) -> Result<(), LibraryError> {
    // "Immediate" takes the write lock now, so a busy database fails before any file moves.
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let stored: Option<String> = tx
        .query_row("SELECT file_path FROM paper WHERE id = ?1", [paper_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| unknown_paper(paper_id))?;
    tx.execute("DELETE FROM paper WHERE id = ?1", [paper_id])?;

    // A file that another paper also points to (only in an edited database) stays.
    let others: Vec<String> = tx
        .prepare("SELECT file_path FROM paper WHERE file_path IS NOT NULL")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let file = stored
        .filter(|stored| !others.iter().any(|other| same_name(other, stored)))
        .and_then(|stored| pdf_location(folder, &stored));
    let set_aside = match &file {
        Some(path) => set_aside(path)?,
        None => None,
    };

    if let Err(error) = tx.commit() {
        if let (Some(from), Some(to)) = (&set_aside, &file) {
            let _ = std::fs::rename(from, to);
        }
        return Err(error.into());
    }
    if let Some(path) = set_aside {
        // The paper is already deleted; a leftover file does no harm to the library.
        if let Err(error) = std::fs::remove_file(&path) {
            eprintln!("could not remove {}: {error}", path.display());
        }
    }
    Ok(())
}

/// Renames a PDF to a free name such as "kim.deleting.pdf" and returns that path.
/// `None` when there is no file. The name keeps ".pdf", so a file left behind by a
/// crash can still be chosen with "Attach PDF".
fn set_aside(path: &Path) -> io::Result<Option<PathBuf>> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
        Ok(metadata) if metadata.is_dir() => return Ok(None),
        Ok(_) => {}
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    for number in 1.. {
        let name = if number == 1 {
            format!("{stem}.deleting.pdf")
        } else {
            format!("{stem}.deleting ({number}).pdf")
        };
        let target = path.with_file_name(name);
        // `rename` would replace an existing file, so the name must be free.
        if !target.try_exists()? {
            std::fs::rename(path, &target)?;
            return Ok(Some(target));
        }
    }
    unreachable!("the loop only ends by returning")
}

/// The page (1-based) the paper was last read at (FR-02.3).
pub fn last_page(conn: &Connection, paper_id: &str) -> Result<u32, LibraryError> {
    conn.query_row("SELECT last_page FROM paper WHERE id = ?1", [paper_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| unknown_paper(paper_id))
}

/// Remembers the page being read (FR-02.3). Nothing else about the paper changes.
pub fn save_last_page(conn: &Connection, paper_id: &str, page: u32) -> Result<(), LibraryError> {
    let changed = conn.execute(
        "UPDATE paper SET last_page = ?1 WHERE id = ?2",
        params![page.max(1), paper_id],
    )?;
    if changed == 0 {
        return Err(unknown_paper(paper_id));
    }
    Ok(())
}

/// All papers, oldest first. `still_extracting` tells whether a pending paper is
/// still in the queue of the background extraction.
pub fn list_papers(
    conn: &Connection,
    folder: &Path,
    still_extracting: &dyn Fn(&str) -> bool,
) -> Result<Vec<PaperListItem>, LibraryError> {
    let rows: Vec<(String, Option<String>, String, String)> = conn
        .prepare("SELECT id, file_path, title, text_status FROM paper ORDER BY created_at, rowid")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, file_path, title, text_status)| {
            let file_missing = file_path
                .as_ref()
                .is_some_and(|stored| !pdf_location(folder, stored).is_some_and(|path| path.is_file()));
            PaperListItem {
                extracting: text_status == "pending" && !file_missing && still_extracting(&id),
                id,
                file_missing,
                file_name: file_path.as_deref().map(display_name),
                title,
                text_status,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;
    use ImportOutcome::*;

    const PDF: &[u8] = b"%PDF-1.4\n1 0 obj << >> endobj\ntrailer << >>\n%%EOF\n";

    struct Setup {
        _root: tempfile::TempDir,
        library: PathBuf,
        downloads: PathBuf,
        conn: Connection,
    }

    fn setup() -> Setup {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("library");
        let downloads = root.path().join("downloads");
        std::fs::create_dir_all(library.join(PDF_FOLDER)).unwrap();
        std::fs::create_dir_all(&downloads).unwrap();
        let conn = create_library(&library, "Review").unwrap();
        Setup { _root: root, library, downloads, conn }
    }

    impl Setup {
        /// Writes a file in the downloads folder and returns its path.
        fn source(&self, name: &str, content: &[u8]) -> PathBuf {
            let path = self.downloads.join(name);
            std::fs::write(&path, content).unwrap();
            path
        }

        /// A valid PDF whose content differs from every other name's.
        fn pdf(&self, name: &str) -> PathBuf {
            self.source(name, format!("%PDF-1.4\n{name}\n%%EOF\n").as_bytes())
        }

        fn import(&self, sources: &[PathBuf]) -> ImportReport {
            import_pdfs(&self.conn, &self.library, sources, false).unwrap()
        }

        fn add_anyway(&self, sources: &[PathBuf]) -> ImportReport {
            import_pdfs(&self.conn, &self.library, sources, true).unwrap()
        }

        fn outcomes(&self, sources: &[PathBuf]) -> Vec<ImportOutcome> {
            self.import(sources).files.into_iter().map(|f| f.outcome).collect()
        }

        fn papers(&self) -> i64 {
            self.conn.query_row("SELECT count(*) FROM paper", [], |r| r.get(0)).unwrap()
        }

        fn stored(&self, name: &str) -> Vec<u8> {
            std::fs::read(self.library.join(PDF_FOLDER).join(name)).unwrap()
        }

        fn files_in_pdfs(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.library.join(PDF_FOLDER))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        fn fill_to(&self, papers: i64) {
            for i in self.papers()..papers {
                self.conn
                    .execute("INSERT INTO paper (file_path) VALUES (?1)", [format!("pdfs/old-{i}.pdf")])
                    .unwrap();
            }
        }
    }

    #[test]
    fn copies_each_pdf_and_creates_an_undecided_paper() {
        let s = setup();
        let source = s.source("Kim 2021.pdf", PDF);
        let report = s.import(std::slice::from_ref(&source));

        assert_eq!(report.added, 1);
        assert_eq!(
            report.files,
            [ImportedFile {
                file_name: "Kim 2021.pdf".into(),
                path: source.to_str().unwrap().into(),
                outcome: Added,
                identical_to: None,
            }]
        );
        assert_eq!(s.stored("Kim 2021.pdf"), PDF);
        // The original stays where it was.
        assert_eq!(std::fs::read(&source).unwrap(), PDF);
        let (path, decision, status): (String, String, String) = s
            .conn
            .query_row("SELECT file_path, decision, text_status FROM paper", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((path.as_str(), decision.as_str(), status.as_str()), ("pdfs/Kim 2021.pdf", "undecided", "pending"));
    }

    #[test]
    fn at_249_papers_one_of_three_files_is_added() {
        // FR-01.4 acceptance criterion.
        let s = setup();
        s.fill_to(249);
        let sources = [s.source("a.pdf", PDF), s.source("b.pdf", PDF), s.source("c.pdf", PDF)];

        let report = s.import(&sources);
        assert_eq!(report.added, 1);
        let outcomes: Vec<_> = report.files.iter().map(|f| f.outcome).collect();
        assert_eq!(outcomes, [Added, OverLimit, OverLimit]);
        assert_eq!(s.papers(), 250);
        assert_eq!(s.files_in_pdfs(), ["a.pdf"]);
    }

    #[test]
    fn skips_a_name_that_is_already_in_the_library_without_overwriting() {
        // FR-01.5 acceptance criterion; Windows ignores case in file names.
        let s = setup();
        s.import(&[s.source("jurnal-a.pdf", PDF)]);
        let other = b"%PDF-1.7\nsomething else\n%%EOF";
        let again = [s.source("Jurnal-A.PDF", other)];

        assert_eq!(s.outcomes(&again), [AlreadyInLibrary]);
        assert_eq!(s.papers(), 1);
        assert_eq!(s.stored("jurnal-a.pdf"), PDF);
    }

    #[test]
    fn two_files_with_the_same_name_in_one_import_add_one_paper() {
        let s = setup();
        let first = s.source("same.pdf", PDF);
        std::fs::create_dir_all(s.downloads.join("other")).unwrap();
        let second = s.downloads.join("other").join("same.pdf");
        std::fs::write(&second, PDF).unwrap();
        assert_eq!(s.outcomes(&[first, second]), [Added, AlreadyInLibrary]);
    }

    #[test]
    fn rejects_files_that_are_not_complete_pdfs() {
        let s = setup();
        let sources = [
            s.source("notes.docx", b"PK\x03\x04 a word document"),
            s.source("cut-off.pdf", b"%PDF-1.4\n1 0 obj << >> endobj\n"),
            s.source("empty.pdf", b""),
            s.downloads.join("does-not-exist.pdf"),
            s.downloads.clone(),
        ];
        assert_eq!(s.outcomes(&sources), [NotPdf, NotPdf, NotPdf, NotPdf, NotPdf]);
        assert_eq!(s.papers(), 0);
        assert!(s.files_in_pdfs().is_empty());
    }

    #[test]
    fn a_bad_file_does_not_stop_the_others() {
        // FR-01.3.
        let s = setup();
        let sources = [s.pdf("a.pdf"), s.source("bad.pdf", b"nope"), s.pdf("b.pdf")];
        let report = s.import(&sources);
        assert_eq!(report.added, 2);
        assert_eq!(s.files_in_pdfs(), ["a.pdf", "b.pdf"]);
    }

    #[test]
    fn attaches_the_file_to_a_restored_paper_even_at_the_limit() {
        // FR-01.5: a paper restored from a backup has a file name but no file yet.
        let s = setup();
        s.conn.execute("INSERT INTO paper (file_path, title) VALUES ('pdfs/b.pdf', 'Restored')", []).unwrap();
        s.fill_to(250);
        assert!(list_papers(&s.conn, &s.library, &|_| true).unwrap()[0].file_missing);

        let report = s.import(&[s.source("B.pdf", PDF)]);
        assert_eq!(report.files[0].outcome, Attached);
        assert_eq!(report.added, 1);
        assert_eq!(s.papers(), 250);
        // The stored name keeps its original spelling.
        assert_eq!(s.files_in_pdfs(), ["b.pdf"]);
        let restored = &list_papers(&s.conn, &s.library, &|_| true).unwrap()[0];
        assert_eq!((restored.title.as_str(), restored.file_missing), ("Restored", false));
    }

    #[test]
    fn reuses_an_identical_leftover_file_and_never_overwrites_a_different_one() {
        let s = setup();
        std::fs::write(s.library.join(PDF_FOLDER).join("same.pdf"), PDF).unwrap();
        let different = b"%PDF-1.4\nthe user's own file\n%%EOF";
        std::fs::write(s.library.join(PDF_FOLDER).join("other.pdf"), different).unwrap();

        let mine = s.pdf("other.pdf");
        let mine_content = std::fs::read(&mine).unwrap();
        let report = s.import(&[s.source("same.pdf", PDF), mine]);
        assert_eq!(report.added, 2);
        assert_eq!(s.files_in_pdfs(), ["other (2).pdf", "other.pdf", "same.pdf"]);
        assert_eq!(s.stored("other.pdf"), different);
        assert_eq!(s.stored("other (2).pdf"), mine_content);

        let names: Vec<_> = list_papers(&s.conn, &s.library, &|_| true)
            .unwrap()
            .into_iter()
            .map(|p| p.file_name.unwrap())
            .collect();
        assert_eq!(names, ["same.pdf", "other (2).pdf"]);
    }

    #[test]
    fn reports_a_write_failure_without_creating_papers() {
        let s = setup();
        // A file where the pdfs folder should be: nothing can be copied into it.
        std::fs::remove_dir(s.library.join(PDF_FOLDER)).unwrap();
        std::fs::write(s.library.join(PDF_FOLDER), "in the way").unwrap();

        assert_eq!(s.outcomes(&[s.source("a.pdf", PDF)]), [WriteFailed]);
        assert_eq!(s.papers(), 0);
    }

    #[test]
    fn recreates_a_deleted_pdfs_folder() {
        let s = setup();
        std::fs::remove_dir(s.library.join(PDF_FOLDER)).unwrap();
        assert_eq!(s.outcomes(&[s.source("a.pdf", PDF)]), [Added]);
    }

    #[test]
    fn lists_papers_oldest_first_and_flags_missing_files() {
        let s = setup();
        s.import(&[s.pdf("first.pdf"), s.pdf("second.pdf")]);
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("first.pdf")).unwrap();

        let papers = list_papers(&s.conn, &s.library, &|_| true).unwrap();
        let summary: Vec<_> = papers.iter().map(|p| (p.file_name.as_deref().unwrap(), p.file_missing)).collect();
        assert_eq!(summary, [("first.pdf", true), ("second.pdf", false)]);
    }

    // ---- T-13: identical content under another name ----

    #[test]
    fn holds_back_a_file_identical_to_one_in_the_library() {
        let s = setup();
        s.import(&[s.source("original.pdf", PDF)]);

        let report = s.import(&[s.source("renamed.pdf", PDF)]);
        assert_eq!(report.added, 0);
        assert_eq!(report.files[0].outcome, Identical);
        assert_eq!(report.files[0].identical_to.as_deref(), Some("original.pdf"));
        assert_eq!(s.papers(), 1);
        assert_eq!(s.files_in_pdfs(), ["original.pdf"]);
    }

    #[test]
    fn add_anyway_imports_the_identical_file() {
        let s = setup();
        s.import(&[s.source("original.pdf", PDF)]);
        let report = s.add_anyway(&[s.source("renamed.pdf", PDF)]);
        assert_eq!(report.files[0].outcome, Added);
        assert_eq!(report.files[0].identical_to, None);
        assert_eq!(s.files_in_pdfs(), ["original.pdf", "renamed.pdf"]);
    }

    #[test]
    fn finds_identical_files_within_one_import_without_stopping_the_others() {
        let s = setup();
        let sources = [s.source("a.pdf", PDF), s.source("copy.pdf", PDF), s.pdf("b.pdf")];
        assert_eq!(s.outcomes(&sources), [Added, Identical, Added]);
    }

    #[test]
    fn same_size_but_different_content_is_not_identical() {
        let s = setup();
        s.import(&[s.source("a.pdf", b"%PDF-1.4 AAAA %%EOF")]);
        assert_eq!(s.outcomes(&[s.source("b.pdf", b"%PDF-1.4 BBBB %%EOF")]), [Added]);
    }

    #[test]
    fn the_name_and_limit_rules_come_before_the_identical_check() {
        let s = setup();
        s.import(&[s.source("a.pdf", PDF)]);
        assert_eq!(s.outcomes(&[s.source("a.pdf", PDF)]), [AlreadyInLibrary]);
        s.fill_to(250);
        assert_eq!(s.outcomes(&[s.source("b.pdf", PDF)]), [OverLimit]);
        // "Add anyway" cannot pass the limit either.
        let report = s.add_anyway(&[s.source("c.pdf", PDF)]);
        assert_eq!(report.files[0].outcome, OverLimit);
    }

    #[test]
    fn a_paper_whose_file_is_missing_is_not_compared() {
        let s = setup();
        s.import(&[s.source("a.pdf", PDF)]);
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("a.pdf")).unwrap();
        assert_eq!(s.outcomes(&[s.source("b.pdf", PDF)]), [Added]);
    }

    #[test]
    fn new_papers_wait_for_text_unless_their_file_is_missing() {
        let s = setup();
        s.import(&[s.pdf("a.pdf"), s.pdf("b.pdf")]);
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("b.pdf")).unwrap();

        let papers = list_papers(&s.conn, &s.library, &|_| true).unwrap();
        let state: Vec<_> = papers.iter().map(|p| (p.text_status.as_str(), p.extracting)).collect();
        assert_eq!(state, [("pending", true), ("pending", false)]);
        let gave_up = list_papers(&s.conn, &s.library, &|_| false).unwrap();
        assert!(!gave_up[0].extracting);
    }

    // ---- T-16: the reader's file, and "Attach PDF" ----

    impl Setup {
        fn id_of(&self, file_path: &str) -> String {
            self.conn.query_row("SELECT id FROM paper WHERE file_path = ?1", [file_path], |r| r.get(0)).unwrap()
        }

        fn file_path_of(&self, id: &str) -> Option<String> {
            self.conn.query_row("SELECT file_path FROM paper WHERE id = ?1", [id], |r| r.get(0)).unwrap()
        }

        fn attach(&self, id: &str, source: &Path) -> ImportOutcome {
            attach_pdf(&self.conn, &self.library, id, source).unwrap()
        }
    }

    #[test]
    fn reads_the_pdf_of_a_paper() {
        let s = setup();
        s.import(&[s.source("a.pdf", PDF)]);
        let id = s.id_of("pdfs/a.pdf");
        let path = pdf_path(&s.conn, &s.library, &id).unwrap();
        assert_eq!(read_pdf(&path).unwrap(), PDF);
    }

    #[test]
    fn a_missing_file_is_reported_as_not_found() {
        let s = setup();
        s.import(&[s.source("a.pdf", PDF)]);
        let id = s.id_of("pdfs/a.pdf");
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("a.pdf")).unwrap();
        assert_eq!(pdf_path(&s.conn, &s.library, &id).unwrap_err().code, ErrorCode::NotFound);

        let no_name: String = s.conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap();
        assert_eq!(pdf_path(&s.conn, &s.library, &no_name).unwrap_err().code, ErrorCode::NotFound);
        assert_eq!(pdf_path(&s.conn, &s.library, "no-such-id").unwrap_err().code, ErrorCode::Other);
    }

    #[test]
    fn never_reads_outside_the_pdfs_folder() {
        let s = setup();
        std::fs::write(s.library.join("secret.pdf"), PDF).unwrap();
        for stored in ["../secret.pdf", "pdfs/../secret.pdf", "secret.pdf", "pdfs/", "pdfs/a/b.pdf", "pdfs\\..\\secret.pdf"] {
            s.conn.execute("DELETE FROM paper", []).unwrap();
            s.conn.execute("INSERT INTO paper (file_path) VALUES (?1)", [stored]).unwrap();
            let id = s.id_of(stored);
            assert_eq!(pdf_path(&s.conn, &s.library, &id).unwrap_err().code, ErrorCode::NotFound, "{stored}");
            assert!(list_papers(&s.conn, &s.library, &|_| true).unwrap()[0].file_missing, "{stored}");
        }
        assert_eq!(pdf_location(&s.library, "pdfs/a b.pdf"), Some(s.library.join("pdfs").join("a b.pdf")));
    }

    #[test]
    fn attach_puts_the_file_back_under_the_papers_own_name() {
        let s = setup();
        s.import(&[s.source("kim.pdf", PDF)]);
        let id = s.id_of("pdfs/kim.pdf");
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("kim.pdf")).unwrap();

        let found = s.source("Kim et al 2021 (final).pdf", PDF);
        assert_eq!(s.attach(&id, &found), Attached);
        assert_eq!(s.files_in_pdfs(), ["kim.pdf"]);
        assert_eq!(s.file_path_of(&id).as_deref(), Some("pdfs/kim.pdf"));
        assert_eq!(read_pdf(&pdf_path(&s.conn, &s.library, &id).unwrap()).unwrap(), PDF);
        // The chosen file stays where it was.
        assert!(found.exists());
    }

    #[test]
    fn attach_never_replaces_a_file_that_is_there() {
        let s = setup();
        s.import(&[s.source("kim.pdf", PDF)]);
        let id = s.id_of("pdfs/kim.pdf");
        assert_eq!(s.attach(&id, &s.pdf("other.pdf")), AlreadyInLibrary);
        assert_eq!(s.stored("kim.pdf"), PDF);
    }

    #[test]
    fn attach_gives_a_paper_without_a_name_the_files_name() {
        let s = setup();
        s.import(&[s.source("taken.pdf", PDF)]);
        let id: String = s.conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap();

        // Another paper already uses this name: the attached file gets a free one.
        let source = s.pdf("taken.pdf");
        assert_eq!(s.attach(&id, &source), Attached);
        assert_eq!(s.file_path_of(&id).as_deref(), Some("pdfs/taken (2).pdf"));
        assert_eq!(s.stored("taken.pdf"), PDF);
        assert_eq!(s.files_in_pdfs(), ["taken (2).pdf", "taken.pdf"]);
    }

    #[test]
    fn attach_refuses_a_file_that_is_not_a_pdf_and_changes_nothing() {
        let s = setup();
        s.import(&[s.source("kim.pdf", PDF)]);
        let id = s.id_of("pdfs/kim.pdf");
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("kim.pdf")).unwrap();

        assert_eq!(s.attach(&id, &s.source("notes.docx", b"PK\x03\x04")), NotPdf);
        assert!(s.files_in_pdfs().is_empty());
        assert_eq!(s.file_path_of(&id).as_deref(), Some("pdfs/kim.pdf"));
        assert_eq!(attach_pdf(&s.conn, &s.library, "no-such-id", &s.pdf("x.pdf")).unwrap_err().code, ErrorCode::Other);
    }

    #[test]
    fn attach_keeps_the_decision_and_other_data() {
        // Rule 1: only the user changes a decision; attaching a file is not a decision.
        let s = setup();
        s.import(&[s.source("kim.pdf", PDF)]);
        let id = s.id_of("pdfs/kim.pdf");
        s.conn.execute("UPDATE paper SET decision = 'exclude', title = 'Kim' WHERE id = ?1", [&id]).unwrap();
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("kim.pdf")).unwrap();

        assert_eq!(s.attach(&id, &s.pdf("found.pdf")), Attached);
        let (decision, title): (String, String) = s
            .conn
            .query_row("SELECT decision, title FROM paper WHERE id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((decision.as_str(), title.as_str()), ("exclude", "Kim"));
    }

    #[test]
    fn attach_reports_a_write_failure() {
        let s = setup();
        s.import(&[s.source("kim.pdf", PDF)]);
        let id = s.id_of("pdfs/kim.pdf");
        std::fs::remove_dir_all(s.library.join(PDF_FOLDER)).unwrap();
        std::fs::write(s.library.join(PDF_FOLDER), "in the way").unwrap();
        assert_eq!(s.attach(&id, &s.pdf("found.pdf")), WriteFailed);
    }

    // ---- T-15: deleting a paper ----

    /// Gives a paper one row in every table that hangs off it, plus a library chat
    /// that quotes it.
    fn add_everything(conn: &Connection, paper: &str) {
        let insert = |sql: &str, id: &str| -> String { conn.query_row(sql, [id], |r| r.get(0)).unwrap() };
        insert("INSERT INTO highlight (paper_id, page, color, text, rects) VALUES (?1, 1, 'green', 't', '[]') RETURNING id", paper);
        let chunk = insert("INSERT INTO text_chunk (paper_id, page_start, page_end, text) VALUES (?1, 1, 1, 't') RETURNING id", paper);
        let suggestion = insert(
            "INSERT INTO suggestion (paper_id, proposed_decision, rationale, model_label) VALUES (?1, 'include', 'r', 'm') RETURNING id",
            paper,
        );
        let thread = insert("INSERT INTO chat_thread (paper_id, scope) VALUES (?1, 'paper') RETURNING id", paper);
        let message = insert("INSERT INTO chat_message (thread_id, role, content) VALUES (?1, 'assistant', 'a') RETURNING id", &thread);
        let library_thread: String = conn
            .query_row("INSERT INTO chat_thread (scope) VALUES ('library') RETURNING id", [], |r| r.get(0))
            .unwrap();
        let library_message = insert(
            "INSERT INTO chat_message (thread_id, role, content) VALUES (?1, 'assistant', 'b') RETURNING id",
            &library_thread,
        );
        for (column, id) in [("suggestion_id", &suggestion), ("message_id", &message), ("message_id", &library_message)] {
            conn.execute(
                &format!("INSERT INTO citation (chunk_id, {column}, quote, verified) VALUES (?1, ?2, 'q', 1)"),
                params![chunk, id],
            )
            .unwrap();
        }
    }

    fn rows(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn delete_removes_the_paper_its_file_and_everything_that_belongs_to_it() {
        let s = setup();
        let source = s.pdf("gone.pdf");
        s.import(&[source.clone(), s.pdf("kept.pdf")]);
        let (gone, kept) = (s.id_of("pdfs/gone.pdf"), s.id_of("pdfs/kept.pdf"));
        add_everything(&s.conn, &gone);
        add_everything(&s.conn, &kept);

        delete_paper(&s.conn, &s.library, &gone).unwrap();

        assert_eq!(s.files_in_pdfs(), ["kept.pdf"]);
        // The user's own file is not touched.
        assert!(source.exists());
        assert_eq!(s.file_path_of(&kept).as_deref(), Some("pdfs/kept.pdf"));
        for table in ["paper", "highlight", "text_chunk", "suggestion"] {
            assert_eq!(rows(&s.conn, table), 1, "{table}");
        }
        // The kept paper's chat and both library chats stay; only the citations that
        // quoted the deleted paper are gone.
        assert_eq!(rows(&s.conn, "chat_thread"), 3);
        assert_eq!(rows(&s.conn, "chat_message"), 3);
        assert_eq!(rows(&s.conn, "citation"), 3);
    }

    #[test]
    fn delete_keeps_references_reasons_and_tags() {
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        let id = s.id_of("pdfs/a.pdf");
        s.conn
            .execute_batch(
                "INSERT INTO reference (id, no, title) VALUES ('r', 1, 't');
                 INSERT INTO exclusion_reason (id, code, label, sort_order) VALUES ('e', 'E1', 'x', 1);
                 INSERT INTO extraction_tag (id, label, sort_order) VALUES ('t', 'x', 1);
                 UPDATE paper SET reference_id = 'r', decision = 'exclude', exclusion_reason_id = 'e';",
            )
            .unwrap();
        delete_paper(&s.conn, &s.library, &id).unwrap();
        for table in ["reference", "exclusion_reason", "extraction_tag"] {
            assert_eq!(rows(&s.conn, table), 1, "{table}");
        }
    }

    #[test]
    fn delete_frees_a_place_and_the_file_name() {
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        s.fill_to(250);
        delete_paper(&s.conn, &s.library, &s.id_of("pdfs/a.pdf")).unwrap();
        assert_eq!(s.outcomes(&[s.pdf("a.pdf")]), [Added]);
        assert_eq!(s.files_in_pdfs(), ["a.pdf"]);
    }

    #[test]
    fn delete_works_for_a_paper_without_a_file() {
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        std::fs::remove_file(s.library.join(PDF_FOLDER).join("a.pdf")).unwrap();
        delete_paper(&s.conn, &s.library, &s.id_of("pdfs/a.pdf")).unwrap();
        let no_name: String = s.conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap();
        delete_paper(&s.conn, &s.library, &no_name).unwrap();
        assert_eq!(s.papers(), 0);
    }

    #[test]
    fn delete_removes_a_read_only_file() {
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        let path = s.library.join(PDF_FOLDER).join("a.pdf");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        delete_paper(&s.conn, &s.library, &s.id_of("pdfs/a.pdf")).unwrap();
        assert!(s.files_in_pdfs().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn delete_changes_nothing_while_another_program_locks_the_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        let id = s.id_of("pdfs/a.pdf");
        add_everything(&s.conn, &id);
        // Share mode 0: nobody else may read, rename, or delete the file.
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(s.library.join(PDF_FOLDER).join("a.pdf"))
            .unwrap();

        assert_eq!(delete_paper(&s.conn, &s.library, &id).unwrap_err().code, ErrorCode::Other);
        assert_eq!((s.papers(), rows(&s.conn, "highlight")), (1, 1));
        drop(lock);
        assert_eq!(s.files_in_pdfs(), ["a.pdf"]);
        delete_paper(&s.conn, &s.library, &id).unwrap();
        assert_eq!(s.papers(), 0);
    }

    #[test]
    fn delete_puts_the_file_back_when_the_database_cannot_be_saved() {
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        let id = s.id_of("pdfs/a.pdf");
        // A deferred foreign key violation only fails at COMMIT, after the file moved.
        s.conn
            .execute_batch(
                "CREATE TABLE blocker (paper_id TEXT REFERENCES paper (id) DEFERRABLE INITIALLY DEFERRED);
                 INSERT INTO blocker SELECT id FROM paper;",
            )
            .unwrap();
        assert!(delete_paper(&s.conn, &s.library, &id).is_err());
        assert_eq!(s.papers(), 1);
        assert_eq!(s.files_in_pdfs(), ["a.pdf"]);
    }

    #[test]
    fn delete_never_overwrites_or_removes_other_files() {
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        // The user's own file happens to have the temporary name.
        std::fs::write(s.library.join(PDF_FOLDER).join("a.deleting.pdf"), "mine").unwrap();
        delete_paper(&s.conn, &s.library, &s.id_of("pdfs/a.pdf")).unwrap();
        assert_eq!(s.files_in_pdfs(), ["a.deleting.pdf"]);
        assert_eq!(s.stored("a.deleting.pdf"), b"mine");
    }

    #[test]
    fn delete_never_removes_a_file_outside_pdfs_or_used_by_another_paper() {
        let s = setup();
        std::fs::write(s.library.join("secret.pdf"), PDF).unwrap();
        s.import(&[s.pdf("a.pdf")]);
        s.conn
            .execute_batch("INSERT INTO paper (file_path) VALUES ('pdfs/../secret.pdf'), ('pdfs/A.pdf');")
            .unwrap();
        delete_paper(&s.conn, &s.library, &s.id_of("pdfs/../secret.pdf")).unwrap();
        delete_paper(&s.conn, &s.library, &s.id_of("pdfs/A.pdf")).unwrap();
        assert!(s.library.join("secret.pdf").exists());
        assert_eq!(s.files_in_pdfs(), ["a.pdf"]);
        assert_eq!(s.papers(), 1);
    }

    #[test]
    fn delete_of_an_unknown_paper_changes_nothing() {
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        assert_eq!(delete_paper(&s.conn, &s.library, "no-such-id").unwrap_err().code, ErrorCode::Other);
        assert_eq!(s.papers(), 1);
        assert_eq!(s.files_in_pdfs(), ["a.pdf"]);
    }

    // ---- T-18: last page read ----

    #[test]
    fn remembers_the_last_page_of_each_paper() {
        let s = setup();
        s.import(&[s.pdf("a.pdf"), s.pdf("b.pdf")]);
        let (a, b) = (s.id_of("pdfs/a.pdf"), s.id_of("pdfs/b.pdf"));
        assert_eq!(last_page(&s.conn, &a).unwrap(), 1);

        save_last_page(&s.conn, &a, 7).unwrap();
        assert_eq!((last_page(&s.conn, &a).unwrap(), last_page(&s.conn, &b).unwrap()), (7, 1));
        // A page below 1 does not exist.
        save_last_page(&s.conn, &a, 0).unwrap();
        assert_eq!(last_page(&s.conn, &a).unwrap(), 1);
        assert_eq!(save_last_page(&s.conn, "no-such-id", 2).unwrap_err().code, ErrorCode::Other);
        assert_eq!(last_page(&s.conn, "no-such-id").unwrap_err().code, ErrorCode::Other);
    }

    #[test]
    fn saving_the_last_page_changes_nothing_else() {
        // Rule 1: reading is not a decision.
        let s = setup();
        s.import(&[s.pdf("a.pdf")]);
        let id = s.id_of("pdfs/a.pdf");
        s.conn.execute("UPDATE paper SET decision = 'include', assessment_note = 'n'", []).unwrap();
        save_last_page(&s.conn, &id, 3).unwrap();
        let row: (String, String, String) = s
            .conn
            .query_row("SELECT decision, assessment_note, file_path FROM paper", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(row, ("include".into(), "n".into(), "pdfs/a.pdf".into()));
    }

    #[test]
    fn the_ui_receives_the_field_names_it_expects() {
        let s = setup();
        let report = serde_json::to_value(s.import(&[s.source("a.pdf", PDF)])).unwrap();
        assert_eq!(report["files"][0]["fileName"], "a.pdf");
        assert_eq!(report["files"][0]["outcome"], "added");
        for field in ["path", "identicalTo"] {
            assert!(report["files"][0].get(field).is_some(), "{field} is missing");
        }
        let paper = serde_json::to_value(&list_papers(&s.conn, &s.library, &|_| true).unwrap()[0]).unwrap();
        for field in ["id", "fileName", "title", "fileMissing", "textStatus", "extracting"] {
            assert!(paper.get(field).is_some(), "{field} is missing");
        }
    }
}
