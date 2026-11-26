// The only place the UI talks to the Rust core.
// Every Tauri command gets a typed wrapper here, so components never call `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { LibraryProblem } from "./libraryProblems";

export interface AppInfo {
  name: string;
  version: string;
}

export function getAppInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

// ---- Libraries (FR-14.1 to FR-14.3). Types mirror src-tauri/src/libraries.rs. ----

export interface LibrarySummary {
  id: number;
  folderPath: string;
  /** Null when the library cannot be read (missing, damaged, ...). */
  name: string | null;
  paperCount: number | null;
  /** UTC, ISO 8601. */
  lastOpenedAt: string | null;
  problem: LibraryProblem | null;
  /** The folder is synced to the cloud: show a warning. */
  cloudSynced: boolean;
  /** Set only on the result of opening a library; data commands will require it. */
  session: number | null;
}

/** The commands below reject with this shape. */
export interface LibraryError {
  code:
    | "not_found"
    | "damaged"
    | "newer_version"
    | "sync_conflict"
    | "already_exists"
    | "invalid_name"
    | "not_registered"
    | "not_active"
    | "other";
  /** Technical detail. Do not show it to the user. */
  detail: string;
}

export function listLibraries(): Promise<LibrarySummary[]> {
  return invoke("list_libraries");
}

export function createLibrary(folderPath: string, name: string): Promise<LibrarySummary> {
  return invoke("create_library", { folderPath, name });
}

/** "Open library": registers an existing library folder and makes it active. */
export function openLibraryFolder(folderPath: string): Promise<LibrarySummary> {
  return invoke("open_library_folder", { folderPath });
}

export function openLibrary(id: number): Promise<LibrarySummary> {
  return invoke("open_library", { id });
}

export function renameLibrary(id: number, name: string): Promise<void> {
  return invoke("rename_library", { id, name });
}

/** "Remove from list". The library folder and its files are kept. */
export function removeLibrary(id: number): Promise<void> {
  return invoke("remove_library", { id });
}

/** "Locate folder": points a missing library to the folder it moved to. */
export function locateLibrary(id: number, folderPath: string): Promise<void> {
  return invoke("locate_library", { id, folderPath });
}

/** Shows the system folder picker. Resolves to null when the user cancels. */
export function pickFolder(): Promise<string | null> {
  return invoke("pick_folder");
}

/** Whether to warn that a chosen folder is synced to the cloud. */
export function isCloudFolder(folderPath: string): Promise<boolean> {
  return invoke("is_cloud_folder", { folderPath });
}

// ---- Papers (FR-01.2 to FR-01.5). Types mirror src-tauri/src/papers.rs. ----

export type ImportOutcome =
  | "added"
  | "attached"
  | "already_in_library"
  | "identical"
  | "not_pdf"
  | "over_limit"
  | "write_failed";

export interface ImportReport {
  files: {
    fileName: string;
    /** The file that was given, to import it again after "Add anyway". */
    path: string;
    outcome: ImportOutcome;
    /** Set for "identical": the paper's file with the same content. */
    identicalTo: string | null;
  }[];
  /** Files that are now in the library, attached ones included. */
  added: number;
}

export interface PaperListItem {
  id: string;
  /** Null for a paper restored from a backup that never had a file name. */
  fileName: string | null;
  title: string;
  fileMissing: boolean;
  textStatus: "pending" | "text_ready" | "no_text";
  /** The text is being extracted in the background ("Extracting text…"). */
  extracting: boolean;
}

/** Shows the system file picker for PDFs. Empty when the user cancels. */
export function pickPdfs(): Promise<string[]> {
  return invoke("pick_pdfs");
}

export function importPdfs(
  session: number,
  paths: string[],
  allowIdentical = false,
): Promise<ImportReport> {
  return invoke("import_pdfs", { session, paths, allowIdentical });
}

export function listPapers(session: number): Promise<PaperListItem[]> {
  return invoke("list_papers", { session });
}

/**
 * Deletes a paper, its PDF copy, and everything that belongs to it (FR-01.7).
 * Only call it after the user has confirmed.
 */
export function deletePaper(session: number, paperId: string): Promise<void> {
  return invoke("delete_paper", { session, paperId });
}

// ---- Reader (FR-02.1). ----

/**
 * The bytes of a paper's PDF. Rejects with code "not_found" when the file is not in
 * the library folder ("PDF file is missing.").
 */
