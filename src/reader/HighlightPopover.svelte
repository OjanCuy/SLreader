<!--
  O-03 Highlight popover (FR-03.1): one button per highlight category of the library
  (1 to 8), each a swatch and the category's name; keys 1–8 pick one while it is open
  (PdfView handles the keys).
  Sits below the selection and flips above it when there is no room (Brief Section 9).
  It never takes focus or the mouse press, so the selection stays.
-->
<script lang="ts">
  import type { HighlightCategory } from "../lib/api";
  import { categoryKey } from "./highlights";

  interface Props {
    categories: HighlightCategory[];
    /** The selection on screen, in CSS pixels from the window's top left. */
    bounds: { left: number; top: number; right: number; bottom: number };
    onpick: (category: HighlightCategory) => void;
  }

  let { categories, bounds, onpick }: Props = $props();

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
  {#each categories as category, index (category.id)}
    <button
      type="button"
      aria-keyshortcuts={categoryKey(index)}
      title={category.label}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => onpick(category)}
    >
      <span class="swatch" style:background="var(--hl-{category.color}-swatch)"></span>
      <span class="label">{category.label}</span>
    </button>
  {/each}
</div>

<style>
  .popover {
    position: fixed;
    z-index: 10;
    display: flex;
    flex-wrap: wrap;
    max-width: min(560px, calc(100vw - 16px));
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
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: var(--radius-full);
  }

  /* A long category name does not make the popover huge. */
  .label {
    max-width: 16ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
