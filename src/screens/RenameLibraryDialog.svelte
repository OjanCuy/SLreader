<!-- Rename a library (FR-14.3). Mounted only while open. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { renameLibrary, type LibrarySummary } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import TextField from "../lib/components/TextField.svelte";
  import { libraryErrorMessage, libraryTitle } from "../lib/libraryTexts";

  interface Props {
    library: LibrarySummary;
    onrenamed: () => void;
    onclose: () => void;
  }

  let { library, onrenamed, onclose }: Props = $props();

  // Starts with the current name; later changes to `library` must not overwrite typing.
  let name = $state(untrack(() => libraryTitle(library)));
  let busy = $state(false);
  let error = $state<string>();

  async function rename() {
    if (name.trim() === "" || busy) return;
    busy = true;
    error = undefined;
    try {
      await renameLibrary(library.id, name);
      onrenamed();
    } catch (failure) {
      error = libraryErrorMessage(failure, "rename");
    } finally {
      busy = false;
    }
  }
</script>

<Dialog open title="Rename library" {busy} {onclose}>
  <TextField
    label="Name"
    bind:value={name}
    onkeydown={(event) => {
      if (event.key === "Enter") rename();
    }}
  />
  {#if error}<InlineBanner tone="error" title={error} />{/if}

  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={name.trim() === ""} loading={busy} onclick={rename}>
      Rename
    </Button>
  {/snippet}
</Dialog>
