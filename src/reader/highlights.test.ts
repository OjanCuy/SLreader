import { describe, expect, it } from "vitest";
import {
  cleanText,
  crossPageMessage,
  groupHighlights,
  HIGHLIGHT_KINDS,
  highlightName,
  mergeLines,
  toShares,
} from "./highlights";
import type { Highlight } from "../lib/api";

describe("highlight colors (FR-03.1)", () => {
  it("has 4 fixed colors with their labels and keys, in popover order", () => {
    expect(HIGHLIGHT_KINDS.map((kind) => [kind.color, kind.label, kind.key])).toEqual([
      ["green", "Inclusion", "1"],
      ["red", "Exclusion", "2"],
      ["blue", "Method", "3"],
      ["yellow", "Result", "4"],
    ]);
  });

  it("names a mark by what its color means and its page", () => {
    expect(highlightName("red", 4)).toBe("Highlight, supports exclusion, page 4");
    expect(highlightName("green", 1)).toBe("Highlight, supports inclusion, page 1");
    expect(highlightName("blue", 2)).toBe("Highlight, method, page 2");
    expect(highlightName("yellow", 9)).toBe("Highlight, quantitative result, page 9");
  });

  it("says a highlight can't cross pages (PRD Section 8)", () => {
    expect(crossPageMessage(4)).toBe("Highlights can't cross pages; saved on page 4.");
  });
});

describe("selection rectangles (FR-03.2)", () => {
  const rect = (left: number, top: number, width: number, height: number) => ({ left, top, width, height });

  it("joins the pieces of one line and keeps lines apart", () => {
    const merged = mergeLines([
      rect(100, 200, 40, 12),
      rect(140, 200, 4, 12), // a space
      rect(144, 201, 60, 11),
      rect(100, 216, 80, 12), // the next line
    ]);
    expect(merged).toEqual([rect(100, 200, 104, 12), rect(100, 216, 80, 12)]);
  });

  it("does not join two columns on the same line", () => {
    const merged = mergeLines([rect(100, 200, 200, 12), rect(400, 200, 200, 12)]);
    expect(merged).toHaveLength(2);
  });

  it("joins overlapping pieces, so they do not darken each other", () => {
    expect(mergeLines([rect(100, 200, 50, 12), rect(120, 200, 50, 12)])).toEqual([rect(100, 200, 70, 12)]);
  });

  it("drops empty pieces", () => {
    expect(mergeLines([rect(100, 200, 0, 12), rect(100, 200, 10, 0.2)])).toEqual([]);
  });

  it("measures in shares of the page, the same at 100% and 200%", () => {
    const at100 = toShares(rect(110, 250, 300, 15), rect(10, 50, 816, 1056));
    const at200 = toShares(rect(220, 450, 600, 30), rect(20, 50, 1632, 2112));
    expect(at100).not.toBeNull();
    expect(at200!.x).toBeCloseTo(at100!.x, 10);
    expect(at200!.y).toBeCloseTo(at100!.y, 10);
    expect(at200!.w).toBeCloseTo(at100!.w, 10);
    expect(at200!.h).toBeCloseTo(at100!.h, 10);
    expect(at100!.x).toBeCloseTo(100 / 816, 10);
  });

  it("cuts a rectangle to the page", () => {
    const page = rect(0, 0, 1000, 1000);
    expect(toShares(rect(-50, 750, 200, 500), page)).toEqual({ x: 0, y: 0.75, w: 0.15, h: 0.25 });
    expect(toShares(rect(1100, 10, 50, 10), page)).toBeNull();
    expect(toShares(rect(10, 10, 50, 10), rect(0, 0, 0, 0))).toBeNull();
  });

  it("keeps one space between words, also across line ends", () => {
    expect(cleanText("  Feed conversion\n ratio   was\t1.4 ")).toBe("Feed conversion ratio was 1.4");
  });

  it("groups highlights by page", () => {
    const at = (id: string, page: number) => ({ id, page }) as Highlight;
    const pages = groupHighlights([at("a", 3), at("b", 1), at("c", 3)]);
    expect(pages.get(3)?.map((h) => h.id)).toEqual(["a", "c"]);
    expect(pages.get(1)?.map((h) => h.id)).toEqual(["b"]);
    expect(pages.get(2)).toBeUndefined();
  });
});
