//! Backups of the prototype web app, `slr-prisma-reader.html` (FR-07.3).
//!
//! `parse` turns one into a `Backup`, and `fit` adjusts it to the library it is
//! restored into. `backup.rs` then restores it like its own backups: merged by id in
//! one transaction, asking first before anything is replaced. This module only reads.
//!
//! - Ids are derived from the old ones, so restoring a file twice adds nothing twice.
//! - A paper with the same PDF name as one in the library is that paper.
//! - A highlight goes into the library's category of the same color; one is added
//!   when the library has none.
//! - An exclusion reason is "E1 Description"; a line without such a code gets the next
//!   free `E` code.
//! - A reference number or year that is not a number is kept in the assessment note.
//! - The app's settings (CSV separator, zoom, view mode) are not read.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::backup::{Backup, BackupHighlight, Category, Criterion, LibraryPart, Paper, Reference, Tag};
use crate::decisions::Decision;
use crate::export::markdown::safe_stem;
use crate::highlights::{self, PaletteColor, Rect};
use crate::libraries::{ErrorCode, LibraryError, PDF_FOLDER};
use crate::metadata::{MAX_REF_NO, MAX_YEAR};
use crate::papers::pdf_location;

/// The `app` field of the prototype's backups.
pub const APP: &str = "slr-prisma-reader";

/// The prototype's 4 colors, with the names new libraries give them.
const COLORS: [(&str, PaletteColor, &str); 4] = [
    ("green", PaletteColor::Green, "Inclusion"),
    ("red", PaletteColor::Red, "Exclusion"),
    ("blue", PaletteColor::Blue, "Method"),
    ("yellow", PaletteColor::Yellow, "Result"),
];

/// What `fit` needs to know about a backup that came from the prototype.
#[derive(Debug, Clone, PartialEq)]
pub struct OldApp {
    /// Ids of the reasons whose line had no code of its own.
    uncoded_reasons: HashSet<String>,
}

// ---- The prototype's format ----

#[derive(Deserialize)]
struct OldBackup {
    version: u64,
    #[serde(default)]
    settings: OldSettings,
    #[serde(default)]
    refs: Vec<OldReference>,
    papers: Vec<OldPaper>,
    highlights: Vec<OldHighlight>,
}

#[derive(Deserialize, Default)]
struct OldSettings {
    #[serde(default)]
    reasons: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
struct OldReference {
    #[serde(default, deserialize_with = "text")]
    no: String,
    #[serde(default, deserialize_with = "text")]
    title: String,
    #[serde(default, deserialize_with = "text")]
    year: String,
    #[serde(default, deserialize_with = "text")]
    source: String,
    #[serde(default, deserialize_with = "text")]
    doi: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldPaper {
    id: String,
    #[serde(default, deserialize_with = "text")]
    file_name: String,
    #[serde(default, deserialize_with = "text")]
    ref_no: String,
    #[serde(default, deserialize_with = "text")]
    title: String,
    #[serde(default, deserialize_with = "text")]
    year: String,
    #[serde(default, deserialize_with = "text")]
    source: String,
    #[serde(default, deserialize_with = "text")]
    doi: String,
    decision: Option<Decision>,
    #[serde(default, deserialize_with = "text")]
    reason: String,
    #[serde(default, deserialize_with = "text")]
    note: String,
    last_page: Option<f64>,
    /// Milliseconds since 1970.
    added: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldHighlight {
    id: String,
    paper_id: String,
    page: u32,
    rects: Vec<Rect>,
    text: String,
    color: String,
    #[serde(default, deserialize_with = "text")]
    note: String,
    #[serde(default, deserialize_with = "text")]
    tag: String,
    created: Option<f64>,
}

/// The prototype keeps what was typed, or a number, or nothing.
fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::String(text) => Ok(text),
        Value::Number(number) => Ok(number.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err(serde::de::Error::custom("expected text")),
    }
}

// ---- Reading ----

fn not_a_backup(detail: impl Into<String>) -> LibraryError {
    LibraryError::new(ErrorCode::NotABackup, detail)
}

/// An id like the app's own, always the same for the same old item (128-bit FNV-1a).
fn id_for(kind: &str, key: &str) -> String {
    const PRIME: u128 = 0x0000000001000000000000000000013B;
    let mut hash: u128 = 0x6c62272e07bb014262b821756295c58d;
    for byte in kind.bytes().chain([0]).chain(key.bytes()) {
        hash = (hash ^ u128::from(byte)).wrapping_mul(PRIME);
    }
    format!("{hash:032x}")
}

fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// `Ok(None)` when empty, `Err` with the text when it is not a number the app accepts.
fn number(text: &str, max: i64) -> Result<Option<i64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    // Excel writes whole numbers as "46.0".
    let digits = text.strip_suffix(".0").unwrap_or(text);
    digits
        .parse::<i64>()
        .ok()
        .filter(|n| digits.bytes().all(|b| b.is_ascii_digit()) && *n <= max)
        .map(Some)
        .ok_or_else(|| text.to_string())
}

/// "E1", "EX-2", "E3." and the like.
fn code_of(word: &str) -> Option<&str> {
    let code = word.trim_end_matches(['.', ':', ')', '-']);
    let letters = code.chars().take_while(char::is_ascii_alphabetic).count();
    let rest = code.get(letters..)?;
    let digits = rest.strip_prefix('-').unwrap_or(rest);
    let fits = (1..=3).contains(&letters)
        && (1..=3).contains(&digits.len())
        && digits.bytes().all(|b| b.is_ascii_digit());
    fits.then_some(code)
}

/// A name for the `pdfs` folder that no other paper of the backup has. The old name
/// itself whenever possible, so that adding that PDF attaches it (FR-01.5).
fn file_name(old: &str, taken: &mut HashSet<String>) -> Option<String> {
    if old.is_empty() {
        return None;
    }
    let plain = pdf_location(Path::new(""), &format!("{PDF_FOLDER}/{old}")).is_some();
    let name = if plain { old.to_string() } else { format!("{}.pdf", safe_stem(old)) };
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, extension)) => (stem.to_string(), format!(".{extension}")),
        None => (name.clone(), String::new()),
    };
    // Windows ignores the case of letters.
    std::iter::once(name)
        .chain((2..).map(|number| format!("{stem} ({number}){extension}")))
        .find(|candidate| taken.insert(candidate.to_lowercase()))
}

