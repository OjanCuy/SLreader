<!-- S-01 Libraries: the first screen. Create a library, or pick one and open it. -->
<script lang="ts">
  import { onMount } from "svelte";
  import {
    listLibraries,
    locateLibrary,
    openLibrary,
    openLibraryFolder,
    pickBackup,
    pickFolder,
    removeLibrary,
    type BackupFile,
    type LibraryError,
    type LibrarySummary,
  } from "../lib/api";
  import { backupErrorMessage, restoredMessage, RESTORE_BUTTON } from "../lib/backupTexts";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import EmptyState from "../lib/components/EmptyState.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import Skeleton from "../lib/components/Skeleton.svelte";
  import { showToast } from "../lib/components/toasts.svelte";
  import { GENERIC_ERROR, libraryErrorMessage, type LibraryAction } from "../lib/libraryTexts";
  import CreateLibraryDialog from "./CreateLibraryDialog.svelte";
  import LibraryCard from "./LibraryCard.svelte";
  import RenameLibraryDialog from "./RenameLibraryDialog.svelte";

  interface Props {
    /** A library was opened and is now the active one. */
    onopen: (library: LibrarySummary) => void;
  }

  let { onopen }: Props = $props();

  /** Null while the list is being read for the first time. */
  let libraries = $state<LibrarySummary[] | null>(null);
  let loadError = $state<string>();
  let creating = $state(false);
  let renaming = $state<LibrarySummary>();
  let removing = $state<LibrarySummary>();
  /** The backup a new library is being made from (FR-07.2). */
  let restoring = $state<BackupFile>();
  // Ignores clicks while a folder dialog or an opening is still going on.
  let working = false;

  onMount(refresh);

  async function refresh() {
    try {
      libraries = await listLibraries();
      loadError = undefined;
    } catch (failure) {
      loadError = libraryErrorMessage(failure, "load");
    }
  }

  async function run(action: LibraryAction, task: () => Promise<void>) {
    if (working) return;
    working = true;
    try {
      await task();
    } catch (failure) {
      // A listed library whose folder is gone explains itself on its card.
      const gone = action === "load" && (failure as LibraryError).code === "not_found";
      if (!gone) showToast(libraryErrorMessage(failure, action), "error");
      await refresh();
    } finally {
      working = false;
    }
  }

  const openListed = (library: LibrarySummary) =>
    run("load", async () => onopen(await openLibrary(library.id)));

  const openFromFolder = () =>
    run("open", async () => {
      const folder = await pickFolder();
      if (folder !== null) onopen(await openLibraryFolder(folder));
    });

  const locate = (library: LibrarySummary) =>
    run("locate", async () => {
      const folder = await pickFolder();
      if (folder === null) return;
      await locateLibrary(library.id, folder);
      await refresh();
    });

  /** "Restore from backup": choose the file, then name and folder in O-01. */
  async function chooseBackup() {
    if (working) return;
    working = true;
    try {
      restoring = (await pickBackup()) ?? undefined;
    } catch (failure) {
      showToast(backupErrorMessage(failure, GENERIC_ERROR), "error");
    } finally {
      working = false;
    }
  }

  function confirmRemove() {
    const library = removing;
    removing = undefined;
    if (!library) return;
    run("remove", async () => {
      await removeLibrary(library.id);
      await refresh();
    });
  }
</script>

<header>
  <h1>SLReader</h1>
  <div class="actions">
    <!-- An empty list has its own large "Create library" button below. -->
    {#if libraries === null || libraries.length > 0}
      <Button variant="primary" size="lg" onclick={() => (creating = true)}>Create library</Button>
    {/if}
    <Button size="lg" onclick={openFromFolder}>Open library</Button>
  </div>
</header>

<main>
  {#if loadError}
    <InlineBanner tone="error" title={loadError}>
      {#snippet actions()}
        <Button onclick={refresh}>Try again</Button>
      {/snippet}
    </InlineBanner>
  {/if}

  {#if libraries === null}
    {#if !loadError}
      <div aria-busy="true"><Skeleton shape="card" count={3} /></div>
    {/if}
  {:else if libraries.length === 0}
    <EmptyState size="display" title="No libraries yet. Create one to start a review.">
      {#snippet action()}
        <Button variant="primary" size="lg" onclick={() => (creating = true)}>Create library</Button>
      {/snippet}
    </EmptyState>
  {:else}
    <ul>
      {#each libraries as library (library.id)}
        <LibraryCard
          {library}
          onopen={() => openListed(library)}
          onlocate={() => locate(library)}
          onrename={() => (renaming = library)}
          onremove={() => (removing = library)}
          onrestore={chooseBackup}
        />
      {/each}
    </ul>
  {/if}
</main>

<footer>
  <Button variant="ghost" onclick={chooseBackup}>{RESTORE_BUTTON}</Button>
</footer>

{#if creating}
  <CreateLibraryDialog
    onclose={() => (creating = false)}
    oncreated={(library) => {
      creating = false;
      onopen(library);
    }}
  />
{/if}

{#if restoring}
  <CreateLibraryDialog
    backup={restoring}
    onclose={() => (restoring = undefined)}
    oncreated={(library) => {
      restoring = undefined;
      showToast(restoredMessage({ addedReasons: 0, addedTags: 0 }), "success");
      onopen(library);
    }}
  />
{/if}

{#if renaming}
  <RenameLibraryDialog
    library={renaming}
    onclose={() => (renaming = undefined)}
    onrenamed={() => {
      renaming = undefined;
      refresh();
    }}
  />
{/if}

<Dialog open={removing !== undefined} title="Remove from list?" onclose={() => (removing = undefined)}>
  <p class="confirm">The library folder and its files stay on your computer.</p>
  {#snippet actions()}
    <Button data-autofocus onclick={() => (removing = undefined)}>Cancel</Button>
    <Button variant="danger" onclick={confirmRemove}>Remove from list</Button>
  {/snippet}
</Dialog>

<style>
  header,
  main,
  footer {
    max-width: 720px;
    margin: 0 auto;
    padding: var(--space-6) var(--panel-padding);
  }

  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  main {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding-top: 0;
  }

  footer {
    padding-top: 0;
  }

  h1 {
    margin: 0;
    font: var(--type-display);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .confirm {
    margin: 0;
  }
</style>
