<script lang="ts">
  import type { HTMLSelectAttributes } from "svelte/elements";
  import { describedBy } from "./describedBy";
  import Field from "./Field.svelte";
  import Icon from "./Icon.svelte";

  interface Props extends Omit<HTMLSelectAttributes, "class" | "value" | "id"> {
    label: string;
    options: ReadonlyArray<{ value: string; label: string; disabled?: boolean }>;
    value?: string;
    hint?: string;
    error?: string;
  }

  let { label, options, value = $bindable(""), hint, error, ...rest }: Props = $props();

  const id = $props.id();
</script>

<Field {id} {label} {hint} {error}>
  <div class="control">
    <select
      {...rest}
      {id}
      aria-invalid={error ? true : undefined}
      aria-describedby={describedBy(id, hint, error)}
      bind:value
    >
      {#each options as option (option.value)}
        <option value={option.value} disabled={option.disabled}>{option.label}</option>
      {/each}
    </select>
    <Icon name="chevron-down" />
  </div>
</Field>

<style>
  .control {
    position: relative;
    display: flex;
    align-items: center;
  }

  select {
    width: 100%;
    min-height: var(--target-dense);
    padding: 0 var(--space-8) 0 var(--space-2);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-sm);
    background: var(--surface-panel);
    font: var(--type-body);
    appearance: none;
    cursor: pointer;
  }

  select:hover {
    border-color: var(--text-secondary);
  }

  select:active {
    background: color-mix(in srgb, var(--text-primary) 7%, var(--surface-panel));
  }

  select[aria-invalid="true"] {
    border-color: var(--status-error);
  }

  select:disabled {
    opacity: 0.5;
    cursor: default;
  }

  /* The chevron sits on top of the select without blocking clicks. */
  .control :global(svg) {
    position: absolute;
    right: var(--space-2);
    color: var(--text-secondary);
    pointer-events: none;
  }
</style>
