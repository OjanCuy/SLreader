<!--
  Temporary: shows which library is open, under the start of the S-02 top bar.
  The rest of the Workspace is built from T-12 on (layout: T-38).
  App.svelte mounts it again for every opened library, so no state carries over.
-->
<script lang="ts">
  import type { LibrarySummary } from "../lib/api";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import { LIBRARY_PROBLEMS } from "../lib/libraryProblems";
  import { CLOUD_WARNING, libraryTitle, paperCountText } from "../lib/libraryTexts";
  import LibrarySwitcher from "./LibrarySwitcher.svelte";

  interface Props {
    library: LibrarySummary;
    onswitch: (library: LibrarySummary) => void;
    onshowall: () => void;
  }

  let { library, onswitch, onshowall }: Props = $props();

  const problem = $derived(library.problem ? LIBRARY_PROBLEMS[library.problem] : null);
</script>

<header>
  <LibrarySwitcher current={library} {onswitch} {onshowall} />
</header>

<main>
  <h1>{libraryTitle(library)}</h1>
  {#if library.paperCount !== null}<p>{paperCountText(library.paperCount)}</p>{/if}
  {#if problem?.readOnly}
    <InlineBanner tone="warning" title={problem.message} />
  {/if}
  {#if library.cloudSynced}
    <InlineBanner tone="warning" title={CLOUD_WARNING} />
  {/if}
</main>

<style>
  header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--panel-padding);
    border-bottom: 1px solid var(--border-subtle);
  }

  main {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-width: 720px;
    margin: 0 auto;
    padding: var(--space-6) var(--panel-padding);
  }

  h1 {
    margin: 0;
    font: var(--type-display);
  }

  p {
    margin: 0;
    color: var(--text-secondary);
  }
</style>
