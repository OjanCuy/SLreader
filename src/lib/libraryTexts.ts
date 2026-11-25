// Texts for the Libraries screen (S-01). Do not reword them: they come from the design
// brief, Section 7. Problem messages for a library are in libraryProblems.ts.

import type { LibraryError } from "./api";
import { LIBRARY_PROBLEMS } from "./libraryProblems";

export const GENERIC_ERROR = "Something went wrong. Try again.";
export const SAVE_ERROR = "Couldn't save: the disk is full or the folder is read-only.";
export const CLOUD_WARNING =
  "This folder is synced to the cloud. Syncing can damage the library database while SLReader is open.";
export const NO_LIBRARY_HERE = "No SLReader library in this folder.";
export const ALREADY_A_LIBRARY =
  "This folder already has a library. Use Open library instead.";

/** What the user was doing when the command failed. */
export type LibraryAction = "open" | "locate" | "create" | "rename" | "remove" | "load";

/** Turns a failed command into the message to show. The technical detail is never shown. */
export function libraryErrorMessage(error: unknown, action: LibraryAction): string {
  switch ((error as Partial<LibraryError> | null)?.code) {
    case "damaged":
      return LIBRARY_PROBLEMS.damaged.message;
    case "newer_version":
      return LIBRARY_PROBLEMS.newer_version.message;
    case "sync_conflict":
      return LIBRARY_PROBLEMS.sync_conflict.message;
    case "already_exists":
      return ALREADY_A_LIBRARY;
    case "not_found":
      if (action === "open" || action === "locate") return NO_LIBRARY_HERE;
      return LIBRARY_PROBLEMS.not_found.message;
    default:
      return action === "create" || action === "rename" ? SAVE_ERROR : GENERIC_ERROR;
  }
}

export function paperCountText(count: number): string {
  return count === 1 ? "1 paper" : `${count} papers`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "Last opened 25 Nov 2026", in the computer's time zone. */
export function lastOpenedText(isoTime: string): string {
  const date = new Date(isoTime);
  return `Last opened ${date.getDate()} ${MONTHS[date.getMonth()]} ${date.getFullYear()}`;
}

/** Name to show for a library; the folder name stands in when it cannot be read. */
export function libraryTitle(library: { name: string | null; folderPath: string }): string {
  if (library.name !== null) return library.name;
  const parts = library.folderPath.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? library.folderPath;
}
