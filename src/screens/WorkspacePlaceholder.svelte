<!-- Temporary: shows which library is open. Replaced by the S-02 Workspace (T-10, T-38). -->
<script lang="ts">
  import type { LibrarySummary } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import { LIBRARY_PROBLEMS } from "../lib/libraryProblems";
  import { libraryTitle, paperCountText } from "../lib/libraryTexts";

  interface Props {
    library: LibrarySummary;
    onback: () => void;
  }

  let { library, onback }: Props = $props();
</script>

<main>
  <div><Button onclick={onback}>All libraries</Button></div>
  <h1>{libraryTitle(library)}</h1>
  {#if library.paperCount !== null}<p>{paperCountText(library.paperCount)}</p>{/if}
  {#if library.problem === "newer_version"}
    <InlineBanner tone="warning" title={LIBRARY_PROBLEMS.newer_version.message} />
  {/if}
</main>

<style>
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
