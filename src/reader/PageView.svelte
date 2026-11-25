<!--
  One PDF page: a canvas with the page drawn on it and the PDF.js text layer on top,
  so the text can be selected and read by a screen reader.
  Only mounted for pages near the visible area (FR-02.4); unmounting frees the canvas.
  On a zoom change the old drawing stays, stretched, until the new one is ready.
-->
<script lang="ts">
  import { TextLayer, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";
  import { canvasPixelRatio, CSS_PER_POINT } from "./layout";
  import { isCancelled } from "./pdfjs";

  interface Props {
    doc: PDFDocumentProxy;
    /** 1-based. */
    number: number;
    zoom: number;
  }

  let { doc, number, zoom }: Props = $props();

  let canvasHost: HTMLDivElement;
  let textHost: HTMLDivElement;

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

<div class="page" style:--total-scale-factor={zoom * CSS_PER_POINT}>
  <div class="canvas" bind:this={canvasHost}></div>
  <div class="text" bind:this={textHost}></div>
</div>

<style>
  .page {
    --scale-round-x: 1px;
    --scale-round-y: 1px;
    position: absolute;
    inset: 0;
  }

  .canvas :global(canvas) {
    display: block;
    width: 100%;
    height: 100%;
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
