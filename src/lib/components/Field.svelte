<script lang="ts">
  // Shared label, hint, and error layout for TextField, TextArea, and Select.
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    id: string;
    label: string;
    hint?: string;
    error?: string;
    children: Snippet;
  }

  let { id, label, hint, error, children }: Props = $props();
</script>

<div class="field">
  <label for={id}>{label}</label>
  {@render children()}
  {#if error}
    <p class="message error" id="{id}-error">
      <Icon name="error" />
      <span>{error}</span>
    </p>
  {/if}
  {#if hint}
    <p class="message" id="{id}-hint">{hint}</p>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }

  label {
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .message {
    display: flex;
    gap: var(--space-1);
    margin: 0;
    color: var(--text-secondary);
    font: var(--type-caption);
  }

  .error {
    color: var(--status-error);
  }
</style>
