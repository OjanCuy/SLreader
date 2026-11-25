// Highlights (FR-03.1, FR-03.2): the library's categories, their texts, and how a text
// selection becomes rectangles in shares (0 to 1) of the page.
// Pure functions without the DOM or PDF.js, so they can be tested on their own.

import type { Highlight, HighlightCategory, HighlightRect, PaletteColor } from "../lib/api";

/** A library has 1 to 8 highlight categories (FR-08.6); keys 1–8 pick them. */
export const MAX_CATEGORIES = 8;

/** The key that picks the category at this place in the popover, "1" to "8". */
export function categoryKey(index: number): string | null {
  return index >= 0 && index < MAX_CATEGORIES ? String(index + 1) : null;
}

/** The category a key picks, if any. */
export function categoryForKey(categories: HighlightCategory[], key: string): HighlightCategory | null {
  const index = categories.findIndex((_, place) => categoryKey(place) === key);
  return index === -1 ? null : categories[index];
}

/** "Highlight, Exclusion, page 4": the category's own name (Brief Section 10). */
export function highlightName(label: string, page: number): string {
  return `Highlight, ${label}, page ${page}`;
}

/** A highlight as a page draws it. */
export interface PageMark {
  id: string;
  color: PaletteColor;
  /** Accessible name. */
  name: string;
  rects: HighlightRect[];
}

/**
 * The marks of each page, by page number (1-based). A highlight whose category is
 * not in the list is left out (the categories could not be read).
 */
export function pageMarks(highlights: Highlight[], categories: HighlightCategory[]): Map<number, PageMark[]> {
  const byId = new Map(categories.map((category) => [category.id, category]));
  const pages = new Map<number, PageMark[]>();
  for (const highlight of highlights) {
    const category = byId.get(highlight.categoryId);
    if (!category) continue;
    const mark: PageMark = {
      id: highlight.id,
      color: category.color,
      name: highlightName(category.label, highlight.page),
      rects: highlight.rects,
    };
    const list = pages.get(highlight.page);
    if (list) list.push(mark);
    else pages.set(highlight.page, [mark]);
  }
  return pages;
}

/** FR-03.5, PRD Section 8. */
export function crossPageMessage(page: number): string {
  return `Highlights can't cross pages; saved on page ${page}.`;
}

/** A rectangle on screen, in CSS pixels. */
export interface ScreenRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Smaller pieces, in CSS pixels, are left out: they are empty line ends, not text. */
const MIN_SIZE = 0.5;

/**
 * Joins the pieces of a selection that lie on the same line and touch or overlap, so a
 * line gets one rectangle and overlapping pieces do not darken each other.
 */
export function mergeLines(rects: ScreenRect[]): ScreenRect[] {
  const sorted = rects
    .filter((rect) => rect.width >= MIN_SIZE && rect.height >= MIN_SIZE)
    .sort((a, b) => a.top - b.top || a.left - b.left);
  const merged: ScreenRect[] = [];
  for (const rect of sorted) {
    const line = merged.find((other) => sameLine(other, rect) && touches(other, rect));
    if (!line) {
      merged.push({ ...rect });
      continue;
    }
    const right = Math.max(line.left + line.width, rect.left + rect.width);
    const bottom = Math.max(line.top + line.height, rect.top + rect.height);
    line.left = Math.min(line.left, rect.left);
    line.top = Math.min(line.top, rect.top);
    line.width = right - line.left;
    line.height = bottom - line.top;
  }
  return merged;
}

/** At least half of the shorter one overlaps vertically. */
function sameLine(a: ScreenRect, b: ScreenRect): boolean {
  const overlap = Math.min(a.top + a.height, b.top + b.height) - Math.max(a.top, b.top);
  return overlap >= 0.5 * Math.min(a.height, b.height);
}

/** The space between them is at most about one character wide. */
function touches(a: ScreenRect, b: ScreenRect): boolean {
  const gap = Math.max(a.left, b.left) - Math.min(a.left + a.width, b.left + b.width);
  return gap <= 0.6 * Math.min(a.height, b.height);
}

/**
 * A rectangle on screen as shares of the page shown in `page`, cut to the page.
 * Null when nothing of it is on the page.
 */
export function toShares(rect: ScreenRect, page: ScreenRect): HighlightRect | null {
  if (page.width <= 0 || page.height <= 0) return null;
  const clamp = (value: number) => Math.min(1, Math.max(0, value));
  const x = clamp((rect.left - page.left) / page.width);
  const y = clamp((rect.top - page.top) / page.height);
  const right = clamp((rect.left + rect.width - page.left) / page.width);
  const bottom = clamp((rect.top + rect.height - page.top) / page.height);
  if (right <= x || bottom <= y) return null;
  return { x, y, w: right - x, h: bottom - y };
}

/** The selected text as saved: one space between words, also across line ends. */
export function cleanText(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}
