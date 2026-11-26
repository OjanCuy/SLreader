<!--
  O-04: imports the reference list from pasted Excel rows or a CSV file (FR-05.2).
  An existing list is only replaced after a confirmation. The text was approved in T-25.
  Messages appear inside the dialog, because a modal dialog covers the toasts.
-->
<script lang="ts">
  import { importReferences, pickReferenceFile } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Dialog from "../lib/components/Dialog.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import TextArea from "../lib/components/TextArea.svelte";
  import { showToast } from "../lib/components/toasts.svelte";
  import { libraryErrorMessage } from "../lib/libraryTexts";
  import { duplicatesMessage, importedMessage, NO_ROWS, replaceMessage } from "../lib/referenceTexts";

  interface Props {
    session: number;
    onclose: () => void;
  }

  let { session, onclose }: Props = $props();

  let text = $state("");
  /** Shown under the field. */
  let fieldError = $state<string>();
  let failure = $state<string>();
  /** The size of the existing list while "Replace reference list?" is shown. */
  let existing = $state<number | null>(null);
  let busy = $state(false);

  async function chooseFile() {
    failure = undefined;
    try {
      const file = await pickReferenceFile();
      if (!file) return;
      if (file.text === null) {
        fieldError = `Could not open ${file.fileName}.`;
      } else {
        text = file.text;
        fieldError = undefined;
      }
    } catch (error) {
      failure = libraryErrorMessage(error, "load");
    }
  }

  async function run(replace: boolean) {
    if (busy) return;
    busy = true;
    fieldError = undefined;
    failure = undefined;
    try {
      const result = await importReferences(session, text, replace);
      if (result.status === "needs_confirmation") {
        existing = result.existing;
        return;
      }
      existing = null;
      if (result.status === "empty") {
        fieldError = NO_ROWS;
        return;
      }
      showToast(importedMessage(result.imported, result.skipped), "success");
      if (result.duplicates.length > 0) showToast(duplicatesMessage(result.duplicates));
      onclose();
    } catch (error) {
      existing = null;
      failure = libraryErrorMessage(error, "create");
    } finally {
      busy = false;
    }
  }
</script>

{#if existing === null}
  <Dialog open title="Import references" size="md" {busy} {onclose}>
    <TextArea
      label="Reference list"
      placeholder="Reference No; Title; Year; Source Database; DOI"
      rows={10}
      spellcheck="false"
      error={fieldError}
      bind:value={text}
    />
    <div>
      <Button onclick={chooseFile}>Choose CSV file</Button>
    </div>
    {#if failure}<InlineBanner tone="error" title={failure} />{/if}
    {#snippet actions()}
      <Button onclick={onclose}>Cancel</Button>
      <Button variant="primary" loading={busy} disabled={text.trim() === ""} onclick={() => run(false)}>
        Import
      </Button>
    {/snippet}
  </Dialog>
{:else}
  <!-- Cancel goes back to the list that was typed. -->
  <Dialog open title="Replace reference list?" {busy} onclose={() => (existing = null)}>
    <p>{replaceMessage(existing)}</p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (existing = null)}>Cancel</Button>
      <Button variant="danger" loading={busy} onclick={() => run(true)}>Replace list</Button>
    {/snippet}
  </Dialog>
{/if}

<style>
  p {
    margin: 0;
  }
</style>