/// The time of a JavaScript timestamp as the library writes times; now without one.
fn time(clock: &Connection, milliseconds: Option<f64>) -> Result<String, LibraryError> {
    Ok(clock.query_row(
        "SELECT coalesce(strftime('%Y-%m-%dT%H:%M:%fZ', ?1 / 1000.0, 'unixepoch'),
                         strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        [milliseconds],
        |r| r.get(0),
    )?)
}

/// A prototype backup as an SLReader one. `backup::parse` checks the result.
pub fn parse(value: Value) -> Result<Backup, LibraryError> {
    let old: OldBackup = serde_json::from_value(value).map_err(|e| not_a_backup(e.to_string()))?;
    if old.version != 1 {
        return Err(not_a_backup(format!("old app backup version {}", old.version)));
    }
    let clock = Connection::open_in_memory()?;

    // The reasons of the settings, then any a paper still has that was removed there.
    let mut reasons: Vec<Criterion> = Vec::new();
    let mut reason_ids = HashSet::new();
    let mut codes = HashSet::new();
    let mut uncoded_reasons = HashSet::new();
    for line in old.settings.reasons.iter().chain(old.papers.iter().map(|p| &p.reason)) {
        let line = line.trim();
        let id = id_for("reason", line);
        if line.is_empty() || !reason_ids.insert(id.clone()) {
            continue;
        }
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let coded = code_of(word).filter(|code| codes.insert(code.to_lowercase()));
        let (code, label) = match coded {
            Some(code) if rest.trim().is_empty() => (code.to_string(), code.to_string()),
            Some(code) => (code.to_string(), rest.trim().to_string()),
            None => {
                uncoded_reasons.insert(id.clone());
                // `fit` gives the code, knowing the library's.
                ("E".to_string(), line.to_string())
            }
        };
        reasons.push(Criterion { id, code, label });
    }

    let mut tags: Vec<Tag> = Vec::new();
    let mut tag_ids = HashSet::new();
    for label in old.settings.tags.iter().chain(old.highlights.iter().map(|h| &h.tag)) {
        let label = label.trim();
        let id = id_for("tag", &label.to_lowercase());
        if !label.is_empty() && tag_ids.insert(id.clone()) {
            tags.push(Tag { id, label: label.to_string() });
        }
    }

    // A row the app cannot hold (no number or no title) is left out of the list; the
    // papers keep their own copy of the metadata.
    let mut references: Vec<Reference> = Vec::new();
    let mut reference_ids = HashSet::new();
    for row in &old.refs {
        let (Ok(Some(no)), Some(title)) = (number(&row.no, MAX_REF_NO), optional(&row.title)) else { continue };
        let id = id_for("reference", &format!("{no}\0{title}"));
        if reference_ids.insert(id.clone()) {
            references.push(Reference {
                id,
                no,
                title,
                year: number(&row.year, MAX_YEAR).unwrap_or(None),
                source_database: optional(&row.source),
                doi: optional(&row.doi),
            });
        }
    }

    let mut highlights_of: HashMap<&str, Vec<BackupHighlight>> = HashMap::new();
    for h in &old.highlights {
        let (_, _, label) = COLORS
            .iter()
            .find(|(name, ..)| *name == h.color)
            .ok_or_else(|| not_a_backup("a highlight has an unknown color"))?;
        highlights_of.entry(&h.paper_id).or_default().push(BackupHighlight {
            id: id_for("highlight", &h.id),
            category_id: id_for("category", label),
            tag_id: optional(&h.tag).map(|tag| id_for("tag", &tag.to_lowercase())),
            page: h.page,
            text: h.text.clone(),
            rects: h.rects.clone(),
            note: h.note.clone(),
            created_at: time(&clock, h.created)?,
        });
    }

    let mut order: Vec<&OldPaper> = old.papers.iter().collect();
    order.sort_by(|a, b| a.added.unwrap_or(0.0).total_cmp(&b.added.unwrap_or(0.0)));
    let mut files = HashSet::new();
    let mut linked = HashSet::new();
    let mut papers = Vec::with_capacity(order.len());
    for paper in order {
        let mut kept = Vec::new();
        let ref_no = number(&paper.ref_no, MAX_REF_NO).unwrap_or_else(|text| {
            kept.push(format!("Reference No: {text}"));
            None
        });
        let year = number(&paper.year, MAX_YEAR).unwrap_or_else(|text| {
            kept.push(format!("Year: {text}"));
            None
        });
        let mut assessment_note = paper.note.clone();
        if !kept.is_empty() {
            if !assessment_note.is_empty() {
                assessment_note.push_str("\n\n");
            }
            assessment_note.push_str(&kept.join("\n"));
        }
        // The prototype shows a paper as linked to the reference with its number.
        let title = paper.title.trim().to_lowercase();
        let free = |r: &&Reference| Some(r.no) == ref_no && !linked.contains(&r.id);
        let reference_id = references
            .iter()
            .filter(free)
            .find(|r| r.title.to_lowercase() == title)
            .or_else(|| references.iter().find(free))
            .map(|r| r.id.clone());
        if let Some(id) = &reference_id {
            linked.insert(id.clone());
        }
        papers.push(Paper {
            id: id_for("paper", &paper.id),
            file_name: file_name(&paper.file_name, &mut files),
            reference_id,
            ref_no,
            title: paper.title.clone(),
            year,
            source_database: optional(&paper.source),
            doi: optional(&paper.doi),
            decision: paper.decision.unwrap_or(Decision::Undecided),
            exclusion_reason_id: optional(&paper.reason).map(|line| id_for("reason", &line)),
            assessment_note,
            last_page: paper.last_page.map_or(1, |page| page.max(1.0) as u32),
            created_at: time(&clock, paper.added)?,
            // Highlights of a paper the file does not have are left out, like the
            // prototype never shows them.
            highlights: highlights_of.remove(paper.id.as_str()).unwrap_or_default(),
        });
    }

    Ok(Backup {
        format: APP.to_string(),
        version: old.version,
        exported_at: String::new(),
        // The prototype has one unnamed library without research questions.
        library: LibraryPart { name: String::new(), research_questions: String::new() },
        inclusion_criteria: Vec::new(),
        exclusion_reasons: reasons,
        extraction_tags: tags,
        highlight_categories: COLORS
            .iter()
            .map(|(_, color, label)| Category {
                id: id_for("category", label),
                label: label.to_string(),
                color: *color,
            })
            .collect(),
        references,
        papers,
        old_app: Some(OldApp { uncoded_reasons }),
    })
}

