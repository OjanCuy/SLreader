// The only place the UI talks to the Rust core.
// Every Tauri command gets a typed wrapper here, so components never call `invoke` directly.
import { invoke } from "@tauri-apps/api/core";
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
}

/** The commands below reject with this shape. */
export interface LibraryError {
  code:
    | "not_found"
    | "damaged"
    | "newer_version"
    | "already_exists"
    | "invalid_name"
    | "not_registered"
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
