<!--
  An opened PDF: ReaderToolbar above a scrolling column of pages (FR-02.1).
  Every page has a placeholder of its exact size, so the scrollbar is right from the
  start; only the pages near the view are drawn (FR-02.4).
  Search covers every page, also the ones not drawn (FR-02.1); Ctrl+F opens it.
  Mounted once per opened PDF, so search starts fresh for each paper. The view starts
  at `startPage` and `startZoom`, and reports the pages read (FR-02.3) and the zoom
  chosen (FR-08.5) after that.
  Selecting text opens the highlight popover (O-03, FR-03.1) unless `onhighlight` is
  missing (read-only library).
-->
<script lang="ts">
  import { tick, untrack } from "svelte";
  import type { Highlight, HighlightCategory, NewHighlight, PageMode } from "../lib/api";
  import SearchField from "../lib/components/SearchField.svelte";
  import HighlightPopover from "./HighlightPopover.svelte";
  import { categoryForKey, pageMarks } from "./highlights";
  import { readSelection, type PageSelection } from "./selection";
  import {
    clampZoom,
    currentPage,
    fitWidthZoom,
    layoutPages,
    pagesToRender,
    positionAt,
    scrollTopFor,
    scrollTopForPage,
    scrollToShow,
    zoomIn,
    zoomOut,
    type Box,
  } from "./layout";
  import PageView from "./PageView.svelte";
  import { readPageTexts, type OpenedPdf } from "./pdfjs";
  import ReaderToolbar from "./ReaderToolbar.svelte";
  import {
    findMatches,
    firstMatchFrom,
    groupByPage,
    indexPage,
    stepMatch,
    type Match,
    type PageIndex,
  } from "./search";

  interface Props {
    pdf: OpenedPdf;
    /** 1-based; a page past the end means the last one. */
    startPage?: number;
    onpagechange?: (page: number) => void;
    /** The zoom to start with; null is "Fit width". */
    startZoom?: number | null;
    /** The user zoomed, or chose "Fit width" (null). */
    onzoom?: (zoom: number | null) => void;
    mode?: PageMode;
    onmode?: (mode: PageMode) => void;
    /** The paper's highlights, drawn on their pages (FR-03.2). */
    highlights?: Highlight[];
    /** The library's highlight categories, in their order (FR-03.1). */
    categories?: HighlightCategory[];
    /**
     * The user picked a category for a selection. `crossed`: the selection went past its
     * first page, and only that page is kept (FR-03.5).
     */
    onhighlight?: (highlight: NewHighlight, crossed: boolean) => void;
    /** The selected highlight, outlined on its page. */
    activeHighlight?: string | null;
    /** Show this highlight and flash it (FR-03.4); `n` changes on every request. */
    jump?: { id: string; n: number } | null;
  }

  let {
    pdf,
    startPage = 1,
    onpagechange,
    startZoom = null,
    onzoom,
    mode = "normal",
    onmode,
    highlights = [],
    categories = [],
    onhighlight,
    activeHighlight = null,
    jump = null,
  }: Props = $props();

  let zoom = $state(untrack(() => clampZoom(startZoom ?? 1)));
  /** "Fit width" stays on until the user zooms: it follows the window size (Brief Section 9). */
  let fitWidth = $state(untrack(() => startZoom === null));

  let scroller: HTMLDivElement;
  let viewWidth = $state(0);
  let viewHeight = $state(0);
  let scrollTop = $state(0);

  const layout = $derived(layoutPages(pdf.sizes, zoom));
  const range = $derived(pagesToRender(layout, scrollTop, viewHeight));
  const page = $derived(currentPage(layout, scrollTop, viewHeight));

  $effect(() => {
    if (viewWidth <= 0) return;
    if (fitWidth) {
      const fitted = fitWidthZoom(pdf.sizes, viewWidth);
      untrack(() => setZoom(fitted).then(showStartPage));
    } else {
      // A chosen zoom: the pages already have their size.
      untrack(showStartPage);
    }
  });

  /** The page last reported, or null until the view is at its start page. */
  let known = $state<number | null>(null);

  /** Once, as soon as the pages have their real size. */
  function showStartPage() {
    if (known !== null || !scroller) return;
    goToPage(Math.min(Math.max(1, startPage), pdf.sizes.length));
    // What is shown now is not new reading: near the end, the last page may not
    // reach the top, and the stored page must not move back because of that.
    known = page;
  }

  $effect(() => {
    const now = page;
    const settled = zooming === 0;
    untrack(() => {
      if (!settled || known === null || now === known) return;
      known = now;
      onpagechange?.(now);
    });
  });

  /** Changes the zoom and keeps the same spot of the same page at the top of the view. */
  async function setZoom(next: number) {
    next = clampZoom(next);
    if (Math.abs(next - zoom) < 1e-4) return;
    const position = positionAt(layout, scroller.scrollTop);
    zooming++;
    zoom = next;
    // Wait until the column has its new height, or the scroll position gets cut off.
    await tick();
    scroller.scrollTop = scrollTopFor(layout, position);
    scrollTop = scroller.scrollTop;
    zooming--;
  }

  /** Zoom changes still under way: until then, `page` may name a page not being read. */
  let zooming = $state(0);

  function zoomBy(change: (zoom: number) => number) {
    const next = change(zoom);
    fitWidth = false;
    setZoom(next);
    onzoom?.(next);
  }

  function chooseFitWidth() {
    fitWidth = true;
    onzoom?.(null);
  }

  function goToPage(number: number) {
    scroller.scrollTop = scrollTopForPage(layout, number);
    scrollTop = scroller.scrollTop;
  }

  // ---- Search (FR-02.1) ----

  let searchField: SearchField;
  let query = $state("");
  /** The text of all pages; read once, at the first search. */
  let index = $state.raw<PageIndex[] | null>(null);
  let reading = false;
  const searching = $derived(query.trim() !== "");
  const matches = $derived(index && searching ? findMatches(index, query) : []);
  const matchesByPage = $derived(groupByPage(matches));
  let current = $state(0);
  const active = $derived<Match | null>(matches[current] ?? null);
  let reveal = $state(0);
  /** The view still has to scroll to the current result. */
  let toShow = false;

  $effect(() => {
    // Runs on every change of the query, so that a failed read is tried again.
    void query;
    if (!searching || index || reading) return;
    reading = true;
    readPageTexts(pdf.doc)
      .then((pages) => (index = pages.map(indexPage)))
      // The document was closed meanwhile, or could not be read: try again next time.
      .catch((failure) => console.error("The text could not be searched", failure))
      .finally(() => (reading = false));
  });

  // New results: start with the first one from the page being read.
  $effect(() => {
    const found = matches;
    untrack(() => {
      if (found.length > 0) showMatch(firstMatchFrom(found, page - 1));
      else current = 0;
    });
  });

  function showMatch(next: number) {
    const match = matches[next];
    if (!match) return;
    current = next;
    reveal++;
    toShow = true;
    // Bring its page near the view, so that it is drawn and the result can be measured.
    if (!range || match.page < range.first || match.page > range.last) goToPage(match.page + 1);
  }

  const step = (by: 1 | -1) => showMatch(stepMatch(current, by, matches.length));

  /** The page of the current result says where on the page it is. */
  function scrollToResult(pageIndex: number, box: Box) {
    const target = layout.pages[pageIndex];
    if (!toShow || !target) return;
    toShow = false;
    scroller.scrollTop = scrollToShow(
      scroller.scrollTop,
      viewHeight,
      target.top + box.top * target.height,
      box.height * target.height,
    );
    scrollTop = scroller.scrollTop;
    // Pages are centred in a column at least as wide as the view.
    const left = (Math.max(layout.width, viewWidth) - target.width) / 2;
    scroller.scrollLeft = scrollToShow(
      scroller.scrollLeft,
      viewWidth,
      left + box.left * target.width,
      box.width * target.width,
    );
  }

  /** Ctrl+F, instead of the web view's own search, which only sees the drawn pages. */
  function onshortcut(event: KeyboardEvent) {
    if (picking && pickByKey(event)) return;
    if (!event.ctrlKey || event.altKey || event.shiftKey || event.key.toLowerCase() !== "f") return;
    event.preventDefault();
    searchField.focus();
  }

  // ---- Highlights (FR-03.1, FR-03.2, FR-03.5) ----

  const marksByPage = $derived(pageMarks(highlights, categories));

  // ---- Jumping to a highlight (FR-03.4) ----

  /** The highlight being flashed after a jump. */
  let flashing = $state<string | null>(null);
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  /** The last request carried out. */
  let jumped = 0;

  $effect(() => {
    const request = jump;
    // Not before the view is at its start page, which would undo the jump.
    if (!request || known === null) return;
    untrack(() => {
      if (request.n === jumped) return;
      jumped = request.n;
      showHighlight(request.id);
    });
  });

  $effect(() => () => clearTimeout(flashTimer));

  async function showHighlight(id: string) {
    const highlight = highlights.find((each) => each.id === id);
    const target = highlight && layout.pages[highlight.page - 1];
    const first = highlight?.rects[0];
    if (!target || !first) return;
    scroller.scrollTop = scrollToShow(
      scroller.scrollTop,
      viewHeight,
      target.top + first.y * target.height,
      first.h * target.height,
    );
    scrollTop = scroller.scrollTop;
    const left = (Math.max(layout.width, viewWidth) - target.width) / 2;
    scroller.scrollLeft = scrollToShow(
      scroller.scrollLeft,
      viewWidth,
      left + first.x * target.width,
      first.w * target.width,
    );
    // Off and on again, so that jumping to the same one flashes again.
    clearTimeout(flashTimer);
    flashing = null;
    await tick();
    flashing = id;
    flashTimer = setTimeout(() => (flashing = null), 2000);
  }

  /** The selection the popover is open for. */
  let picking = $state.raw<PageSelection | null>(null);

  /** Opens the popover for the current selection, or closes it when there is none. */
  function checkSelection() {
    if (!onhighlight || categories.length === 0) return;
    picking = readSelection(window.getSelection(), scroller);
  }

  /** After the mouse is released the selection is final. */
  function onpointerup() {
    setTimeout(checkSelection);
  }

  /** Selecting with Shift and the arrow keys. */
  function onkeyup(event: KeyboardEvent) {
    if (event.shiftKey || event.key === "Shift") checkSelection();
  }

  function pick(category: HighlightCategory) {
    const chosen = picking;
    picking = null;
    if (!chosen) return;
    window.getSelection()?.removeAllRanges();
    onhighlight?.(
      { page: chosen.page, categoryId: category.id, text: chosen.text, rects: chosen.rects },
      chosen.crossed,
    );
  }

  /** Keys 1–8 pick a category and Esc closes, while the popover is open (Brief O-03). */
  function pickByKey(event: KeyboardEvent): boolean {
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest("input, textarea, select, [contenteditable]")) return false;
    if (event.key === "Escape") {
      event.preventDefault();
      picking = null;
      return true;
    }
    if (event.ctrlKey || event.altKey || event.metaKey) return false;
    const category = categoryForKey(categories, event.key);
    if (!category) return false;
    event.preventDefault();
    pick(category);
    return true;
  }

  function onselectionchange() {
    // The selection went away (a click elsewhere, the page redrawn at a new zoom).
    if (picking && (window.getSelection()?.isCollapsed ?? true)) picking = null;
  }

  function onscroll() {
    scrollTop = scroller.scrollTop;
    // Follow the selection; it is gone when its page is no longer drawn.
    if (picking) checkSelection();
  }
