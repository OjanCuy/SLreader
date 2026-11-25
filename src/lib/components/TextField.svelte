<script lang="ts">
  import type { HTMLInputAttributes } from "svelte/elements";
  import { describedBy } from "./describedBy";
  import Field from "./Field.svelte";

  interface Props extends Omit<HTMLInputAttributes, "class" | "value" | "prefix" | "id"> {
    label: string;
    value?: string;
    hint?: string;
    /** Error message. Setting it puts the field in the invalid state. */
    error?: string;
    /** Tabular digits and a numeric keyboard (No Ref, year, page). */
    numeric?: boolean;
    prefix?: string;
    suffix?: string;
  }

  let {
    label,
    value = $bindable(""),
    hint,
    error,
    numeric = false,
    prefix,
    suffix,
    type = "text",
    ...rest
  }: Props = $props();

  const id = $props.id();
</script>

<Field {id} {label} {hint} {error}>
  <div class="control" class:invalid={error} class:read-only={rest.readonly} class:disabled={rest.disabled}>
    {#if prefix}<span class="affix">{prefix}</span>{/if}
    <input
      {...rest}
      {id}
      {type}
      class:numeric
      inputmode={numeric ? "numeric" : rest.inputmode}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedBy(id, hint, error)}
      bind:value
    />
    {#if suffix}<span class="affix">{suffix}</span>{/if}
  </div>
</Field>

<style>
  .control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--target-dense);
    padding: 0 var(--space-2);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-sm);
    background: var(--surface-panel);
  }

  .control:hover {
    border-color: var(--text-secondary);
  }

  /* The ring goes around the whole box, including prefix and suffix. */
  .control:focus-within {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: var(--focus-ring-width);
  }

  .invalid,
  .invalid:hover {
    border-color: var(--status-error);
  }

  .read-only {
    border-style: dashed;
    background: var(--surface-app);
  }

  .disabled {
    opacity: 0.5;
    pointer-events: none;
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    font: var(--type-body);
  }

  .numeric {
    font-variant-numeric: var(--type-numeric);
  }

  .affix {
    color: var(--text-secondary);
    white-space: nowrap;
  }
</style>
