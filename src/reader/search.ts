// Text search in a PDF (FR-02.1): finds a query in the text of every page and says
// which text items, and which characters of them, each result covers.
// Pure functions without PDF.js, so they can be tested on their own.
//
// A search ignores upper and lower case, accents, ligatures ("ﬁ" is "fi"), the kind
// of quote mark, and line breaks. A word broken by a hyphen at the end of a line is
// found with and without the hyphen.

/** One piece of text as PDF.js delivers it, in reading order. */
export interface TextItem {
  str: string;
  /** The line ends after this item. */
  hasEOL: boolean;
}

/** A place in a page's text: an item and a position in its `str`. */
export interface Spot {
  item: number;
  offset: number;
}

export interface Match {
  /** 0-based. */
  page: number;
  start: Spot;
  /** Just after the last character. */
  end: Spot;
}

/** A page's text ready for searching. `from[i]` and `to[i]` locate `text[i]`. */
export interface PageIndex {
  text: string;
  from: Spot[];
  to: Spot[];
}

/** Stands for a hyphen at the end of a line: the query may contain it or not. */
const BREAK = "\u0001";
const HYPHENS = /[‐‑]/g;
const SINGLE_QUOTES = /[‘’‚‛′]/g;
const DOUBLE_QUOTES = /[“”„‟″]/g;

/** The searchable form of one character: may be empty or longer than the original. */
function fold(character: string): string {
  return character
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(HYPHENS, "-")
    .replace(SINGLE_QUOTES, "'")
    .replace(DOUBLE_QUOTES, '"');
}

interface Unit {
  /** One UTF-16 unit. */
  value: string;
  from: Spot;
  to: Spot;
}

function foldItems(items: TextItem[]): Unit[] {
  const units: Unit[] = [];
  items.forEach(({ str, hasEOL }, item) => {
    let offset = 0;
    for (const character of str) {
      const from = { item, offset };
      offset += character.length;
      const to = { item, offset };
      const folded = fold(character);
      for (let i = 0; i < folded.length; i++) units.push({ value: folded[i], from, to });
    }
    if (hasEOL) {
      const end = { item, offset };
      units.push({ value: "\n", from: end, to: end });
    }
  });
  return units;
}

const isSpace = (value: string | undefined) => value !== undefined && /\s/.test(value);
const isLetter = (value: string | undefined) => value !== undefined && /\p{L}/u.test(value);

export function indexPage(items: TextItem[]): PageIndex {
  const units = foldItems(items);
  const page: PageIndex = { text: "", from: [], to: [] };
  const push = (value: string, unit: Unit) => {
    page.text += value;
    page.from.push(unit.from);
    page.to.push(unit.to);
  };

  for (let i = 0; i < units.length; i++) {
    const unit = units[i];
    const softHyphen = unit.value === "­";
    if ((softHyphen || unit.value === "-") && isLetter(page.text.at(-1))) {
      // Is this the end of a line, with the word going on in the next one?
      let next = i + 1;
      let lineEnds = false;
      while (isSpace(units[next]?.value)) {
        if (units[next].value === "\n") lineEnds = true;
        next++;
      }
      if (lineEnds && isLetter(units[next]?.value)) {
        push(BREAK, unit);
        i = next - 1;
        continue;
      }
    }
    // A soft hyphen elsewhere is invisible.
    if (softHyphen) continue;
    if (isSpace(unit.value)) {
      if (page.text.length > 0 && !page.text.endsWith(" ")) push(" ", unit);
      continue;
    }
    push(unit.value, unit);
  }
  return page;
}

/** The query as a pattern over `PageIndex.text`; null when there is nothing to find. */
function pattern(query: string): RegExp | null {
  const folded = Array.from(query, fold).join("").replace(/­/g, "").replace(/\s+/g, " ").trim();
  if (folded === "") return null;
  const parts = Array.from(folded, (character) =>
    character === "-" ? `[-${BREAK}]` : character.replace(/[.*+?^${}()|[\]\\]/, "\\$&"),
  );
  return new RegExp(parts.join(`${BREAK}?`), "gu");
}

/** All results in reading order, page by page. Results do not overlap. */
export function findMatches(pages: PageIndex[], query: string): Match[] {
  const wanted = pattern(query);
  if (!wanted) return [];
  return pages.flatMap(({ text, from, to }, page) =>
    Array.from(text.matchAll(wanted), (found) => ({
      page,
      start: from[found.index],
      end: to[found.index + found[0].length - 1],
    })),
  );
}

/** The results of each page that has any. */
export function groupByPage(matches: Match[]): Map<number, Match[]> {
  const pages = new Map<number, Match[]>();
  for (const match of matches) {
    const list = pages.get(match.page);
    if (list) list.push(match);
    else pages.set(match.page, [match]);
  }
  return pages;
}

/** The parts of single items that a result covers, to draw it over the text layer. */
export function segments(
  match: Match,
  lengthOf: (item: number) => number,
): { item: number; from: number; to: number }[] {
  const parts = [];
  for (let item = match.start.item; item <= match.end.item; item++) {
    const from = item === match.start.item ? match.start.offset : 0;
    const to = item === match.end.item ? match.end.offset : lengthOf(item);
    if (to > from) parts.push({ item, from, to });
  }
  return parts;
}

/** Where a new search starts: the first result on `page` (0-based) or after it. */
export function firstMatchFrom(matches: Match[], page: number): number {
  const index = matches.findIndex((match) => match.page >= page);
  return Math.max(0, index);
}

/** The next (+1) or previous (-1) result, going round at both ends. */
export function stepMatch(current: number, step: 1 | -1, total: number): number {
  return total > 0 ? (current + step + total) % total : 0;
}
