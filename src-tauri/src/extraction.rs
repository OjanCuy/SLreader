//! Background text extraction (FR-01.6): one worker thread with its own database
//! connection, so reading, deciding, and importing never wait for it.
//!
//! The worker handles the papers with `text_status = 'pending'` of the open library.
//! After the app is closed, the remaining papers stay pending and are picked up
//! again the next time the library is opened (PRD Section 8).
//!
//! A password-protected paper stays pending until the user opens it in the reader with
//! the right password (T-19). That password is handed to the worker, kept in memory
//! only until the paper has been tried once, and never written anywhere (PRD Section 8).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::db::library::open_library;
use crate::papers::pdf_location;
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
    /// Passwords of locked papers of the target library, by paper id. Each one is
    /// removed when its paper is tried, and all are dropped when the target changes
    /// or the worker runs out of work.
    passwords: HashMap<String, String>,
    /// pdfium.dll could not be loaded: nothing is extracted in this run.
    unavailable: bool,
}

impl State {
    /// Points the worker at a library. Passwords and failures of another library (or
    /// an earlier opening of the same one) do not carry over.
    fn aim_at(&mut self, target: Target) {
        if self.target.as_ref() != Some(&target) {
            self.passwords.clear();
        }
        self.target = Some(target);
        self.failed.clear();
    }
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
        state.aim_at(Target { session, folder: folder.to_path_buf() });
        state.woken = true;
        self.0.wake.notify_one();
    }

    /// Extracts a password-protected paper with the password the user just opened it
    /// with. The password is used once and then dropped, whether it works or not.
    pub fn extract_with_password(&self, session: u64, folder: &Path, paper_id: &str, password: String) {
        let mut state = lock(&self.0.state);
        if state.unavailable {
            return;
        }
        state.aim_at(Target { session, folder: folder.to_path_buf() });
        state.passwords.insert(paper_id.to_string(), password);
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
        // Nothing left to do: a password that was not used (its paper already had its
        // text, was deleted, or is in a read-only library) is not kept. A request that
        // came in meanwhile has woken the worker again and keeps its password.
        let mut state = lock(&shared.state);
        if !state.woken {
            state.passwords.clear();
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
        let password = lock(&shared.state).passwords.remove(&paper_id);

        let saved = file_path
            .and_then(|stored| pdf_location(&target.folder, &stored))
            .ok_or(())
            .and_then(|path| extract_pages(pdfium, &path, password.as_deref()).map_err(|_| ()))
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
    use crate::text::tests::{make_locked_pdf, make_pdf, test_dll};
    use std::sync::mpsc;
    use std::time::Duration;

    const TEXT: &str = "Recirculating aquaculture systems for tilapia production in Indonesia\n\
                        Feed conversion ratio was 1.4 in treatment B and 1.7 in treatment C.";

    const PASSWORD: &str = "kunci-rahasia-19";

    /// A library with one paper per (file name, pages, file present) entry. A file
    /// whose name starts with "locked" is protected with `PASSWORD`.
    fn library(papers: &[(&str, &[&str], bool)]) -> (tempfile::TempDir, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        std::fs::create_dir_all(dir.path().join(PDF_FOLDER)).unwrap();
        let ids = papers
            .iter()
            .map(|(name, pages, present)| {
                let path = format!("{PDF_FOLDER}/{name}");
                if *present {
                    let bytes =
                        if name.starts_with("locked") { make_locked_pdf(pages, PASSWORD) } else { make_pdf(pages) };
                    std::fs::write(dir.path().join(&path), bytes).unwrap();
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

    fn passwords_held(extraction: &Extraction) -> usize {
        lock(&extraction.0.state).passwords.len()
    }

    /// Every byte the library folder holds on disk, database and journal files included.
    fn folder_bytes(dir: &Path) -> Vec<u8> {
        let mut bytes = Vec::new();
        for entry in walk(dir) {
            bytes.extend(std::fs::read(entry).unwrap());
        }
        bytes
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .flat_map(|path| if path.is_dir() { walk(&path) } else { vec![path] })
            .collect()
    }

    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle.as_bytes())
    }

    #[test]
    fn a_locked_paper_gets_its_text_once_the_right_password_is_given() {
        let (dir, ids) = library(&[("locked.pdf", &[TEXT], true)]);
        let (extraction, receiver) = start();
        extraction.work_on(3, dir.path());
        assert_eq!(wait_for(&receiver, 1), [3]);
        assert_eq!(statuses(dir.path()), ["pending"]);
        assert!(!extraction.still_extracting(&ids[0]));

        // A wrong password is tried once and dropped.
        extraction.extract_with_password(3, dir.path(), &ids[0], "wrong".into());
        assert_eq!(wait_for(&receiver, 1), [3]);
        assert_eq!(statuses(dir.path()), ["pending"]);
        assert!(!extraction.still_extracting(&ids[0]));
        assert_eq!(passwords_held(&extraction), 0);

        extraction.extract_with_password(3, dir.path(), &ids[0], PASSWORD.into());
        assert_eq!(wait_for(&receiver, 1), [3]);
        assert_eq!(statuses(dir.path()), ["text_ready"]);
        assert_eq!(passwords_held(&extraction), 0);

        // The password is not written anywhere in the library folder (PRD Section 8).
        drop(extraction);
        assert!(!contains(&folder_bytes(dir.path()), PASSWORD));
    }

    #[test]
    fn an_unused_password_is_not_kept() {
        // The paper already has its text: the password has nothing to unlock.
        let (dir, ids) = library(&[("text.pdf", &[TEXT], true)]);
        let (extraction, receiver) = start();
        extraction.work_on(1, dir.path());
        assert_eq!(wait_for(&receiver, 1), [1]);

        extraction.extract_with_password(1, dir.path(), &ids[0], PASSWORD.into());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while passwords_held(&extraction) > 0 {
            assert!(std::time::Instant::now() < deadline, "the password was kept");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(statuses(dir.path()), ["text_ready"]);
    }

    #[test]
    fn a_password_is_not_used_for_another_library() {
        let (first, ids) = library(&[("locked.pdf", &[TEXT], true)]);
        let (other, _) = library(&[]);
        let state = Mutex::new(State::default());
        let mut state = lock(&state);
        state.aim_at(Target { session: 1, folder: first.path().into() });
        state.passwords.insert(ids[0].clone(), PASSWORD.into());
        // Opening the same library again, or another one, drops it.
        state.aim_at(Target { session: 2, folder: first.path().into() });
        assert!(state.passwords.is_empty());
        state.passwords.insert(ids[0].clone(), PASSWORD.into());
        state.aim_at(Target { session: 3, folder: other.path().into() });
        assert!(state.passwords.is_empty());
        // An import into the library being worked on keeps it.
        state.passwords.insert(ids[0].clone(), PASSWORD.into());
        state.aim_at(Target { session: 3, folder: other.path().into() });
        assert_eq!(state.passwords.len(), 1);
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
