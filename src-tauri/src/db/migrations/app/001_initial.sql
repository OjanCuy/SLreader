-- App database, version 1. Lives in the app data folder and is shared by all libraries.
-- It holds only the list of library folders (no names or research questions:
-- those stay in each library's own database), the AI providers, and app settings.

CREATE TABLE library_registry (
  id             INTEGER PRIMARY KEY,
  folder_path    TEXT NOT NULL UNIQUE,
  last_opened_at TEXT,
  -- Set when the folder can no longer be found (FR-14.5).
  missing        INTEGER NOT NULL DEFAULT 0 CHECK (missing IN (0, 1))
);

-- There is deliberately no API key column: keys live only in the
-- OS keychain (FR-09.2).
CREATE TABLE ai_provider (
  id            INTEGER PRIMARY KEY,
  kind          TEXT NOT NULL CHECK (kind IN ('claude', 'openai_compatible')),
  name          TEXT NOT NULL,
  base_url      TEXT,
  model         TEXT NOT NULL,
  is_active     INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1)),
  consent_given INTEGER NOT NULL DEFAULT 0 CHECK (consent_given IN (0, 1))
);

-- Only one provider can be active at a time (FR-09.1).
CREATE UNIQUE INDEX ai_provider_one_active ON ai_provider (is_active) WHERE is_active = 1;

-- Keys: csv_delimiter, retrieval_chunks, view_mode, zoom, left_hidden, last_library_id (PRD 7a).
CREATE TABLE app_setting (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
