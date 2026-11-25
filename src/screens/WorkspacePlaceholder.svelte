<!--
  Temporary: the open library with its papers, under the start of the S-02 top bar.
  The real Workspace layout comes with T-38.
  App.svelte mounts it again for every opened library, so no state carries over.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    importPdfs,
    listPapers,
    pickPdfs,
    type LibrarySummary,
    type PaperListItem,
  } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import { dismissToast, showToast } from "../lib/components/toasts.svelte";
  import { ADD_ANYWAY, identicalMessage, importMessages, SKIP } from "../lib/importTexts";
  import { LIBRARY_PROBLEMS } from "../lib/libraryProblems";
  import { CLOUD_WARNING, libraryErrorMessage, libraryTitle } from "../lib/libraryTexts";
  import DropZone from "./DropZone.svelte";
  import LibrarySwitcher from "./LibrarySwitcher.svelte";

  interface Props {
    library: LibrarySummary;
    onswitch: (library: LibrarySummary) => void;
    onshowall: () => void;
  }

  let { library, onswitch, onshowall }: Props = $props();

  const problem = $derived(library.problem ? LIBRARY_PROBLEMS[library.problem] : null);
  // Set for every opened library (T-10).
  const session = $derived(library.session ?? -1);

  let papers = $state<PaperListItem[] | null>(null);
  let importing = $state(false);

  /** Open "Add anyway / Skip" questions; they belong to this library only. */
  const questions: number[] = [];

  onMount(() => {
    refresh();
    return () => questions.forEach(dismissToast);
  });

  async function refresh() {
    try {
      papers = await listPapers(session);
    } catch (failure) {
      showToast(libraryErrorMessage(failure, "load"), "error");
    }
  }

  async function importPaths(paths: string[]) {
    if (paths.length === 0 || importing) return;
    if (problem?.readOnly) {
      showToast(problem.message, "error");
      return;
    }
    importing = true;
    try {
      await runImport(paths, false);
    } finally {
      importing = false;
    }
  }

  async function runImport(paths: string[], allowIdentical: boolean) {
    try {
      const report = await importPdfs(session, paths, allowIdentical);
      for (const { message, tone } of importMessages(report)) showToast(message, tone);
      for (const file of report.files) {
        if (file.outcome === "identical" && file.identicalTo !== null) {
          ask(file.path, file.identicalTo);
        }
      }
      await refresh();
    } catch (failure) {
      showToast(libraryErrorMessage(failure, "create"), "error");
    }
  }

  function ask(path: string, existing: string) {
    const id = showToast(identicalMessage(existing), "neutral", [
      // Not blocked by a running import: the Rust side handles one at a time.
      { label: ADD_ANYWAY, onselect: () => runImport([path], true) },
      { label: SKIP, onselect: () => {} },
    ]);
    questions.push(id);
  }

  async function upload() {
    if (importing) return;
    try {
      await importPaths(await pickPdfs());
    } catch (failure) {
      showToast(libraryErrorMessage(failure, "load"), "error");
    }
  }
</script>

{#snippet uploadButton()}
  <Button
    variant="primary"
    icon="upload"
    loading={importing}
    disabled={problem?.readOnly}
    onclick={upload}
  >
    Upload PDF
  </Button>
{/snippet}

<header>
  <LibrarySwitcher current={library} {onswitch} {onshowall} />
  <div class="actions">{@render uploadButton()}</div>
</header>

<main>
  <h1>{libraryTitle(library)}</h1>
  {#if problem?.readOnly}
    <InlineBanner tone="warning" title={problem.message} />
  {/if}
  {#if library.cloudSynced}
    <InlineBanner tone="warning" title={CLOUD_WARNING} />
  {/if}

  {#if papers === null}
    <!-- Loading: the list is short and local, so nothing is shown meanwhile. -->
  {:else if papers.length === 0}
    <EmptyState title="No papers yet.">
      {#snippet action()}
        <div class="empty-action">
          {@render uploadButton()}
          {#if !problem?.readOnly}<span>or drop PDF files here</span>{/if}
        </div>
      {/snippet}
    </EmptyState>
  {:else}
    <ul aria-label="Papers">
      {#each papers as paper (paper.id)}
        <li>
          <span class="name">{paper.title || paper.fileName}</span>
          {#if paper.fileMissing}
            <span class="missing"><Icon name="error" />PDF file is missing.</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</main>

<DropZone ondrop={importPaths} />

<style>
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
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

  .empty-action {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-secondary);
  }

  ul {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border-subtle);
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  .name {
    font: var(--type-paper-list);
    overflow-wrap: anywhere;
  }

  .missing {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--status-error);
    font: var(--type-caption);
  }
</style>
