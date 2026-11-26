//! Backups of a library as JSON (FR-07.1, FR-07.2), and restoring them into the open
//! library or into a new one (Brief S-01).
//!
//! A backup holds the library's name and research questions, its inclusion criteria,
//! exclusion reasons, extraction tags, and highlight categories, the reference list,
//! and every paper with its metadata, decision, assessment note, and highlights. It
//! holds no PDF, no extracted text, no chats or AI suggestions, no app setting, and
//! never an API key (FR-09.2).
//!
//! Restoring merges by id and never deletes anything the user made:
//! - a paper or highlight whose id the library has is replaced by the backup's version
//!   (the UI asks first and names how many, FR-07.2); the others are added;
//! - criteria, reasons, tags, and categories the library has stay as they are. One
//!   with another id counts as the same when its code (reasons, criteria) or its name
//!   (tags, categories) is the same; what is still missing is added and reported
//!   (PRD Section 8, T-34);
//! - references the library has stay as they are; missing ones are added.
//!
//! Everything is one transaction: a restore that fails changes nothing.
//!
//! Restoring writes `paper.decision`, but only with decisions the user saved in the
//! backup, and only after the user chose to restore (rule 1 of CLAUDE.md).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::criteria;
use crate::db::library::MAX_PAPERS;
use crate::decisions::Decision;
use crate::export::markdown::safe_stem;
use crate::highlights::{self, NewHighlight, PaletteColor, Rect};
use crate::libraries::{ErrorCode, LibraryError, PDF_FOLDER};
use crate::papers::pdf_location;

/// Marks a file as an SLReader backup.
const FORMAT: &str = "slreader-backup";
/// Raised when the format changes; a newer backup is refused, not guessed at.
const VERSION: u64 = 1;
/// Far more than 250 papers with their highlights need.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub format: String,
    pub version: u64,
    pub exported_at: String,
    pub library: LibraryPart,
    /// Lists are in their order in Settings.
    pub inclusion_criteria: Vec<Criterion>,
    pub exclusion_reasons: Vec<Criterion>,
    pub extraction_tags: Vec<Tag>,
    pub highlight_categories: Vec<Category>,
    pub references: Vec<Reference>,
    pub papers: Vec<Paper>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPart {
    pub name: String,
    /// One per line.
    pub research_questions: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    pub code: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub label: String,
    pub color: PaletteColor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    pub id: String,
    pub no: i64,
    pub title: String,
    pub year: Option<i64>,
    pub source_database: Option<String>,
    pub doi: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paper {
    pub id: String,
    /// The PDF's name in the library's `pdfs` folder; the PDF itself is not in the
    /// backup. Adding a PDF with this name attaches it again (FR-01.5).
    pub file_name: Option<String>,
    pub reference_id: Option<String>,
    pub ref_no: Option<i64>,
    pub title: String,
    pub year: Option<i64>,
    pub source_database: Option<String>,
    pub doi: Option<String>,
    pub decision: Decision,
    /// Kept whatever the decision, like in the library (FR-04.3).
    pub exclusion_reason_id: Option<String>,
    pub assessment_note: String,
    pub last_page: u32,
    pub created_at: String,
    pub highlights: Vec<BackupHighlight>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupHighlight {
    pub id: String,
    pub category_id: String,
    pub tag_id: Option<String>,
    pub page: u32,
    pub text: String,
    pub rects: Vec<Rect>,
    pub note: String,
    pub created_at: String,
}

/// What restoring a backup into the open library would replace (FR-07.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub replaced_papers: usize,
    pub replaced_highlights: usize,
    /// Papers in the library afterwards; more than 250 is refused (FR-01.4).
    pub papers_after: usize,
}

/// What a restore added to the library's lists (PRD Section 8).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReport {
    pub added_reasons: usize,
    pub added_tags: usize,
}

fn not_a_backup(detail: impl Into<String>) -> LibraryError {
    LibraryError::new(ErrorCode::NotABackup, detail)
}

