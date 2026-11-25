// Highlights (FR-03.1, FR-03.2): the 4 fixed colors, their texts, and how a text
// selection becomes rectangles in shares (0 to 1) of the page.
// Pure functions without the DOM or PDF.js, so they can be tested on their own.

import type { Highlight, HighlightColor, HighlightRect } from "../lib/api";

export interface HighlightKind {
  color: HighlightColor;
  /** The button label in the popover (O-03). */
  label: string;
  /** What the color means, for the mark's accessible name. */
  meaning: string;
  /** The design token family: --hl-<token>-fill and --hl-<token>-swatch. */
  token: "include" | "exclude" | "method" | "result";
  /** The key that picks it while the popover is open. */
  key: "1" | "2" | "3" | "4";
}

/** In popover order (Brief O-03). The meanings are fixed (FR-03.1). */
export const HIGHLIGHT_KINDS: readonly HighlightKind[] = [
  { color: "green", label: "Inclusion", meaning: "supports inclusion", token: "include", key: "1" },
  { color: "red", label: "Exclusion", meaning: "supports exclusion", token: "exclude", key: "2" },
  { color: "blue", label: "Method", meaning: "method", token: "method", key: "3" },
  { color: "yellow", label: "Result", meaning: "quantitative result", token: "result", key: "4" },
];

export function kindOf(color: HighlightColor): HighlightKind {
  return HIGHLIGHT_KINDS.find((kind) => kind.color === color) ?? HIGHLIGHT_KINDS[0];
}

/** "Highlight, supports exclusion, page 4" (Brief Section 10). */
export function highlightName(color: HighlightColor, page: number): string {
  return `Highlight, ${kindOf(color).meaning}, page ${page}`;
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

/** The highlights of each page, by page number (1-based). */
export function groupHighlights(highlights: Highlight[]): Map<number, Highlight[]> {
  const pages = new Map<number, Highlight[]>();
  for (const highlight of highlights) {
    const list = pages.get(highlight.page);
    if (list) list.push(highlight);
    else pages.set(highlight.page, [highlight]);
  }
  return pages;
}
