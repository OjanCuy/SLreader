<!--
  One PDF page: a canvas with the page drawn on it and the PDF.js text layer on top,
  so the text can be selected and read by a screen reader.
  Only mounted for pages near the visible area (FR-02.4); unmounting frees the canvas.
  On a zoom change the old drawing stays, stretched, until the new one is ready.
-->
<script lang="ts">
  import { TextLayer, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";
  import { untrack } from "svelte";
  import type { Highlight, PageMode } from "../lib/api";
  import { highlightName, kindOf } from "./highlights";
  import { canvasPixelRatio, CSS_PER_POINT, type Box } from "./layout";
  import { isCancelled } from "./pdfjs";
  import { segments, type Match } from "./search";

  interface Props {
    doc: PDFDocumentProxy;
    /** 1-based. */
    number: number;
    zoom: number;
    /** Colors the drawn page only; the marks and the text layer stay on top. */
    mode?: PageMode;
    /** The search results on this page. */
    matches?: Match[];
    /** The current search result, when it is on this page. */
    active?: Match | null;
    /** Changes each time the user moves to a result, also to the same one again. */
    reveal?: number;
    /** Where the current result is on the page, once it can be measured. */
    onactive?: (box: Box) => void;
    /** The highlights on this page (FR-03.2). */
    highlights?: Highlight[];
  }

  let {
    doc,
    number,
    zoom,
    mode = "normal",
    highlights = [],
    matches = [],
    active = null,
    reveal = 0,
    onactive,
  }: Props = $props();

  let pageElement: HTMLDivElement;
  let canvasHost: HTMLDivElement;
  let textHost: HTMLDivElement;

  /** The text elements now on screen, one per text item. */
  let textDivs = $state.raw<HTMLElement[] | null>(null);
  let marks = $state.raw<(Box & { active: boolean })[]>([]);

  // Measures where the results are in the text layer. The boxes are shares of the page,
  // so they stay in place while the zoom changes.
  $effect(() => {
    const divs = textDivs;
    const current = active;
    void reveal;
    const frame = pageElement.getBoundingClientRect();
    if (!divs || matches.length === 0 || frame.width <= 0 || frame.height <= 0) {
      marks = [];
      return;
    }
    const found = matches.flatMap((match) =>
      boxesOf(match, divs, frame).map((box) => ({ ...box, active: match === current })),
    );
    marks = found;
    const first = found.find((mark) => mark.active);
    if (first) untrack(() => onactive?.(first));
  });

  function boxesOf(match: Match, divs: HTMLElement[], frame: DOMRect): Box[] {
    const length = (item: number) => divs[item]?.textContent?.length ?? 0;
    return segments(match, length).flatMap(({ item, from, to }) => {
      const text = divs[item]?.firstChild;
      if (!text || !divs[item].isConnected || to > length(item)) return [];
      const range = document.createRange();
      range.setStart(text, from);
      range.setEnd(text, to);
      return Array.from(range.getClientRects())
        .filter((rect) => rect.width > 0 && rect.height > 0)
        .map((rect) => ({
          left: (rect.left - frame.left) / frame.width,
          top: (rect.top - frame.top) / frame.height,
          width: rect.width / frame.width,
          height: rect.height / frame.height,
        }));
    });
  }

  $effect(() => {
    const scale = zoom * CSS_PER_POINT;
    let stopped = false;
    let render: RenderTask | undefined;
    let text: TextLayer | undefined;

    async function draw() {
      const page = await doc.getPage(number);
      if (stopped) return;
      const viewport = page.getViewport({ scale });
      const ratio = canvasPixelRatio(viewport.width, viewport.height, window.devicePixelRatio);
      const canvas = document.createElement("canvas");
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      render = page.render({
        canvas,
        viewport,
        transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0],
      });
      await render.promise;
      if (stopped) return free(canvas);
      replaceCanvas(canvas);

      const layer = document.createElement("div");
      layer.className = "textLayer";
      text = new TextLayer({ textContentSource: page.streamTextContent(), container: layer, viewport });
      await text.render();
      if (stopped) return;
      textHost.replaceChildren(layer);
      textDivs = text.textDivs;
    }

    draw().catch((failure) => {
      // A page that fails to draw stays blank; the other pages are not affected.
      if (!stopped && !isCancelled(failure)) console.error(`Page ${number} could not be drawn`, failure);
    });
    return () => {
      stopped = true;
      render?.cancel();
      text?.cancel();
    };
  });

  // Unmounted: give the canvas memory back at once instead of waiting for garbage collection.
  $effect(() => () => {
    for (const canvas of canvasHost.querySelectorAll("canvas")) free(canvas);
  });

  function replaceCanvas(canvas: HTMLCanvasElement) {
    for (const old of canvasHost.querySelectorAll("canvas")) free(old);
    canvasHost.replaceChildren(canvas);
  }

  function free(canvas: HTMLCanvasElement) {
    canvas.width = 0;
    canvas.height = 0;
    canvas.remove();
  }
</script>

<div
  class="page {mode}"
  data-page={number}
  bind:this={pageElement}
  style:--total-scale-factor={zoom * CSS_PER_POINT}