fn same_text(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

// ---- Export ----

/// The file name the save dialog starts with, e.g. "Aquaculture backup 2026-11-26.json".
pub fn file_name(conn: &Connection) -> Result<String, LibraryError> {
    let (name, date): (String, String) =
        conn.query_row("SELECT name, date('now', 'localtime') FROM library", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(format!("{} backup {date}.json", safe_stem(&name)))
}

/// The open library as a backup. Only reads.
pub fn export(conn: &Connection) -> Result<String, LibraryError> {
    // One transaction, so that every part is read from the same state.
    let read = conn.unchecked_transaction()?;
    let backup = collect(&read)?;
    drop(read);
    serde_json::to_string_pretty(&backup).map_err(|error| LibraryError::new(ErrorCode::Other, error.to_string()))
}

fn collect(conn: &Connection) -> Result<Backup, LibraryError> {
    let (exported_at, name, research_questions): (String, String, String) = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), name, research_questions FROM library",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let coded = |table: &str| -> Result<Vec<Criterion>, LibraryError> {
        let mut stmt = conn.prepare(&format!("SELECT id, code, label FROM {table} ORDER BY sort_order, rowid"))?;
        let rows = stmt.query_map([], |r| Ok(Criterion { id: r.get(0)?, code: r.get(1)?, label: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    };
    let extraction_tags =
        highlights::list_tags(conn)?.into_iter().map(|tag| Tag { id: tag.id, label: tag.label }).collect();
    let categories = highlights::list_categories(conn)?;
    let known_categories: HashSet<&str> = categories.iter().map(|c| c.id.as_str()).collect();

    let mut stmt =
        conn.prepare("SELECT id, no, title, year, source_database, doi FROM reference ORDER BY no, rowid")?;
    let references = stmt
        .query_map([], |r| {
            Ok(Reference {
                id: r.get(0)?,
                no: r.get(1)?,
                title: r.get(2)?,
                year: r.get(3)?,
                source_database: r.get(4)?,
                doi: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut highlights_of = conn.prepare(
        "SELECT id, highlight_category_id, extraction_tag_id, page, text, rects, note, created_at
         FROM highlight WHERE paper_id = ?1 ORDER BY page, created_at, rowid",
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, file_path, reference_id, ref_no, title, year, source_database, doi, decision,
                exclusion_reason_id, assessment_note, last_page, created_at
         FROM paper ORDER BY created_at, rowid",
    )?;
    let mut rows = stmt.query([])?;
    let mut papers = Vec::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let file_path: Option<String> = row.get(1)?;
        let decision: String = row.get(8)?;
        let decision = serde_json::from_value(serde_json::Value::String(decision))
            .map_err(|error| LibraryError::new(ErrorCode::Other, error.to_string()))?;
        let mut highlights = Vec::new();
        let mut found = highlights_of.query([&id])?;
        while let Some(h) = found.next()? {
            let highlight_id: String = h.get(0)?;
            let category_id: String = h.get(1)?;
            let rects: String = h.get(5)?;
            // Rows the app cannot show either (see `highlights::list_highlights`).
            let Ok(rects) = serde_json::from_str::<Vec<Rect>>(&rects) else {
                eprintln!("highlight {highlight_id} could not be read and is not in the backup");
                continue;
            };
            if !known_categories.contains(category_id.as_str()) {
                eprintln!("highlight {highlight_id} has an unknown category and is not in the backup");
                continue;
            }
            highlights.push(BackupHighlight {
                id: highlight_id,
                category_id,
                tag_id: h.get(2)?,
                page: h.get(3)?,
                text: h.get(4)?,
                rects,
                note: h.get(6)?,
                created_at: h.get(7)?,
            });
        }
        papers.push(Paper {
            id,
            file_name: file_path.map(|path| path.rsplit('/').next().unwrap_or_default().to_string()),
            reference_id: row.get(2)?,
            ref_no: row.get(3)?,
            title: row.get(4)?,
            year: row.get(5)?,
            source_database: row.get(6)?,
            doi: row.get(7)?,
            decision,
            exclusion_reason_id: row.get(9)?,
            assessment_note: row.get(10)?,
            last_page: row.get(11)?,
            created_at: row.get(12)?,
            highlights,
        });
    }

    Ok(Backup {
        format: FORMAT.to_string(),
        version: VERSION,
        exported_at,
        library: LibraryPart { name, research_questions },
        inclusion_criteria: coded("inclusion_criterion")?,
        exclusion_reasons: coded("exclusion_reason")?,
        extraction_tags,
        highlight_categories: categories
            .into_iter()
            .map(|c| Category { id: c.id, label: c.label, color: c.color })
            .collect(),
        references,
        papers,
    })
}

// ---- Reading a backup ----

/// Reads and checks a backup file. Nothing is changed.
pub fn read_file(path: &Path) -> Result<Backup, LibraryError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(not_a_backup("the file is too large"));
    }
    parse(&bytes)
}

/// A backup, checked: `NotABackup` for anything else, `NewerBackup` for one made by a
/// newer SLReader.
pub fn parse(bytes: &[u8]) -> Result<Backup, LibraryError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| not_a_backup(e.to_string()))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(not_a_backup("not marked as an SLReader backup"));
    }
    match value.get("version").and_then(|v| v.as_u64()) {
        Some(version) if version > VERSION => {
            return Err(LibraryError::new(ErrorCode::NewerBackup, format!("backup version {version}")));
        }
        Some(1..) => {}
        _ => return Err(not_a_backup("no version")),
    }
    let backup: Backup = serde_json::from_value(value).map_err(|e| not_a_backup(e.to_string()))?;
    check(&backup)?;
    Ok(backup)
}

/// Ids as the app makes them: 32 lowercase hex digits.
fn is_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn unique_ids<'a>(kind: &str, ids: impl Iterator<Item = &'a str>) -> Result<HashSet<&'a str>, LibraryError> {
    let mut seen = HashSet::new();
    for id in ids {
        if !is_id(id) || !seen.insert(id) {
            return Err(not_a_backup(format!("a {kind} id is not valid or used twice")));
        }
    }
    Ok(seen)
}

/// Everything restoring relies on, so that a restore can only fail on the disk.
fn check(backup: &Backup) -> Result<(), LibraryError> {
    let blank = |text: &str| text.trim().is_empty();
    let bad = |detail: &str| Err(not_a_backup(detail.to_string()));

    unique_ids("criterion", backup.inclusion_criteria.iter().map(|c| c.id.as_str()))?;
    let reasons = unique_ids("reason", backup.exclusion_reasons.iter().map(|c| c.id.as_str()))?;
    if backup.inclusion_criteria.iter().chain(&backup.exclusion_reasons).any(|c| blank(&c.code) || blank(&c.label)) {
        return bad("a criterion without code or description");
    }
    let tags = unique_ids("tag", backup.extraction_tags.iter().map(|t| t.id.as_str()))?;
    if backup.extraction_tags.iter().any(|t| blank(&t.label)) {
        return bad("a tag without a name");
    }

    let categories = unique_ids("category", backup.highlight_categories.iter().map(|c| c.id.as_str()))?;
    let mut colors = HashSet::new();
    let mut labels = HashSet::new();
    for category in &backup.highlight_categories {
        if blank(&category.label)
            || !colors.insert(category.color.as_str())
            || !labels.insert(category.label.trim().to_lowercase())
        {
            return bad("highlight categories repeat a color or a name");
        }
    }
    if !(1..=PaletteColor::ALL.len()).contains(&categories.len()) {
        return bad("a library has 1 to 8 highlight categories");
    }

    let references = unique_ids("reference", backup.references.iter().map(|r| r.id.as_str()))?;
    if backup.papers.len() > MAX_PAPERS as usize {
        return bad("more papers than a library can hold");
    }
    unique_ids("paper", backup.papers.iter().map(|p| p.id.as_str()))?;
    unique_ids("highlight", backup.papers.iter().flat_map(|p| &p.highlights).map(|h| h.id.as_str()))?;

    let mut files = HashSet::new();
    let mut linked = HashSet::new();
    for paper in &backup.papers {
        if let Some(name) = &paper.file_name {
            // Never a path out of the `pdfs` folder.
            let plain = pdf_location(Path::new(""), &format!("{PDF_FOLDER}/{name}")).is_some();
            if !plain || !files.insert(name.to_lowercase()) {
                return bad("a file name is not valid or used twice");
            }
        }
        if let Some(reference) = &paper.reference_id
            && (!references.contains(reference.as_str()) || !linked.insert(reference.as_str()))
        {
            return bad("a paper links to a missing or shared reference");
        }
        if paper.exclusion_reason_id.as_ref().is_some_and(|id| !reasons.contains(id.as_str())) {
            return bad("a paper has a missing exclusion reason");
        }
        if paper.last_page < 1 {
            return bad("a page number is below 1");
        }
        for highlight in &paper.highlights {
            if !categories.contains(highlight.category_id.as_str())
                || highlight.tag_id.as_ref().is_some_and(|id| !tags.contains(id.as_str()))
            {
                return bad("a highlight has a missing category or tag");
            }
            highlights::check(&NewHighlight {
                page: highlight.page,
                category_id: highlight.category_id.clone(),
                text: highlight.text.clone(),
                rects: highlight.rects.clone(),
            })
            .map_err(|error| not_a_backup(error.detail))?;
        }
    }
    Ok(())
}

