<!--
  Temporary: the open library with its paper list on the left, the reader (T-16) in
  the middle, and the Decision and Highlights tabs on the right, under the start of
  the S-02 top bar. The real Workspace layout comes with T-38.
  App.svelte mounts it again for every opened library, so no state carries over.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import DecisionPanel from "../decision/DecisionPanel.svelte";
  import { createDecisionStore } from "../decision/decisionStore.svelte";
  import {
    deletePaper,
    importPdfs,
    listPapers,
    onPaperTextChanged,
    pickPdfs,
    type LibrarySummary,
    type PaperListItem,
  } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import ProgressBar from "../lib/components/ProgressBar.svelte";
  import TabBar, { tabId, tabPanelId } from "../lib/components/TabBar.svelte";
  import { dismissToast, showToast } from "../lib/components/toasts.svelte";
  import { ADD_ANYWAY, identicalMessage, importMessages, SKIP } from "../lib/importTexts";
  import { LIBRARY_PROBLEMS } from "../lib/libraryProblems";
  import { CLOUD_WARNING, libraryErrorMessage, libraryTitle } from "../lib/libraryTexts";
  import { createHighlightStore } from "../reader/highlightStore.svelte";
  import HighlightsPanel from "../reader/HighlightsPanel.svelte";
  import Reader from "../reader/Reader.svelte";
  import DeletePaperDialog from "./DeletePaperDialog.svelte";
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
  /** The paper open in the reader. */
  let selected = $state<string | null>(null);
  const selectedPaper = $derived(papers?.find((paper) => paper.id === selected));
  /** Shared by the reader and the Highlights tab. */
  const highlights = createHighlightStore();
  const decisions = createDecisionStore();
  /** The right panel's tab; it stays when another paper is opened. */
  let tab = $state<"decision" | "highlights">("decision");
  const TABS = [
    { value: "decision", label: "Decision" },
    { value: "highlights", label: "Highlights" },
  ] as const;

  $effect(() => {
    const id = selected;
    untrack(() => (id === null ? decisions.close() : decisions.open(session, id)));
  });
  /** The paper in the "Delete paper?" dialog. */
  let confirming = $state<PaperListItem | null>(null);
  let deleting = $state(false);

  /** Open "Add anyway / Skip" questions; they belong to this library only. */
  const questions: number[] = [];

  onMount(() => {
    refresh();
    // The background extraction reports each finished paper (FR-01.6).
    const stop = onPaperTextChanged((changed) => {
      if (changed === session) refresh();
    });
    stop.catch(() => {});
    return () => {
      highlights.flush();
      decisions.dispose();
      questions.forEach(dismissToast);
      stop.then((unlisten) => unlisten()).catch(() => {});
    };
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

  async function confirmDelete() {
    const paper = confirming;
    if (!paper || deleting) return;
    deleting = true;
    try {
      await deletePaper(session, paper.id);
      decisions.forget(paper.id);
      if (selected === paper.id) selected = null;
      await refresh();
    } catch (failure) {
      showToast(libraryErrorMessage(failure, "remove"), "error");
    } finally {
      deleting = false;
      confirming = null;
    }
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

<div class="workspace">
  <header>
    <LibrarySwitcher current={library} {onswitch} {onshowall} />
    <div class="actions">{@render uploadButton()}</div>
  </header>

  <div class="body" class:with-panel={selectedPaper}>
    <nav aria-label="Papers">
      <h1>{libraryTitle(library)}</h1>
      {#if problem?.readOnly}
        <InlineBanner tone="warning" title={problem.message} />
      {/if}
      {#if library.cloudSynced}
        <InlineBanner tone="warning" title={CLOUD_WARNING} />
      {/if}

      {#if papers !== null && papers.length > 0}
        <ul>
          {#each papers as paper (paper.id)}
            <li class:selected={paper.id === selected}>
              <button
                class="name"
                aria-current={paper.id === selected ? "true" : undefined}
                onclick={() => (selected = paper.id)}
              >
                {paper.title || paper.fileName}
              </button>
              {#if paper.extracting}
                <ProgressBar label="Extracting text…" indeterminate />
              {/if}
              {#if paper.fileMissing}
                <span class="missing"><Icon name="error" />PDF file is missing.</span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </nav>

    <main>
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
        <Reader
          {session}
          paperId={selected}
          readOnly={problem?.readOnly ?? false}
          onattached={refresh}
          store={highlights}
        />
      {/if}
    </main>

    {#if selectedPaper}
      <aside>
        <TabBar tabs={TABS} bind:value={tab} prefix="paper" />
        <div
          class="tab-panel"
          role="tabpanel"
          id={tabPanelId("paper")}
          aria-labelledby={tabId("paper", tab)}
        >
          {#if tab === "decision"}
            <DecisionPanel
              store={decisions}
              name={selectedPaper.fileName ?? selectedPaper.title}
              readOnly={problem?.readOnly ?? false}
              ondelete={() => (confirming = selectedPaper)}
            />
          {:else}
            <HighlightsPanel store={highlights} readOnly={problem?.readOnly ?? false} />
          {/if}
        </div>
      </aside>
    {/if}
  </div>
</div>

{#if confirming}
  <DeletePaperDialog
    name={confirming.fileName ?? confirming.title}
    busy={deleting}
    onconfirm={confirmDelete}
    onclose={() => (confirming = null)}
  />
{/if}

<DropZone ondrop={importPaths} />

<style>
  .workspace {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100vh;
  }

  .body {
    display: grid;
    grid-template-columns: 260px minmax(0, 1fr);
    min-height: 0;
  }

  /* The right panel narrows before the reader gets too small; T-38 adds the stacked mode. */
  .body.with-panel {
    grid-template-columns: 260px minmax(0, 1fr) clamp(240px, 30vw, 380px);
  }

  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-left: 1px solid var(--border-subtle);
    background: var(--surface-panel);
  }

  .tab-panel {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 0;
    padding: var(--space-4) var(--panel-padding);
    overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    background: var(--surface-panel);
  }

  main {
    min-width: 0;
    min-height: 0;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--panel-padding);
    border-bottom: 1px solid var(--border-subtle);
  }

  h1 {
    margin: 0;
    font: var(--type-display);
    overflow-wrap: anywhere;
  }

  .empty-action {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    color: var(--text-secondary);
  }

  ul {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border-subtle);
    list-style: none;
  }

  li {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--border-subtle);
  }

  li.selected {
    background: var(--accent-subtle);
  }

  .name {
    padding: var(--space-1);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-primary);
    font: var(--type-paper-list);
    text-align: left;
    overflow-wrap: anywhere;
    cursor: pointer;
  }

  .name:hover {
    text-decoration: underline;
  }

  .name:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring);
  }

  .missing {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--status-error);
    font: var(--type-caption);
  }
</style>
