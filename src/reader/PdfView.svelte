<!--
  An opened PDF: ReaderToolbar above a scrolling column of pages (FR-02.1).
  Every page has a placeholder of its exact size, so the scrollbar is right from the
  start; only the pages near the view are drawn (FR-02.4).
  Search covers every page, also the ones not drawn (FR-02.1); Ctrl+F opens it.
  Mounted once per opened PDF, so zoom, position, and search start fresh for each paper.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";
  import SearchField from "../lib/components/SearchField.svelte";
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

  let { pdf }: { pdf: OpenedPdf } = $props();

  let zoom = $state(1);
  /** "Fit width" stays on until the user zooms: it follows the window size (Brief Section 9). */
  let fitWidth = $state(true);

  let scroller: HTMLDivElement;
  let viewWidth = $state(0);
  let viewHeight = $state(0);
  let scrollTop = $state(0);

  const layout = $derived(layoutPages(pdf.sizes, zoom));
  const range = $derived(pagesToRender(layout, scrollTop, viewHeight));
  const page = $derived(currentPage(layout, scrollTop, viewHeight));

  $effect(() => {
    if (fitWidth && viewWidth > 0) {
      const fitted = fitWidthZoom(pdf.sizes, viewWidth);
      untrack(() => setZoom(fitted));
    }
  });

  /** Changes the zoom and keeps the same spot of the same page at the top of the view. */
  async function setZoom(next: number) {
    next = clampZoom(next);
    if (Math.abs(next - zoom) < 1e-4) return;
    const position = positionAt(layout, scroller.scrollTop);
    zoom = next;
    // Wait until the column has its new height, or the scroll position gets cut off.
    await tick();
    scroller.scrollTop = scrollTopFor(layout, position);
    scrollTop = scroller.scrollTop;
  }

  function zoomBy(change: (zoom: number) => number) {
    fitWidth = false;
    setZoom(change(zoom));
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
    if (!event.ctrlKey || event.altKey || event.shiftKey || event.key.toLowerCase() !== "f") return;
    event.preventDefault();
    searchField.focus();
  }
</script>

<svelte:window onkeydown={onshortcut} />

<div class="pdf-view">
  <ReaderToolbar
    {zoom}
    {page}
    total={pdf.sizes.length}
    onzoomout={() => zoomBy(zoomOut)}
    onzoomin={() => zoomBy(zoomIn)}
    onfitwidth={() => (fitWidth = true)}
    ongotopage={goToPage}
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
    onscroll={() => (scrollTop = scroller.scrollTop)}
  >
    <div class="column" style:height="{layout.height}px" style:min-width="{layout.width}px">
      {#each layout.pages as box, slot (slot)}
        <div
          class="slot"
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
    background: var(--surface-panel);
  }
</style>