// ---- Restoring ----

fn ids_in(conn: &Connection, table: &str) -> Result<HashSet<String>, LibraryError> {
    let mut stmt = conn.prepare(&format!("SELECT id FROM {table}"))?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// What restoring `backup` into the library would replace. Only reads.
pub fn preview(conn: &Connection, backup: &Backup) -> Result<RestorePreview, LibraryError> {
    let papers = ids_in(conn, "paper")?;
    let highlights = ids_in(conn, "highlight")?;
    let replaced_papers = backup.papers.iter().filter(|p| papers.contains(&p.id)).count();
    let replaced_highlights =
        backup.papers.iter().flat_map(|p| &p.highlights).filter(|h| highlights.contains(&h.id)).count();
    Ok(RestorePreview {
        replaced_papers,
        replaced_highlights,
        papers_after: papers.len() + backup.papers.len() - replaced_papers,
    })
}

/// Restores a backup (see the module comment). `fresh` is for a library that was just
/// created for it: it takes the backup's research questions and highlight categories
/// as they are, and nothing is reported as added.
pub fn restore(conn: &Connection, folder: &Path, backup: &Backup, fresh: bool) -> Result<RestoreReport, LibraryError> {
    let transaction = conn.unchecked_transaction()?;
    let conn: &Connection = &transaction;
    let papers_after = preview(conn, backup)?.papers_after;
    if papers_after > MAX_PAPERS as usize {
        return Err(LibraryError::new(ErrorCode::OverLimit, format!("{papers_after} papers")));
    }

    merge_list(conn, "inclusion_criterion", "code", &coded(&backup.inclusion_criteria))?;
    let (reasons, added_reasons) =
        merge_list(conn, "exclusion_reason", "code", &coded(&backup.exclusion_reasons))?;
    let tag_items: Vec<ListItem<'_>> = backup
        .extraction_tags
        .iter()
        .map(|t| ListItem { id: &t.id, key: &t.label, code: None, label: &t.label })
        .collect();
    let (tags, added_tags) = merge_list(conn, "extraction_tag", "label", &tag_items)?;
    let categories = if fresh {
        adopt_categories(conn, &backup.highlight_categories)?
    } else {
        merge_categories(conn, &backup.highlight_categories)?
    };

    for reference in &backup.references {
        conn.execute(
            "INSERT INTO reference (id, no, title, year, source_database, doi)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6 WHERE NOT EXISTS (SELECT 1 FROM reference WHERE id = ?1)",
            params![
                reference.id,
                reference.no,
                reference.title,
                reference.year,
                reference.source_database,
                reference.doi
            ],
        )?;
    }
    restore_questions(conn, &backup.library.research_questions, fresh)?;
    restore_papers(conn, folder, backup, &reasons)?;
    restore_highlights(conn, backup, &categories, &tags)?;

    transaction.commit()?;
    Ok(if fresh { RestoreReport::default() } else { RestoreReport { added_reasons, added_tags } })
}

fn coded(list: &[Criterion]) -> Vec<ListItem<'_>> {
    list.iter().map(|c| ListItem { id: &c.id, key: &c.code, code: Some(&c.code), label: &c.label }).collect()
}

struct ListItem<'a> {
    id: &'a str,
    /// What makes two items the same when their ids differ: the code, or a tag's name.
    key: &'a str,
    code: Option<&'a str>,
    label: &'a str,
}

/// Maps each item of the backup to the library's item: the same id, else the same
/// code or name, else a new item at the end of the list. Returns the map and how many
/// were added. Items the library has are never changed.
fn merge_list(
    conn: &Connection,
    table: &str,
    key_column: &str,
    items: &[ListItem],
) -> Result<(HashMap<String, String>, usize), LibraryError> {
    let mut known: Vec<(String, String)> = conn
        .prepare(&format!("SELECT id, {key_column} FROM {table} ORDER BY sort_order, rowid"))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let next = format!("(SELECT coalesce(max(sort_order), -1) + 1 FROM {table})");
    let mut map = HashMap::new();
    let mut added = 0;
    for item in items {
        let found = known
            .iter()
            .find(|(id, _)| id == item.id)
            .or_else(|| known.iter().find(|(_, key)| same_text(key, item.key)))
            .map(|(id, _)| id.clone());
        let target = match found {
            Some(id) => id,
            None => {
                match item.code {
                    Some(code) => conn.execute(
                        &format!("INSERT INTO {table} (id, code, label, sort_order) VALUES (?1, ?2, ?3, {next})"),
                        params![item.id, code.trim(), item.label.trim()],
                    )?,
                    None => conn.execute(
                        &format!("INSERT INTO {table} (id, label, sort_order) VALUES (?1, ?2, {next})"),
                        params![item.id, item.label.trim()],
                    )?,
                };
                known.push((item.id.to_string(), item.key.to_string()));
                added += 1;
                item.id.to_string()
            }
        };
        map.insert(item.id.to_string(), target);
    }
    Ok((map, added))
}

