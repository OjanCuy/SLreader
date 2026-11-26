<!--
  RefLink (FR-05.5; Brief Section 8): the `[#46]` links of a note, shown under its
  field. Nothing is shown for a note without links.
-->
<script lang="ts">
  import { linkedNumbers } from "../refLinks";

  interface Props {
    note: string;
    onopen: (refNo: number) => void;
  }

  let { note, onopen }: Props = $props();

  const numbers = $derived(linkedNumbers(note));
</script>

{#if numbers.length > 0}
  <div class="links">
    {#each numbers as number (number)}
      <button type="button" onclick={() => onopen(number)}>[#{number}]</button>
    {/each}
  </div>
{/if}

<style>
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  button {
    min-height: var(--target-dense);
    padding: 0 var(--space-1);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--accent);
    font: var(--type-body-small);
    font-variant-numeric: var(--type-numeric);
    cursor: pointer;
  }

  button:hover {
    text-decoration: underline;
  }

  button:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring);
  }
</style>
