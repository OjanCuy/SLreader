<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  interface Props {
    tone?: "info" | "warning" | "error";
    title: string;
    /** Optional extra text under the title. */
    children?: Snippet;
    actions?: Snippet;
  }

  let { tone = "info", title, children, actions }: Props = $props();
</script>

<div class="banner {tone}" role={tone === "error" ? "alert" : "status"}>
  <span class="icon"><Icon name={tone} size={20} /></span>
  <div class="content">
    <p class="title">{title}</p>
    {#if children}<div class="body">{@render children()}</div>{/if}
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </div>
</div>

<style>
  .banner {
    display: flex;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--border-control);
    border-radius: var(--radius-md);
    background: var(--surface-panel);
  }

  .icon {
    display: inline-flex;
    color: var(--text-secondary);
  }

  .warning {
    border-color: var(--status-warning);
  }

  .warning .icon {
    color: var(--status-warning);
  }

  .error {
    border-color: var(--status-error);
  }

  .error .icon {
    color: var(--status-error);
  }

  .content {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .title {
    margin: 0;
    font: var(--type-section);
    line-height: 20px;
  }

  .body {
    font: var(--type-body-small);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
</style>
