<!--
  Place once, at the root of the app. Both live regions are always in the page,
  so screen readers announce new toasts: errors assertively, the rest politely.
  Note: an open modal Dialog covers this region.
-->
<script lang="ts">
  import Toast from "./Toast.svelte";
  import { dismissToast, toasts } from "./toasts.svelte";
</script>

<div class="region">
  {#each ["polite", "assertive"] as const as live (live)}
    <div class="stack" aria-live={live}>
      {#each toasts.filter((t) => (t.tone === "error") === (live === "assertive")) as toast (toast.id)}
        <Toast
          tone={toast.tone}
          message={toast.message}
          actions={toast.actions}
          onclose={() => dismissToast(toast.id)}
        />
      {/each}
    </div>
  {/each}
</div>

<style>
  .region {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: min(360px, calc(100vw - 2 * var(--space-4)));
    pointer-events: none;
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .stack:empty {
    margin-top: calc(-1 * var(--space-2));
  }
</style>
