//! The criteria of a library: research questions, inclusion criteria, exclusion
//! reasons, and extraction tags (FR-08.1, FR-08.2, FR-08.4).
//!
//! Deleting an exclusion reason or a tag never deletes a paper or a highlight and never
//! changes a decision: the database only empties the reference to it
//! (`ON DELETE SET NULL`), so the paper counts as "No reason" and the highlight has no
//! tag. The UI asks the user before every delete (O-10).
//!
//! Every function receives the connection of the open library from
//! `libraries::library_for_*` (FR-14.4).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::libraries::{ErrorCode, LibraryError};

/// Which of the three lists of the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListKind {
    Inclusion,
    Exclusion,
    Tag,
}

impl ListKind {
    fn table(self) -> &'static str {
        match self {
            ListKind::Inclusion => "inclusion_criterion",
            ListKind::Exclusion => "exclusion_reason",
            ListKind::Tag => "extraction_tag",
        }
    }

    /// Tags have a name only.
    fn has_code(self) -> bool {
        self != ListKind::Tag
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub id: String,
    /// Empty for a tag.
    pub code: String,
    /// The description, or the tag name.
    pub label: String,
}

fn invalid(detail: &str) -> LibraryError {
    LibraryError::new(ErrorCode::Other, detail.to_string())
}

