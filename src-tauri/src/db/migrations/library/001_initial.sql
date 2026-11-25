-- Library database, version 1: every entity of PRD Section 7a except AIProvider and AppSetting.
-- One database per library folder, so no table needs a library_id (PRD 7a).
-- IDs are random 32-character hex strings, so a backup restored into another
-- library never clashes with IDs that are already there.
-- Times are UTC text in ISO 8601, e.g. 2026-11-25T10:30:48.515Z.

-- Exactly one row: the library this database belongs to (rule 6 of CLAUDE.md).
CREATE TABLE library (
  id                 TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  name               TEXT NOT NULL CHECK (trim(name) <> ''),
  -- One research question per line (Brief S-04).
  research_questions TEXT NOT NULL DEFAULT '',
  created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  singleton          INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK (singleton = 1)
);

-- Reference list imported from CSV or Excel (FR-05.2).
CREATE TABLE reference (
  id              TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  no              INTEGER NOT NULL,
  title           TEXT NOT NULL,
  year            INTEGER,
  source_database TEXT,
  doi             TEXT
);

CREATE TABLE inclusion_criterion (
  id         TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  code       TEXT NOT NULL,
  label      TEXT NOT NULL,
  sort_order INTEGER NOT NULL
);

CREATE TABLE exclusion_reason (
  id         TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  code       TEXT NOT NULL,
  label      TEXT NOT NULL,
  sort_order INTEGER NOT NULL
);

CREATE TABLE extraction_tag (
  id         TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  label      TEXT NOT NULL,
  sort_order INTEGER NOT NULL
);

CREATE TABLE paper (
  id                  TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  -- Relative to the library folder, e.g. pdfs/kim-2021.pdf, so the folder can be moved (FR-07.4).
  -- Empty for a paper restored from a backup whose PDF is not attached yet (FR-07.3).
  file_path           TEXT UNIQUE,
  -- One reference links to at most one paper (FR-05.4).
  -- Replacing the reference list keeps the paper's own copy of the metadata below.
  reference_id        TEXT UNIQUE REFERENCES reference (id) ON DELETE SET NULL,
  ref_no              INTEGER,
  title               TEXT NOT NULL DEFAULT '',
  year                INTEGER,
  source_database     TEXT,
  doi                 TEXT,
  -- Changed only by a user action, never by an AI suggestion (FR-11.4).
  decision            TEXT NOT NULL DEFAULT 'undecided'
                        CHECK (decision IN ('undecided', 'include', 'exclude')),
  -- Kept when the decision changes, but only counts while decision = 'exclude' (FR-04.3).
  -- Deleting the reason turns the paper into "No reason" (FR-08.1).
  exclusion_reason_id TEXT REFERENCES exclusion_reason (id) ON DELETE SET NULL,
  assessment_note     TEXT NOT NULL DEFAULT '',
  last_page           INTEGER NOT NULL DEFAULT 1 CHECK (last_page >= 1),
  text_status         TEXT NOT NULL DEFAULT 'pending'
                        CHECK (text_status IN ('pending', 'text_ready', 'no_text')),
  created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- At most 250 papers per library (FR-01.4). The import code checks first and
-- shows the message; this trigger is the last line of defence.
CREATE TRIGGER paper_limit
BEFORE INSERT ON paper
WHEN (SELECT count(*) FROM paper) >= 250
BEGIN
  SELECT RAISE(ABORT, 'library limit of 250 papers reached');
END;

CREATE TABLE highlight (
  id                TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  paper_id          TEXT NOT NULL REFERENCES paper (id) ON DELETE CASCADE,
  -- Deleting the tag clears it from the highlight (FR-08.2).
  extraction_tag_id TEXT REFERENCES extraction_tag (id) ON DELETE SET NULL,
  page              INTEGER NOT NULL CHECK (page >= 1),
  -- green = supports inclusion, red = supports exclusion, blue = method,
  -- yellow = quantitative result (FR-03.1).
  color             TEXT NOT NULL CHECK (color IN ('green', 'red', 'blue', 'yellow')),
  text              TEXT NOT NULL,
  -- JSON list of rectangles relative to the page, from 0 to 1 (FR-03.2).
  rects             TEXT NOT NULL CHECK (json_valid(rects)),
  note              TEXT NOT NULL DEFAULT '',
  created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX highlight_by_paper ON highlight (paper_id, page);

-- Deleting a chunk also deletes the citations that quote it. Rebuilding the
-- index must re-embed existing chunks (pending -> embedded), not recreate them.
CREATE TABLE text_chunk (
  id           TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  paper_id     TEXT NOT NULL REFERENCES paper (id) ON DELETE CASCADE,
  page_start   INTEGER NOT NULL CHECK (page_start >= 1),
  page_end     INTEGER NOT NULL CHECK (page_end >= page_start),
  text         TEXT NOT NULL,
  embedding    BLOB,
  embed_status TEXT NOT NULL DEFAULT 'pending'
                 CHECK (embed_status IN ('pending', 'embedded', 'failed'))
);

CREATE INDEX text_chunk_by_paper ON text_chunk (paper_id, page_start);

CREATE TABLE chat_thread (
  id              TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  -- Empty for a chat across the whole library.
  paper_id        TEXT REFERENCES paper (id) ON DELETE CASCADE,
  scope           TEXT NOT NULL CHECK (scope IN ('paper', 'library')),
  -- Only used by library chats (FR-13.4).
  decision_filter TEXT CHECK (decision_filter IN ('all', 'include', 'undecided', 'exclude')),
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  -- A paper chat needs a paper; a library chat must not have one (PRD 7a).
  CHECK ((scope = 'paper') = (paper_id IS NOT NULL))
);

CREATE TABLE chat_message (
  id          TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  thread_id   TEXT NOT NULL REFERENCES chat_thread (id) ON DELETE CASCADE,
  role        TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
  content     TEXT NOT NULL,
  model_label TEXT,
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX chat_message_by_thread ON chat_message (thread_id, created_at);

-- AI suggestion history (FR-11.5). A suggestion never changes paper.decision.
CREATE TABLE suggestion (
  id                 TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  paper_id           TEXT NOT NULL REFERENCES paper (id) ON DELETE CASCADE,
  proposed_reason_id TEXT REFERENCES exclusion_reason (id) ON DELETE SET NULL,
  proposed_decision  TEXT NOT NULL CHECK (proposed_decision IN ('include', 'exclude')),
  rationale          TEXT NOT NULL,
  status             TEXT NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending', 'accepted', 'dismissed')),
  model_label        TEXT NOT NULL,
  created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX suggestion_by_paper ON suggestion (paper_id, created_at);

CREATE TABLE citation (
  id            TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  chunk_id      TEXT NOT NULL REFERENCES text_chunk (id) ON DELETE CASCADE,
  message_id    TEXT REFERENCES chat_message (id) ON DELETE CASCADE,
  suggestion_id TEXT REFERENCES suggestion (id) ON DELETE CASCADE,
  quote         TEXT NOT NULL,
  -- False: the quote was not found in the paper ("Unverified quote", FR-11.3).
  verified      INTEGER NOT NULL CHECK (verified IN (0, 1)),
  -- Exactly one of message_id and suggestion_id (PRD 7a).
  CHECK ((message_id IS NULL) <> (suggestion_id IS NULL))
);

CREATE INDEX citation_by_chunk ON citation (chunk_id);
CREATE INDEX citation_by_message ON citation (message_id);
CREATE INDEX citation_by_suggestion ON citation (suggestion_id);