export function readPaperPdf(session: number, paperId: string): Promise<ArrayBuffer> {
  return invoke("read_paper_pdf", { session, paperId });
}

/** The page (1-based) the paper was last read at (FR-02.3). */
export function getLastPage(session: number, paperId: string): Promise<number> {
  return invoke("get_last_page", { session, paperId });
}

/** Remembers the page being read. Rejects for a read-only library. */
export function saveLastPage(session: number, paperId: string, page: number): Promise<void> {
  return invoke("save_last_page", { session, paperId, page });
}

/** How PDF pages are shown (FR-02.2). One setting for the whole app. */
export type PageMode = "normal" | "dark" | "sepia";

export function getPageMode(): Promise<PageMode> {
  return invoke("get_page_mode");
}

export function setPageMode(mode: PageMode): Promise<void> {
  return invoke("set_page_mode", { mode });
}

/**
 * The user opened a password-protected paper with this password: its text is
 * extracted once with it. The password is never stored. Rejects for a read-only library.
 */
export function extractLockedPaperText(session: number, paperId: string, password: string): Promise<void> {
  return invoke("extract_locked_paper_text", { session, paperId, password });
}

// ---- Highlights (FR-03.1, FR-03.2). Types mirror src-tauri/src/highlights.rs. ----

/** The highlight palette (FR-08.6). Each color is used by one category of a library at most. */
export type PaletteColor = "green" | "red" | "blue" | "yellow" | "purple" | "orange" | "pink" | "gray";

/** A highlight category of the open library: 1 to 8 per library, in their order. */
export interface HighlightCategory {
  id: string;
  label: string;
  color: PaletteColor;
  sortOrder: number;
}

/** An area of a page as shares (0 to 1): left, top, width, height. */
export interface HighlightRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface NewHighlight {
  /** 1-based. */
  page: number;
  categoryId: string;
  text: string;
  rects: HighlightRect[];
}

export interface Highlight extends NewHighlight {
  id: string;
  note: string;
  extractionTagId: string | null;
  /** UTC, ISO 8601. */
  createdAt: string;
}

/** The highlight categories of the open library, in their order (FR-03.1). */
export function listHighlightCategories(session: number): Promise<HighlightCategory[]> {
  return invoke("list_highlight_categories", { session });
}

/** The highlights of a paper, by page. */
export function listHighlights(session: number, paperId: string): Promise<Highlight[]> {
  return invoke("list_highlights", { session, paperId });
}

/** Saves a new highlight. Rejects for a read-only library. */
export function addHighlight(session: number, paperId: string, highlight: NewHighlight): Promise<Highlight> {
  return invoke("add_highlight", { session, paperId, highlight });
}

/** What the user can change on a highlight (FR-03.3); all of it is sent every time. */
export interface HighlightEdit {
  categoryId: string;
  /** Null is "No tag". */
  extractionTagId: string | null;
  note: string;
}

/** Rejects for a read-only library or a highlight that no longer exists. */
export function updateHighlight(session: number, highlightId: string, edit: HighlightEdit): Promise<void> {
  return invoke("update_highlight", { session, highlightId, edit });
}

/** Deletes a highlight and its note. Only call it after the user has confirmed. */
export function deleteHighlight(session: number, highlightId: string): Promise<void> {
  return invoke("delete_highlight", { session, highlightId });
}

export interface ExtractionTag {
  id: string;
  label: string;
}

/** The extraction tags of the open library, in their order (FR-08.2). */
export function listExtractionTags(session: number): Promise<ExtractionTag[]> {
  return invoke("list_extraction_tags", { session });
}

// ---- Screening (FR-04.1 to FR-04.4). Types mirror src-tauri/src/decisions.rs. ----

export type Decision = "undecided" | "include" | "exclude";

export interface Screening {
  decision: Decision;
  /** Only counts while the decision is "exclude" (FR-04.3). */
  exclusionReasonId: string | null;
  assessmentNote: string;
}

export function getScreening(session: number, paperId: string): Promise<Screening> {
  return invoke("get_screening", { session, paperId });
}

/** Only for the user's own decision action: nothing else may change a decision (FR-11.4). */
export function setDecision(session: number, paperId: string, decision: Decision): Promise<void> {
  return invoke("set_decision", { session, paperId, decision });
}

