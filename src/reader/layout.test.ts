import { describe, expect, it } from "vitest";
import {
  canvasPixelRatio,
  currentPage,
  fitWidthZoom,
  layoutPages,
  MAX_ZOOM,
  MIN_ZOOM,
  PAGE_GAP,
  pagesToRender,
  parsePage,
  positionAt,
  scrollTopFor,
  scrollTopForPage,
  zoomIn,
  zoomLabel,
  zoomOut,
} from "./layout";

// US Letter: 612 x 792 points = 816 x 1056 CSS pixels at 100%.
const LETTER = { width: 612, height: 792 };
const pages = (count: number) => Array.from({ length: count }, () => LETTER);
const STEP = 1056 + PAGE_GAP;

describe("layoutPages", () => {
  it("stacks pages with a gap and shows 100% at printed size", () => {
    const layout = layoutPages(pages(3), 1);
    expect(layout.pages).toEqual([
      { top: 16, width: 816, height: 1056 },
      { top: 16 + STEP, width: 816, height: 1056 },
      { top: 16 + 2 * STEP, width: 816, height: 1056 },
    ]);
    expect(layout.height).toBe(16 + 3 * STEP);
    expect(layout.width).toBe(816 + 2 * PAGE_GAP);
  });

  it("scales with the zoom and keeps mixed page sizes", () => {
    const layout = layoutPages([LETTER, { width: 792, height: 612 }], 2);
    expect(layout.pages[0]).toMatchObject({ width: 1632, height: 2112 });
    expect(layout.pages[1]).toMatchObject({ width: 2112, height: 1632 });
    expect(layout.width).toBe(2112 + 2 * PAGE_GAP);
  });
});

describe("pagesToRender (FR-02.4)", () => {
  const layout = layoutPages(pages(300), 1);

  it("renders only the visible pages and one on each side", () => {
    expect(pagesToRender(layout, 0, 800)).toEqual({ first: 0, last: 1 });
    // Scrolled to the middle of a 300-page PDF, with pages 150 and 151 in view.
    const top = layout.pages[149].top + 600;
    expect(pagesToRender(layout, top, 800)).toEqual({ first: 148, last: 151 });
    expect(pagesToRender(layout, layout.height - 800, 800)).toEqual({ first: 298, last: 299 });
  });

  it("handles an empty document and scrolling past the end", () => {
    expect(pagesToRender(layoutPages([], 1), 0, 800)).toBeNull();
    expect(pagesToRender(layout, layout.height + 5000, 800)).toEqual({ first: 298, last: 299 });
  });

  it("can render several pages at once when zoomed out", () => {
    const small = layoutPages(pages(10), 0.25);
    expect(pagesToRender(small, 0, 1000, 0)).toEqual({ first: 0, last: 3 });
  });
});

describe("currentPage", () => {
  const layout = layoutPages(pages(5), 1);

  it("is the page that fills most of the view", () => {
    expect(currentPage(layout, 0, 800)).toBe(1);
    // 300 px of page 1 and 484 px of page 2 are visible.
    expect(currentPage(layout, layout.pages[0].top + 1056 - 300, 800)).toBe(2);
    expect(currentPage(layout, layout.height - 800, 800)).toBe(5);
  });

  it("is 0 without pages", () => {
    expect(currentPage(layoutPages([], 1), 0, 800)).toBe(0);
  });
});

describe("reading position across zoom levels", () => {
  it("keeps the same spot of the same page", () => {
    const at100 = layoutPages(pages(20), 1);
    const scrollTop = at100.pages[7].top + 1056 * 0.4;
    const position = positionAt(at100, scrollTop);
    expect(position.page).toBe(7);
    expect(position.offset).toBeCloseTo(0.4);

    const at200 = layoutPages(pages(20), 2);
    expect(scrollTopFor(at200, position)).toBe(Math.round(at200.pages[7].top + 2112 * 0.4));
    // And back: the position does not drift.
    expect(positionAt(at200, scrollTopFor(at200, position)).offset).toBeCloseTo(0.4, 3);
  });

  it("goes to the top of a page", () => {
    const layout = layoutPages(pages(5), 1);
    expect(scrollTopForPage(layout, 1)).toBe(0);
    expect(scrollTopForPage(layout, 3)).toBe(layout.pages[2].top - PAGE_GAP);
    expect(scrollTopForPage(layout, 99)).toBe(0);
  });
});

describe("zoom", () => {
  it("steps in and out, also from a fit-width zoom between steps", () => {
    expect(zoomIn(1)).toBe(1.1);
    expect(zoomOut(1)).toBe(0.9);
    expect(zoomIn(1.18)).toBe(1.25);
    expect(zoomOut(1.18)).toBe(1.1);
  });

  it("stops at the smallest and largest zoom", () => {
    expect(zoomIn(MAX_ZOOM)).toBe(MAX_ZOOM);
    expect(zoomOut(MIN_ZOOM)).toBe(MIN_ZOOM);
  });

  it("fits the widest page in the view", () => {
    // 816 px page + gaps fits 848 px exactly at 100%.
    expect(fitWidthZoom(pages(2), 816 + 2 * PAGE_GAP)).toBeCloseTo(1);
    const wide = [LETTER, { width: 792, height: 612 }];
    const zoom = fitWidthZoom(wide, 1000);
    expect(layoutPages(wide, zoom).width).toBeLessThanOrEqual(1000);
  });

  it("stays within the zoom limits and survives a hidden view", () => {
    expect(fitWidthZoom(pages(1), 50)).toBe(MIN_ZOOM);
    expect(fitWidthZoom(pages(1), 100_000)).toBe(MAX_ZOOM);
    expect(fitWidthZoom(pages(1), 0)).toBe(1);
    expect(fitWidthZoom([], 800)).toBe(1);
  });

  it("is shown as a percentage", () => {
    expect(zoomLabel(1)).toBe("100%");
    expect(zoomLabel(0.67)).toBe("67%");
    expect(zoomLabel(1.1849)).toBe("118%");
  });
});

describe("parsePage", () => {
  it("accepts a page number and keeps it within the document", () => {
    expect(parsePage("3", 12)).toBe(3);
    expect(parsePage(" 12 ", 12)).toBe(12);
    expect(parsePage("99", 12)).toBe(12);
    expect(parsePage("0", 12)).toBe(1);
  });

  it("rejects anything that is not a whole number", () => {
    for (const text of ["", "abc", "2.5", "-1", "3a"]) expect(parsePage(text, 12)).toBeNull();
    expect(parsePage("1", 0)).toBeNull();
  });
});

describe("canvasPixelRatio", () => {
  it("matches the screen for normal pages", () => {
    expect(canvasPixelRatio(816, 1056, 1.5)).toBe(1.5);
  });

  it("lowers the resolution of very large pages", () => {
    // A page at 400% on a 200% screen would need 6528 x 8448 x 4 pixels.
    const ratio = canvasPixelRatio(3264, 4224, 2);
    expect(ratio).toBeLessThan(2);
    expect(3264 * 4224 * ratio * ratio).toBeLessThanOrEqual(4096 * 4096 + 1);
  });

  it("falls back to 1 for an unknown screen ratio", () => {
    expect(canvasPixelRatio(816, 1056, 0)).toBe(1);
  });
});