// ---- Fitting to a library ----

/// The backup as it merges into this library. Only reads.
pub fn fit(conn: &Connection, backup: &Backup, old: &OldApp) -> Result<Backup, LibraryError> {
    let mut fitted = backup.clone();
    fit_categories(conn, &mut fitted)?;
    fit_references(conn, &mut fitted)?;
    fit_papers(conn, &mut fitted)?;
    fit_reasons(conn, &mut fitted, old)?;
    Ok(fitted)
}

/// By color only: the library's categories stay as they are, and a color the
/// highlights use that the library lacks becomes a new one. There is always room, as
/// a library with 8 categories has every color.
fn fit_categories(conn: &Connection, backup: &mut Backup) -> Result<(), LibraryError> {
    let mut fitted: Vec<Category> = highlights::list_categories(conn)?
        .into_iter()
        .map(|c| Category { id: c.id, label: c.label, color: c.color })
        .collect();
    let used: HashSet<String> =
        backup.papers.iter().flat_map(|p| &p.highlights).map(|h| h.category_id.clone()).collect();
    let mut map = HashMap::new();
    for category in backup.highlight_categories.iter().filter(|c| used.contains(&c.id)) {
        let target = match fitted.iter().find(|c| c.color == category.color) {
            Some(found) => found.id.clone(),
            None => {
                let mut id = category.id.clone();
                while fitted.iter().any(|c| c.id == id) {
                    id = id_for("category", &id);
                }
                let label = std::iter::once(category.label.clone())
                    .chain((2..).map(|number| format!("{} ({number})", category.label)))
                    .find(|label| !fitted.iter().any(|c| c.label.trim().to_lowercase() == label.to_lowercase()))
                    .expect("an endless range");
                fitted.push(Category { id: id.clone(), label, color: category.color });
                id
            }
        };
        map.insert(category.id.clone(), target);
    }
    for highlight in backup.papers.iter_mut().flat_map(|p| &mut p.highlights) {
        highlight.category_id = map[&highlight.category_id].clone();
    }
    backup.highlight_categories = fitted;
    Ok(())
}

