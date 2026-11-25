// Page layout of the reader (FR-02.1, FR-02.4): where each page sits at a zoom level,
// which pages are near the visible area, and how to keep the reading position.
// Pure functions without PDF.js, so they can be tested on their own.

/** CSS pixels per PDF point at 100%: 100% shows a page at its printed size. */
export const CSS_PER_POINT = 96 / 72;

/** Space between pages, and above the first and below the last page, in CSS pixels. */
export const PAGE_GAP = 16;

export const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];
export const MIN_ZOOM = ZOOM_STEPS[0];
export const MAX_ZOOM = ZOOM_STEPS[ZOOM_STEPS.length - 1];

/** Pages rendered before and after the visible ones, so scrolling shows no blank page. */
export const EXTRA_PAGES = 1;

/** Largest canvas, in device pixels: bigger ones use too much memory or fail to draw. */
export const MAX_CANVAS_PIXELS = 4096 * 4096;

/** A page at 100% zoom, in PDF points, with the page's own rotation applied. */
export interface PageSize {
  width: number;
  height: number;
}

/** A page at the current zoom, in CSS pixels. */
export interface PageBox {
  top: number;
  width: number;
  height: number;
}

export interface Layout {
  pages: PageBox[];
  /** Height of the whole scrolled area. */
  height: number;
  /** Width of the widest page plus the gaps on both sides. */
  width: number;
}

/** Where the reader is: a page (0-based) and how far down it, as a share of its height. */
export interface Position {
  page: number;
  offset: number;
}

export function layoutPages(sizes: PageSize[], zoom: number): Layout {
  const scale = zoom * CSS_PER_POINT;
  let top = PAGE_GAP;
  let widest = 0;
  const pages = sizes.map(({ width, height }) => {
    const box = { top, width: Math.floor(width * scale), height: Math.floor(height * scale) };
    top += box.height + PAGE_GAP;
    widest = Math.max(widest, box.width);
    return box;
  });
  return { pages, height: top, width: widest + 2 * PAGE_GAP };
}

/** The first page whose bottom is below `y`, or the last page. */
function pageAtY(pages: PageBox[], y: number): number {
  let low = 0;
  let high = pages.length - 1;
  while (low < high) {
    const middle = (low + high) >> 1;
    const page = pages[middle];
    if (page.top + page.height + PAGE_GAP > y) high = middle;
    else low = middle + 1;
  }
  return low;
}

/** The pages to render (0-based, inclusive): the visible ones plus `extra` on each side. */
export function pagesToRender(
  layout: Layout,
  scrollTop: number,
  viewHeight: number,
  extra = EXTRA_PAGES,
): { first: number; last: number } | null {
  const { pages } = layout;
  if (pages.length === 0) return null;
  const first = pageAtY(pages, scrollTop);
  let last = first;
  while (last + 1 < pages.length && pages[last + 1].top < scrollTop + viewHeight) last++;
  return { first: Math.max(0, first - extra), last: Math.min(pages.length - 1, last + extra) };
}

/** The page number (1-based) shown in the toolbar: the page that fills most of the view. */
export function currentPage(layout: Layout, scrollTop: number, viewHeight: number): number {
  const range = pagesToRender(layout, scrollTop, viewHeight, 0);
  if (!range) return 0;
  let best = range.first;
  let bestShown = -1;
  for (let i = range.first; i <= range.last; i++) {
    const { top, height } = layout.pages[i];
    const shown = Math.min(top + height, scrollTop + viewHeight) - Math.max(top, scrollTop);
    if (shown > bestShown) {
      best = i;
      bestShown = shown;
    }
  }
  return best + 1;
}

export function positionAt(layout: Layout, scrollTop: number): Position {
  if (layout.pages.length === 0) return { page: 0, offset: 0 };
  const page = pageAtY(layout.pages, scrollTop);
  const { top, height } = layout.pages[page];
  return { page, offset: height > 0 ? (scrollTop - top) / height : 0 };
}

/** The scroll position that shows `position` at the top of the view. */
export function scrollTopFor(layout: Layout, position: Position): number {
  const page = layout.pages[Math.min(position.page, layout.pages.length - 1)];
  if (!page) return 0;
  return Math.max(0, Math.round(page.top + position.offset * page.height));
}

/** The scroll position for "go to page" (1-based): the page's top edge just below the gap. */
export function scrollTopForPage(layout: Layout, pageNumber: number): number {
  const page = layout.pages[pageNumber - 1];
  return page ? Math.max(0, page.top - PAGE_GAP) : 0;
}

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

// A zoom from "Fit width" lies between steps; these move to the next step either way.
const EPSILON = 0.001;

export function zoomIn(zoom: number): number {
  return ZOOM_STEPS.find((step) => step > zoom + EPSILON) ?? MAX_ZOOM;
}

export function zoomOut(zoom: number): number {
  return [...ZOOM_STEPS].reverse().find((step) => step < zoom - EPSILON) ?? MIN_ZOOM;
}

/** The zoom at which the widest page fills `viewWidth` (the area inside the scrollbar). */
export function fitWidthZoom(sizes: PageSize[], viewWidth: number): number {
  const widest = Math.max(0, ...sizes.map((size) => size.width));
  const room = viewWidth - 2 * PAGE_GAP;
  if (widest <= 0 || room <= 0) return 1;
  return clampZoom(room / (widest * CSS_PER_POINT));
}

/** "100%" */
export function zoomLabel(zoom: number): string {
  return `${Math.round(zoom * 100)}%`;
}

/**
 * The page typed in the toolbar: a whole number, kept within 1..total.
 * Anything else gives null, and the field shows the current page again.
 */
export function parsePage(text: string, total: number): number | null {
  const trimmed = text.trim();
  if (total < 1 || !/^\d+$/.test(trimmed)) return null;
  return Math.min(total, Math.max(1, Number(trimmed)));
}

/** Device pixels per CSS pixel for a page canvas: the screen's ratio, lower for huge pages. */
export function canvasPixelRatio(
  width: number,
  height: number,
  devicePixelRatio: number,
  maxPixels = MAX_CANVAS_PIXELS,
): number {
  const ratio = devicePixelRatio > 0 ? devicePixelRatio : 1;
  const area = width * height;
  if (area <= 0 || area * ratio * ratio <= maxPixels) return ratio;
  return Math.sqrt(maxPixels / area);
}
