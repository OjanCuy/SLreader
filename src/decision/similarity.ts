// Ordering the reference list by how much a title looks like a paper's file name
// (FR-05.3). Pure functions.

import type { ReferenceChoice } from "../lib/api";

/** Words too common to say anything about which paper it is. */
const COMMON = new Set([
  "the", "of", "and", "for", "in", "on", "to", "with", "an", "by", "from",
  "dan", "di", "ke", "yang", "untuk", "pada", "dengan", "dari", "terhadap",
]);

/** Lower case, without accents. */
export function plain(text: string): string {
  return text.normalize("NFKD").replace(/\p{M}/gu, "").toLowerCase();
}

/** The telling words of a text, without a plural "s". */
function words(text: string): Set<string> {
  return new Set(
    plain(text)
      .split(/[^\p{L}\p{N}]+/u)
      .filter((word) => word.length > 1 && !COMMON.has(word))
      .map((word) => (word.length > 3 && word.endsWith("s") ? word.slice(0, -1) : word)),
  );
}

export interface RankedReference {
  reference: ReferenceChoice;
  /** The most similar reference that this paper can be linked to. */
  best: boolean;
}

/**
 * The references, most similar to `fileName` first; equal ones keep the order of the
 * list. The share of words in common counts, and the year in the name breaks near ties.
 */
export function rankReferences(
  references: ReferenceChoice[],
  fileName: string,
  paperId: string,
): RankedReference[] {
  const name = words(fileName.replace(/\.pdf$/i, ""));
  const scored = references.map((reference, index) => {
    const title = words(reference.title);
    const shared = [...title].filter((word) => name.has(word)).length;
    const total = title.size + name.size;
    const year = reference.year !== null && name.has(String(reference.year)) ? 0.05 : 0;
    return {
      reference,
      index,
      score: (total === 0 ? 0 : (2 * shared) / total) + year,
      // One word in common says too little, unless it is the whole title.
      likely: shared >= 2 || (shared === 1 && title.size === 1),
    };
  });
  scored.sort((a, b) => b.score - a.score || a.index - b.index);
  const best = scored.find(
    ({ likely, reference }) =>
      likely && (reference.linkedPaperId === null || reference.linkedPaperId === paperId),
  );
  return scored.map((each) => ({ reference: each.reference, best: each === best }));
}

/** Whether every typed word occurs in the number or the title. */
export function matchesQuery(reference: ReferenceChoice, query: string): boolean {
  const text = plain(`${reference.no} ${reference.title}`);
  return plain(query)
    .split(/\s+/)
    .every((word) => text.includes(word));
}
