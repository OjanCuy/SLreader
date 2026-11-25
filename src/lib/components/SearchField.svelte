<!--
  SearchField (Brief Section 8): a search box with the result count "n/m", previous
  and next buttons, and a clear button. Enter goes to the next result, Shift+Enter
  to the previous one, Esc clears. Texts are always passed in.
-->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import Spinner from "./Spinner.svelte";

  interface Props {
    /** Placeholder and accessible name. */
    label: string;
    previousLabel: string;
    nextLabel: string;
    clearLabel: string;
    value?: string;
    /** `current` is 1-based, and 0 when there are no results. Null: nothing to show. */
    results: { current: number; total: number } | null;
    /** The results are still being prepared. */
    busy?: boolean;
    onprevious: () => void;
    onnext: () => void;
  }

  let {
    label,
    previousLabel,
    nextLabel,
    clearLabel,
    value = $bindable(""),
    results,
    busy = false,
    onprevious,
    onnext,
  }: Props = $props();

  let input: HTMLInputElement;
  const none = $derived(results !== null && results.total === 0);

  export function focus() {
    input.focus();
    input.select();
  }

  function clear() {
    value = "";
    input.focus();
  }

  function onkeydown(event: KeyboardEvent) {
    // Enter that only confirms a character being composed (e.g. Japanese input).
    if (event.isComposing) return;
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.shiftKey) onprevious();
      else onnext();
    } else if (event.key === "Escape") {
      event.preventDefault();
      if (value === "") input.blur();
      else value = "";
    }
  }
</script>

<div class="search" role="search">
  <div class="box">
    <input
      bind:this={input}
      bind:value
      type="text"
      aria-label={label}
      placeholder={label}
      spellcheck="false"
      autocomplete="off"
      {onkeydown}
    />
    <span class="count" class:none role="status">
      {#if busy}
        <Spinner />
      {:else if results}
        {#if none}<Icon name="warning" />{/if}
        {results.current}/{results.total}
      {/if}
    </span>
  </div>
  <IconButton icon="chevron-up" label={previousLabel} disabled={!results?.total} onclick={onprevious} />
  <IconButton icon="chevron-down" label={nextLabel} disabled={!results?.total} onclick={onnext} />
  <IconButton icon="close" label={clearLabel} disabled={value === ""} onclick={clear} />
</div>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .box {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 28ch;
    min-height: var(--target-dense);
    padding: 0 var(--space-2);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-sm);
    background: var(--surface-panel);
  }

  .box:hover {
    border-color: var(--text-secondary);
  }

  .box:focus-within {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: var(--focus-ring-width);
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text-primary);
    font: var(--type-body);
  }

  .count {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--text-secondary);
    font: var(--type-caption);
    font-variant-numeric: var(--type-numeric);
    white-space: nowrap;
  }

  .none {
    color: var(--status-warning);
  }
</style>
