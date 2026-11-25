//! The user's screening of a paper: decision, exclusion reason, and assessment note
//! (FR-04.1 to FR-04.4).
//!
//! `paper.decision` is written by `set_decision` only, and that runs only for a user
//! action (rule 1 of CLAUDE.md, FR-11.4). Each function writes one column, so saving a
//! note or a reason can never change the decision.
//!
//! The reason is kept when the decision changes, but only counts while the decision is
//! "exclude" (FR-04.3); summaries and exports must check that.
//!
//! Every function receives the connection of the open library from
//! `libraries::library_for_*` (FR-14.4).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::libraries::{ErrorCode, LibraryError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Undecided,
    Include,
    Exclude,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Undecided => "undecided",
            Decision::Include => "include",
            Decision::Exclude => "exclude",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [Decision::Undecided, Decision::Include, Decision::Exclude]
            .into_iter()
            .find(|decision| decision.as_str() == text)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screening {
    pub decision: Decision,
    pub exclusion_reason_id: Option<String>,
    pub assessment_note: String,
}

/// An exclusion reason of the library, for the reason choice. Managing them is T-35.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExclusionReason {
    pub id: String,
    pub code: String,
    pub label: String,
}

fn unknown(detail: String) -> LibraryError {
    LibraryError::new(ErrorCode::Other, detail)
}

/// Fails when the update matched no paper.
fn one_paper(changed: usize, paper_id: &str) -> Result<(), LibraryError> {
    if changed == 0 {
        return Err(unknown(format!("no paper with id {paper_id}")));
    }
    Ok(())
}

