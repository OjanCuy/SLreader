<script lang="ts">
  import type { HTMLTextareaAttributes } from "svelte/elements";
  import { describedBy } from "./describedBy";
  import Field from "./Field.svelte";

  interface Props extends Omit<HTMLTextareaAttributes, "class" | "value" | "id" | "rows"> {
    label: string;
    value?: string;
    hint?: string;
    /** Error message. Setting it puts the field in the invalid state. */
    error?: string;
    rows?: number;
    /** Grow with the text, from `rows` up to `maxRows`. */
    autoGrow?: boolean;
    maxRows?: number;
  }

  let {
    label,
    value = $bindable(""),
    hint,
    error,
    rows = 3,
    autoGrow = false,
    maxRows,
    ...rest
  }: Props = $props();

  const id = $props.id();
</script>

<Field {id} {label} {hint} {error}>
  <textarea
    {...rest}
    {id}
    {rows}
    class:auto-grow={autoGrow}
    style:--min-rows={rows}
    style:--max-rows={autoGrow ? maxRows : undefined}
    aria-invalid={error ? true : undefined}
    aria-describedby={describedBy(id, hint, error)}
    bind:value
  ></textarea>
</Field>

<style>
  textarea {
    /* Must match the line height of --type-body. */
    --line: 20px;
    --chrome: calc(2 * var(--space-2) + 2px);
    width: 100%;
    padding: var(--space-2);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-sm);
    background: var(--surface-panel);
    font: var(--type-body);
    resize: vertical;
  }

  textarea:hover {
    border-color: var(--text-secondary);
  }

  .auto-grow {
    field-sizing: content;
    min-height: calc(var(--min-rows) * var(--line) + var(--chrome));
    max-height: calc(var(--max-rows, 1000) * var(--line) + var(--chrome));
    resize: none;
  }

  textarea[aria-invalid="true"] {
    border-color: var(--status-error);
  }

  /* Disabled text areas also match :read-only, so leave them out. */
  textarea:read-only:not(:disabled) {
    border-style: dashed;
    background: var(--surface-app);
  }

  textarea:disabled {
    opacity: 0.5;
  }
</style>
