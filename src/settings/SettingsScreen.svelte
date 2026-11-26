<!--
  S-04 Settings, the "This library" group (FR-08.1, FR-08.2, FR-08.4). It covers the
  workspace, which stays as it was behind it. The "App" group comes with T-37 and
  T-41. Text approved in T-35.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Button from "../lib/components/Button.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import { SAVE_ERROR } from "../lib/libraryTexts";
  import CriteriaList from "./CriteriaList.svelte";
  import type { SettingsStore } from "./settingsStore.svelte";
  import { SECTIONS, type SettingsSection } from "./settingsTexts";

  interface Props {
    store: SettingsStore;
    section: SettingsSection;
    readOnly: boolean;
    /** The library's read-only message, if it has one. */
    notice?: string;
    onclose: () => void;
  }

  let { store, section = $bindable(), readOnly, notice, onclose }: Props = $props();

  const title = $derived(SECTIONS.find((each) => each.value === section)?.label);

  let nav: HTMLElement;
  onMount(() => nav.querySelector<HTMLElement>('[aria-current="page"]')?.focus());
</script>

<div class="settings">
  <header>
    <h1>Settings</h1>
    <Button onclick={onclose}>Back to reader</Button>
  </header>

  <div class="body">
    <nav aria-label="Settings" bind:this={nav}>
      <h2 class="group">This library</h2>
      <ul>
        {#each SECTIONS as each (each.value)}
          <li>
            <button
              type="button"
              aria-current={section === each.value ? "page" : undefined}
              onclick={() => (section = each.value)}
            >
              {each.label}
            </button>
          </li>
        {/each}
      </ul>
    </nav>

    <main>
      <div class="content">
        <h2>{title}</h2>
        {#if readOnly && notice}
          <InlineBanner tone="warning" title={notice} />
        {/if}
        {#if store.failed}
          <InlineBanner tone="error" title={SAVE_ERROR}>
            {#snippet actions()}
              <Button onclick={() => store.retry()}>Try again</Button>
            {/snippet}
          </InlineBanner>
        {/if}

        {#key section}
          <CriteriaList {store} kind={section} {readOnly} />
        {/key}
      </div>
    </main>
  </div>
</div>

<style>
  /* Above the workspace and its highlight popover, below toasts and the drop zone. */
  .settings {
    position: fixed;
    inset: 0;
    z-index: 15;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    background: var(--surface-app);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--panel-padding);
    border-bottom: 1px solid var(--border-subtle);
    background: var(--surface-panel);
  }

  h1 {
    margin: 0;
    font: var(--type-display);
  }

  .body {
    display: grid;
    grid-template-columns: clamp(160px, 25vw, 200px) minmax(0, 1fr);
    min-height: 0;
  }

  nav {
    overflow-y: auto;
    padding: var(--space-4) var(--space-2);
    border-right: 1px solid var(--border-subtle);
    background: var(--surface-panel);
  }

  .group {
    margin: 0 0 var(--space-2);
    padding: 0 var(--space-2);
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  nav button {
    width: 100%;
    min-height: var(--target-dense);
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-primary);
    font: var(--type-body);
    text-align: left;
    cursor: pointer;
  }

  nav button:hover {
    background: var(--surface-sunken);
  }

  nav button:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring);
  }

  nav button[aria-current="page"] {
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 600;
  }

  main {
    min-width: 0;
    overflow-y: auto;
  }

  .content {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: 880px;
    margin: 0 auto;
    padding: var(--space-6) var(--panel-padding);
  }

  .content h2 {
    margin: 0;
    font: var(--type-display);
  }

</style>
