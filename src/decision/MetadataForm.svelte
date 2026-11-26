<!--
  MetadataForm (FR-05.1, FR-05.4; Brief Section 8): reference number, year, title,
  source database, and DOI, saved automatically. A reference number another paper
  uses gets a warning; it is saved all the same. Text approved in T-24.
-->
<script lang="ts">
  import type { Metadata } from "../lib/api";
  import InlineBanner from "../lib/components/InlineBanner.svelte";
  import TextArea from "../lib/components/TextArea.svelte";
  import TextField from "../lib/components/TextField.svelte";
  import { sameRefNoMessage } from "./metadataTexts";

  interface Props {
    metadata: Metadata;
    /** Other papers with the same reference number. */
    sameRefNo: string[];
    saved: boolean;
    readOnly: boolean;
    onchange: (patch: Partial<Metadata>) => void;
    /** A field was left: save at once. */
    ondone: () => void;
    /** The reference number field was left. */
    onrefnodone: () => void;
  }

  let { metadata, sameRefNo, saved, readOnly, onchange, ondone, onrefnodone }: Props = $props();

  const shown = (number: number | null) => (number === null ? "" : String(number));

  /** Keeps only digits in the field; an empty field means no number. */
  function typedNumber(input: HTMLInputElement, maxDigits: number): number | null {
    const digits = input.value.replace(/\D/g, "").slice(0, maxDigits);
    if (input.value !== digits) input.value = digits;
    return digits === "" ? null : Number(digits);
  }
</script>

<div class="form">
  <div class="numbers">
    <TextField
      label="Reference No"
      numeric
      value={shown(metadata.refNo)}
      readonly={readOnly}
      oninput={(event) => onchange({ refNo: typedNumber(event.currentTarget, 9) })}
      onblur={onrefnodone}
    />
    <TextField
      label="Year"
      numeric
      value={shown(metadata.year)}
      readonly={readOnly}
      oninput={(event) => onchange({ year: typedNumber(event.currentTarget, 4) })}
      onblur={ondone}
    />
  </div>
  {#if metadata.refNo !== null && sameRefNo.length > 0}
    <InlineBanner tone="warning" title={sameRefNoMessage(metadata.refNo, sameRefNo)} />
  {/if}
  <div class="title">
    <TextArea
      label="Title"
      value={metadata.title}
      rows={2}
      autoGrow
      maxRows={6}
      readonly={readOnly}
      oninput={(event) => {
        // A title is one line; a pasted line break becomes a space.
        const title = event.currentTarget.value.replace(/[\r\n]+/g, " ");
        if (title !== event.currentTarget.value) event.currentTarget.value = title;
        onchange({ title });
      }}
      onkeydown={(event) => {
        if (event.key === "Enter") event.preventDefault();
      }}
      onblur={ondone}
    />
  </div>
  <TextField
    label="Source database"
    value={metadata.sourceDatabase}
    readonly={readOnly}
    oninput={(event) => onchange({ sourceDatabase: event.currentTarget.value })}
    onblur={ondone}
  />
  <TextField
    label="DOI"
    value={metadata.doi}
    readonly={readOnly}
    oninput={(event) => onchange({ doi: event.currentTarget.value })}
    onblur={ondone}
  />
  <span class="saved" role="status">{saved ? "Saved" : ""}</span>
</div>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .numbers {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
  }

  .title :global(textarea) {
    font: var(--type-paper-list);
  }

  .saved {
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .saved:empty {
    height: 0;
    margin-top: calc(-1 * var(--space-3));
  }
</style>