/// For the open library: the same id, else the same name, else the same color, else a
/// new category. A new one always fits: a library with 8 categories uses every color.
fn merge_categories(conn: &Connection, categories: &[Category]) -> Result<HashMap<String, String>, LibraryError> {
    let mut known = highlights::list_categories(conn)?;
    let mut map = HashMap::new();
    for category in categories {
        let found = known
            .iter()
            .find(|c| c.id == category.id)
            .or_else(|| known.iter().find(|c| same_text(&c.label, &category.label)))
            .or_else(|| known.iter().find(|c| c.color == category.color))
            .map(|c| c.id.clone());
        let target = match found {
            Some(id) => id,
            None => {
                conn.execute(
                    "INSERT INTO highlight_category (id, label, color, sort_order)
                     VALUES (?1, ?2, ?3, (SELECT coalesce(max(sort_order), -1) + 1 FROM highlight_category))",
                    params![category.id, category.label.trim(), category.color.as_str()],
                )?;
                known.push(highlights::HighlightCategory {
                    id: category.id.clone(),
                    label: category.label.clone(),
                    color: category.color,
                    sort_order: 0,
                });
                category.id.clone()
            }
        };
        map.insert(category.id.clone(), target);
    }
    Ok(map)
}

/// For a new library: its categories become exactly the backup's. The 4 defaults are
/// unused, so a default whose color the backup has takes the backup's name and place,
/// and the others are removed.
fn adopt_categories(conn: &Connection, categories: &[Category]) -> Result<HashMap<String, String>, LibraryError> {
    let defaults = highlights::list_categories(conn)?;
    // Names are unique: free them before giving them out again.
    conn.execute("UPDATE highlight_category SET label = id", [])?;
    let mut kept = HashSet::new();
    let mut map = HashMap::new();
    for (order, category) in categories.iter().enumerate() {
        let target = match defaults.iter().find(|d| d.color == category.color) {
            Some(default) => {
                conn.execute(
                    "UPDATE highlight_category SET label = ?1, sort_order = ?2 WHERE id = ?3",
                    params![category.label.trim(), order as i64, default.id],
                )?;
                kept.insert(default.id.clone());
                default.id.clone()
            }
            None => {
                conn.execute(
                    "INSERT INTO highlight_category (id, label, color, sort_order) VALUES (?1, ?2, ?3, ?4)",
                    params![category.id, category.label.trim(), category.color.as_str(), order as i64],
                )?;
                category.id.clone()
            }
        };
        map.insert(category.id.clone(), target);
    }
    for default in defaults.iter().filter(|d| !kept.contains(&d.id)) {
        conn.execute("DELETE FROM highlight_category WHERE id = ?1", [&default.id])?;
    }
    Ok(map)
}

/// A new library takes the backup's questions; the open one gets the ones it lacks,
/// at the end.
fn restore_questions(conn: &Connection, questions: &str, fresh: bool) -> Result<(), LibraryError> {
    if fresh {
        return criteria::set_research_questions(conn, questions);
    }
    let current = criteria::research_questions(conn)?;
    let mut have: Vec<&str> = current.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let mut text = current.trim_end().to_string();
    let mut changed = false;
    for line in questions.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if have.iter().any(|known| same_text(known, line)) {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(line);
        have.push(line);
        changed = true;
    }
    if changed {
        criteria::set_research_questions(conn, &text)?;
    }
    Ok(())
}

/// A file path for a paper added from the backup. Its own name when no paper uses it:
/// a PDF already in the folder under that name is then its file (FR-07.3). Otherwise a
/// numbered name that neither a paper nor a file has, so it never takes another
/// paper's PDF.
fn path_for_new_paper(folder: &Path, used: &HashMap<String, String>, wanted: &str) -> String {
    if !used.contains_key(&wanted.to_lowercase()) {
        return wanted.to_string();
    }
    let (stem, extension) = match wanted.rsplit_once('.') {
        Some((stem, extension)) if !stem.ends_with('/') => (stem, format!(".{extension}")),
        _ => (wanted, String::new()),
    };
    (2..)
        .map(|number| format!("{stem} ({number}){extension}"))
        .find(|candidate| !used.contains_key(&candidate.to_lowercase()) && !folder.join(candidate).exists())
        .expect("an endless range")
}

