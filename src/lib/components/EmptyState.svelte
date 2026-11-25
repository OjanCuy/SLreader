<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    /** One sentence. */
    description?: string;
    /** "display" for a whole screen, "section" inside a panel. */
    size?: "display" | "section";
    /** One action, usually a Button. */
    action?: Snippet;
  }

  let { title, description, size = "section", action }: Props = $props();
</script>

<div class="empty">
  <p class="title {size}">{title}</p>
  {#if description}<p class="description">{description}</p>{/if}
  {#if action}<div class="action">{@render action()}</div>{/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-8) var(--panel-padding);
    text-align: center;
  }

  p {
    max-width: 48ch;
    margin: 0;
  }

  .display {
    font: var(--type-display);
  }

  .section {
    font: var(--type-section);
  }

  .description {
    color: var(--text-secondary);
  }

  .action {
    margin-top: var(--space-2);
  }
</style>