</script>

<svelte:window onkeydown={onshortcut} />
<!-- On the document: a selection may end outside the pages. -->
<svelte:document {onselectionchange} {onpointerup} {onkeyup} />

<div class="pdf-view">
  <ReaderToolbar
    {zoom}
    {page}
    total={pdf.sizes.length}
    onzoomout={() => zoomBy(zoomOut)}
    onzoomin={() => zoomBy(zoomIn)}
    onfitwidth={chooseFitWidth}
    ongotopage={goToPage}
    {mode}
    {onmode}
  >
    {#snippet search()}
      <SearchField
        bind:this={searchField}
        bind:value={query}
        label="Search in PDF"
        previousLabel="Previous result"
        nextLabel="Next result"
        clearLabel="Clear search"
        results={searching && index
          ? { current: matches.length > 0 ? current + 1 : 0, total: matches.length }
          : null}
        busy={searching && !index}
        onprevious={() => step(-1)}
        onnext={() => step(1)}
      />
    {/snippet}
  </ReaderToolbar>
  <div
    class="scroller"
    bind:this={scroller}
    bind:clientWidth={viewWidth}
    bind:clientHeight={viewHeight}
    {onscroll}
  >
    <div class="column" style:height="{layout.height}px" style:min-width="{layout.width}px">
      {#each layout.pages as box, slot (slot)}
        <div
          class="slot {mode}"
          style:top="{box.top}px"
          style:width="{box.width}px"
          style:height="{box.height}px"
          style:margin-left="{-box.width / 2}px"
        >
          {#if range && slot >= range.first && slot <= range.last}
            <PageView
              doc={pdf.doc}
              number={slot + 1}
              {zoom}
              {mode}
              highlights={marksByPage.get(slot + 1)}
              {activeHighlight}
              {flashing}
              matches={matchesByPage.get(slot)}
              active={active?.page === slot ? active : null}
              reveal={active?.page === slot ? reveal : 0}
              onactive={(box) => scrollToResult(slot, box)}
            />
          {/if}
        </div>
      {/each}
    </div>
  </div>
  {#if picking}
    <HighlightPopover {categories} bounds={picking.bounds} onpick={pick} />
  {/if}
</div>

<style>
  .pdf-view {
    display: flex;
    flex-direction: column;
    min-width: 0;
    height: 100%;
  }

  .scroller {
    flex: 1;
    min-height: 0;
    /* Always shown, so "Fit width" does not flip between two widths as it appears. */
    overflow-y: scroll;
    overflow-x: auto;
    background: var(--surface-sunken);
  }

  .column {
    position: relative;
  }

  .slot {
    position: absolute;
    left: 50%;
    outline: 1px solid var(--border-subtle);
    background: var(--page-normal);
  }

  /* The color of a page that is not drawn yet. */
  .slot.dark {
    background: var(--page-dark);
  }

  .slot.sepia {
    background: var(--page-sepia);
  }
</style>
