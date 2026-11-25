// Reads the user's text selection in the reader for a new highlight (FR-03.1, FR-03.2).
//
// Pages are the elements with `data-page` (PageView); their text is in the PDF.js
// text layer. A selection over several pages keeps only the part on the first page
// with selected text (FR-03.5), and says so with `crossed`.

import type { HighlightRect } from "../lib/api";
import { cleanText, mergeLines, toShares, type ScreenRect } from "./highlights";

export interface PageSelection {
  /** 1-based. */
  page: number;
  text: string;
  /** Shares of the page, one per line. */
  rects: HighlightRect[];
  /** The selection went on to later pages, which are left out. */
  crossed: boolean;
  /** Where the selection is on screen, for the popover. */
  bounds: { left: number; top: number; right: number; bottom: number };
}

interface PagePart {
  page: number;
  text: string;
  screen: ScreenRect[];
  frame: DOMRect;
}

export function readSelection(selection: Selection | null, root: HTMLElement): PageSelection | null {
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);

  const parts: PagePart[] = [];
  for (const page of root.querySelectorAll<HTMLElement>("[data-page]")) {
    const layer = page.querySelector<HTMLElement>(".textLayer");
    if (!layer || !range.intersectsNode(layer)) continue;
    const part = readPage(range, layer);
    if (part.text !== "" && part.screen.length > 0) {
      parts.push({ ...part, page: Number(page.dataset.page), frame: page.getBoundingClientRect() });
    }
  }
  const first = parts[0];
  if (!first) return null;

  const lines = mergeLines(first.screen);
  const rects = lines.map((line) => toShares(line, first.frame)).filter((rect) => rect !== null);
  if (rects.length === 0) return null;
  return {
    page: first.page,
    text: first.text,
    rects,
    crossed: parts.length > 1,
    bounds: {
      left: Math.min(...lines.map((line) => line.left)),
      top: Math.min(...lines.map((line) => line.top)),
      right: Math.max(...lines.map((line) => line.left + line.width)),
      bottom: Math.max(...lines.map((line) => line.top + line.height)),
    },
  };
}

/** The selected text of one text layer, and its pieces on screen. */
function readPage(range: Range, layer: HTMLElement): { text: string; screen: ScreenRect[] } {
  const walker = document.createTreeWalker(layer, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
  let text = "";
  const screen: ScreenRect[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    // A line end in the text layer: words on both sides stay apart.
    if (node.nodeName === "BR") {
      if (text !== "") text += " ";
      continue;
    }
    if (node.nodeType !== Node.TEXT_NODE || !range.intersectsNode(node)) continue;
    if (node.parentElement?.closest(".endOfContent")) continue;
    const part = document.createRange();
    part.selectNodeContents(node);
    // Only the selected part of the first and last text pieces.
    if (range.compareBoundaryPoints(Range.START_TO_START, part) > 0) {
      part.setStart(range.startContainer, range.startOffset);
    }
    if (range.compareBoundaryPoints(Range.END_TO_END, part) < 0) {
      part.setEnd(range.endContainer, range.endOffset);
    }
    if (part.collapsed) continue;
    text += part.toString();
    for (const rect of Array.from(part.getClientRects())) {
      screen.push({ left: rect.left, top: rect.top, width: rect.width, height: rect.height });
    }
  }
  return { text: cleanText(text), screen };
}