/// The items of a list, in their order.
pub fn list(conn: &Connection, kind: ListKind) -> Result<Vec<Item>, LibraryError> {
    let code = if kind.has_code() { "code" } else { "''" };
    let mut stmt = conn.prepare(&format!(
        "SELECT id, {code}, label FROM {} ORDER BY sort_order, rowid",
        kind.table()
    ))?;
    let rows = stmt.query_map([], |r| Ok(Item { id: r.get(0)?, code: r.get(1)?, label: r.get(2)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The code and label as stored: trimmed, and not empty where the list needs them.
fn checked<'a>(kind: ListKind, code: &'a str, label: &'a str) -> Result<(&'a str, &'a str), LibraryError> {
    let (code, label) = (code.trim(), label.trim());
    if label.is_empty() || (kind.has_code() && code.is_empty()) {
        return Err(invalid("a required field is empty"));
    }
    Ok((code, label))
}

/// Adds an item at the end of its list and returns its id.
pub fn add(conn: &Connection, kind: ListKind, code: &str, label: &str) -> Result<String, LibraryError> {
    let (code, label) = checked(kind, code, label)?;
    let table = kind.table();
    let next = format!("(SELECT coalesce(max(sort_order), -1) + 1 FROM {table})");
    let id = if kind.has_code() {
        conn.query_row(
            &format!("INSERT INTO {table} (code, label, sort_order) VALUES (?1, ?2, {next}) RETURNING id"),
            params![code, label],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            &format!("INSERT INTO {table} (label, sort_order) VALUES (?1, {next}) RETURNING id"),
            [label],
            |r| r.get(0),
        )?
    };
    Ok(id)
}

/// Renames an item. Codes may repeat: the UI warns about it.
pub fn update(conn: &Connection, kind: ListKind, id: &str, code: &str, label: &str) -> Result<(), LibraryError> {
    let (code, label) = checked(kind, code, label)?;
    let table = kind.table();
    let changed = if kind.has_code() {
        conn.execute(
            &format!("UPDATE {table} SET code = ?1, label = ?2 WHERE id = ?3"),
            params![code, label, id],
        )?
    } else {
        conn.execute(&format!("UPDATE {table} SET label = ?1 WHERE id = ?2"), params![label, id])?
    };
    if changed == 0 {
        return Err(invalid("no such item"));
    }
    Ok(())
}

/// How many papers use an exclusion reason (whatever their decision, since all of them
/// lose it), or how many highlights use a tag. Inclusion criteria are used by nothing.
pub fn uses(conn: &Connection, kind: ListKind, id: &str) -> Result<u32, LibraryError> {
    let sql = match kind {
        ListKind::Inclusion => return Ok(0),
        ListKind::Exclusion => "SELECT count(*) FROM paper WHERE exclusion_reason_id = ?1",
        ListKind::Tag => "SELECT count(*) FROM highlight WHERE extraction_tag_id = ?1",
    };
    Ok(conn.query_row(sql, [id], |r| r.get(0))?)
}

/// Deletes an item. Only for a delete the user has confirmed. Deleting one that is
/// already gone is fine.
pub fn delete(conn: &Connection, kind: ListKind, id: &str) -> Result<(), LibraryError> {
    conn.execute(&format!("DELETE FROM {} WHERE id = ?1", kind.table()), [id])?;
    Ok(())
}

/// Puts a list in the order of `ids`, which must name every item of it exactly once.
/// For tags this is also the order of the export columns (FR-08.2).
pub fn reorder(conn: &Connection, kind: ListKind, ids: &[String]) -> Result<(), LibraryError> {
    let table = kind.table();
    let transaction = conn.unchecked_transaction()?;
    let mut stored: Vec<String> = list(&transaction, kind)?.into_iter().map(|item| item.id).collect();
    let mut wanted = ids.to_vec();
    stored.sort();
    wanted.sort();
    if stored != wanted {
        return Err(invalid("the order does not match the list"));
    }
    {
        let mut update = transaction.prepare(&format!("UPDATE {table} SET sort_order = ?1 WHERE id = ?2"))?;
        for (position, id) in ids.iter().enumerate() {
            update.execute(params![position as i64, id])?;
        }
    }
    transaction.commit()?;
    Ok(())
}

/// The research questions, one per line (FR-08.4).
pub fn research_questions(conn: &Connection) -> Result<String, LibraryError> {
    conn.query_row("SELECT research_questions FROM library", [], |r| r.get(0))
        .optional()?
        .ok_or_else(|| invalid("no library row"))
}

/// Saved as typed: trimming here would fight the user's typing.
pub fn set_research_questions(conn: &Connection, text: &str) -> Result<(), LibraryError> {
    if conn.execute("UPDATE library SET research_questions = ?1", [text])? == 0 {
        return Err(invalid("no library row"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::{create_library, open_library};

    const ALL: [ListKind; 3] = [ListKind::Inclusion, ListKind::Exclusion, ListKind::Tag];

    fn library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        (dir, conn)
    }

    fn labels(conn: &Connection, kind: ListKind) -> Vec<String> {
        list(conn, kind).unwrap().into_iter().map(|item| item.label).collect()
    }

    #[test]
    fn a_new_library_has_empty_lists_and_no_questions() {
        let (_dir, conn) = library();
        for kind in ALL {
            assert!(list(&conn, kind).unwrap().is_empty());
        }
        assert_eq!(research_questions(&conn).unwrap(), "");
    }

    #[test]
    fn adds_renames_and_keeps_everything_across_reopening() {
        let (dir, conn) = library();
        for kind in ALL {
            let first = add(&conn, kind, " E1 ", " Wrong population ").unwrap();
            add(&conn, kind, "E2", "Review").unwrap();
            update(&conn, kind, &first, "E9", "Not fish").unwrap();
        }
        set_research_questions(&conn, "RQ1?\nRQ2? ").unwrap();
        drop(conn);
        let conn = open_library(dir.path()).unwrap().conn;
        for kind in ALL {
            let items = list(&conn, kind).unwrap();
            assert_eq!(labels(&conn, kind), ["Not fish", "Review"]);
            assert_eq!(items[0].code, if kind == ListKind::Tag { "" } else { "E9" });
        }
        assert_eq!(research_questions(&conn).unwrap(), "RQ1?\nRQ2? ");
    }

    #[test]
    fn refuses_empty_required_fields_and_keeps_the_old_value() {
        let (_dir, conn) = library();
        for kind in ALL {
            assert!(add(&conn, kind, "E1", "  ").is_err());
            let id = add(&conn, kind, "E1", "Kept").unwrap();
            assert!(update(&conn, kind, &id, "E1", "").is_err());
            assert!(update(&conn, kind, "nope", "E1", "x").is_err());
            assert_eq!(labels(&conn, kind), ["Kept"]);
        }
        assert!(add(&conn, ListKind::Inclusion, " ", "x").is_err());
        assert!(add(&conn, ListKind::Exclusion, "", "x").is_err());
        assert!(add(&conn, ListKind::Tag, "", "x").is_ok());
    }

    #[test]
    fn reorders_a_whole_list_or_not_at_all() {
        let (_dir, conn) = library();
        let ids: Vec<String> =
            ["a", "b", "c"].iter().map(|label| add(&conn, ListKind::Tag, "", label).unwrap()).collect();
        let order = |picked: &[usize]| picked.iter().map(|&at| ids[at].clone()).collect::<Vec<_>>();

        reorder(&conn, ListKind::Tag, &order(&[2, 0, 1])).unwrap();
        assert_eq!(labels(&conn, ListKind::Tag), ["c", "a", "b"]);
        for wrong in [order(&[0, 1]), order(&[0, 0, 1]), order(&[0, 1, 2, 2])] {
            assert!(reorder(&conn, ListKind::Tag, &wrong).is_err());
        }
        assert_eq!(labels(&conn, ListKind::Tag), ["c", "a", "b"]);
        // A new item still comes last.
        add(&conn, ListKind::Tag, "", "d").unwrap();
        assert_eq!(labels(&conn, ListKind::Tag), ["c", "a", "b", "d"]);
    }

    #[test]
    fn deleting_a_reason_keeps_the_papers_and_their_decisions() {
        // FR-08.1; rule 1 of CLAUDE.md.
        let (_dir, conn) = library();
        let used = add(&conn, ListKind::Exclusion, "E1", "Used").unwrap();
        let other = add(&conn, ListKind::Exclusion, "E2", "Other").unwrap();
        for (decision, reason) in [("exclude", &used), ("include", &used), ("exclude", &other)] {
            conn.execute(
                "INSERT INTO paper (decision, exclusion_reason_id, assessment_note) VALUES (?1, ?2, 'n')",
                params![decision, reason],
            )
            .unwrap();
        }
        // The include keeps the reason too (FR-04.3), so it counts as a use.
        assert_eq!(uses(&conn, ListKind::Exclusion, &used).unwrap(), 2);

        delete(&conn, ListKind::Exclusion, &used).unwrap();
        delete(&conn, ListKind::Exclusion, &used).unwrap();
        let papers: Vec<(String, Option<String>, String)> = conn
            .prepare("SELECT decision, exclusion_reason_id, assessment_note FROM paper ORDER BY rowid")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            papers,
            [
                ("exclude".into(), None, "n".into()),
                ("include".into(), None, "n".into()),
                ("exclude".into(), Some(other), "n".into()),
            ]
        );
    }

    #[test]
    fn deleting_a_tag_keeps_the_highlights_and_their_notes() {
        // FR-08.2.
        let (_dir, conn) = library();
        let tag = add(&conn, ListKind::Tag, "", "FCR").unwrap();
        conn.execute("INSERT INTO paper (id) VALUES ('p')", []).unwrap();
        conn.execute(
            "INSERT INTO highlight (paper_id, highlight_category_id, page, text, rects, note, extraction_tag_id)
             VALUES ('p', (SELECT id FROM highlight_category WHERE color = 'green'), 1, 't', '[]', 'n', ?1)",
            [&tag],
        )
        .unwrap();
        assert_eq!(uses(&conn, ListKind::Tag, &tag).unwrap(), 1);

        delete(&conn, ListKind::Tag, &tag).unwrap();
        let kept: (Option<String>, String) = conn
            .query_row("SELECT extraction_tag_id, note FROM highlight", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(kept, (None, "n".into()));
    }

    #[test]
    fn an_inclusion_criterion_has_no_uses() {
        let (_dir, conn) = library();
        let id = add(&conn, ListKind::Inclusion, "I1", "Fish").unwrap();
        assert_eq!(uses(&conn, ListKind::Inclusion, &id).unwrap(), 0);
        delete(&conn, ListKind::Inclusion, &id).unwrap();
        assert!(list(&conn, ListKind::Inclusion).unwrap().is_empty());
    }
}