>
  <div class="canvas" bind:this={canvasHost}></div>
  <!-- HighlightMark: shares of the page, so they fit every zoom. Below the text too. -->
  <div class="highlights">
    {#each highlights as highlight (highlight.id)}
      <div
        class="highlight {kindOf(highlight.color).token}"
        role="img"
        aria-label={highlightName(highlight.color, highlight.page)}
        data-highlight={highlight.id}
      >
        {#each highlight.rects as rect, index (index)}
          <div
            class="hl-rect"
            style:left="{rect.x * 100}%"
            style:top="{rect.y * 100}%"
            style:width="{rect.w * 100}%"
            style:height="{rect.h * 100}%"
          ></div>
        {/each}
      </div>
    {/each}
  </div>
  <!-- Below the transparent text, so selecting text is not affected. -->
  <div class="marks" aria-hidden="true">
    {#each marks as mark, index (index)}
      <div
        class="mark"
        class:active={mark.active}
        style:left="{mark.left * 100}%"
        style:top="{mark.top * 100}%"
        style:width="{mark.width * 100}%"
        style:height="{mark.height * 100}%"
      ></div>
    {/each}
  </div>
  <div class="text" bind:this={textHost}></div>
</div>

<style>
  .page {
    --scale-round-x: 1px;
    --scale-round-y: 1px;
    position: absolute;
    inset: 0;
  }

  .canvas {
    position: absolute;
    inset: 0;
  }

  .canvas :global(canvas) {
    display: block;
    width: 100%;
    height: 100%;
  }

  /* Page modes (FR-02.2) change how the drawing looks, without drawing it again. */
  .dark .canvas {
    filter: var(--page-dark-filter);
  }

  .sepia .canvas::after {
    content: "";
    position: absolute;
    inset: 0;
    background: var(--page-sepia);
    mix-blend-mode: multiply;
  }

  .highlights,
  .highlight,
  .marks {
    position: absolute;
    inset: 0;
    overflow: clip;
    pointer-events: none;
  }

  .hl-rect {
    position: absolute;
    mix-blend-mode: multiply;
  }

  .include .hl-rect {
    background: var(--hl-include-fill);
  }

  .exclude .hl-rect {
    background: var(--hl-exclude-fill);
  }

  .method .hl-rect {
    background: var(--hl-method-fill);
  }

  .result .hl-rect {
    background: var(--hl-result-fill);
  }

  /* On a dark page "multiply" would hide them (Brief Section 3, T-18). */
  .dark .hl-rect {
    mix-blend-mode: normal;
  }

  .mark {
    position: absolute;
    background: var(--search-match-fill);
    mix-blend-mode: multiply;
  }

  .mark.active {
    background: var(--search-active-fill);
    outline: 2px solid var(--search-active-border);
  }

  /* On a dark page "multiply" would hide the marks (Brief Section 3). */
  .dark .mark {
    background: var(--search-dark-match-fill);
    mix-blend-mode: normal;
  }

  .dark .mark.active {
    background: var(--search-dark-active-fill);
    outline-color: var(--search-dark-active-border);
  }

  /* The PDF.js text layer (from pdfjs-dist/web/pdf_viewer.css, reduced to what is
     used): transparent text placed exactly over the drawn text. */
  .text :global(.textLayer) {
    position: absolute;
    inset: 0;
    overflow: clip;
    line-height: 1;
    text-align: initial;
    letter-spacing: normal;
    word-spacing: normal;
    text-size-adjust: none;
    forced-color-adjust: none;
    transform-origin: 0 0;
    caret-color: CanvasText;
    --min-font-size: 1;
    --text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
    --min-font-size-inv: calc(1 / var(--min-font-size));
  }

  .text :global(.textLayer :is(span, br)) {
    position: absolute;
    color: transparent;
    white-space: pre;
    cursor: text;
    transform-origin: 0% 0%;
    user-select: text;
  }

  .text :global(.textLayer > :not(.markedContent)),
  .text :global(.textLayer .markedContent span:not(.markedContent)) {
    z-index: 1;
    --font-height: 0;
    font-size: calc(var(--text-scale-factor) * var(--font-height));
    --scale-x: 1;
    --rotate: 0deg;
    transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
  }

  .text :global(.textLayer .markedContent) {
    display: contents;
  }

  .text :global(.textLayer span[role="img"]) {
    cursor: default;
    user-select: none;
  }

  .text :global(.textLayer ::selection) {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    color: transparent;
  }

  .text :global(.textLayer br::selection) {
    background: transparent;
  }

  .text :global(.textLayer .endOfContent) {
    display: block;
    position: absolute;
    inset: 100% 0 0;
    z-index: 0;
    cursor: default;
    user-select: none;
  }

  /* While selecting, keeps the selection from jumping to the end of the page. */
  .text :global(.textLayer.selecting .endOfContent) {
    top: 0;
  }

  /* Pages with their own rotation. */
  .text :global([data-main-rotation="90"]) {
    transform: rotate(90deg) translateY(-100%);
  }

  .text :global([data-main-rotation="180"]) {
    transform: rotate(180deg) translate(-100%, -100%);
  }

  .text :global([data-main-rotation="270"]) {
    transform: rotate(270deg) translateX(-100%);
  }
</style>