fn restore_papers(
    conn: &Connection,
    folder: &Path,
    backup: &Backup,
    reasons: &HashMap<String, String>,
) -> Result<(), LibraryError> {
    let existing = ids_in(conn, "paper")?;
    // Paths in use, without case like Windows, and the paper using each.
    let mut used: HashMap<String, String> = conn
        .prepare("SELECT file_path, id FROM paper WHERE file_path IS NOT NULL")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?.to_lowercase(), r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    // Their links are set again below; clearing them first lets papers swap references.
    for paper in backup.papers.iter().filter(|p| existing.contains(&p.id)) {
        conn.execute("UPDATE paper SET reference_id = NULL WHERE id = ?1", [&paper.id])?;
    }
    for paper in &backup.papers {
        let reason = paper.exclusion_reason_id.as_ref().map(|id| reasons[id].clone());
        // One reference links to one paper (FR-05.4): a paper outside the backup keeps its link.
        let reference = match &paper.reference_id {
            Some(reference) => conn
                .query_row(
                    "SELECT 1 FROM paper WHERE reference_id = ?1 AND id <> ?2",
                    params![reference, paper.id],
                    |_| Ok(()),
                )
                .optional()?
                .is_none()
                .then_some(reference),
            None => None,
        };
        let wanted = paper.file_name.as_ref().map(|name| format!("{PDF_FOLDER}/{name}"));
        let file_path = if existing.contains(&paper.id) {
            let current: Option<String> =
                conn.query_row("SELECT file_path FROM paper WHERE id = ?1", [&paper.id], |r| r.get(0))?;
            // The PDF the paper has stays; one without gets the backup's name if free.
            current.or_else(|| wanted.filter(|path| used.get(&path.to_lowercase()).is_none_or(|id| *id == paper.id)))
        } else {
            wanted.map(|path| path_for_new_paper(folder, &used, &path))
        };
        let decision = paper.decision.as_str();
        if existing.contains(&paper.id) {
            // Where the user stopped reading and when the paper was added stay.
            conn.execute(
                "UPDATE paper SET file_path = ?2, reference_id = ?3, ref_no = ?4, title = ?5, year = ?6,
                        source_database = ?7, doi = ?8, decision = ?9, exclusion_reason_id = ?10,
                        assessment_note = ?11
                 WHERE id = ?1",
                params![
                    paper.id,
                    file_path,
                    reference,
                    paper.ref_no,
                    paper.title,
                    paper.year,
                    paper.source_database,
                    paper.doi,
                    decision,
                    reason,
                    paper.assessment_note,
                ],
            )?;
        } else {
            conn.execute(
                "INSERT INTO paper (id, file_path, reference_id, ref_no, title, year, source_database, doi,
                                    decision, exclusion_reason_id, assessment_note, last_page, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    paper.id,
                    file_path,
                    reference,
                    paper.ref_no,
                    paper.title,
                    paper.year,
                    paper.source_database,
                    paper.doi,
                    decision,
                    reason,
                    paper.assessment_note,
                    paper.last_page,
                    paper.created_at,
                ],
            )?;
        }
        if let Some(path) = file_path {
            used.insert(path.to_lowercase(), paper.id.clone());
        }
    }
    Ok(())
}

