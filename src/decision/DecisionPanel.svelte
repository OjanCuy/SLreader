<!--
  S-02 Tab Decision (FR-04.1 to FR-04.4, FR-05.1; Brief Sections 6 and 7): file name,
  reference choice, metadata, decision, exclusion reason (only while excluded),
  assessment note, and "Delete paper" at the bottom. Text approved in T-22, T-24, and
  T-26. [#n] links (T-27) and AI suggestions (T-46) are added later.
-->
<script lang="ts">
  import Button from "../lib/components/Button.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import Select from "../lib/components/Select.svelte";
  import TextArea from "../lib/components/TextArea.svelte";
  import { GENERIC_ERROR, SAVE_ERROR } from "../lib/libraryTexts";
  import DecisionControl from "./DecisionControl.svelte";
  import type { DecisionStore } from "./decisionStore.svelte";
  import MetadataForm from "./MetadataForm.svelte";
  import ReferencePicker from "./ReferencePicker.svelte";

  interface Props {
    store: DecisionStore;
    /** The paper's file name, or its title when it has none. */
    name: string;
    readOnly: boolean;
    /** Opens the "Delete paper?" dialog. */
    ondelete: () => void;
    /** Opens Settings at the exclusion criteria. */
    onsettings: () => void;
  }

  let { store, name, readOnly, ondelete, onsettings }: Props = $props();

  const reasonOptions = $derived([
    { value: "", label: "No reason" },
    ...store.reasons.map((reason) => ({ value: reason.id, label: `${reason.code} - ${reason.label}` })),
  ]);
</script>

<div class="panel">
  <p class="name">{name}</p>

  {#if store.saveFailed}
    <InlineBanner tone="error" title={SAVE_ERROR}>
      {#snippet actions()}
        <Button onclick={() => store.retry()}>Try again</Button>
      {/snippet}
    </InlineBanner>
  {/if}

  {#if store.status === "error"}
    <p class="problem" role="alert"><Icon name="error" />{GENERIC_ERROR}</p>
  {:else if store.status === "ready" && store.screening && store.metadata}
    {#key store.paperId}
      <ReferencePicker
        references={store.references}
        paperId={store.paperId ?? ""}
        paperName={name}
        {readOnly}
        onlink={(referenceId, replace) => store.link(referenceId, replace)}
        onunlink={() => store.unlink()}
      />

      <MetadataForm
        metadata={store.metadata}
        sameRefNo={store.sameRefNo}
        saved={store.metadataSaved}
        {readOnly}
        onchange={(patch) => store.editMetadata(patch)}
        ondone={() => store.flush()}
      />

      <DecisionControl
        value={store.screening.decision}
        disabled={readOnly}
        onchange={(decision) => store.decide(decision)}
      />

      {#if store.screening.decision === "exclude"}
        <Select
          label="Exclusion reason"
          options={reasonOptions}
          value={store.screening.exclusionReasonId ?? ""}
          disabled={readOnly}
          onchange={(event) => store.chooseReason(event.currentTarget.value || null)}
        />
        {#if store.reasons.length === 0}
          <div><Button variant="ghost" onclick={onsettings}>Add in Settings</Button></div>
        {/if}
      {/if}

      <TextArea
        label="Assessment notes"
        value={store.screening.assessmentNote}
        rows={4}
        autoGrow
        maxRows={16}
        readonly={readOnly}
        oninput={(event) => store.editNote(event.currentTarget.value)}
        onblur={() => store.flush()}
      />
      <span class="saved" role="status">{store.noteSaved ? "Saved" : ""}</span>
    {/key}
  {/if}

  {#if !readOnly}
    <div class="delete">
      <Button variant="ghost" onclick={ondelete}>Delete paper</Button>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-height: 100%;
    padding: var(--space-4) var(--panel-padding);
  }

  .name {
    margin: 0;
    font: var(--type-paper-title);
    overflow-wrap: anywhere;
  }

  .problem {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--status-error);
  }

  .saved {
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .saved:empty {
    height: 0;
    margin-top: calc(-1 * var(--space-3));
  }

  .delete {
    display: flex;
    flex-direction: column;
    margin-top: auto;
    padding-top: var(--space-2);
    border-top: 1px solid var(--border-subtle);
  }
</style>