/** Null is "No reason". */
export function setExclusionReason(session: number, paperId: string, reasonId: string | null): Promise<void> {
  return invoke("set_exclusion_reason", { session, paperId, reasonId });
}

export function setAssessmentNote(session: number, paperId: string, note: string): Promise<void> {
  return invoke("set_assessment_note", { session, paperId, note });
}

export interface ExclusionReason {
  id: string;
  code: string;
  label: string;
}

/** The exclusion reasons of the open library, in their order (FR-08.1). */
export function listExclusionReasons(session: number): Promise<ExclusionReason[]> {
  return invoke("list_exclusion_reasons", { session });
}

export interface Summary {
  papers: number;
  include: number;
  exclude: number;
  undecided: number;
  /** Excluded papers without a reason (FR-04.2). */
  noReason: number;
  /** Every reason of the library, in its order; counts only excluded papers (FR-04.3). */
  reasons: (ExclusionReason & { count: number })[];
}

/** The counts of the open library for the summary bar (FR-04.5). */
export function getSummary(session: number): Promise<Summary> {
  return invoke("get_summary", { session });
}

// ---- Metadata (FR-05.1, FR-05.4). Types mirror src-tauri/src/metadata.rs. ----

export interface Metadata {
  refNo: number | null;
  title: string;
  year: number | null;
  /** Empty when not filled in, like `doi`. */
  sourceDatabase: string;
  doi: string;
}

export interface PaperMetadata {
  metadata: Metadata;
  /** Other papers with the same reference number: their file name, or title. */
  sameRefNo: string[];
}

export function getMetadata(session: number, paperId: string): Promise<PaperMetadata> {
  return invoke("get_metadata", { session, paperId });
}

/** Saves all fields; resolves to the other papers with the same reference number. */
export function setMetadata(session: number, paperId: string, metadata: Metadata): Promise<string[]> {
  return invoke("set_metadata", { session, paperId, metadata });
}

// ---- Reference list (FR-05.2). Types mirror src-tauri/src/references.rs. ----

export type ReferenceImport =
  /** No usable row; the old list is unchanged. */
  | { status: "empty" }
  /** A list exists: nothing changed, ask the user before replacing it. */
  | { status: "needs_confirmation"; existing: number }
  | { status: "imported"; imported: number; skipped: number; duplicates: number[] };

/** `replace` only after the user has confirmed replacing the existing list. */
export function importReferences(session: number, text: string, replace = false): Promise<ReferenceImport> {
  return invoke("import_references", { session, text, replace });
}

export interface ReferenceFile {
  fileName: string;
  /** Null when the file cannot be read as text. */
  text: string | null;
}

/** Shows the system file picker for a CSV file and reads it. Null when the user cancels. */
export function pickReferenceFile(): Promise<ReferenceFile | null> {
  return invoke("pick_reference_file");
}

// ---- Library criteria (FR-08.1, FR-08.2, FR-08.4). Types mirror src-tauri/src/criteria.rs. ----

/** Inclusion criteria, exclusion reasons, or extraction tags. */
export type CriteriaKind = "inclusion" | "exclusion" | "tag";

export interface Criterion {
  id: string;
  /** Empty for a tag. */
  code: string;
  /** The description, or the tag name. */
  label: string;
}

export function listCriteria(session: number, kind: CriteriaKind): Promise<Criterion[]> {
  return invoke("list_criteria", { session, kind });
}

/** Resolves to the id of the new item, added at the end of its list. */
export function addCriterion(session: number, kind: CriteriaKind, code: string, label: string): Promise<string> {
  return invoke("add_criterion", { session, kind, code, label });
}

export function updateCriterion(
  session: number,
  kind: CriteriaKind,
  id: string,
  code: string,
  label: string,
): Promise<void> {
  return invoke("update_criterion", { session, kind, id, code, label });
}

/** How many papers use an exclusion reason, or highlights a tag. */
export function countCriterionUses(session: number, kind: CriteriaKind, id: string): Promise<number> {
  return invoke("count_criterion_uses", { session, kind, id });
}

/** Only call it after the user has confirmed. Papers and highlights are kept. */
export function deleteCriterion(session: number, kind: CriteriaKind, id: string): Promise<void> {
  return invoke("delete_criterion", { session, kind, id });
}

