<!-- O-02: full-window overlay while files are dragged over the app. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { onFileDrag } from "../lib/api";
  import Icon from "../lib/components/Icon.svelte";
  import { DROP_HERE, hasPdf, ONLY_PDFS } from "../lib/importTexts";

  interface Props {
    /** Called with every dropped path; the import reports files it cannot use. */
    ondrop: (paths: string[]) => void;
  }

  let { ondrop }: Props = $props();

  /** null: nothing is being dragged. */
  let state = $state<"ready" | "rejected" | null>(null);

  onMount(() => {
    const stop = onFileDrag((event) => {
      if (event.type === "enter") {
        state = hasPdf(event.paths) ? "ready" : "rejected";
      } else if (event.type === "leave") {
        state = null;
      } else {
        const accepted = hasPdf(event.paths);
        state = null;
        if (accepted) ondrop(event.paths);
      }
    });
    // Without the listener, Upload PDF still works.
    stop.catch(() => {});
    return () => {
      stop.then((unlisten) => unlisten()).catch(() => {});
    };
  });
</script>

{#if state}
  <!-- Mouse-only feedback while dragging; the result is announced by the toasts. -->
  <div class="overlay" class:rejected={state === "rejected"} aria-hidden="true">
    <div class="target">
      <Icon name={state === "rejected" ? "warning" : "upload"} size={32} />
      <p>{state === "rejected" ? ONLY_PDFS : DROP_HERE}</p>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-6);
    background: color-mix(in srgb, var(--surface-app) 85%, transparent);
    pointer-events: none;
  }

  .target {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    width: 100%;
    height: 100%;
    border: 2px dashed var(--accent);
    border-radius: var(--radius-md);
    color: var(--accent);
  }

  .rejected .target {
    border-color: var(--status-warning);
    color: var(--status-warning);
  }

  p {
    margin: 0;
    font: var(--type-display);
  }
</style>
