//! `[#46]` links in assessment notes and highlight notes (FR-05.5, FR-03.6): finding
//! the paper a link opens, and updating links after a reference number changed.
//!
//! Only note text is ever changed here, and only when the user has confirmed it. A
//! link is `[#`, one to nine digits, and `]`; `src/lib/refLinks.ts` reads the same.
//!
//! Every function receives the connection of the open library from
//! `libraries::library_for_*` (FR-14.4).

use rusqlite::{Connection, OptionalExtension, params};

use crate::libraries::LibraryError;

/// The paper a link to `ref_no` opens: the one added first, if several have the number.
pub fn paper_for(conn: &Connection, ref_no: i64) -> Result<Option<String>, LibraryError> {
    Ok(conn
        .query_row(
            "SELECT id FROM paper WHERE ref_no = ?1 ORDER BY created_at, rowid LIMIT 1",
            [ref_no],
            |r| r.get(0),
        )
        .optional()?)
}

/// `text` with every link to `from` pointing to `to`; `None` if it has no such link.
fn rewrite(text: &str, from: i64, to: i64) -> Option<String> {
    let mut result = String::new();
    // Everything before this byte is already in `result`.
    let mut copied = 0;
    for (start, _) in text.match_indices("[#") {
        let rest = &text[start + 2..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if !(1..=9).contains(&digits) || rest.as_bytes().get(digits) != Some(&b']') {
            continue;
        }
        if rest[..digits].parse() == Ok(from) {
            result.push_str(&text[copied..start]);
            result.push_str(&format!("[#{to}]"));
            copied = start + 2 + digits + 1;
        }
    }
    (copied > 0).then(|| result + &text[copied..])
}

/// The two kinds of notes: the query that reads them, and the one that saves one.
const NOTES: [(&str, &str); 2] = [
    (
        "SELECT id, assessment_note FROM paper WHERE instr(assessment_note, '[#') > 0",
        "UPDATE paper SET assessment_note = ?1 WHERE id = ?2",
    ),
    (
        "SELECT id, note FROM highlight WHERE instr(note, '[#') > 0",
        "UPDATE highlight SET note = ?1 WHERE id = ?2",
    ),
];

/// The notes that link to `from`, with their text after the update, per kind.
fn changes(conn: &Connection, from: i64, to: i64) -> Result<Vec<(&'static str, String, String)>, LibraryError> {
    // While a paper still has the old number, its links are right.
    if from == to || paper_for(conn, from)?.is_some() {
        return Ok(Vec::new());
    }
    let mut changed = Vec::new();
    for (select, update) in NOTES {
        let mut stmt = conn.prepare(select)?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (id, note) = row?;
            if let Some(new) = rewrite(&note, from, to) {
                changed.push((update, id, new));
            }
        }
    }
    Ok(changed)
}

/// How many notes an update from `from` to `to` would change; 0 when there is nothing
/// to offer.
pub fn count(conn: &Connection, from: i64, to: i64) -> Result<usize, LibraryError> {
    Ok(changes(conn, from, to)?.len())
}

