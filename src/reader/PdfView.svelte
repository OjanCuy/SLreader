<!--
  An opened PDF: ReaderToolbar above a scrolling column of pages (FR-02.1).
  Every page has a placeholder of its exact size, so the scrollbar is right from the
  start; only the pages near the view are drawn (FR-02.4).
  Mounted once per opened PDF, so zoom and position start fresh for each paper.
-->
<script lang="ts">
  import { tick, untrack } from "svelte";
  import {
    clampZoom,
    currentPage,
    fitWidthZoom,
    layoutPages,
    pagesToRender,
    positionAt,
    scrollTopFor,
    scrollTopForPage,
    zoomIn,
    zoomOut,
  } from "./layout";
  import PageView from "./PageView.svelte";
  import type { OpenedPdf } from "./pdfjs";
  import ReaderToolbar from "./ReaderToolbar.svelte";

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
</script>

<div class="pdf-view">
  <ReaderToolbar
    {zoom}
    {page}
    total={pdf.sizes.length}
    onzoomout={() => zoomBy(zoomOut)}
    onzoomin={() => zoomBy(zoomIn)}
    onfitwidth={() => (fitWidth = true)}
    ongotopage={goToPage}
  />
  <div
    class="scroller"
    bind:this={scroller}
    bind:clientWidth={viewWidth}
    bind:clientHeight={viewHeight}
    onscroll={() => (scrollTop = scroller.scrollTop)}
  >
    <div class="column" style:height="{layout.height}px" style:min-width="{layout.width}px">
      {#each layout.pages as box, index (index)}
        <div
          class="slot"
          style:top="{box.top}px"
          style:width="{box.width}px"
          style:height="{box.height}px"
          style:margin-left="{-box.width / 2}px"
        >
          {#if range && index >= range.first && index <= range.last}
            <PageView doc={pdf.doc} number={index + 1} {zoom} />
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
