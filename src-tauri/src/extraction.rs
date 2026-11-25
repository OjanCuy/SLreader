//! Background text extraction (FR-01.6): one worker thread with its own database
//! connection, so reading, deciding, and importing never wait for it.
//!
//! The worker handles the papers with `text_status = 'pending'` of the open library.
//! After the app is closed, the remaining papers stay pending and are picked up
//! again the next time the library is opened (PRD Section 8).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::db::library::open_library;
use crate::text::{extract_pages, load_pdfium, save_text};

/// Called after each paper with the session it belongs to.
type Notify = Box<dyn Fn(u64) + Send>;

pub struct Extraction(Arc<Shared>);

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

#[derive(Default)]
struct State {
    /// The library to work on: set whenever a library is opened or PDFs are imported.
    target: Option<Target>,
    woken: bool,
    /// Papers whose text could not be read in this run (missing, damaged, or
    /// password-protected file). They stay "pending" and are tried again after the
    /// next import or opening.
    failed: HashSet<String>,
    /// pdfium.dll could not be loaded: nothing is extracted in this run.
    unavailable: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct Target {
    session: u64,
    folder: PathBuf,
}

impl Extraction {
    /// Starts the worker thread. `dll` is the path of pdfium.dll.
    pub fn start(dll: PathBuf, notify: impl Fn(u64) + Send + 'static) -> Self {
        let shared = Arc::new(Shared { state: Mutex::default(), wake: Condvar::new() });
        let worker = Arc::clone(&shared);
        let notify: Notify = Box::new(notify);
        std::thread::Builder::new()
            .name("text-extraction".into())
            .spawn(move || run(&worker, &dll, &notify))
            .expect("could not start the text extraction thread");
        Extraction(shared)
    }

    /// Asks the worker to extract the pending papers of this library. Papers that
    /// failed earlier are tried again.
    pub fn work_on(&self, session: u64, folder: &Path) {
        let mut state = lock(&self.0.state);
        state.target = Some(Target { session, folder: folder.to_path_buf() });
        state.failed.clear();
        state.woken = true;
        self.0.wake.notify_one();
    }

