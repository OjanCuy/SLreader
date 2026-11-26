<!--
  O-09 Load a template: shows what the list lacks and adds it. Mounted only while open.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import { SAVE_ERROR } from "../lib/libraryTexts";
  import type { SettingsStore } from "./settingsStore.svelte";
  import {
    addedNote,
    LOAD,
    LOAD_TEMPLATE,
    missingItems,
    NOTHING_TO_ADD,
    TEMPLATE_NAME,
    type TemplateKind,
  } from "./templates";

  interface Props {
    store: SettingsStore;
    kind: TemplateKind;
    onclose: () => void;
  }

  let { store, kind, onclose }: Props = $props();

  // As the list was when the dialog opened, so the text stays while saving.
  const items = untrack(() => missingItems(kind, store.lists[kind].rows));

  let busy = $state(false);
  let failed = $state(false);

  async function load() {
    if (busy || items.length === 0) return;
    busy = true;
    failed = false;
    const saved = await store.addTemplate(kind, items);
    busy = false;
    // A failed save keeps the dialog open, so the user can try again.
    if (saved) onclose();
    else failed = true;
  }
</script>

<Dialog open title={LOAD_TEMPLATE} size="md" {busy} {onclose}>
  <h3>{TEMPLATE_NAME}</h3>
  {#if items.length === 0}
    <p>{NOTHING_TO_ADD}</p>
  {:else}
    <p>{addedNote(items.length)}</p>
    <ul>
      {#each items as item (item.code + item.label)}
        <li>{item.code ? `${item.code} - ${item.label}` : item.label}</li>
      {/each}
    </ul>
  {/if}
  {#if failed}<InlineBanner tone="error" title={SAVE_ERROR} />{/if}

  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={items.length === 0} loading={busy} onclick={load}>
      {LOAD}
    </Button>
  {/snippet}
</Dialog>

<style>
  h3,
  p {
    margin: 0;
  }

  h3 {
    font: var(--type-body);
    font-weight: 600;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--text-secondary);
  }
</style>