/// A reference the library has with the same number and title is that reference.
fn fit_references(conn: &Connection, backup: &mut Backup) -> Result<(), LibraryError> {
    let known: Vec<(String, i64, String)> = conn
        .prepare("SELECT id, no, title FROM reference ORDER BY rowid")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    for reference in &mut backup.references {
        let same = known
            .iter()
            .find(|(_, no, title)| *no == reference.no && title.trim().to_lowercase() == reference.title.to_lowercase());
        let Some((id, ..)) = same else { continue };
        for paper in &mut backup.papers {
            if paper.reference_id.as_ref() == Some(&reference.id) {
                paper.reference_id = Some(id.clone());
            }
        }
        reference.id = id.clone();
    }
    Ok(())
}

/// A paper the library has under the same PDF name is that paper.
fn fit_papers(conn: &Connection, backup: &mut Backup) -> Result<(), LibraryError> {
    struct Known {
        id: String,
        file_path: Option<String>,
        ref_no: Option<i64>,
        reference_id: Option<String>,
    }
    let known: Vec<Known> = conn
        .prepare("SELECT id, file_path, ref_no, reference_id FROM paper")?
        .query_map([], |r| {
            Ok(Known { id: r.get(0)?, file_path: r.get(1)?, ref_no: r.get(2)?, reference_id: r.get(3)? })
        })?
        .collect::<Result<_, _>>()?;
    // Papers restored from this file before keep their place.
    let mut claimed: HashSet<String> =
        backup.papers.iter().map(|p| p.id.clone()).filter(|id| known.iter().any(|k| k.id == *id)).collect();
    for paper in &mut backup.papers {
        if !claimed.contains(&paper.id)
            && let Some(name) = &paper.file_name
        {
            let wanted = format!("{PDF_FOLDER}/{name}").to_lowercase();
            let same = known.iter().find(|k| {
                k.file_path.as_ref().is_some_and(|path| path.to_lowercase() == wanted) && !claimed.contains(&k.id)
            });
            if let Some(same) = same {
                paper.id = same.id.clone();
                claimed.insert(same.id.clone());
            }
        }
        // Only a change of the reference number lifts a link.
        if paper.reference_id.is_none()
            && let Some(same) = known.iter().find(|k| k.id == paper.id && k.ref_no == paper.ref_no)
        {
            paper.reference_id = same.reference_id.clone();
        }
    }
    Ok(())
}

