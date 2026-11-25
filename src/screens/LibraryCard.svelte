<script lang="ts">
  import type { LibrarySummary } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import Menu, { type MenuItem } from "../lib/components/Menu.svelte";
  import { LIBRARY_PROBLEMS } from "../lib/libraryProblems";
  import { lastOpenedText, libraryTitle, paperCountText } from "../lib/libraryTexts";

  interface Props {
    library: LibrarySummary;
    onopen: () => void;
    onlocate: () => void;
    onrename: () => void;
    onremove: () => void;
  }

  let { library, onopen, onlocate, onrename, onremove }: Props = $props();

  const title = $derived(libraryTitle(library));
  const problem = $derived(library.problem ? LIBRARY_PROBLEMS[library.problem] : null);
  // A read-only library still opens; a missing or damaged one does not.
  const canOpen = $derived(!problem || problem.readOnly);

  const meta = $derived(
    [
      library.paperCount === null ? null : paperCountText(library.paperCount),
      library.lastOpenedAt === null ? null : lastOpenedText(library.lastOpenedAt),
    ].filter((text) => text !== null),
  );

  const menuItems = $derived<MenuItem[]>([
    // Only a library that can be edited can be renamed.
    { label: "Rename", disabled: problem !== null, onselect: () => onrename() },
    { label: "Remove from list", onselect: () => onremove() },
  ]);
</script>

{#snippet details()}
  <span class="name">{title}</span>
  {#if meta.length > 0}
    <span class="meta">
      {#each meta as text (text)}<span>{text}</span>{/each}
    </span>
  {/if}
  {#if library.name === null}
    <span class="path">{library.folderPath}</span>
  {/if}
{/snippet}

<li class="card" class:openable={canOpen}>
  <div class="main">
    {#if canOpen}
      <button type="button" class="details" title={library.folderPath} onclick={onopen}>
        {@render details()}
      </button>
    {:else}
      <div class="details">{@render details()}</div>
    {/if}

    {#if problem}
      <div class="problem" class:warning={problem.readOnly}>
        <Icon name={problem.readOnly ? "warning" : "error"} />
        <span>{problem.message}</span>
        {#if library.problem === "not_found"}
          <Button onclick={onlocate}>Locate folder</Button>
        {/if}
      </div>
    {/if}
  </div>

  <Menu label="Options for {title}" iconOnly align="end" items={menuItems} />
</li>

<style>
  .card {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
  }

  .openable:hover {
    border-color: var(--border-control);
  }

  .main {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    overflow-wrap: anywhere;
  }

  button.details {
    cursor: pointer;
  }

  button.details:hover {
    background: color-mix(in srgb, var(--text-primary) 5%, transparent);
  }

  button.details:active {
    background: color-mix(in srgb, var(--text-primary) 12%, transparent);
  }

  .name {
    font: var(--type-section);
    font-size: 14px;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-4);
    color: var(--text-secondary);
    font: var(--type-body-small);
    font-variant-numeric: var(--type-numeric);
  }

  .path {
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .problem {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2) var(--space-2);
    color: var(--status-error);
    font: var(--type-body-small);
  }

  .warning {
    color: var(--status-warning);
  }
</style>
