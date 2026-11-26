// `[#46]` links in notes (FR-05.5): one to nine digits, as src-tauri/src/ref_links.rs
// reads them. Also the texts of the links (Brief Sections 7 and 8, approved in T-27).

const LINK = /\[#(\d{1,9})\]/g;

/** The reference numbers a note links to, each once, in the order they appear. */
export function linkedNumbers(note: string): number[] {
  return [...new Set(Array.from(note.matchAll(LINK), (match) => Number(match[1])))];
}

/** The note with every link to `from` pointing to `to`. */
export function renumberLinks(note: string, from: number, to: number): string {
  return note.replace(LINK, (link, digits: string) => (Number(digits) === from ? `[#${to}]` : link));
}

export const noPaperMessage = (no: number) => `No paper with Reference No ${no}.`;

export function offerMessage(count: number, from: number, to: number): string {
  return count === 1
    ? `1 note links to #${from}. Update it to #${to}?`
    : `${count} notes link to #${from}. Update them to #${to}?`;
}

export const updatedMessage = (count: number) => `${count} ${count === 1 ? "note" : "notes"} updated.`;