fn restore_highlights(
    conn: &Connection,
    backup: &Backup,
    categories: &HashMap<String, String>,
    tags: &HashMap<String, String>,
) -> Result<(), LibraryError> {
    let existing = ids_in(conn, "highlight")?;
    for paper in &backup.papers {
        for highlight in &paper.highlights {
            let rects = serde_json::to_string(&highlight.rects)
                .map_err(|error| LibraryError::new(ErrorCode::Other, error.to_string()))?;
            let values = params![
                highlight.id,
                paper.id,
                categories[&highlight.category_id],
                highlight.tag_id.as_ref().map(|id| &tags[id]),
                highlight.page,
                highlight.text,
                rects,
                highlight.note,
                highlight.created_at,
            ];
            let sql = if existing.contains(&highlight.id) {
                "UPDATE highlight SET paper_id = ?2, highlight_category_id = ?3, extraction_tag_id = ?4,
                        page = ?5, text = ?6, rects = ?7, note = ?8, created_at = ?9
                 WHERE id = ?1"
            } else {
                "INSERT INTO highlight (id, paper_id, highlight_category_id, extraction_tag_id, page, text,
                                        rects, note, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
            };
            conn.execute(sql, values)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;
    use crate::export::{self, markdown};
    use crate::settings::CsvDelimiter;

    const R: Rect = Rect { x: 0.1, y: 0.2, w: 0.5, h: 0.02 };

    fn library(name: &str) -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(PDF_FOLDER)).unwrap();
        let conn = create_library(dir.path(), name).unwrap();
        (dir, conn)
    }

    fn one(conn: &Connection, sql: &str) -> String {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
    }

    fn category(conn: &Connection, color: &str) -> String {
        one(conn, &format!("SELECT id FROM highlight_category WHERE color = '{color}'"))
    }

    fn add_highlight(conn: &Connection, paper: &str, page: u32, color: &str, tag: Option<&str>, text: &str) -> String {
        let new = NewHighlight { page, category_id: category(conn, color), text: text.into(), rects: vec![R] };
        let id = highlights::add_highlight(conn, paper, &new).unwrap().id;
        conn.execute("UPDATE highlight SET extraction_tag_id = ?1 WHERE id = ?2", params![tag, id]).unwrap();
        id
    }

    /// A library with a bit of everything; returns the two paper ids.
    fn filled(conn: &Connection) -> (String, String) {
        conn.execute_batch(
            "UPDATE library SET research_questions = 'What feed works best?
How is FCR measured?';
             INSERT INTO inclusion_criterion (code, label, sort_order) VALUES ('I1', 'Fish farms', 0);
             INSERT INTO exclusion_reason (code, label, sort_order) VALUES ('E1', 'Not fish', 0), ('E3', 'Wrong population', 1);
             INSERT INTO extraction_tag (label, sort_order) VALUES ('FCR', 0), ('Species', 1);
             INSERT INTO reference (no, title, year, source_database, doi) VALUES (5, 'Feed trial A', 2021, 'Scopus', '10.1/a');
             UPDATE highlight_category SET label = 'Supports inclusion' WHERE color = 'green';",
        )
        .unwrap();
        let a: String = conn
            .query_row(
                "INSERT INTO paper (file_path, reference_id, ref_no, title, year, source_database, doi, decision,
                                    exclusion_reason_id, assessment_note, last_page)
                 VALUES ('pdfs/kim-2021.pdf', (SELECT id FROM reference), 5, 'Feed trial A', 2021, 'Scopus',
                         '10.1/a', 'include', (SELECT id FROM exclusion_reason WHERE code = 'E3'), 'Good; see [#7]', 4)
                 RETURNING id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let b: String = conn
            .query_row(
                "INSERT INTO paper (file_path, title, decision, exclusion_reason_id)
                 VALUES ('pdfs/lee.pdf', 'Pond study', 'exclude', (SELECT id FROM exclusion_reason WHERE code = 'E3'))
                 RETURNING id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let fcr = one(conn, "SELECT id FROM extraction_tag WHERE label = 'FCR'");
        let h = add_highlight(conn, &a, 3, "blue", Some(&fcr), "The FCR was 1.4");
        conn.execute("UPDATE highlight SET note = 'compare [#7]' WHERE id = ?1", [h]).unwrap();
        add_highlight(conn, &b, 1, "green", None, "Tilapia only");
        (a, b)
    }

    /// What the user sees of a library, independent of ids.
    fn view(conn: &Connection) -> Vec<String> {
        let mut out = vec![
            export::decisions_csv(conn, CsvDelimiter::Semicolon).unwrap(),
            export::extraction_csv(conn, CsvDelimiter::Semicolon).unwrap(),
            criteria::research_questions(conn).unwrap(),
        ];
        out.extend(markdown::markdown_files(conn).unwrap().into_iter().map(|f| format!("{}\n{}", f.name, f.contents)));
        for kind in [criteria::ListKind::Inclusion, criteria::ListKind::Exclusion, criteria::ListKind::Tag] {
            out.extend(criteria::list(conn, kind).unwrap().into_iter().map(|i| format!("{} {}", i.code, i.label)));
        }
        out.extend(highlights::list_categories(conn).unwrap().into_iter().map(|c| format!("{} {:?}", c.label, c.color)));
        out.extend(
            conn.prepare("SELECT no || ' ' || title FROM reference ORDER BY no")
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap),
        );
        out
    }

    fn backup_of(conn: &Connection) -> Backup {
        parse(export(conn).unwrap().as_bytes()).unwrap()
    }

    #[test]
    fn a_backup_restores_into_a_new_library_exactly() {
        let (_a, source) = library("Aquaculture");
        filled(&source);
        let backup = backup_of(&source);
        let (dir, target) = library("Copy");
        assert_eq!(restore(&target, dir.path(), &backup, true).unwrap(), RestoreReport::default());
        assert_eq!(view(&target), view(&source));
        // The paper still knows its PDF's name, to attach it again (FR-07.3).
        assert_eq!(one(&target, "SELECT file_path FROM paper WHERE title = 'Pond study'"), "pdfs/lee.pdf");
        assert_eq!(one(&target, "SELECT text_status FROM paper WHERE title = 'Pond study'"), "pending");
    }

    #[test]
    fn a_backup_holds_no_pdf_text_chats_or_app_settings() {
        let (_dir, conn) = library("R");
        let (a, _) = filled(&conn);
        conn.execute("INSERT INTO text_chunk (paper_id, page_start, page_end, text) VALUES (?1, 1, 1, 'CHUNK TEXT')", [&a])
            .unwrap();
        conn.execute("INSERT INTO chat_thread (id, paper_id, scope) VALUES ('t', ?1, 'paper')", [&a]).unwrap();
        conn.execute(
            "INSERT INTO chat_message (thread_id, role, content) VALUES ('t', 'user', 'CHAT TEXT')",
            [],
        )
        .unwrap();
        let json = export(&conn).unwrap();
        assert!(!json.contains("CHUNK TEXT") && !json.contains("CHAT TEXT"));
        assert!(!json.to_lowercase().contains("key"));
        assert!(json.contains("\"format\": \"slreader-backup\""));
    }

    #[test]
    fn exporting_changes_nothing() {
        let (_dir, conn) = library("R");
        filled(&conn);
        let before = conn.total_changes();
        export(&conn).unwrap();
        assert_eq!(conn.total_changes(), before);
    }

    #[test]
    fn restoring_into_the_same_library_replaces_only_what_the_backup_has() {
        let (dir, conn) = library("R");
        let (a, b) = filled(&conn);
        let backup = backup_of(&conn);
        let kept_highlight: String = one(&conn, "SELECT id FROM highlight WHERE text = 'Tilapia only'");
        // Changes after the backup.
        conn.execute("UPDATE paper SET decision = 'exclude', assessment_note = 'changed' WHERE id = ?1", [&a]).unwrap();
        conn.execute("UPDATE paper SET last_page = 9 WHERE id = ?1", [&a]).unwrap();
        conn.execute("DELETE FROM highlight WHERE text = 'The FCR was 1.4'", []).unwrap();
        let new_highlight = add_highlight(&conn, &b, 2, "red", None, "Added later");
        let later: String =
            conn.query_row("INSERT INTO paper (file_path, decision) VALUES ('pdfs/new.pdf', 'include') RETURNING id", [], |r| r.get(0)).unwrap();

        assert_eq!(
            preview(&conn, &backup).unwrap(),
            RestorePreview { replaced_papers: 2, replaced_highlights: 1, papers_after: 3 }
        );
        assert_eq!(restore(&conn, dir.path(), &backup, false).unwrap(), RestoreReport::default());

        let paper = |id: &str| -> (String, String, i64) {
            conn.query_row("SELECT decision, assessment_note, last_page FROM paper WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap()
        };
        assert_eq!(paper(&a), ("include".into(), "Good; see [#7]".into(), 9));
        // Nothing the backup lacks is lost.
        assert_eq!(paper(&later), ("include".into(), String::new(), 1));
        let texts: Vec<String> = conn
            .prepare("SELECT text FROM highlight ORDER BY text")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(texts, ["Added later", "The FCR was 1.4", "Tilapia only"]);
        assert!(ids_in(&conn, "highlight").unwrap().contains(&new_highlight));
        assert!(ids_in(&conn, "highlight").unwrap().contains(&kept_highlight));
        // The lists were already there: nothing doubled.
        assert_eq!(count(&conn, "exclusion_reason"), 2);
        assert_eq!(count(&conn, "highlight_category"), 4);
        assert_eq!(count(&conn, "reference"), 1);
        assert_eq!(criteria::research_questions(&conn).unwrap(), "What feed works best?\nHow is FCR measured?");
    }

    #[test]
    fn decisions_change_only_for_papers_in_the_backup() {
        // Rule 1: a restore only writes decisions the user saved in the backup.
        let (dir, conn) = library("R");
        let (a, _) = filled(&conn);
        let mut backup = backup_of(&conn);
        backup.papers.retain(|p| p.id == a);
        conn.execute("UPDATE paper SET decision = 'undecided'", []).unwrap();
        restore(&conn, dir.path(), &backup, false).unwrap();
        let decisions: Vec<(String, String)> = conn
            .prepare("SELECT title, decision FROM paper ORDER BY title")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(decisions, [("Feed trial A".into(), "include".into()), ("Pond study".into(), "undecided".into())]);
    }

    #[test]
    fn reasons_and_tags_match_by_code_and_name_and_missing_ones_are_added() {
        let (_s, source) = library("A");
        filled(&source);
        source.execute("INSERT INTO extraction_tag (label, sort_order) VALUES ('Feed', 2)", []).unwrap();
        source.execute("INSERT INTO exclusion_reason (code, label, sort_order) VALUES ('E9', 'Review', 2)", []).unwrap();
        let backup = backup_of(&source);

        let (dir, target) = library("B");
        target
            .execute_batch(
                "INSERT INTO exclusion_reason (code, label, sort_order) VALUES ('e3 ', 'Other words', 0);
                 INSERT INTO extraction_tag (label, sort_order) VALUES ('fcr', 0);",
            )
            .unwrap();
        let report = restore(&target, dir.path(), &backup, false).unwrap();
        // E1 and E9 are new; E3 is the library's own. Species and Feed are new.
        assert_eq!(report, RestoreReport { added_reasons: 2, added_tags: 2 });
        let listed = |kind| -> Vec<String> {
            criteria::list(&target, kind).unwrap().into_iter().map(|i| format!("{}:{}", i.code, i.label)).collect()
        };
        assert_eq!(listed(criteria::ListKind::Exclusion), ["e3 :Other words", "E1:Not fish", "E9:Review"]);
        assert_eq!(listed(criteria::ListKind::Tag), [":fcr", ":Species", ":Feed"]);
        // The excluded paper uses the library's own E3, the FCR highlight its own tag.
        assert_eq!(
            one(&target, "SELECT r.label FROM paper p JOIN exclusion_reason r ON r.id = p.exclusion_reason_id WHERE p.title = 'Pond study'"),
            "Other words"
        );
        assert_eq!(
            one(&target, "SELECT t.label FROM highlight h JOIN extraction_tag t ON t.id = h.extraction_tag_id"),
            "fcr"
        );
        // Missing research questions are added after the library's own.
        assert_eq!(criteria::research_questions(&target).unwrap(), "What feed works best?\nHow is FCR measured?");
    }

    #[test]
    fn categories_match_by_name_then_color_in_the_open_library() {
        let (_s, source) = library("A");
        let (a, _) = filled(&source);
        source
            .execute_batch(
                "UPDATE highlight_category SET label = 'Method', color = 'purple' WHERE color = 'blue';
                 INSERT INTO highlight_category (label, color, sort_order) VALUES ('Measurements', 'orange', 9);",
            )
            .unwrap();
        add_highlight(&source, &a, 5, "orange", None, "Measured weekly");
        let backup = backup_of(&source);

        let (dir, target) = library("B");
        target.execute("UPDATE highlight_category SET label = 'Proof' WHERE color = 'green'", []).unwrap();
        restore(&target, dir.path(), &backup, false).unwrap();
        let category_of = |text: &str| {
            target
                .query_row(
                    "SELECT c.label || ' ' || c.color FROM highlight h JOIN highlight_category c ON c.id = h.highlight_category_id WHERE h.text = ?1",
                    [text],
                    |r| r.get::<_, String>(0),
                )
                .unwrap()
        };
        assert_eq!(category_of("The FCR was 1.4"), "Method blue"); // same name
        assert_eq!(category_of("Tilapia only"), "Proof green"); // same color
        assert_eq!(category_of("Measured weekly"), "Measurements orange"); // added
        assert_eq!(count(&target, "highlight_category"), 5);
    }

    #[test]
    fn a_new_library_takes_the_backups_categories_as_they_are() {
        let (_s, source) = library("A");
        source
            .execute_batch(
                "DELETE FROM highlight_category WHERE color IN ('red', 'yellow');
                 -- Green and blue swap names.
                 UPDATE highlight_category SET label = 'tmp' WHERE color = 'blue';
                 UPDATE highlight_category SET label = 'Method' WHERE color = 'green';
                 UPDATE highlight_category SET label = 'Inclusion', sort_order = -1 WHERE color = 'blue';
                 INSERT INTO highlight_category (label, color, sort_order) VALUES ('Gray zone', 'gray', 5);",
            )
            .unwrap();
        let backup = backup_of(&source);
        let (dir, target) = library("B");
        restore(&target, dir.path(), &backup, true).unwrap();
        let listed: Vec<(String, PaletteColor)> =
            highlights::list_categories(&target).unwrap().into_iter().map(|c| (c.label, c.color)).collect();
        assert_eq!(
            listed,
            [
                ("Inclusion".into(), PaletteColor::Blue),
                ("Method".into(), PaletteColor::Green),
                ("Gray zone".into(), PaletteColor::Gray)
            ]
        );
    }

    #[test]
    fn more_than_250_papers_is_refused_and_changes_nothing() {
        let (_s, source) = library("A");
        filled(&source);
        let backup = backup_of(&source);
        let (dir, target) = library("B");
        for n in 0..249 {
            target.execute("INSERT INTO paper (title) VALUES (?1)", [n.to_string()]).unwrap();
        }
        assert_eq!(preview(&target, &backup).unwrap().papers_after, 251);
        let error = restore(&target, dir.path(), &backup, false).unwrap_err();
        assert_eq!(error.code, ErrorCode::OverLimit);
        assert_eq!(count(&target, "paper"), 249);
        assert_eq!(count(&target, "exclusion_reason"), 0);
    }

    #[test]
    fn a_failure_halfway_changes_nothing() {
        let (_s, source) = library("A");
        filled(&source);
        let backup = backup_of(&source);
        let (dir, target) = library("B");
        // Saving the highlights fails, after the lists and papers were written.
        target
            .execute_batch(
                "CREATE TRIGGER fail BEFORE INSERT ON highlight BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
            )
            .unwrap();
        let before = view(&target);
        assert!(restore(&target, dir.path(), &backup, false).is_err());
        assert_eq!(view(&target), before);
        assert_eq!(count(&target, "paper"), 0);
    }

    #[test]
    fn a_new_paper_never_takes_another_papers_pdf() {
        let (_s, source) = library("A");
        filled(&source);
        let backup = backup_of(&source);
        let (dir, target) = library("B");
        // Another paper already uses kim-2021.pdf; lee.pdf lies in the folder unused.
        target.execute("INSERT INTO paper (file_path, title) VALUES ('pdfs/KIM-2021.pdf', 'Other')", []).unwrap();
        std::fs::write(dir.path().join("pdfs/kim-2021 (2).pdf"), "x").unwrap();
        std::fs::write(dir.path().join("pdfs/lee.pdf"), "x").unwrap();
        restore(&target, dir.path(), &backup, false).unwrap();
        assert_eq!(one(&target, "SELECT file_path FROM paper WHERE title = 'Feed trial A'"), "pdfs/kim-2021 (3).pdf");
        // FR-07.3: the PDF with the paper's name is its file.
        assert_eq!(one(&target, "SELECT file_path FROM paper WHERE title = 'Pond study'"), "pdfs/lee.pdf");
        assert_eq!(one(&target, "SELECT file_path FROM paper WHERE title = 'Other'"), "pdfs/KIM-2021.pdf");
    }

    #[test]
    fn references_follow_the_backup_but_never_leave_another_paper() {
        let (dir, conn) = library("R");
        let (a, b) = filled(&conn);
        let first = one(&conn, "SELECT id FROM reference");
        let second: String = conn
            .query_row("INSERT INTO reference (no, title) VALUES (6, 'Pond study') RETURNING id", [], |r| r.get(0))
            .unwrap();
        conn.execute("UPDATE paper SET reference_id = ?1 WHERE id = ?2", params![second, b]).unwrap();
        let mut backup = backup_of(&conn);
        // In the backup the two papers have swapped references.
        backup.papers.iter_mut().for_each(|p| {
            p.reference_id = Some(if p.id == a { second.clone() } else { first.clone() });
        });
        restore(&conn, dir.path(), &backup, false).unwrap();
        let link = |id: &str| -> Option<String> {
            conn.query_row("SELECT reference_id FROM paper WHERE id = ?1", [id], |r| r.get(0)).unwrap()
        };
        assert_eq!((link(&a), link(&b)), (Some(second.clone()), Some(first.clone())));

        // A paper outside the backup keeps its reference.
        let other: String =
            conn.query_row("INSERT INTO paper (title) VALUES ('Other') RETURNING id", [], |r| r.get(0)).unwrap();
        conn.execute("UPDATE paper SET reference_id = NULL WHERE id = ?1", [&a]).unwrap();
        conn.execute("UPDATE paper SET reference_id = ?1 WHERE id = ?2", params![second, other]).unwrap();
        restore(&conn, dir.path(), &backup, false).unwrap();
        assert_eq!((link(&a), link(&other)), (None, Some(second)));
    }

    fn json(conn: &Connection) -> serde_json::Value {
        serde_json::from_str(&export(conn).unwrap()).unwrap()
    }

    #[test]
    fn anything_but_a_valid_backup_is_refused() {
        let (_dir, conn) = library("R");
        filled(&conn);
        let good = json(&conn);
        let refused = |value: &serde_json::Value| parse(value.to_string().as_bytes()).unwrap_err().code;
        let changed = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut value = good.clone();
            change(&mut value);
            refused(&value)
        };
        assert!(parse(good.to_string().as_bytes()).is_ok());
        // With a byte order mark too.
        assert!(parse(format!("\u{feff}{good}").as_bytes()).is_ok());
        assert_eq!(parse(b"Reference No;Title").unwrap_err().code, ErrorCode::NotABackup);
        assert_eq!(parse(b"{\"papers\": []}").unwrap_err().code, ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["format"] = "other-app".into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["version"] = 2.into()), ErrorCode::NewerBackup);
        assert_eq!(changed(&|v| v["version"] = 0.into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][0]["decision"] = "maybe".into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][0]["fileName"] = "../x.pdf".into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][0]["fileName"] = "LEE.pdf".into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][0]["exclusionReasonId"] = "0".repeat(32).into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][1]["id"] = v["papers"][0]["id"].clone()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][0]["id"] = "x'; DROP TABLE paper; --".into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][0]["highlights"][0]["rects"][0]["x"] = 0.9.into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["papers"][0]["highlights"][0]["page"] = 0.into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["highlightCategories"] = serde_json::json!([])), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["highlightCategories"][1]["color"] = "green".into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["highlightCategories"][1]["color"] = "teal".into()), ErrorCode::NotABackup);
        assert_eq!(changed(&|v| v["extractionTags"][0]["label"] = " ".into()), ErrorCode::NotABackup);
    }

    #[test]
    fn suggests_a_dated_file_name() {
        let (_dir, conn) = library("Fish: review?");
        let name = file_name(&conn).unwrap();
        assert!(name.starts_with("Fish_ review_ backup 20"), "{name}");
        assert!(name.ends_with(".json"));
    }
}
