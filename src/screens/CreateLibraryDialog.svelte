<!--
  O-01: name and folder for a new library (FR-14.1). Mounted only while open.
  With `backup`, the new library is made from that backup ("Restore from backup",
  FR-07.2) and the name starts as the backup's.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import {
    createLibrary,
    createLibraryFromBackup,
    isCloudFolder,
    pickFolder,
    type BackupFile,
    type LibrarySummary,
  } from "../lib/api";
  import { backupErrorMessage } from "../lib/backupTexts";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import TextField from "../lib/components/TextField.svelte";
  import { CLOUD_WARNING, libraryErrorMessage } from "../lib/libraryTexts";

  interface Props {
    backup?: BackupFile;
    oncreated: (library: LibrarySummary) => void;
    onclose: () => void;
  }

  let { backup, oncreated, onclose }: Props = $props();

  let name = $state(untrack(() => backup?.libraryName ?? ""));
  let folder = $state("");
  let busy = $state(false);
  let error = $state<string>();
  let cloud = $state(false);

  const ready = $derived(name.trim() !== "" && folder !== "");

  async function chooseFolder() {
    try {
      const chosen = await pickFolder();
      if (chosen !== null) {
        folder = chosen;
        error = undefined;
        // Only a warning: if the check fails, creating is still possible.
        cloud = await isCloudFolder(chosen).catch(() => false);
      }
    } catch (failure) {
      error = libraryErrorMessage(failure, "load");
    }
  }

  async function create() {
    if (!ready || busy) return;
    busy = true;
    error = undefined;
    try {
      oncreated(
        backup
          ? await createLibraryFromBackup(folder, name, backup.path)
          : await createLibrary(folder, name),
      );
    } catch (failure) {
      // The dialog stays open with what was typed, so the user can try again.
      error = backupErrorMessage(failure, libraryErrorMessage(failure, "create"));
    } finally {
      busy = false;
    }
  }
</script>

<Dialog open title="Create library" size="md" {busy} {onclose}>
  <TextField
    label="Name"
    bind:value={name}
    onkeydown={(event) => {
      if (event.key === "Enter") create();
    }}
  />
  <div class="folder">
    <div class="path"><TextField label="Folder" readonly value={folder} /></div>
    <Button onclick={chooseFolder}>Choose folder</Button>
  </div>
  {#if cloud}<InlineBanner tone="warning" title={CLOUD_WARNING} />{/if}
  {#if error}<InlineBanner tone="error" title={error} />{/if}

  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!ready} loading={busy} onclick={create}>
      Create library
    </Button>
  {/snippet}
</Dialog>

<style>
  .folder {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
  }

  .path {
    flex: 1;
    min-width: 0;
  }
</style>
