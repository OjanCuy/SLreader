<!--
  S-02 Tab Highlights (FR-03.3, FR-03.4; Brief Section 6): the legend of the library's
  categories and a card for every highlight of the open paper, by page.
-->
<script lang="ts">
  import type { Highlight } from "../lib/api";
  import Icon from "../lib/components/Icon.svelte";
  import { GENERIC_ERROR } from "../lib/libraryTexts";
  import DeleteHighlightDialog from "./DeleteHighlightDialog.svelte";
  import HighlightCard from "./HighlightCard.svelte";
  import type { HighlightStore } from "./highlightStore.svelte";

  interface Props {
    store: HighlightStore;
    readOnly: boolean;
  }

  let { store, readOnly }: Props = $props();

  /** The highlight in the "Delete highlight?" dialog. */
  let confirming = $state.raw<Highlight | null>(null);
  let deleting = $state(false);

  // The question belongs to the paper it was asked for.
  $effect(() => {
    void store.paperId;
    confirming = null;
  });

  async function confirmDelete() {
    const highlight = confirming;
    if (!highlight || deleting) return;
    deleting = true;
    try {
      await store.remove(highlight.id);
    } finally {
      deleting = false;
      confirming = null;
    }
  }
</script>

<div class="panel">
  {#if store.status === "error"}
    <p class="problem" role="alert"><Icon name="error" />{GENERIC_ERROR}</p>
  {:else if store.status === "ready"}
    <ul class="legend">
      {#each store.categories as category (category.id)}
        <li>
          <span class="swatch" style:background="var(--hl-{category.color}-swatch)"></span>
          {category.label}
        </li>
      {/each}
    </ul>

    {#if store.highlights.length === 0}
      <p class="empty">Select text in the PDF, then pick a color to add a highlight.</p>
    {:else}
      <ul class="cards">
        {#each store.highlights as highlight (highlight.id)}
          <HighlightCard
            {highlight}
            categories={store.categories}
            tags={store.tags}
            active={highlight.id === store.activeId}
            saved={store.saved.has(highlight.id)}
            {readOnly}
            onshow={() => store.show(highlight.id)}
            oncategory={(categoryId) => store.edit(highlight.id, { categoryId })}
            ontag={(extractionTagId) => store.edit(highlight.id, { extractionTagId })}
            onnote={(note) => store.editNote(highlight.id, note)}
            onnotedone={() => store.flush(highlight.id)}
            ondelete={() => (confirming = highlight)}
          />
        {/each}
      </ul>
    {/if}
  {/if}
</div>

{#if confirming}
  <DeleteHighlightDialog
    page={confirming.page}
    busy={deleting}
    onconfirm={confirmDelete}
    onclose={() => (confirming = null)}
  />
{/if}

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4) var(--panel-padding);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    overflow-wrap: anywhere;
  }

  .swatch {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: var(--radius-full);
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .empty {
    margin: 0;
    color: var(--text-secondary);
    font: var(--type-body);
  }

  .problem {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--status-error);
    font: var(--type-body);
  }
</style>
