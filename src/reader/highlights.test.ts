import { describe, expect, it } from "vitest";
import type { Highlight, HighlightCategory, PaletteColor } from "../lib/api";
import {
  categoryForKey,
  categoryKey,
  cleanText,
  crossPageMessage,
  highlightName,
  mergeLines,
  pageMarks,
  toShares,
} from "./highlights";

const category = (id: string, label: string, color: PaletteColor, sortOrder = 0): HighlightCategory => ({
  id,
  label,
  color,
  sortOrder,
});

describe("highlight categories (FR-03.1, FR-08.6)", () => {
  const eight = (["green", "red", "blue", "yellow", "purple", "orange", "pink", "gray"] as const).map(
    (color, index) => category(`c${index}`, `Category ${index + 1}`, color, index),
  );

  it("gives keys 1 to 8 in popover order", () => {
    expect([0, 1, 7].map(categoryKey)).toEqual(["1", "2", "8"]);
    expect(categoryKey(8)).toBeNull();
    expect(categoryKey(-1)).toBeNull();
    expect(categoryForKey(eight, "8")?.color).toBe("gray");
    expect(categoryForKey(eight, "1")?.color).toBe("green");
    expect(categoryForKey(eight.slice(0, 4), "5")).toBeNull();
    expect(categoryForKey(eight, "9")).toBeNull();
    expect(categoryForKey(eight, "a")).toBeNull();
  });

  it("names a mark by its category and page", () => {
    expect(highlightName("Exclusion", 4)).toBe("Highlight, Exclusion, page 4");
    expect(highlightName("Sample size", 1)).toBe("Highlight, Sample size, page 1");
  });

  it("draws each highlight with its category's color and name, by page", () => {
    const at = (id: string, page: number, categoryId: string) =>
      ({ id, page, categoryId, rects: [{ x: 0, y: 0, w: 1, h: 0.1 }] }) as Highlight;
    const categories = [category("a", "Method", "blue"), category("b", "Site", "orange")];
    const pages = pageMarks([at("h1", 3, "a"), at("h2", 1, "b"), at("h3", 3, "b"), at("h4", 2, "gone")], categories);
    expect(pages.get(3)?.map((mark) => [mark.id, mark.color, mark.name])).toEqual([
      ["h1", "blue", "Highlight, Method, page 3"],
      ["h3", "orange", "Highlight, Site, page 3"],
    ]);
    expect(pages.get(1)?.map((mark) => mark.id)).toEqual(["h2"]);
    // A highlight whose category is unknown is not drawn.
    expect(pages.get(2)).toBeUndefined();
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
});
