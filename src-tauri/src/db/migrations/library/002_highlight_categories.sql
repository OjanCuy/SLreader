-- Library database, version 2: highlight categories per library (PRD FR-03.1, FR-08.6).
-- Before this version a highlight had one of 4 fixed colors. Now each library has
-- 1 to 8 categories, each with a name and one color of a fixed palette of 8.
-- Every existing highlight moves to the default category of its old color; nothing
-- is lost. The app saves library-v1-backup.sqlite before running this.

CREATE TABLE highlight_category (
  id         TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  label      TEXT NOT NULL CHECK (trim(label) <> ''),
  -- The palette (PRD 7a). Each color is used by one category at most.
  color      TEXT NOT NULL UNIQUE
               CHECK (color IN ('green', 'red', 'blue', 'yellow', 'purple', 'orange', 'pink', 'gray')),
  sort_order INTEGER NOT NULL
);

-- Names do not repeat, whatever their case or the spaces around them.
CREATE UNIQUE INDEX highlight_category_label ON highlight_category (lower(trim(label)));

-- 1 to 8 categories (FR-08.6). The app checks first and explains; these are the
-- last line of defence.
CREATE TRIGGER highlight_category_most
BEFORE INSERT ON highlight_category
WHEN (SELECT count(*) FROM highlight_category) >= 8
BEGIN
  SELECT RAISE(ABORT, 'a library has at most 8 highlight categories');
END;

CREATE TRIGGER highlight_category_least
BEFORE DELETE ON highlight_category
WHEN (SELECT count(*) FROM highlight_category) <= 1
BEGIN
  SELECT RAISE(ABORT, 'a library keeps at least 1 highlight category');
END;

-- The 4 defaults, with the meanings of the old fixed colors (FR-03.1).
INSERT INTO highlight_category (label, color, sort_order) VALUES
  ('Inclusion', 'green', 0),
  ('Exclusion', 'red', 1),
  ('Method', 'blue', 2),
  ('Result', 'yellow', 3);

-- SQLite cannot change a column's constraints in place, so the table is rebuilt.
-- Nothing refers to highlight rows, so dropping the old table removes nothing else.
CREATE TABLE highlight_v2 (
  id                    TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  paper_id              TEXT NOT NULL REFERENCES paper (id) ON DELETE CASCADE,
  -- A category in use cannot be deleted: its highlights are moved to another
  -- category first (FR-08.6), so no highlight is ever lost.
  highlight_category_id TEXT NOT NULL REFERENCES highlight_category (id) ON DELETE RESTRICT,
  -- Deleting the tag clears it from the highlight (FR-08.2).
  extraction_tag_id     TEXT REFERENCES extraction_tag (id) ON DELETE SET NULL,
  page                  INTEGER NOT NULL CHECK (page >= 1),
  text                  TEXT NOT NULL,
  -- JSON list of rectangles relative to the page, from 0 to 1 (FR-03.2).
  rects                 TEXT NOT NULL CHECK (json_valid(rects)),
  note                  TEXT NOT NULL DEFAULT '',
  created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- The old CHECK allowed only these 4 colors, and each has a default category, so the
-- join keeps every row. Rows keep their order, which breaks ties in created_at.
INSERT INTO highlight_v2
  (id, paper_id, highlight_category_id, extraction_tag_id, page, text, rects, note, created_at)
SELECT h.id, h.paper_id, c.id, h.extraction_tag_id, h.page, h.text, h.rects, h.note, h.created_at
FROM highlight AS h
JOIN highlight_category AS c ON c.color = h.color
ORDER BY h.rowid;

DROP TABLE highlight;
ALTER TABLE highlight_v2 RENAME TO highlight;

CREATE INDEX highlight_by_paper ON highlight (paper_id, page);
CREATE INDEX highlight_by_category ON highlight (highlight_category_id);
