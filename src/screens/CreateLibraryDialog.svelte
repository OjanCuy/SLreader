<!-- O-01: name and folder for a new library (FR-14.1). Mounted only while open. -->
<script lang="ts">
  import { createLibrary, pickFolder, type LibrarySummary } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import TextField from "../lib/components/TextField.svelte";
  import { libraryErrorMessage } from "../lib/libraryTexts";

  interface Props {
    oncreated: (library: LibrarySummary) => void;
    onclose: () => void;
  }

  let { oncreated, onclose }: Props = $props();

  let name = $state("");
  let folder = $state("");
  let busy = $state(false);
  let error = $state<string>();

  const ready = $derived(name.trim() !== "" && folder !== "");

  async function chooseFolder() {
    try {
      const chosen = await pickFolder();
      if (chosen !== null) {
        folder = chosen;
        error = undefined;
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
      oncreated(await createLibrary(folder, name));
    } catch (failure) {
      // The dialog stays open with what was typed, so the user can try again.
      error = libraryErrorMessage(failure, "create");
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
