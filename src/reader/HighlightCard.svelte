<!--
  HighlightCard (FR-03.3, FR-03.4; Brief Section 8): category swatch (click to change
  it), page, tag, the highlighted text (click to jump to it), note with its [#n]
  links (FR-03.6), and delete. Text approved in T-21.
-->
<script lang="ts">
  import type { ExtractionTag, Highlight, HighlightCategory } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import RefLinks from "../lib/components/RefLinks.svelte";
  import Select from "../lib/components/Select.svelte";
  import TextArea from "../lib/components/TextArea.svelte";

  interface Props {
    highlight: Highlight;
    categories: HighlightCategory[];
    tags: ExtractionTag[];
    active: boolean;
    /** The note has just been saved. */
    saved: boolean;
    readOnly: boolean;
    onshow: () => void;
    oncategory: (categoryId: string) => void;
    ontag: (tagId: string | null) => void;
    onnote: (note: string) => void;
    onnotedone: () => void;
    ondelete: () => void;
    /** Opens Settings at the extraction tags. */
    onsettings: () => void;
    /** A [#n] link was clicked. */
    onopenref: (refNo: number) => void;
  }

  let {
    highlight,
    categories,
    tags,
    active,
    saved,
    readOnly,
    onshow,
    oncategory,
    ontag,
    onnote,
    onnotedone,
    ondelete,
    onsettings,
    onopenref,
  }: Props = $props();

  const id = $props.id();

  const category = $derived(categories.find((each) => each.id === highlight.categoryId));
  const options = $derived([
    { value: "", label: "No tag" },
    ...tags.map((tag) => ({ value: tag.id, label: tag.label })),
  ]);

  /** The list of categories is open. It is in the flow, so the panel never cuts it off. */
  let choosing = $state(false);
  let card: HTMLElement;
  let trigger = $state<HTMLButtonElement>();

  // A new or selected highlight comes into view.
  $effect(() => {
    if (active) card.scrollIntoView?.({ block: "nearest" });
  });

  function choose(categoryId: string) {
    choosing = false;
    trigger?.focus();
    if (categoryId !== highlight.categoryId) oncategory(categoryId);
  }
</script>

<li class="card" class:active bind:this={card} aria-current={active ? "true" : undefined}>
  <div class="top">
    <button
      bind:this={trigger}
      type="button"
      class="category"
      title="Change category"
      aria-label="Change category: {category?.label}"
      aria-expanded={choosing}
      aria-controls={choosing ? `${id}-categories` : undefined}
      disabled={readOnly}
      onclick={() => (choosing = !choosing)}
    >
      <span class="swatch" style:background="var(--hl-{category?.color}-swatch)"></span>
      <span class="name">{category?.label}</span>
    </button>
    <span class="page">p. {highlight.page}</span>
    {#if !readOnly}
      <IconButton icon="trash" label="Delete highlight" onclick={ondelete} />
    {/if}
  </div>

  {#if choosing}
    <ul class="choices" id="{id}-categories" aria-label="Change category">
      {#each categories as each (each.id)}
        <li>
          <button
            type="button"
            class="category"
            aria-pressed={each.id === highlight.categoryId}
            onclick={() => choose(each.id)}
            onkeydown={(event) => {
              if (event.key !== "Escape") return;
              event.preventDefault();
              choosing = false;
              trigger?.focus();
            }}
          >
            <span class="swatch" style:background="var(--hl-{each.color}-swatch)"></span>
            <span class="name">{each.label}</span>
            {#if each.id === highlight.categoryId}<Icon name="check" />{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <button type="button" class="text" onclick={onshow}>{highlight.text}</button>

  <Select
    label="Tag"
    {options}
    value={highlight.extractionTagId ?? ""}
    disabled={readOnly}
    onchange={(event) => ontag(event.currentTarget.value || null)}
  />
  {#if tags.length === 0}
    <div><Button variant="ghost" onclick={onsettings}>Add in Settings</Button></div>
  {/if}

  <TextArea
    label="Note"
    value={highlight.note}
    rows={2}
    autoGrow
    maxRows={8}
    readonly={readOnly}
    oninput={(event) => onnote(event.currentTarget.value)}
    onblur={onnotedone}
  />
  <span class="saved" role="status">{saved ? "Saved" : ""}</span>
  <RefLinks note={highlight.note} onopen={onopenref} />
</li>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font: var(--type-body-small);
  }

  .card.active {
    border-color: var(--accent);
    outline: 1px solid var(--accent);
  }

  .top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .category {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    min-height: var(--target-dense);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-primary);
    font: var(--type-body-small);
    cursor: pointer;
  }

  .category:hover:not(:disabled) {
    background: var(--surface-sunken);
  }

  .category:disabled {
    cursor: default;
  }

  .swatch {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: var(--radius-full);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .page {
    margin-left: auto;
    color: var(--text-secondary);
    font-variant-numeric: var(--type-numeric);
    white-space: nowrap;
  }

  .choices {
    margin: 0;
    padding: var(--space-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    list-style: none;
  }

  .choices .category {
    width: 100%;
  }

  .text {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 6;
    line-clamp: 6;
    padding: var(--space-1);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-primary);
    font: var(--type-body-small);
    text-align: left;
    overflow-wrap: anywhere;
    cursor: pointer;
  }

  .text:hover {
    text-decoration: underline;
  }

  .saved {
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .saved:empty {
    height: 0;
    margin-top: calc(-1 * var(--space-2));
  }
</style>