/// Points the links to `from` at `to`, in all notes or in none. Only for an update the
/// user has confirmed. Returns how many notes changed.
pub fn update(conn: &Connection, from: i64, to: i64) -> Result<usize, LibraryError> {
    let transaction = conn.unchecked_transaction()?;
    let changed = changes(&transaction, from, to)?;
    for (sql, id, note) in &changed {
        transaction.execute(sql, params![note, id])?;
    }
    transaction.commit()?;
    Ok(changed.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::library::create_library;

    fn library() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = create_library(dir.path(), "R").unwrap();
        (dir, conn)
    }

    fn paper(conn: &Connection, id: &str, ref_no: Option<i64>, note: &str) {
        conn.execute(
            "INSERT INTO paper (id, ref_no, assessment_note, decision) VALUES (?1, ?2, ?3, 'include')",
            params![id, ref_no, note],
        )
        .unwrap();
    }

    fn highlight(conn: &Connection, paper_id: &str, note: &str) {
        conn.execute(
            "INSERT INTO highlight (paper_id, highlight_category_id, page, text, rects, note)
             VALUES (?1, (SELECT id FROM highlight_category WHERE color = 'green'), 1, 'see [#12]', '[]', ?2)",
            [paper_id, note],
        )
        .unwrap();
    }

    fn column(conn: &Connection, sql: &str) -> Vec<String> {
        conn.prepare(sql).unwrap().query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn rewrites_exact_links_only() {
        let text = "[#12] and [#012], not [#123], [#1], [# 12], #12, [12] or [#1234567890]. é[#[#12]";
        assert_eq!(
            rewrite(text, 12, 4).unwrap(),
            "[#4] and [#4], not [#123], [#1], [# 12], #12, [12] or [#1234567890]. é[#[#4]"
        );
        for none in ["", "[#13]", "[#12", "[#]", "12"] {
            assert_eq!(rewrite(none, 12, 4), None, "{none:?}");
        }
    }

    #[test]
    fn a_link_opens_the_paper_added_first() {
        let (_dir, conn) = library();
        assert_eq!(paper_for(&conn, 12).unwrap(), None);
        paper(&conn, "a", Some(12), "");
        paper(&conn, "b", Some(12), "");
        assert_eq!(paper_for(&conn, 12).unwrap().as_deref(), Some("a"));
    }

    #[test]
    fn updates_both_kinds_of_notes_and_nothing_else() {
        let (_dir, conn) = library();
        paper(&conn, "a", Some(14), "Me: [#12] and [#12]");
        paper(&conn, "b", None, "As [#12], unlike [#3]");
        paper(&conn, "c", Some(3), "No link");
        highlight(&conn, "c", "Compare [#12]");
        highlight(&conn, "c", "[#3]");

        assert_eq!(count(&conn, 12, 14).unwrap(), 3);
        assert_eq!(update(&conn, 12, 14).unwrap(), 3);
        assert_eq!(
            column(&conn, "SELECT assessment_note FROM paper ORDER BY id"),
            ["Me: [#14] and [#14]", "As [#14], unlike [#3]", "No link"]
        );
        assert_eq!(column(&conn, "SELECT note FROM highlight ORDER BY rowid"), ["Compare [#14]", "[#3]"]);
        assert_eq!(column(&conn, "SELECT DISTINCT text FROM highlight"), ["see [#12]"]);
        // Never a decision (rule 1 of CLAUDE.md).
        assert_eq!(column(&conn, "SELECT DISTINCT decision FROM paper"), ["include"]);
        assert_eq!(count(&conn, 12, 14).unwrap(), 0);
    }

    #[test]
    fn offers_nothing_while_a_paper_still_has_the_old_number() {
        let (_dir, conn) = library();
        paper(&conn, "a", Some(12), "[#12]");
        assert_eq!(count(&conn, 12, 14).unwrap(), 0);
        assert_eq!(update(&conn, 12, 14).unwrap(), 0);
        assert_eq!(count(&conn, 7, 7).unwrap(), 0);
        assert_eq!(column(&conn, "SELECT assessment_note FROM paper"), ["[#12]"]);
    }

    #[test]
    fn a_failed_update_changes_no_note() {
        let (_dir, conn) = library();
        paper(&conn, "a", None, "[#12]");
        highlight(&conn, "a", "[#12]");
        conn.execute_batch(
            "CREATE TRIGGER fail BEFORE UPDATE ON highlight BEGIN SELECT RAISE(ABORT, 'disk full'); END;",
        )
        .unwrap();
        assert!(update(&conn, 12, 14).is_err());
        assert_eq!(column(&conn, "SELECT assessment_note FROM paper"), ["[#12]"]);
    }
}