pub fn screening(conn: &Connection, paper_id: &str) -> Result<Screening, LibraryError> {
    let (decision, exclusion_reason_id, assessment_note): (String, Option<String>, String) = conn
        .query_row(
            "SELECT decision, exclusion_reason_id, assessment_note FROM paper WHERE id = ?1",
            [paper_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| unknown(format!("no paper with id {paper_id}")))?;
    let decision =
        Decision::parse(&decision).ok_or_else(|| unknown(format!("unknown decision {decision}")))?;
    Ok(Screening { decision, exclusion_reason_id, assessment_note })
}

/// The user's decision (FR-04.1).
pub fn set_decision(conn: &Connection, paper_id: &str, decision: Decision) -> Result<(), LibraryError> {
    let changed =
        conn.execute("UPDATE paper SET decision = ?1 WHERE id = ?2", params![decision.as_str(), paper_id])?;
    one_paper(changed, paper_id)
}

/// The exclusion reason, `None` for "No reason" (FR-04.2).
pub fn set_exclusion_reason(conn: &Connection, paper_id: &str, reason_id: Option<&str>) -> Result<(), LibraryError> {
    if let Some(reason) = reason_id {
        let exists: Option<i64> = conn
            .query_row("SELECT 1 FROM exclusion_reason WHERE id = ?1", [reason], |r| r.get(0))
            .optional()?;
        if exists.is_none() {
            return Err(unknown("no such exclusion reason".into()));
        }
    }
    let changed = conn.execute(
        "UPDATE paper SET exclusion_reason_id = ?1 WHERE id = ?2",
        params![reason_id, paper_id],
    )?;
    one_paper(changed, paper_id)
}

/// The assessment note (FR-04.4).
pub fn set_assessment_note(conn: &Connection, paper_id: &str, note: &str) -> Result<(), LibraryError> {
    let changed = conn.execute("UPDATE paper SET assessment_note = ?1 WHERE id = ?2", params![note, paper_id])?;
    one_paper(changed, paper_id)
}

/// The library's exclusion reasons, in their order (FR-08.1).
pub fn list_reasons(conn: &Connection) -> Result<Vec<ExclusionReason>, LibraryError> {
    let mut stmt = conn.prepare("SELECT id, code, label FROM exclusion_reason ORDER BY sort_order, rowid")?;
    let rows =
        stmt.query_map([], |r| Ok(ExclusionReason { id: r.get(0)?, code: r.get(1)?, label: r.get(2)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::{create_library, open_library};

    fn library_with_paper() -> (tempfile::TempDir, Connection, String) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        let id = add_paper(&conn);
        (dir, conn, id)
    }

    fn add_paper(conn: &Connection) -> String {
        conn.query_row("INSERT INTO paper DEFAULT VALUES RETURNING id", [], |r| r.get(0)).unwrap()
    }

    fn add_reason(conn: &Connection, code: &str, order: i64) -> String {
        conn.query_row(
            "INSERT INTO exclusion_reason (code, label, sort_order) VALUES (?1, 'Wrong population', ?2) RETURNING id",
            params![code, order],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn a_new_paper_is_undecided_without_reason_or_note() {
        let (_dir, conn, paper) = library_with_paper();
        assert_eq!(
            screening(&conn, &paper).unwrap(),
            Screening { decision: Decision::Undecided, exclusion_reason_id: None, assessment_note: String::new() }
        );
    }

    #[test]
    fn saves_decision_reason_and_note_across_reopening() {
        let (dir, conn, paper) = library_with_paper();
        let other = add_paper(&conn);
        let reason = add_reason(&conn, "E3", 0);
        set_decision(&conn, &paper, Decision::Exclude).unwrap();
        set_exclusion_reason(&conn, &paper, Some(&reason)).unwrap();
        set_assessment_note(&conn, &paper, "  Pond trial only, see [#12]\n").unwrap();
        drop(conn);

        let conn = open_library(dir.path()).unwrap().conn;
        assert_eq!(
            screening(&conn, &paper).unwrap(),
            Screening {
                decision: Decision::Exclude,
                exclusion_reason_id: Some(reason),
                assessment_note: "  Pond trial only, see [#12]\n".into(),
            }
        );
        assert_eq!(screening(&conn, &other).unwrap().decision, Decision::Undecided);
    }

    #[test]
    fn the_decision_moves_freely_and_keeps_the_reason() {
        // PRD 7a: any decision to any other. The reason only counts while excluded (FR-04.3).
        let (_dir, conn, paper) = library_with_paper();
        let reason = add_reason(&conn, "E3", 0);
        set_exclusion_reason(&conn, &paper, Some(&reason)).unwrap();
        for decision in [Decision::Exclude, Decision::Include, Decision::Undecided, Decision::Exclude] {
            set_decision(&conn, &paper, decision).unwrap();
            let now = screening(&conn, &paper).unwrap();
            assert_eq!((now.decision, now.exclusion_reason_id), (decision, Some(reason.clone())));
        }
        // "No reason" is allowed while excluded (FR-04.2).
        set_exclusion_reason(&conn, &paper, None).unwrap();
        assert_eq!(screening(&conn, &paper).unwrap().exclusion_reason_id, None);
    }

    #[test]
    fn only_set_decision_changes_the_decision() {
        // Rule 1 of CLAUDE.md (FR-11.4).
        let (_dir, conn, paper) = library_with_paper();
        let reason = add_reason(&conn, "E3", 0);
        set_decision(&conn, &paper, Decision::Include).unwrap();
        set_exclusion_reason(&conn, &paper, Some(&reason)).unwrap();
        set_assessment_note(&conn, &paper, "exclude this one").unwrap();
        list_reasons(&conn).unwrap();
        assert_eq!(screening(&conn, &paper).unwrap().decision, Decision::Include);
    }

    #[test]
    fn refuses_unknown_papers_reasons_and_decisions() {
        let (_dir, conn, paper) = library_with_paper();
        assert!(set_decision(&conn, "nope", Decision::Include).is_err());
        assert!(set_exclusion_reason(&conn, "nope", None).is_err());
        assert!(set_exclusion_reason(&conn, &paper, Some("nope")).is_err());
        assert!(set_assessment_note(&conn, "nope", "n").is_err());
        assert!(screening(&conn, "nope").is_err());
        assert!(serde_json::from_str::<Decision>(r#""maybe""#).is_err());
        assert_eq!(serde_json::to_value(Decision::Exclude).unwrap(), "exclude");
        assert_eq!(screening(&conn, &paper).unwrap().decision, Decision::Undecided);
    }

    #[test]
    fn lists_reasons_in_their_order() {
        let (_dir, conn, _) = library_with_paper();
        assert!(list_reasons(&conn).unwrap().is_empty());
        add_reason(&conn, "E2", 1);
        add_reason(&conn, "E1", 0);
        let codes: Vec<String> = list_reasons(&conn).unwrap().into_iter().map(|r| r.code).collect();
        assert_eq!(codes, ["E1", "E2"]);
    }
}
