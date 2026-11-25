<!--
  DecisionControl (FR-04.1; Brief Section 8): Include / Exclude / Undecided, each with
  its icon and label; the chosen one is filled with its decision color.
  Native radio buttons give arrow-key navigation and screen reader support.
-->
<script lang="ts">
  import type { Decision } from "../lib/api";
  import Icon, { type IconName } from "../lib/components/Icon.svelte";

  interface Props {
    value: Decision;
    disabled?: boolean;
    onchange: (decision: Decision) => void;
  }

  let { value, disabled = false, onchange }: Props = $props();

  const name = $props.id();

  const OPTIONS: { value: Decision; label: string; icon: IconName }[] = [
    { value: "include", label: "Include", icon: "check-circle" },
    { value: "exclude", label: "Exclude", icon: "close" },
    { value: "undecided", label: "Undecided", icon: "minus" },
  ];
</script>

<fieldset {disabled}>
  <legend>Decision</legend>
  <div class="options">
    {#each OPTIONS as option (option.value)}
      <label class={option.value}>
        <input
          type="radio"
          {name}
          value={option.value}
          checked={value === option.value}
          onchange={() => onchange(option.value)}
        />
        <span><Icon name={option.icon} />{option.label}</span>
      </label>
    {/each}
  </div>
</fieldset>

<style>
  fieldset {
    min-width: 0;
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

  .options {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  label {
    position: relative;
    display: flex;
    flex: 1;
  }

  input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  span {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    min-height: var(--target-primary);
    padding: 0 var(--space-2);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-sm);
    background: var(--surface-panel);
    white-space: nowrap;
    pointer-events: none;
    transition: background-color var(--motion-fast);
  }

  .include span {
    color: var(--decision-include);
  }

  .exclude span {
    color: var(--decision-exclude);
  }

  .undecided span {
    color: var(--decision-undecided);
  }

  label:hover span {
    background: color-mix(in srgb, var(--text-primary) 7%, var(--surface-panel));
  }

  .include input:checked + span {
    border-color: var(--decision-include);
    background: var(--decision-include);
    color: var(--decision-include-on);
    font-weight: 600;
  }

  .exclude input:checked + span {
    border-color: var(--decision-exclude);
    background: var(--decision-exclude);
    color: var(--decision-exclude-on);
    font-weight: 600;
  }

  .undecided input:checked + span {
    border-color: var(--decision-undecided);
    background: var(--decision-undecided);
    color: var(--decision-undecided-on);
    font-weight: 600;
  }

  input:focus-visible + span {
    outline: var(--focus-ring-width) solid var(--focus-ring);
    outline-offset: var(--focus-ring-width);
  }

  fieldset:disabled .options {
    opacity: 0.5;
  }

  fieldset:disabled input {
    cursor: default;
  }
</style>
