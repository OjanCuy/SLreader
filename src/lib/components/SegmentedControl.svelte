<script lang="ts" generics="T extends string">
  interface Props {
    /** Names the group, e.g. "Appearance". Hidden visually unless `showLabel`. */
    label: string;
    options: ReadonlyArray<{ value: T; label: string }>;
    value: T;
    showLabel?: boolean;
    disabled?: boolean;
    onchange?: (value: T) => void;
  }

  let {
    label,
    options,
    value = $bindable(),
    showLabel = false,
    disabled = false,
    onchange,
  }: Props = $props();

  const name = $props.id();
</script>

<!-- Native radio buttons give arrow-key navigation and screen reader support. -->
<fieldset {disabled}>
  <legend class:hidden={!showLabel}>{label}</legend>
  <div class="segments">
    {#each options as option (option.value)}
      <label>
        <input
          type="radio"
          {name}
          value={option.value}
          checked={value === option.value}
          onchange={() => {
            value = option.value;
            onchange?.(option.value);
          }}
        />
        <span>{option.label}</span>
      </label>
    {/each}
  </div>
</fieldset>

<style>
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-1);
    padding: 0;
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .segments {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border-control);
    border-radius: var(--radius-sm);
    background: var(--surface-panel);
  }

  label {
    position: relative;
    display: inline-flex;
  }

  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  span {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: calc(var(--target-dense) - 6px);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    transition: background-color var(--motion-fast);
    pointer-events: none;
  }

  label:hover span {
    background: color-mix(in srgb, var(--text-primary) 7%, transparent);
  }

  label:active span {
    background: color-mix(in srgb, var(--text-primary) 14%, transparent);
  }

  /* Selected: border and weight as well as color. */
  input:checked + span {
    border-color: var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 600;
  }

  input:focus-visible + span {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: var(--focus-ring-width);
  }

  fieldset:disabled .segments {
    opacity: 0.5;
  }

  fieldset:disabled input {
    cursor: default;
  }
</style>