/** `ids` names every item of the list, in the new order. */
export function reorderCriteria(session: number, kind: CriteriaKind, ids: string[]): Promise<void> {
  return invoke("reorder_criteria", { session, kind, ids });
}

export function getResearchQuestions(session: number): Promise<string> {
  return invoke("get_research_questions", { session });
}

export function setResearchQuestions(session: number, text: string): Promise<void> {
  return invoke("set_research_questions", { session, text });
}

/** A reference of the list, with the paper it is linked to. */
export interface ReferenceChoice {
  id: string;
  no: number;
  title: string;
  year: number | null;
  linkedPaperId: string | null;
  /** That paper's file name, or its title when it has no file. */
  linkedPaperName: string | null;
}

/** The reference list in the order it was imported (FR-05.3). */
export function listReferences(session: number): Promise<ReferenceChoice[]> {
  return invoke("list_references", { session });
}

export type ReferenceLink =
  /** Another paper uses the reference; nothing changed. */
  | { status: "taken"; paper: string }
  /** Filled fields would change: ask the user, then link with `replace`. */
  | { status: "needs_confirmation" }
  | { status: "linked"; metadata: Metadata; sameRefNo: string[] };

/** `replace` only after the user has confirmed replacing the paper's metadata. */
export function linkReference(
  session: number,
  paperId: string,
  referenceId: string,
  replace = false,
): Promise<ReferenceLink> {
  return invoke("link_reference", { session, paperId, referenceId, replace });
}

/** The paper keeps its metadata. */
export function unlinkReference(session: number, paperId: string): Promise<void> {
  return invoke("unlink_reference", { session, paperId });
}

// ---- [#n] links in notes (FR-05.5). ----

/** The paper a `[#n]` link opens; null when no paper has the number. */
export function paperForRefNo(session: number, refNo: number): Promise<string | null> {
  return invoke("paper_for_ref_no", { session, refNo });
}

/** How many notes an update would change; 0 when there is nothing to offer. */
export function countNoteLinks(session: number, from: number, to: number): Promise<number> {
  return invoke("count_note_links", { session, from, to });
}

/** Only after the user has confirmed. Resolves to how many notes changed. */
export function updateNoteLinks(session: number, from: number, to: number): Promise<number> {
  return invoke("update_note_links", { session, from, to });
}

// ---- Exports (FR-06.1 to FR-06.3). ----

/**
 * Asks where to save and writes the decisions as CSV. Resolves to the file name, or
 * null when the user cancels.
 */
export function exportDecisions(session: number): Promise<string | null> {
  return invoke("export_decisions", { session });
}

/** The same for the extraction: one column per tag, one row per paper. */
export function exportExtraction(session: number): Promise<string | null> {
  return invoke("export_extraction", { session });
}

export interface MarkdownExport {
  /** The new folder inside the chosen one, e.g. "highlights". */
  folderName: string;
  fileCount: number;
}

/**
 * Asks for a folder and writes one Markdown file per paper into a new folder inside
 * it. Null when the user cancels.
 */
export function exportMarkdown(session: number): Promise<MarkdownExport | null> {
  return invoke("export_markdown", { session });
}

/** Shows the system file picker for one PDF. Null when the user cancels. */
export function pickPdf(): Promise<string | null> {
  return invoke("pick_pdf");
}

/**
 * "Attach PDF": copies a PDF into the library as the file of a paper whose file is
 * missing. Resolves to "attached", "already_in_library", "not_pdf", or "write_failed".
 */
export function attachPdf(session: number, paperId: string, path: string): Promise<ImportOutcome> {
  return invoke("attach_pdf", { session, paperId, path });
}

export type FileDrag =
  | { type: "enter"; paths: string[] }
  | { type: "drop"; paths: string[] }
  | { type: "leave" };

/** Files dragged over the window, with their paths. Returns a function that stops listening. */
export function onFileDrag(handler: (event: FileDrag) => void): Promise<() => void> {
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === "enter" || payload.type === "drop") {
      handler({ type: payload.type, paths: payload.paths });
    } else if (payload.type === "leave") {
      handler({ type: "leave" });
    }
  });
}

/**
 * Called with the session of a library each time the background extraction has
 * finished a paper of it. Returns a function that stops listening.
 */
export function onPaperTextChanged(handler: (session: number) => void): Promise<() => void> {
  return listen<number>("paper-text-changed", (event) => handler(event.payload));
}
