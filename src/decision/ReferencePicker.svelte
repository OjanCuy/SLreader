<!--
  ReferencePicker (FR-05.3, FR-05.4; Brief Section 8): a combobox of the reference
  list, ordered by similarity to the paper's file name, searched by typing. A reference
  of another paper cannot be chosen. Replacing filled metadata is confirmed. The list
  is in the flow, so the panel never cuts it off. Text approved in T-26.
-->
<script lang="ts">
  import { tick } from "svelte";
  import type { ReferenceChoice, ReferenceLink } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import TextField from "../lib/components/TextField.svelte";
  import { showToast } from "../lib/components/toasts.svelte";
  import {
    BEST_MATCH,
    NO_MATCHES,
    NO_REFERENCES,
    PICKER_LABEL,
    PICKER_PLACEHOLDER,
    referenceName,
    replaceMetadataMessage,
    takenBy,
    takenMessage,
  } from "./referencePickerTexts";
  import { matchesQuery, rankReferences, type RankedReference } from "./similarity";

  interface Props {
    references: ReferenceChoice[];
    paperId: string;
    /** The paper's file name, which the order of the list follows. */
    paperName: string;
    readOnly: boolean;
    onlink: (referenceId: string, replace: boolean) => Promise<ReferenceLink | null>;
    onunlink: () => void;
  }

  let { references, paperId, paperName, readOnly, onlink, onunlink }: Props = $props();

  const id = $props.id();

  const linked = $derived(references.find((each) => each.linkedPaperId === paperId));

  let open = $state(false);
  let query = $state("");
  /** The option the arrow keys are on; -1 for none. */
  let active = $state(-1);
  let box = $state<HTMLElement>();

  const ranked = $derived(open ? rankReferences(references, paperName, paperId) : []);
  const shown = $derived(
    query.trim() === "" ? ranked : ranked.filter((each) => matchesQuery(each.reference, query)),
  );

  const taken = (reference: ReferenceChoice) =>
    reference.linkedPaperId !== null && reference.linkedPaperId !== paperId;

  const input = () => box?.querySelector("input");

  function show() {
    if (open || readOnly) return;
    open = true;
    query = "";
    active = -1;
  }

  function hide() {
    open = false;
    query = "";
    active = -1;
  }

  async function highlight(index: number) {
    active = index;
    await tick();
    document.getElementById(`${id}-option-${index}`)?.scrollIntoView?.({ block: "nearest" });
  }

  /** The next option that can be chosen, going round the list. */
  function step(by: 1 | -1) {
    for (let moved = 1; moved <= shown.length; moved++) {
      const from = active < 0 && by < 0 ? 0 : active;
      const next = (((from + by * moved) % shown.length) + shown.length) % shown.length;
      if (!taken(shown[next]!.reference)) return highlight(next);
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (readOnly) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      show();
      step(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter" && open) {
      event.preventDefault();
      const option = shown[active];
      if (option) choose(option);
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      hide();
    }
  }

  function ontyped(value: string) {
    show();
    query = value;
    // Enter then takes the first result.
    active = -1;
    tick().then(() => {
      if (query.trim() !== "") step(1);
    });
  }

  /** The reference in the "Replace metadata?" dialog. */
  let confirming = $state.raw<ReferenceChoice | null>(null);
  let linking = $state(false);

  async function apply(reference: ReferenceChoice, replace: boolean) {
    if (linking) return;
    linking = true;
    try {
      const result = await onlink(reference.id, replace);
      confirming = result?.status === "needs_confirmation" ? reference : null;
      if (result?.status === "taken") showToast(takenMessage(reference.no, result.paper), "error");
    } finally {
      linking = false;
    }
  }

  function choose({ reference }: RankedReference) {
    if (taken(reference)) return;
    hide();
    if (reference.id !== linked?.id) apply(reference, false);
  }
</script>

{#if references.length === 0}
  {#if !readOnly}
    <p class="none">{NO_REFERENCES}</p>
  {/if}
{:else}
  <div class="picker" bind:this={box}>
    <TextField
      label={PICKER_LABEL}
      placeholder={PICKER_PLACEHOLDER}
      value={open ? query : linked ? referenceName(linked.no, linked.title) : ""}
      readonly={readOnly}
      role="combobox"
      autocomplete="off"
      spellcheck="false"
      aria-autocomplete="list"
      aria-expanded={open}
      aria-controls={open ? `${id}-list` : undefined}
      aria-activedescendant={open && active >= 0 ? `${id}-option-${active}` : undefined}
      oninput={(event) => ontyped(event.currentTarget.value)}
      onclick={show}
      onblur={hide}
      {onkeydown}
    />

    {#if open}
      <!-- The field keeps the focus, also when the list's scrollbar is dragged. -->
      <ul
        id="{id}-list"
        role="listbox"
        tabindex="-1"
        aria-label={PICKER_LABEL}
        onmousedown={(event) => event.preventDefault()}
      >
        {#each shown as option, index (option.reference.id)}
          <!-- The keys are handled by the field, which keeps the focus. -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <li
            id="{id}-option-{index}"
            role="option"
            aria-selected={option.reference.id === linked?.id}
            aria-disabled={taken(option.reference) || undefined}
            class:active={index === active}
            onclick={() => choose(option)}
          >
            <span class="reference">{referenceName(option.reference.no, option.reference.title)}</span>
            {#if option.best}
              <span class="best"><Icon name="check-circle" size={12} />{BEST_MATCH}</span>
            {/if}
            {#if taken(option.reference)}
              <span class="taken">{takenBy(option.reference.linkedPaperName ?? "")}</span>
            {/if}
            {#if option.reference.id === linked?.id}<Icon name="check" />{/if}
          </li>
        {/each}
      </ul>
      {#if shown.length === 0}
        <p class="none" role="status">{NO_MATCHES}</p>
      {/if}
    {/if}

    {#if linked && !readOnly && !open}
      <div>
        <Button
          variant="ghost"
          onclick={() => {
            onunlink();
            input()?.focus();
          }}
        >
          Unlink
        </Button>
      </div>
    {/if}
  </div>
{/if}

{#if confirming}
  <Dialog open title="Replace metadata?" busy={linking} onclose={() => (confirming = null)}>
    <p class="message">{replaceMetadataMessage(confirming.no)}</p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirming = null)}>Cancel</Button>
      <Button variant="danger" loading={linking} onclick={() => confirming && apply(confirming, true)}>
        Replace
      </Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  ul {
    max-height: 240px;
    overflow-y: auto;
    margin: 0;
    padding: var(--space-1);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    list-style: none;
  }

  ul:empty {
    display: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    min-height: var(--target-dense);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    font: var(--type-body-small);
    cursor: pointer;
  }

  li:hover,
  li.active {
    background: var(--accent-subtle);
  }

  li.active {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: calc(-1 * var(--focus-ring-width));
  }

  li[aria-disabled="true"] {
    background: transparent;
    color: var(--text-secondary);
    cursor: default;
  }

  .reference {
    flex: 1 1 100%;
    overflow-wrap: anywhere;
  }

  .best {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--accent);
    font: var(--type-caption);
  }

  .taken {
    font: var(--type-caption);
    overflow-wrap: anywhere;
  }

  .none {
    margin: 0;
    color: var(--text-secondary);
    font: var(--type-body-small);
  }

  .message {
    margin: 0;
  }
</style>
