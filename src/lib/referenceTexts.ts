// UI texts of O-04 "Import references" (Brief Section 7, approved in T-25).

export const NO_ROWS = "No reference rows found.";

export function importedMessage(imported: number, skipped: number): string {
  return `${imported} ${imported === 1 ? "row" : "rows"} imported, ${skipped} skipped.`;
}

/** How many repeated numbers are named before "and N more". */
const NAMED_DUPLICATES = 5;

export function duplicatesMessage(numbers: number[]): string {
  const named = numbers.slice(0, NAMED_DUPLICATES).join(", ");
  const more = numbers.length - NAMED_DUPLICATES;
  return `Duplicate Reference No: ${named}${more > 0 ? ` and ${more} more` : ""}.`;
}

export function replaceMessage(existing: number): string {
  const list = existing === 1 ? "reference" : "references";
  return `The current ${existing} ${list} will be replaced. Papers keep their metadata, but their links to the old references are removed.`;
}
