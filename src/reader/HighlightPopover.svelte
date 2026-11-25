<!--
  O-03 Highlight popover (FR-03.1): 4 buttons, each a swatch and its label; keys 1–4
  pick a color while it is open (PdfView handles the keys).
  Sits below the selection and flips above it when there is no room (Brief Section 9).
  It never takes focus or the mouse press, so the selection stays.
-->
<script lang="ts">
  import type { HighlightColor } from "../lib/api";
  import { HIGHLIGHT_KINDS } from "./highlights";

  interface Props {
    /** The selection on screen, in CSS pixels from the window's top left. */
    bounds: { left: number; top: number; right: number; bottom: number };
    onpick: (color: HighlightColor) => void;
  }

  let { bounds, onpick }: Props = $props();

  const GAP = 8;

  let width = $state(0);
  let height = $state(0);

  const top = $derived.by(() => {
    const below = bounds.bottom + GAP;
    if (below + height <= window.innerHeight - GAP) return below;
    return Math.max(GAP, bounds.top - GAP - height);
  });
  const left = $derived(Math.min(Math.max(GAP, bounds.left), Math.max(GAP, window.innerWidth - width - GAP)));
</script>

<div
  class="popover"
  role="group"
  style:top="{top}px"
  style:left="{left}px"
  bind:offsetWidth={width}
  bind:offsetHeight={height}
>
  {#each HIGHLIGHT_KINDS as kind (kind.color)}
    <button
      type="button"
      aria-keyshortcuts={kind.key}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => onpick(kind.color)}
    >
      <span class="swatch" style:background="var(--hl-{kind.token}-swatch)"></span>
      {kind.label}
    </button>
  {/each}
</div>

<style>
  .popover {
    position: fixed;
    z-index: 10;
    display: flex;
    gap: var(--space-1);
    padding: var(--space-1);
    border: var(--border-float);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
    box-shadow: var(--shadow-float);
  }

  button {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--target-dense);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-primary);
    font: var(--type-body);
    cursor: pointer;
  }

  button:hover {
    background: var(--surface-sunken);
  }

  button:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: calc(-1 * var(--focus-ring-width));
  }

  .swatch {
    width: 12px;
    height: 12px;
    border-radius: var(--radius-full);
  }
</style>