    /// Whether a pending paper is still expected to get its text in this run.
    pub fn still_extracting(&self, paper_id: &str) -> bool {
        let state = lock(&self.0.state);
        !state.unavailable && !state.failed.contains(paper_id)
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run(shared: &Shared, dll: &Path, notify: &Notify) {
    let pdfium = match load_pdfium(dll) {
        Ok(pdfium) => pdfium,
        Err(error) => {
            eprintln!("text extraction is off: could not load {}: {error}", dll.display());
            lock(&shared.state).unavailable = true;
            return;
        }
    };
    loop {
        let target = {
            let mut state = lock(&shared.state);
            while !state.woken {
                state = shared.wake.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            state.woken = false;
            state.target.clone()
        };
        if let Some(target) = target {
            work(shared, &pdfium, &target, notify);
        }
    }
}

/// Extracts papers one by one until none are left or another library is opened.
fn work(shared: &Shared, pdfium: &pdfium_render::prelude::Pdfium, target: &Target, notify: &Notify) {
    // The worker's own connection. A read-only library is never written to.
    let Ok(opened) = open_library(&target.folder) else { return };
    if opened.read_only.is_some() {
        return;
    }
    let mut conn = opened.conn;

    loop {
        let failed = {
            let state = lock(&shared.state);
            if state.target.as_ref() != Some(target) {
                return;
            }
            state.failed.clone()
        };
        let Ok(next) = next_pending(&conn, &failed) else { return };
        let Some((paper_id, file_path)) = next else { return };

        let saved = file_path
            .ok_or(())
            .and_then(|path| extract_pages(pdfium, &target.folder.join(path)).map_err(|_| ()))
            .and_then(|pages| save_text(&mut conn, &paper_id, &pages).map_err(|_| ()));
        if saved.is_err() {
            lock(&shared.state).failed.insert(paper_id);
        }
        notify(target.session);
    }
}

/// The oldest pending paper that has not failed in this run: its id and file path.
fn next_pending(
    conn: &rusqlite::Connection,
    failed: &HashSet<String>,
) -> rusqlite::Result<Option<(String, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_path FROM paper WHERE text_status = 'pending' ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
    for row in rows {
        let (id, path) = row?;
        if !failed.contains(&id) {
            return Ok(Some((id, path)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;
    use crate::libraries::PDF_FOLDER;
    use crate::text::tests::{make_pdf, test_dll};
    use std::sync::mpsc;
    use std::time::Duration;

    const TEXT: &str = "Recirculating aquaculture systems for tilapia production in Indonesia\n\
                        Feed conversion ratio was 1.4 in treatment B and 1.7 in treatment C.";

    /// A library with one paper per (file name, pages, file present) entry.
    fn library(papers: &[(&str, &[&str], bool)]) -> (tempfile::TempDir, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        std::fs::create_dir_all(dir.path().join(PDF_FOLDER)).unwrap();
        let ids = papers
            .iter()
            .map(|(name, pages, present)| {
                let path = format!("{PDF_FOLDER}/{name}");
                if *present {
                    std::fs::write(dir.path().join(&path), make_pdf(pages)).unwrap();
                }
                conn.query_row("INSERT INTO paper (file_path) VALUES (?1) RETURNING id", [&path], |r| r.get(0))
                    .unwrap()
            })
            .collect();
        (dir, ids)
    }

    fn statuses(dir: &Path) -> Vec<String> {
        let conn = open_library(dir).unwrap().conn;
        let mut stmt = conn.prepare("SELECT text_status FROM paper ORDER BY created_at, rowid").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    fn start() -> (Extraction, mpsc::Receiver<u64>) {
        let (sender, receiver) = mpsc::channel();
        let extraction = Extraction::start(test_dll(), move |session| {
            let _ = sender.send(session);
        });
        (extraction, receiver)
    }

    fn wait_for(receiver: &mpsc::Receiver<u64>, count: usize) -> Vec<u64> {
        (0..count).map(|_| receiver.recv_timeout(Duration::from_secs(30)).unwrap()).collect()
    }

    #[test]
    fn extracts_pending_papers_in_the_background() {
        let (dir, ids) = library(&[
            ("text.pdf", &[TEXT, TEXT], true),
            ("scan.pdf", &["", ""], true),
            ("missing.pdf", &[TEXT], false),
        ]);
        let (extraction, receiver) = start();
        extraction.work_on(7, dir.path());

        assert_eq!(wait_for(&receiver, 3), [7, 7, 7]);
        assert_eq!(statuses(dir.path()), ["text_ready", "no_text", "pending"]);
        // The missing file is not retried in a loop, and the UI stops showing progress.
        assert!(receiver.recv_timeout(Duration::from_millis(300)).is_err());
        assert!(!extraction.still_extracting(&ids[2]));

        // Once the file is there (e.g. attached later), a new request picks it up.
        std::fs::write(dir.path().join(PDF_FOLDER).join("missing.pdf"), make_pdf(&[TEXT])).unwrap();
        extraction.work_on(8, dir.path());
        assert_eq!(wait_for(&receiver, 1), [8]);
        assert_eq!(statuses(dir.path()), ["text_ready", "no_text", "text_ready"]);
    }

    #[test]
    fn a_read_only_library_is_not_touched() {
        let (dir, _) = library(&[("text.pdf", &[TEXT], true)]);
        std::fs::write(dir.path().join("library (conflicted copy).sqlite"), "x").unwrap();
        let (extraction, receiver) = start();
        extraction.work_on(1, dir.path());

        assert!(receiver.recv_timeout(Duration::from_millis(500)).is_err());
        std::fs::remove_file(dir.path().join("library (conflicted copy).sqlite")).unwrap();
        assert_eq!(statuses(dir.path()), ["pending"]);
    }

    #[test]
    fn without_pdfium_nothing_is_extracted_and_nothing_looks_busy() {
        let (dir, ids) = library(&[("text.pdf", &[TEXT], true)]);
        let extraction = Extraction::start(dir.path().join("no-such.dll"), |_| {});
        extraction.work_on(1, dir.path());

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while extraction.still_extracting(&ids[0]) {
            assert!(std::time::Instant::now() < deadline, "the worker never gave up");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(statuses(dir.path()), ["pending"]);
    }
}
