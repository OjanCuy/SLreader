<!--
  ReaderToolbar (Brief Section 6, S-02): zoom out / in, Fit width, the page field,
  and search. Page mode joins it in T-18.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "../lib/components/Button.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import { MAX_ZOOM, MIN_ZOOM, parsePage, zoomLabel } from "./layout";

  interface Props {
    zoom: number;
    /** 1-based page that fills most of the view. */
    page: number;
    total: number;
    onzoomout: () => void;
    onzoomin: () => void;
    onfitwidth: () => void;
    ongotopage: (page: number) => void;
    /** The SearchField. */
    search: Snippet;
  }

  let { zoom, page, total, onzoomout, onzoomin, onfitwidth, ongotopage, search }: Props = $props();

  const id = $props.id();
  let typed = $state("");
  let editing = $state(false);
  const shown = $derived(editing ? typed : String(page));

  function startEditing() {
    typed = String(page);
    editing = true;
  }

  function finish() {
    if (!editing) return;
    editing = false;
    const target = parsePage(typed, total);
    if (target !== null) ongotopage(target);
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      finish();
    } else if (event.key === "Escape") {
      // Leaves the page as it was.
      editing = false;
      (event.currentTarget as HTMLInputElement).blur();
    }
  }
</script>

<div class="toolbar">
  <div class="group">
    <IconButton icon="minus" label="Zoom out" disabled={zoom <= MIN_ZOOM} onclick={onzoomout} />
    <span class="zoom">{zoomLabel(zoom)}</span>
    <IconButton icon="plus" label="Zoom in" disabled={zoom >= MAX_ZOOM} onclick={onzoomin} />
    <Button variant="ghost" onclick={onfitwidth}>Fit width</Button>
  </div>
  <div class="group page">
    <label for="{id}-page">Page</label>
    <input
      id="{id}-page"
      value={shown}
      inputmode="numeric"
      size="4"
      onfocus={startEditing}
      oninput={(event) => (typed = event.currentTarget.value)}
      onblur={finish}
      {onkeydown}
    />
    <span class="total">/ {total}</span>
  </div>
  {@render search()}
</div>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-1) var(--panel-padding);
    border-bottom: 1px solid var(--border-subtle);
    background: var(--surface-panel);
  }

  .group {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .zoom {
    min-width: 5ch;
    font: var(--type-body-small);
    font-variant-numeric: var(--type-numeric);
    text-align: center;
  }

  .page {
    gap: var(--space-2);
    font: var(--type-body-small);
    color: var(--text-secondary);
  }

  input {
    width: 5ch;
    min-height: var(--target-dense);
    padding: 0 var(--space-2);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-sm);
    background: var(--surface-panel);
    color: var(--text-primary);
    font: var(--type-body);
    font-variant-numeric: var(--type-numeric);
    text-align: right;
  }

  input:hover {
    border-color: var(--text-secondary);
  }

  input:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: var(--focus-ring-width);
  }

  .total {
    font-variant-numeric: var(--type-numeric);
  }
</style>
