// UI texts of backups and restoring, O-05 and O-06 (Brief Section 7; the new ones were
// approved in T-32).

import type { RestorePreview, RestoreReport } from "./api";

export const BACKUP_ITEM = "Backup (JSON)";
export const RESTORE_ITEM = "Restore from backup…";
/** The button on the Libraries screen and on a damaged library's card. */
export const RESTORE_BUTTON = "Restore from backup";

export const NOT_A_BACKUP = "This file isn't a SLReader backup.";
export const NEWER_BACKUP =
  "This backup was made with a newer SLReader. Update the app to restore it.";
/** Papers per library (FR-01.4). */
export const PAPER_LIMIT = 250;
export const overLimitMessage = (papers: number) =>
  `Library limit is 250 papers. This backup would make ${papers}.`;

export const RESTORE_TITLE = "Restore from backup?";
export const RESTORE_CONFIRM = "Restore";

const counted = (count: number, one: string, many: string) =>
  `${count} ${count === 1 ? one : many}`;

/** "12 papers and 40 highlights in this library will be replaced …" */
export function replaceMessage({ replacedPapers, replacedHighlights }: RestorePreview): string {
  const parts = [
    replacedPapers > 0 ? counted(replacedPapers, "paper", "papers") : null,
    replacedHighlights > 0 ? counted(replacedHighlights, "highlight", "highlights") : null,
  ].filter((part) => part !== null);
  return `${parts.join(" and ")} in this library will be replaced with their versions from the backup. Everything else is kept.`;
}

/** "Backup restored. Added 2 exclusion reasons and 1 tag from the backup." */
export function restoredMessage({ addedReasons, addedTags }: RestoreReport): string {
  const added = [
    addedReasons > 0 ? counted(addedReasons, "exclusion reason", "exclusion reasons") : null,
    addedTags > 0 ? counted(addedTags, "tag", "tags") : null,
  ].filter((part) => part !== null);
  return added.length === 0
    ? "Backup restored."
    : `Backup restored. Added ${added.join(" and ")} from the backup.`;
}

/** The message for a backup that cannot be read or restored; `fallback` for the rest. */
export function backupErrorMessage(error: unknown, fallback: string): string {
  switch ((error as { code?: string } | null)?.code) {
    case "not_a_backup":
      return NOT_A_BACKUP;
    case "newer_backup":
      return NEWER_BACKUP;
    default:
      return fallback;
  }
}