/// A line without a code is the library's reason with that description, or gets the
/// next `E` code that neither the library nor the backup uses.
fn fit_reasons(conn: &Connection, backup: &mut Backup, old: &OldApp) -> Result<(), LibraryError> {
    let known: Vec<(String, String, String)> = conn
        .prepare("SELECT id, code, label FROM exclusion_reason ORDER BY sort_order, rowid")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    let mut codes: HashSet<String> = known
        .iter()
        .map(|(_, code, _)| code.as_str())
        .chain(
            backup.exclusion_reasons.iter().filter(|r| !old.uncoded_reasons.contains(&r.id)).map(|r| r.code.as_str()),
        )
        .map(|code| code.trim().to_lowercase())
        .collect();
    let mut next = 1;
    for reason in backup.exclusion_reasons.iter_mut().filter(|r| old.uncoded_reasons.contains(&r.id)) {
        let same = known
            .iter()
            .find(|(id, ..)| *id == reason.id)
            .or_else(|| known.iter().find(|(.., label)| label.trim().to_lowercase() == reason.label.to_lowercase()));
        if let Some((id, code, _)) = same {
            for paper in &mut backup.papers {
                if paper.exclusion_reason_id.as_ref() == Some(&reason.id) {
                    paper.exclusion_reason_id = Some(id.clone());
                }
            }
            reason.id = id.clone();
            reason.code = code.clone();
            continue;
        }
        while !codes.insert(format!("e{next}")) {
            next += 1;
        }
        reason.code = format!("E{next}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::backup::{self, RestorePreview, RestoreReport};
    use crate::criteria::{self, ListKind};
    use crate::db::library::create_library;
    use crate::papers;

    fn library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(PDF_FOLDER)).unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        (dir, conn)
    }

    /// As `exportJSON` of the prototype writes it.
    fn old_file() -> Value {
        json!({
            "app": APP, "version": 1, "exportedAt": "2026-11-24T01:00:00.000Z",
            "settings": {
                "reasons": ["E1 Wrong document type", "E3 Not about fish farming"],
                "tags": ["Spesies", "FCR"],
                "delimiter": ",", "zoom": 1.25, "viewMode": "dark", "leftHidden": false
            },
            "refs": [
                {"no": "46", "title": "Feed trial A", "year": "2021", "source": "Scopus", "doi": "10.1/a"},
                {"no": "47", "title": "Pond study", "year": "", "source": "", "doi": ""}
            ],
            "papers": [
                {"id": "m3abc1", "fileName": "kim-2021.pdf", "refNo": "46", "title": "Feed trial A",
                 "year": "2021", "source": "Scopus", "doi": "10.1/a", "decision": "include", "reason": "",
                 "note": "Compare with [#47]", "lastPage": 4, "numPages": 9, "hasFile": true,
                 "added": 1700000000000u64},
                {"id": "m3abc2", "fileName": "lee.pdf", "refNo": "47", "title": "Pond study",
                 "year": "", "source": "", "doi": "", "decision": "exclude",
                 "reason": "E3 Not about fish farming", "note": "", "lastPage": 1, "numPages": 0,
                 "hasFile": true, "added": 1700000000001u64}
            ],
            "highlights": [
                {"id": "h1", "paperId": "m3abc1", "page": 3, "text": "The FCR was 1.4", "color": "yellow",
                 "rects": [{"x": 0.1, "y": 0.2, "w": 0.5, "h": 0.02}], "note": "key result", "tag": "FCR",
                 "created": 1700000100000u64},
                {"id": "h2", "paperId": "m3abc2", "page": 1, "text": "Tilapia only", "color": "green",
                 "rects": [{"x": 0.0, "y": 0.0, "w": 1.0, "h": 0.02}], "note": "", "tag": "",
                 "created": 1700000100001u64},
                {"id": "h3", "paperId": "gone", "page": 1, "text": "Orphan", "color": "red",
                 "rects": [{"x": 0.1, "y": 0.1, "w": 0.1, "h": 0.1}], "note": "", "tag": "", "created": 1}
            ]
        })
    }

    fn read(value: &Value) -> Backup {
        backup::parse(value.to_string().as_bytes()).unwrap()
    }

    fn rows(conn: &Connection, sql: &str) -> Vec<String> {
        conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    const PAPERS: &str = "SELECT concat_ws('|', p.file_path, p.ref_no, p.title, p.year, p.decision, r.code,
                                 f.no, p.assessment_note, p.last_page)
                          FROM paper p LEFT JOIN exclusion_reason r ON r.id = p.exclusion_reason_id
                          LEFT JOIN reference f ON f.id = p.reference_id ORDER BY p.created_at";
    const HIGHLIGHTS: &str = "SELECT concat_ws('|', h.text, h.page, c.label, c.color, t.label, h.note, h.created_at)
                              FROM highlight h JOIN highlight_category c ON c.id = h.highlight_category_id
                              LEFT JOIN extraction_tag t ON t.id = h.extraction_tag_id ORDER BY h.created_at";

    #[test]
    fn restores_everything_into_a_library() {
        let (dir, conn) = library();
        let report = backup::restore(&conn, dir.path(), &read(&old_file()), false).unwrap();
        assert_eq!(report, RestoreReport { added_reasons: 2, added_tags: 2 });
        assert_eq!(
            rows(&conn, PAPERS),
            [
                "pdfs/kim-2021.pdf|46|Feed trial A|2021|include|46|Compare with [#47]|4",
                "pdfs/lee.pdf|47|Pond study|exclude|E3|47||1",
            ]
        );
        assert_eq!(
            rows(&conn, HIGHLIGHTS),
            [
                "The FCR was 1.4|3|Result|yellow|FCR|key result|2023-11-14T22:15:00.000Z",
                "Tilapia only|1|Inclusion|green||2023-11-14T22:15:00.001Z",
            ]
        );
        assert_eq!(rows(&conn, "SELECT label FROM highlight_category"), ["Inclusion", "Exclusion", "Method", "Result"]);
        assert_eq!(rows(&conn, "SELECT concat_ws('|', code, label) FROM exclusion_reason ORDER BY sort_order"), [
            "E1|Wrong document type",
            "E3|Not about fish farming",
        ]);
    }

    #[test]
    fn works_for_a_new_library() {
        let (dir, conn) = library();
        backup::restore(&conn, dir.path(), &read(&old_file()), true).unwrap();
        assert_eq!(rows(&conn, PAPERS).len(), 2);
        assert_eq!(rows(&conn, HIGHLIGHTS).len(), 2);
        assert_eq!(rows(&conn, "SELECT label FROM highlight_category"), ["Inclusion", "Exclusion", "Method", "Result"]);
        assert_eq!(criteria::research_questions(&conn).unwrap(), "");
    }

    #[test]
    fn adding_the_pdfs_afterwards_attaches_them() {
        // FR-07.3 acceptance criterion.
        let (dir, conn) = library();
        backup::restore(&conn, dir.path(), &read(&old_file()), false).unwrap();
        let sources = tempfile::tempdir().unwrap();
        let files: Vec<_> = ["kim-2021.pdf", "LEE.pdf"]
            .iter()
            .map(|name| {
                let path = sources.path().join(name);
                std::fs::write(&path, format!("%PDF-1.4 {name} %%EOF")).unwrap();
                path
            })
            .collect();
        let report = papers::import_pdfs(&conn, dir.path(), &files, false).unwrap();
        assert!(report.files.iter().all(|f| f.outcome == papers::ImportOutcome::Attached));
        for id in rows(&conn, "SELECT id FROM paper") {
            assert!(papers::pdf_path(&conn, dir.path(), &id).is_ok());
        }
        assert_eq!(rows(&conn, HIGHLIGHTS).len(), 2);
    }

    #[test]
    fn restoring_twice_adds_nothing_twice() {
        let (dir, conn) = library();
        let backup = read(&old_file());
        backup::restore(&conn, dir.path(), &backup, false).unwrap();
        let before = [rows(&conn, PAPERS), rows(&conn, HIGHLIGHTS)];
        assert_eq!(
            backup::preview(&conn, &backup).unwrap(),
            RestorePreview { replaced_papers: 2, replaced_highlights: 2, papers_after: 2 }
        );
        // Reading the file again gives the same ids.
        let again = backup::restore(&conn, dir.path(), &read(&old_file()), false).unwrap();
        assert_eq!(again, RestoreReport::default());
        assert_eq!([rows(&conn, PAPERS), rows(&conn, HIGHLIGHTS)], before);
        assert_eq!(rows(&conn, "SELECT id FROM reference").len(), 2);
        assert_eq!(rows(&conn, "SELECT id FROM highlight_category").len(), 4);
    }

    #[test]
    fn a_paper_with_the_same_pdf_name_is_that_paper() {
        let (dir, conn) = library();
        conn.execute_batch(
            "INSERT INTO reference (id, no, title) VALUES ('mine', 46, 'FEED TRIAL A');
             INSERT INTO paper (id, file_path, last_page) VALUES ('uploaded', 'pdfs/KIM-2021.pdf', 7);
             INSERT INTO paper (id, file_path, decision) VALUES ('other', 'pdfs/other.pdf', 'exclude');
             INSERT INTO highlight (id, paper_id, highlight_category_id, page, text, rects)
             SELECT 'new', 'uploaded', id, 2, 'Mine', '[{\"x\":0.1,\"y\":0.1,\"w\":0.1,\"h\":0.1}]'
             FROM highlight_category WHERE color = 'blue';",
        )
        .unwrap();
        let backup = read(&old_file());
        // It counts as replaced, so the user is asked first.
        assert_eq!(
            backup::preview(&conn, &backup).unwrap(),
            RestorePreview { replaced_papers: 1, replaced_highlights: 0, papers_after: 3 }
        );
        backup::restore(&conn, dir.path(), &backup, false).unwrap();
        assert_eq!(
            rows(&conn, "SELECT concat_ws('|', file_path, decision, last_page, reference_id) FROM paper WHERE id = 'uploaded'"),
            ["pdfs/KIM-2021.pdf|include|7|mine"]
        );
        assert_eq!(rows(&conn, "SELECT text FROM highlight WHERE paper_id = 'uploaded' ORDER BY page"), ["Mine", "The FCR was 1.4"]);
        assert_eq!(rows(&conn, "SELECT decision FROM paper WHERE id = 'other'"), ["exclude"]);
        // The library's reference 46 is used; only 47 is new.
        assert_eq!(rows(&conn, "SELECT id FROM reference").len(), 2);
    }

    #[test]
    fn asking_what_would_be_replaced_changes_nothing() {
        // Rule 1: the decision changes only after the user chose "Restore".
        let (_dir, conn) = library();
        conn.execute_batch(
            "INSERT INTO paper (id, file_path, decision) VALUES ('uploaded', 'pdfs/kim-2021.pdf', 'exclude');
             UPDATE highlight_category SET color = 'gray' WHERE color = 'yellow';
             INSERT INTO exclusion_reason (code, label, sort_order) VALUES ('E3', 'Mine', 0);",
        )
        .unwrap();
        let mut file = old_file();
        file["settings"]["reasons"] = json!(["Not fish"]);
        let before = conn.total_changes();
        let preview = backup::preview(&conn, &read(&file)).unwrap();
        assert_eq!(preview, RestorePreview { replaced_papers: 1, replaced_highlights: 0, papers_after: 2 });
        assert_eq!(conn.total_changes(), before);
        assert_eq!(rows(&conn, "SELECT decision FROM paper"), ["exclude"]);
    }

    #[test]
    fn keeps_a_reference_link_when_the_number_stays() {
        let (dir, conn) = library();
        conn.execute_batch(
            "INSERT INTO reference (id, no, title) VALUES ('mine', 46, 'Another title');
             INSERT INTO paper (id, file_path, reference_id, ref_no)
             VALUES ('uploaded', 'pdfs/kim-2021.pdf', 'mine', 46);",
        )
        .unwrap();
        let mut file = old_file();
        file["refs"] = json!([]);
        backup::restore(&conn, dir.path(), &read(&file), false).unwrap();
        assert_eq!(rows(&conn, "SELECT reference_id FROM paper WHERE id = 'uploaded'"), ["mine"]);
    }

    #[test]
    fn colors_map_to_the_librarys_categories_and_missing_ones_are_added() {
        let (dir, conn) = library();
        conn.execute_batch(
            "UPDATE highlight_category SET label = 'Numbers' WHERE color = 'yellow';
             DELETE FROM highlight_category WHERE color = 'green';
             -- The name of the missing color is taken by another one.
             UPDATE highlight_category SET label = 'inclusion' WHERE color = 'blue';",
        )
        .unwrap();
        backup::restore(&conn, dir.path(), &read(&old_file()), false).unwrap();
        assert_eq!(
            rows(&conn, "SELECT concat_ws('|', label, color) FROM highlight_category ORDER BY sort_order"),
            ["Exclusion|red", "inclusion|blue", "Numbers|yellow", "Inclusion (2)|green"]
        );
        assert!(rows(&conn, HIGHLIGHTS)[0].contains("|Numbers|yellow|"));
        assert!(rows(&conn, HIGHLIGHTS)[1].contains("|Inclusion (2)|green|"));
        // The user recolors the added one; restoring again still works.
        conn.execute("UPDATE highlight_category SET color = 'gray' WHERE color = 'green'", []).unwrap();
        backup::restore(&conn, dir.path(), &read(&old_file()), false).unwrap();
        assert!(rows(&conn, HIGHLIGHTS)[1].contains("|Inclusion (3)|green|"));
    }

    #[test]
    fn a_reason_without_a_code_gets_a_free_one() {
        let (dir, conn) = library();
        conn.execute("INSERT INTO exclusion_reason (code, label, sort_order) VALUES ('E2', 'Language', 0)", []).unwrap();
        let mut file = old_file();
        file["settings"]["reasons"] = json!(["E1 First", "Not fish", "E1 Second with the same code", "language"]);
        // Removed from the settings, but a paper still has it.
        file["papers"][1]["reason"] = json!("Not fish at all");
        let backup = read(&file);
        backup::restore(&conn, dir.path(), &backup, false).unwrap();
        let listed = || -> Vec<String> {
            criteria::list(&conn, ListKind::Exclusion)
                .unwrap()
                .into_iter()
                .map(|r| format!("{}|{}", r.code, r.label))
                .collect()
        };
        let expected = [
            "E2|Language",
            "E1|First",
            "E3|Not fish",
            "E4|E1 Second with the same code",
            "E5|Not fish at all",
        ];
        assert_eq!(listed(), expected);
        assert!(rows(&conn, PAPERS)[1].contains("|exclude|E5|"));
        // Again: the same reasons, none doubled.
        backup::restore(&conn, dir.path(), &backup, false).unwrap();
        assert_eq!(listed(), expected);
    }

    #[test]
    fn text_in_number_fields_moves_to_the_note() {
        let (dir, conn) = library();
        let mut file = old_file();
        file["papers"][0]["refNo"] = json!("46a");
        file["papers"][0]["year"] = json!("n.d.");
        file["papers"][1]["refNo"] = json!("47.0");
        file["papers"][1]["year"] = json!(2019);
        backup::restore(&conn, dir.path(), &read(&file), false).unwrap();
        assert_eq!(
            rows(&conn, PAPERS),
            [
                "pdfs/kim-2021.pdf|Feed trial A|include|Compare with [#47]\n\nReference No: 46a\nYear: n.d.|4",
                "pdfs/lee.pdf|47|Pond study|2019|exclude|E3|47||1",
            ]
        );
    }

    #[test]
    fn file_names_stay_inside_the_pdfs_folder() {
        let mut file = old_file();
        file["papers"][0]["fileName"] = json!("../x: y.pdf");
        file["papers"][1]["fileName"] = json!(".._x_ y.pdf");
        let names: Vec<_> = read(&file).papers.into_iter().map(|p| p.file_name.unwrap()).collect();
        assert_eq!(names, [".._x_ y.pdf", ".._x_ y (2).pdf"]);
    }

    #[test]
    fn a_broken_file_is_refused() {
        let refused = |change: &dyn Fn(&mut Value)| {
            let mut file = old_file();
            change(&mut file);
            backup::parse(file.to_string().as_bytes()).unwrap_err().code
        };
        assert_eq!(refused(&|v| v["version"] = json!(2)), ErrorCode::NotABackup);
        assert_eq!(refused(&|v| v["papers"] = json!("none")), ErrorCode::NotABackup);
        assert_eq!(refused(&|v| v["papers"][0]["decision"] = json!("maybe")), ErrorCode::NotABackup);
        assert_eq!(refused(&|v| v["papers"][1]["id"] = json!("m3abc1")), ErrorCode::NotABackup);
        assert_eq!(refused(&|v| v["highlights"][0]["color"] = json!("teal")), ErrorCode::NotABackup);
        assert_eq!(refused(&|v| v["highlights"][0]["rects"][0]["x"] = json!(0.9)), ErrorCode::NotABackup);
        assert_eq!(refused(&|v| v["highlights"][0]["text"] = json!(" ")), ErrorCode::NotABackup);
    }

    #[test]
    fn more_than_250_papers_is_counted_and_refused() {
        // The prototype has no limit.
        let (dir, conn) = library();
        let mut file = old_file();
        file["papers"] = (0..251).map(|n| json!({"id": n.to_string(), "fileName": format!("{n}.pdf")})).collect();
        let backup = read(&file);
        assert_eq!(backup::preview(&conn, &backup).unwrap().papers_after, 251);
        let error = backup::restore(&conn, dir.path(), &backup, false).unwrap_err();
        assert_eq!(error.code, ErrorCode::OverLimit);
        assert_eq!(rows(&conn, "SELECT id FROM paper").len(), 0);
    }
}
