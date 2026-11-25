// The only place the UI talks to the Rust core.
// Every Tauri command gets a typed wrapper here, so components never call `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
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
